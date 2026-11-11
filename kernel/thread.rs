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
/// The endpoint the thread's faults are delivered to.
pub const FAULT_SLOT: usize = 3;
pub const TCB_SLOTS: usize = 4;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadState {
    /// Not runnable: never started, or suspended. Zero, so a zeroed TCB is
    /// inactive.
    Inactive = 0,
    /// Runnable, and in the scheduler's ready queue (whether or not it is
    /// the thread currently on the CPU).
    Running,
    /// Waiting on the notification at `blocked_on`.
    BlockedOnNotification,
    /// Waiting to send on the endpoint at `blocked_on`; the message is in
    /// its registers and IPC buffer.
    BlockedOnSend,
    /// Waiting to receive on the endpoint at `blocked_on`.
    BlockedOnReceive,
    /// Waiting for an answer through the reply object at `blocked_on`.
    BlockedOnReply,
}

#[repr(C)]
pub struct Tcb {
    /// Saved user registers. First, so the trap entry code can find them at
    /// the TCB's address.
    pub context: UserContext,
    pub slots: [Slot; TCB_SLOTS],
    pub state: ThreadState,
    pub priority: u8,
    /// Maximum controlled priority: the highest priority this thread may
    /// give itself or another thread when used as the authority, as in
    /// seL4. 0 for new threads; 255 for the root task.
    pub mcp: u8,
    /// User address of the IPC buffer, inside the frame in `IPC_BUFFER_SLOT`.
    pub ipc_buffer: usize,
    /// Links in the scheduler's ready queue.
    pub(crate) queue_next: *mut Tcb,
    pub(crate) queue_prev: *mut Tcb,
    /// The object this thread is blocked on (kernel address), when blocked.
    pub(crate) blocked_on: usize,
    /// Links in that object's queue of waiting threads.
    pub(crate) ipc_next: *mut Tcb,
    pub(crate) ipc_prev: *mut Tcb,
    /// The notification bound to this thread, if any.
    pub(crate) bound_notification: *mut crate::notification::Notification,
    /// While blocked on send: the badge the message carries and whether it
    /// is a CALL.
    pub(crate) ipc_badge: usize,
    pub(crate) ipc_call: bool,
    /// While blocked on send: whether the capability used may send
    /// capabilities (GRANT) and whether a reply may (GRANT_REPLY).
    pub(crate) ipc_grant: bool,
    pub(crate) ipc_grant_reply: bool,
    /// While blocked on receive: the reply object lent for a CALL.
    pub(crate) ipc_reply: *mut crate::endpoint::Reply,
    /// The fault being delivered, while the thread waits for its handler
    /// (kind 0 when none, so a zeroed TCB has none).
    pub(crate) fault: crate::fault::Fault,
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
    // SAFETY: per the caller's guarantee; the pointer is valid (see
    // `ipc_buffer_ptr`).
    unsafe { ipc_buffer_ptr(tcb).map(|b| &mut *b) }
}

/// The thread's IPC buffer as a raw pointer, for copying between two
/// threads' buffers (which userspace may have made the same memory).
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn ipc_buffer_ptr(tcb: *const Tcb) -> Option<*mut IpcBuffer> {
    // SAFETY: per the caller's guarantee; reads two fields.
    let (frame, va) = unsafe { ((*tcb).slots[IPC_BUFFER_SLOT].cap, (*tcb).ipc_buffer) };
    // The frame capability in the TCB keeps the frame from being reused
    // while the thread exists, and `frame_kernel_address` checks that a
    // whole IpcBuffer fits inside it.
    arch::frame_kernel_address(frame, va).map(|at| at as *mut IpcBuffer)
}

/// Take a blocked thread off whatever it is blocked on. Returns whether it
/// was blocked.
///
/// # Safety
/// `tcb` must point at a live TCB.
unsafe fn cancel_blocking(tcb: *mut Tcb) -> bool {
    // SAFETY: per the caller's guarantee; a blocked thread's object is live.
    unsafe {
        match (*tcb).state {
            ThreadState::BlockedOnNotification => crate::notification::cancel(tcb),
            ThreadState::BlockedOnSend | ThreadState::BlockedOnReceive => crate::endpoint::cancel(tcb),
            ThreadState::BlockedOnReply => crate::endpoint::cancel_reply(tcb),
            ThreadState::Inactive | ThreadState::Running => return false,
        }
        (*tcb).state = ThreadState::Inactive;
    }
    true
}

/// Stop a thread. One blocked in a system call has the call restarted, so
/// resuming it repeats the call, as in seL4.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn suspend(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        if cancel_blocking(tcb) {
            // A thread delivering a fault isn't in a system call: drop the
            // fault, and resuming it re-runs the faulting instruction.
            if !(*tcb).fault.take() {
                (*tcb).context.restart_syscall();
            }
        }
        crate::sched::make_inactive(tcb);
    }
}

/// Make an inactive thread runnable. A thread that is runnable or blocked
/// is left as it is, as in seL4.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn resume(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        if (*tcb).state == ThreadState::Inactive {
            crate::sched::make_runnable(tcb);
        }
    }
}

#[cfg(feature = "audit")]
pub(crate) mod audit {
    //! The audit's checks of threads (`crate::audit`).

    use sap_abi::object;

    use super::{CSPACE_SLOT, FAULT_SLOT, IPC_BUFFER_SLOT, Tcb, ThreadState, VSPACE_SLOT};
    use crate::arch::ArchCap;
    use crate::audit::ledger::{Entry, Identity, Ledger};
    use crate::cap::Cap;

    /// Check `tcb`, whose ledger entry is `entry`, after the ready queues
    /// and wait queues have counted it: in a ready queue exactly when
    /// runnable, in a wait queue exactly when waiting there, blocked on a
    /// live object of the right type, linked back by the reply object and
    /// notification it uses, and holding the right kinds of capability in
    /// its slots.
    ///
    /// # Safety
    /// `tcb` must be a live thread.
    pub unsafe fn thread(tcb: *mut Tcb, entry: Entry, ledger: &Ledger) {
        // SAFETY: per the caller's guarantee.
        let t = unsafe { &*tcb };
        let state = t.state;
        let at = "audit: thread";

        let runnable = state == ThreadState::Running;
        let queued = entry.queued;
        assert!(
            queued == runnable as u32,
            "{at} {tcb:p} ({state:?}) is in {queued} ready queue(s)"
        );

        let waiting = matches!(
            state,
            ThreadState::BlockedOnSend
                | ThreadState::BlockedOnReceive
                | ThreadState::BlockedOnNotification
        );
        let waits = entry.waits;
        assert!(
            waits == waiting as u32,
            "{at} {tcb:p} ({state:?}) is in {waits} wait queue(s)"
        );

        let linked = !t.ipc_prev.is_null() || !t.ipc_next.is_null();
        let blocked_on = t.blocked_on;
        let blocked_on_live = |object_type| {
            let identity = Identity::at(blocked_on, object_type);
            blocked_on != 0 && ledger.live(identity).is_some()
        };

        match state {
            ThreadState::Inactive | ThreadState::Running => {
                assert!(
                    blocked_on == 0,
                    "{at} {tcb:p} ({state:?}) is blocked on something"
                );
                assert!(
                    !linked,
                    "{at} {tcb:p} ({state:?}) is linked into a wait queue"
                );
            }

            ThreadState::BlockedOnSend | ThreadState::BlockedOnReceive => {
                let live = blocked_on_live(object::ENDPOINT);
                assert!(
                    live,
                    "{at} {tcb:p} waits on {blocked_on:#x}, not a live endpoint"
                );
            }

            ThreadState::BlockedOnNotification => {
                let live = blocked_on_live(object::NOTIFICATION);
                assert!(
                    live,
                    "{at} {tcb:p} waits on {blocked_on:#x}, not a live notification"
                );
            }

            ThreadState::BlockedOnReply => {
                let live = blocked_on_live(object::REPLY);
                assert!(
                    live,
                    "{at} {tcb:p} waits on {blocked_on:#x}, not a live reply object"
                );
                assert!(
                    !linked,
                    "{at} {tcb:p} waits for a reply but is linked into a wait queue"
                );
                // SAFETY: a live reply object.
                let caller = unsafe { crate::endpoint::audit::caller(blocked_on as *mut _) };
                assert!(
                    caller == tcb,
                    "{at} {tcb:p}'s reply object doesn't name it as caller"
                );
            }
        }

        let reply = t.ipc_reply;
        if !reply.is_null() {
            let receiving = state == ThreadState::BlockedOnReceive;
            assert!(
                receiving,
                "{at} {tcb:p} holds a lent reply object but isn't receiving"
            );
            let live = ledger
                .live(Identity::at(reply as usize, object::REPLY))
                .is_some();
            assert!(
                live,
                "{at} {tcb:p} holds {reply:p}, which isn't a live reply object"
            );
            // SAFETY: a live reply object.
            let receiver = unsafe { crate::endpoint::audit::receiver(reply) };
            assert!(
                receiver == tcb,
                "{at} {tcb:p}'s lent reply object is lent elsewhere"
            );
        }

        let bound = t.bound_notification;
        if !bound.is_null() {
            let live = ledger
                .live(Identity::at(bound as usize, object::NOTIFICATION))
                .is_some();
            assert!(
                live,
                "{at} {tcb:p} is bound to {bound:p}, which isn't a live notification"
            );
            // SAFETY: a live notification.
            let back = unsafe { (*bound).bound };
            assert!(back == tcb, "{at} {tcb:p}'s notification isn't bound to it");
        }

        let kinds = [
            (
                CSPACE_SLOT,
                matches!(t.slots[CSPACE_SLOT].cap, Cap::Null | Cap::CNode { .. }),
            ),
            (
                VSPACE_SLOT,
                matches!(
                    t.slots[VSPACE_SLOT].cap,
                    Cap::Null | Cap::Arch(ArchCap::PageTable { .. })
                ),
            ),
            (
                IPC_BUFFER_SLOT,
                matches!(
                    t.slots[IPC_BUFFER_SLOT].cap,
                    Cap::Null | Cap::Arch(ArchCap::Frame { .. })
                ),
            ),
            (
                FAULT_SLOT,
                matches!(t.slots[FAULT_SLOT].cap, Cap::Null | Cap::Endpoint { .. }),
            ),
        ];
        for (slot, right_kind) in kinds {
            assert!(
                right_kind,
                "{at} {tcb:p}'s slot {slot} holds the wrong kind of capability"
            );
        }
    }
}

/// Finalise a TCB whose last capability was deleted: stop the thread,
/// unbind its notification, and delete the capabilities it holds.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn finalise(tcb: *mut Tcb, depth: usize) -> Result<(), DeleteError> {
    // SAFETY: per the caller's guarantee.
    unsafe {
        cancel_blocking(tcb);
        crate::sched::make_inactive(tcb);
        let bound = (*tcb).bound_notification;
        if !bound.is_null() {
            (*bound).bound = core::ptr::null_mut();
            (*tcb).bound_notification = core::ptr::null_mut();
        }
        Slot::delete_all(&mut (*tcb).slots, depth)
    }
}
