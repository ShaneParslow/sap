//! Invoking kernel objects: the `CALL` syscall on a capability to a kernel
//! object. The kernel finds the capability in the caller's capability
//! space, decodes the message as an operation on that object type (the
//! label picks the operation, message words and extra capabilities carry
//! the arguments), performs it, and replies with an error code as the
//! label and any results as message words.
//!
//! Operations follow seL4's invocations of the same name.

use sap_abi::{error, label, MessageInfo, MAX_EXTRA_CAPS, MESSAGE_REGISTERS};

use crate::arch::UserContext;
use crate::cap::Cap;
use crate::cspace::{self, LookupError, Slot};
use crate::thread::{self, Tcb};
use crate::untyped;

/// An invocation's result: the number of result words, or an error code.
pub type Result<T = usize> = core::result::Result<T, usize>;

impl From<LookupError> for usize {
    fn from(_: LookupError) -> usize {
        error::FAILED_LOOKUP
    }
}

/// The message of an invocation: words in registers, then the IPC buffer.
pub struct Message<'a> {
    info: MessageInfo,
    context: &'a mut UserContext,
    tcb: *const Tcb,
}

impl Message<'_> {
    pub fn label(&self) -> usize {
        self.info.label()
    }

    /// Set result word `i` (registers only, for now).
    pub fn set_result(&mut self, i: usize, value: usize) {
        assert!(i < MESSAGE_REGISTERS, "results beyond the registers aren't supported yet");
        self.context.set_result(2 + i, value);
    }

    /// Message word `i`, or TRUNCATED_MESSAGE if the message is shorter.
    pub fn word(&self, i: usize) -> Result {
        if i >= self.info.length() {
            return Err(error::TRUNCATED_MESSAGE);
        }
        if i < MESSAGE_REGISTERS {
            return Ok(self.context.arg(2 + i));
        }
        // SAFETY: the calling thread's TCB is live.
        let buffer = unsafe { thread::ipc_buffer(self.tcb) }.ok_or(error::TRUNCATED_MESSAGE)?;
        Ok(buffer.msg[i])
    }

    /// The slot and capability named by extra-capability address `i`.
    pub fn extra_cap(&self, i: usize) -> Result<(*mut Slot, Cap)> {
        if i >= self.info.extra_caps() || i >= MAX_EXTRA_CAPS {
            return Err(error::TRUNCATED_MESSAGE);
        }
        // SAFETY: the calling thread's TCB is live.
        let (buffer, root) = unsafe { (thread::ipc_buffer(self.tcb), thread::cspace(self.tcb)) };
        let buffer = buffer.ok_or(error::TRUNCATED_MESSAGE)?;
        Ok(cspace::lookup_cap(root, buffer.caps[i])?)
    }
}

/// Handle `CALL` from thread `tcb`, whose registers are `context`.
///
/// # Safety
/// `tcb` must be the live TCB that holds `context`.
pub unsafe fn call(tcb: *const Tcb, context: &mut UserContext) {
    let cptr = context.arg(0);
    let info = MessageInfo(context.arg(1));
    // SAFETY: per the caller's guarantee.
    let root = unsafe { thread::cspace(tcb) };
    let mut message = Message { info, context, tcb };
    let reply = match cspace::lookup_cap(root, cptr) {
        Ok((slot, cap)) => invoke(slot, cap, &mut message),
        Err(e) => Err(e.into()),
    };
    let info = match reply {
        Ok(results) => MessageInfo::new(error::OK, 0, results),
        Err(code) => MessageInfo::new(code, 0, 0),
    };
    message.context.set_result(0, 0);
    message.context.set_result(1, info.0);
}

fn invoke(slot: *mut Slot, cap: Cap, message: &mut Message) -> Result {
    match cap {
        Cap::CNode { .. } => cnode(cap, message),
        Cap::Untyped { .. } => untyped(slot, message),
        Cap::Arch(a) => crate::arch::invoke(slot, a, message),
        _ => Err(error::ILLEGAL_OPERATION),
    }
}

fn untyped(slot: *mut Slot, m: &mut Message) -> Result {
    if m.info.label() != label::UNTYPED_RETYPE {
        return Err(error::ILLEGAL_OPERATION);
    }
    let (_, root) = m.extra_cap(0)?;
    // As in seL4, depth 0 names the root CNode itself.
    let dest = match m.word(3)? {
        0 => root,
        depth => {
            let node = cspace::lookup_target(root, m.word(2)?, depth)?;
            // SAFETY: lookup returns live slots.
            unsafe { (*node).cap }
        }
    };
    let request = untyped::Request {
        object_type: m.word(0)?,
        size_bits: m.word(1)?,
        dest,
        offset: m.word(4)?,
        count: m.word(5)?,
    };
    // SAFETY: slot is live and holds the invoked untyped capability.
    unsafe { untyped::retype(slot, &request) }?;
    Ok(0)
}

fn cnode(cnode: Cap, m: &mut Message) -> Result {
    let target = |index: usize, depth: usize| -> Result<*mut Slot> {
        Ok(cspace::lookup_target(cnode, m.word(index)?, m.word(depth)?)?)
    };
    match m.info.label() {
        label::CNODE_REVOKE => {
            let slot = target(0, 1)?;
            // SAFETY: lookup returns live slots.
            unsafe { cspace::revoke(slot) }.map_err(|_| error::ILLEGAL_OPERATION)?;
            Ok(0)
        }
        label::CNODE_DELETE => {
            let slot = target(0, 1)?;
            // SAFETY: as above.
            unsafe { cspace::delete(slot) }.map_err(|_| error::ILLEGAL_OPERATION)?;
            Ok(0)
        }
        op @ (label::CNODE_COPY | label::CNODE_MINT | label::CNODE_MOVE) => {
            let dest = target(0, 1)?;
            let (_, src_root) = m.extra_cap(0)?;
            let src = cspace::lookup_target(src_root, m.word(2)?, m.word(3)?)?;
            // SAFETY: lookup returns live slots.
            let (src_cap, dest_cap) = unsafe { ((*src).cap, (*dest).cap) };
            if !dest_cap.is_null() {
                return Err(error::DELETE_FIRST);
            }
            if src_cap.is_null() {
                return Err(error::FAILED_LOOKUP);
            }
            if op == label::CNODE_MOVE {
                // SAFETY: both live; dest is empty.
                unsafe { cspace::move_cap(src, dest) };
                return Ok(0);
            }
            let mut new = derive(src, src_cap)?.masked(m.word(4)?);
            if op == label::CNODE_MINT {
                new = mint(new, m.word(5)?)?;
            }
            // SAFETY: both live; dest is empty.
            unsafe { cspace::insert_derived(new, src, dest) };
            Ok(0)
        }
        _ => Err(error::ILLEGAL_OPERATION),
    }
}

/// The capability a copy of `cap` (in `slot`) gets, following seL4's
/// `deriveCap`: an untyped capability can only be copied while nothing has
/// been retyped from it, and a copied frame capability starts unmapped.
fn derive(slot: *mut Slot, cap: Cap) -> Result<Cap> {
    match cap {
        // SAFETY: slot is live.
        Cap::Untyped { .. } if unsafe { cspace::has_children(slot) } => Err(error::REVOKE_FIRST),
        Cap::Arch(crate::arch::ArchCap::Frame { pa, size_bits, rights, .. }) => {
            Ok(Cap::Arch(crate::arch::unmapped_frame(pa, size_bits, rights)))
        }
        other => Ok(other),
    }
}

/// Apply MINT's data word: a new guard for a CNode capability (bits 0..6 =
/// guard size, the rest = guard value).
fn mint(cap: Cap, data: usize) -> Result<Cap> {
    match cap {
        Cap::CNode { ptr, radix, .. } => {
            let guard_bits = (data & 0x3f) as u8;
            if radix as usize + guard_bits as usize > usize::BITS as usize {
                return Err(error::RANGE_ERROR);
            }
            Ok(Cap::CNode { ptr, radix, guard_bits, guard: data >> 6 })
        }
        other => Ok(other),
    }
}
