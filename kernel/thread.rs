//! Threads: the thread control block (TCB), the kernel object a thread
//! capability names.
//!
//! A TCB holds a thread's saved user registers, its scheduling state, and,
//! as in seL4, slots for the capabilities that define where it runs: its
//! capability space root, its address space root and its IPC buffer frame.
//! Holding copies (rather than pointers) means deleting the address space or
//! capability space also takes the thread's reference away. A TCB occupies
//! a block of `1 << TCB_BITS` bytes, from untyped memory or, for the root
//! task, boot memory.

use sap_abi::IpcBuffer;

use crate::arch::{self, UserContext};
use crate::cap::Cap;
use crate::cspace::{DeleteError, Slot};

/// log2 of a TCB's size in memory, as in seL4 on RISC-V.
pub const TCB_BITS: usize = 10;

pub const MAX_PRIORITY: u8 = 255;

/// Indices of the TCB's own capability slots.
pub const CSPACE_SLOT: usize = 0;
pub const VSPACE_SLOT: usize = 1;
pub const IPC_BUFFER_SLOT: usize = 2;
const TCB_SLOTS: usize = 3;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadState {
    /// Not runnable: never started, or suspended. Zero, so a zeroed TCB is
    /// inactive.
    Inactive = 0,
    /// Runnable, and in the scheduler's ready queue (whether or not it is
    /// the thread currently on the CPU).
    Running,
}

#[repr(C)]
pub struct Tcb {
    /// Saved user registers. First, so the trap entry code can find them at
    /// the TCB's address.
    pub context: UserContext,
    pub slots: [Slot; TCB_SLOTS],
    pub state: ThreadState,
    pub priority: u8,
    /// User address of the IPC buffer, inside the frame in `IPC_BUFFER_SLOT`.
    pub ipc_buffer: usize,
    /// Links in the scheduler's ready queue.
    pub(crate) queue_next: *mut Tcb,
    pub(crate) queue_prev: *mut Tcb,
}

const _: () = assert!(core::mem::size_of::<Tcb>() <= 1 << TCB_BITS);

impl Tcb {
    /// Zero `1 << TCB_BITS` bytes at kernel address `at`, which makes an
    /// inactive TCB with empty slots.
    ///
    /// # Safety
    /// `at` must be valid for writes of that many bytes and not otherwise in
    /// use.
    pub unsafe fn init(at: usize) -> *mut Tcb {
        // SAFETY: per the caller's guarantee; all-zero is a valid Tcb
        // (Cap::Null, null links, Inactive, zeroed registers).
        unsafe { core::ptr::write_bytes(at as *mut u8, 0, 1 << TCB_BITS) };
        at as *mut Tcb
    }

}

// The functions below take a raw pointer and read single fields, because
// the syscall path holds `&mut` to the thread's registers (inside the TCB)
// while it calls them; a `&Tcb` would alias it.

/// The root of the thread's capability space.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn cspace(tcb: *const Tcb) -> Cap {
    // SAFETY: per the caller's guarantee; reads one field.
    unsafe { (*tcb).slots[CSPACE_SLOT].cap }
}

/// The thread's IPC buffer, if it has a valid one.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn ipc_buffer(tcb: *const Tcb) -> Option<&'static mut IpcBuffer> {
    // SAFETY: per the caller's guarantee; reads two fields.
    let (frame, va) = unsafe { ((*tcb).slots[IPC_BUFFER_SLOT].cap, (*tcb).ipc_buffer) };
    let at = arch::frame_kernel_address(frame, va)?;
    // SAFETY: the frame capability in the TCB keeps the frame from being
    // reused while the thread exists, and `frame_kernel_address` checked
    // that a whole IpcBuffer fits inside it.
    Some(unsafe { &mut *(at as *mut IpcBuffer) })
}

/// Finalise a TCB whose last capability was deleted: stop the thread and
/// delete the capabilities it holds.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn finalise(tcb: *mut Tcb, depth: usize) -> Result<(), DeleteError> {
    // SAFETY: per the caller's guarantee.
    unsafe {
        crate::sched::make_inactive(tcb);
        Slot::delete_all(&mut (*tcb).slots, depth)
    }
}
