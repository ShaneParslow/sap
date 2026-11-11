//! The audit: a check of the kernel's own data structures, run before every
//! return to user mode in kernels built with the `audit` feature
//! (`cargo run --features audit`). A broken invariant panics, naming it, at
//! the first return to user mode after the operation that broke it. Built
//! without the feature, none of this exists.
//!
//! Each module checks its own structures, in an `audit` submodule beside
//! them: the derivation lists (`cspace`), the ready queues (`sched`),
//! threads' blocking state (`thread`), and the queues of threads waiting
//! on endpoints and notifications (`endpoint`, `notification`). This module
//! decides what to check: everything reachable from the root task's thread
//! and the ready queues, following capabilities in reached slots, threads'
//! blocking and binding, and waiting threads.
//!
//! It trusts that the pointers it follows name objects of the right type.
//!
//! An audit visits every slot of every reachable CNode, so it costs about
//! as much as the CNodes are big: fine for `test-kernel`, slow for a full
//! boot. `SAP_AUDIT_INTERVAL=N` at build time audits every Nth return to
//! user mode instead, which is faster but reports a broken invariant up to
//! N returns after the operation that broke it.
//!
//! TODO: objects nothing reachable names (a CNode whose only capability is
//! inside itself) aren't checked; a ledger of every object retype creates
//! would reach them, and would let the audit check pointers before following
//! them (the next step of the audit).

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::cap::Cap;
use crate::endpoint::Endpoint;
use crate::global::Global;
use crate::notification::Notification;
use crate::thread::Tcb;

/// Most objects one audit reaches; reaching more panics.
const MAX_REACHED: usize = 8192;

/// An object the audit checks.
#[derive(Clone, Copy)]
pub enum Object {
    CNode { ptr: usize, radix: u8 },
    Thread(*mut Tcb),
    Endpoint(*mut Endpoint),
    Notification(*mut Notification),
}

impl Object {
    /// The object `cap` names, if the audit checks objects of its type.
    pub fn named_by(cap: Cap) -> Option<Object> {
        match cap {
            Cap::CNode { ptr, radix, .. } => Some(Object::CNode { ptr, radix }),
            Cap::Thread { ptr } => Some(Object::Thread(ptr as *mut Tcb)),
            Cap::Endpoint { ptr, .. } => Some(Object::Endpoint(ptr as *mut Endpoint)),
            Cap::Notification { ptr, .. } => Some(Object::Notification(ptr as *mut Notification)),
            _ => None,
        }
    }

    fn address(&self) -> usize {
        match *self {
            Object::CNode { ptr, .. } => ptr,
            Object::Thread(tcb) => tcb as usize,
            Object::Endpoint(ep) => ep as usize,
            Object::Notification(ntfn) => ntfn as usize,
        }
    }
}

/// The objects one audit has reached: in the order reached, which is the
/// order they are checked in, and indexed by address, to reach each once.
struct Reached {
    objects: [Object; MAX_REACHED],
    by_address: [u16; MAX_REACHED],
    len: usize,
}

const _: () = assert!(MAX_REACHED <= u16::MAX as usize + 1);

impl Reached {
    /// Add `object`, unless it was reached already.
    fn add(&mut self, object: Object) {
        let address = object.address();
        let objects = &self.objects;
        let found = self.by_address[..self.len]
            .binary_search_by_key(&address, |&i| objects[i as usize].address());
        let Err(position) = found else {
            return;
        };

        assert!(
            self.len < MAX_REACHED,
            "audit: more than {MAX_REACHED} objects reached"
        );
        self.objects[self.len] = object;
        self.by_address
            .copy_within(position..self.len, position + 1);
        self.by_address[position] = self.len as u16;
        self.len += 1;
    }
}

static REACHED: Global<Reached> = Global::new(Reached {
    objects: [Object::Thread(core::ptr::null_mut()); MAX_REACHED],
    by_address: [0; MAX_REACHED],
    len: 0,
});

/// Audit every this many returns to user mode (`SAP_AUDIT_INTERVAL`).
const INTERVAL: usize = parse_interval(env!("SAP_AUDIT_INTERVAL"));

const fn parse_interval(text: &str) -> usize {
    let digits = text.as_bytes();
    let mut value = 0;
    let mut i = 0;
    while i < digits.len() {
        assert!(
            digits[i].is_ascii_digit(),
            "SAP_AUDIT_INTERVAL must be a number"
        );
        value = value * 10 + (digits[i] - b'0') as usize;
        i += 1;
    }
    assert!(value > 0, "SAP_AUDIT_INTERVAL must be at least 1");
    value
}

/// Returns to user mode since the last audit.
static SINCE_LAST: AtomicUsize = AtomicUsize::new(0);

/// Check everything reachable, every `INTERVAL` calls. Called before every
/// return to user mode.
pub fn run() {
    let since_last = SINCE_LAST.load(Ordering::Relaxed) + 1;
    if since_last < INTERVAL {
        SINCE_LAST.store(since_last, Ordering::Relaxed);
        return;
    }
    SINCE_LAST.store(0, Ordering::Relaxed);

    // SAFETY: the audit runs between kernel operations, and nothing else
    // uses REACHED.
    let reached = unsafe { REACHED.get() };
    reached.len = 0;

    reached.add(Object::Thread(crate::fault::root()));
    crate::sched::audit::queues(|tcb| reached.add(Object::Thread(tcb)));

    let mut next = 0;
    while next < reached.len {
        let object = reached.objects[next];
        next += 1;
        // SAFETY: reached objects are live (see the module comment).
        unsafe { check(object, reached) };
    }
}

/// Check one object, adding the objects it leads to.
///
/// # Safety
/// `object` must be live.
unsafe fn check(object: Object, reached: &mut Reached) {
    let mut found = |cap: Cap| {
        if let Some(object) = Object::named_by(cap) {
            reached.add(object);
        }
    };

    // SAFETY: per the caller's guarantee.
    unsafe {
        match object {
            Object::CNode { ptr, radix } => crate::cspace::audit::cnode(ptr, radix, found),

            Object::Thread(tcb) => {
                for i in 0..crate::thread::TCB_SLOTS {
                    found(crate::cspace::audit::slot(&raw mut (*tcb).slots[i]));
                }
                crate::thread::audit::thread(tcb, |object| reached.add(object));
            }

            Object::Endpoint(ep) => {
                crate::endpoint::audit::queue(ep, |tcb| reached.add(Object::Thread(tcb)));
            }

            Object::Notification(ntfn) => {
                let found = |tcb| reached.add(Object::Thread(tcb));
                crate::notification::audit::notification(ntfn, found);
            }
        }
    }
}
