//! Threads: the thread control block (TCB), the kernel object a thread
//! capability names.
//!
//! A TCB holds a thread's saved user registers, its scheduling state and the
//! address space it runs in. It is placed in memory like any other kernel
//! object (from untyped memory, or from boot memory for the root task), in a
//! block of `1 << TCB_BITS` bytes.

use crate::arch::UserContext;

/// log2 of a TCB's size in memory, as in seL4 on RISC-V.
pub const TCB_BITS: usize = 10;

pub const MAX_PRIORITY: u8 = 255;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadState {
    /// Not runnable: never started, or suspended.
    Inactive,
    /// Runnable, and in the scheduler's ready queue (whether or not it is
    /// the thread currently on the CPU).
    Running,
}

#[repr(C)]
pub struct Tcb {
    /// Saved user registers. First, so the trap entry code can find them at
    /// the TCB's address.
    pub context: UserContext,
    pub state: ThreadState,
    pub priority: u8,
    /// Physical address of the root page table of the thread's address
    /// space.
    ///
    /// TODO: replace with a capability to the address space kept in the
    /// TCB, so deleting the address space stops the thread (M3, capability
    /// spaces).
    pub vspace_root: usize,
    /// Links in the scheduler's ready queue.
    pub(crate) queue_next: *mut Tcb,
    pub(crate) queue_prev: *mut Tcb,
}

const _: () = assert!(core::mem::size_of::<Tcb>() <= 1 << TCB_BITS);

impl Tcb {
    /// Write an inactive TCB at kernel address `at`, which must be
    /// `1 << TCB_BITS` bytes of memory the caller owns.
    ///
    /// # Safety
    /// `at` must be valid for writes of a `Tcb` and not otherwise in use.
    pub unsafe fn init(at: usize, vspace_root: usize) -> *mut Tcb {
        let tcb = at as *mut Tcb;
        // SAFETY: per the caller's guarantee.
        unsafe {
            tcb.write(Tcb {
                context: UserContext::default(),
                state: ThreadState::Inactive,
                priority: 0,
                vspace_root,
                queue_next: core::ptr::null_mut(),
                queue_prev: core::ptr::null_mut(),
            });
        }
        tcb
    }
}
