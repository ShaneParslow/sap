//! Notifications, as in seL4: a notification is idle, has threads waiting
//! on it, or is active with signals nobody has taken yet. `Signal` hands the
//! signaller's badge to the first waiter, or ORs it into the pending bits;
//! `Wait` takes the pending bits, blocking while there are none; `Poll`
//! takes them without blocking.
//!
//! "Active" is a state of its own rather than "the bits are nonzero", so a
//! signal through an unbadged capability (badge 0) is still pending.
//!
//! The waiter queue is threaded through the waiting threads' TCBs, so a
//! notification is a fixed 32 bytes however many threads wait on it.

use core::ptr;

use crate::sched;
use crate::thread::{Tcb, ThreadState};

/// log2 of a notification's size in memory.
pub const NOTIFICATION_BITS: usize = 5;

#[repr(C)]
pub struct Notification {
    state: State,
    /// The thread bound to this notification: a signal wakes it from RECV
    /// on any endpoint.
    pub bound: *mut Tcb,
}

/// Waiters and pending signals never coexist: a signal that finds a thread
/// waiting goes straight to it. `Idle` is zero, so a zeroed notification is
/// idle.
#[repr(usize)]
#[derive(Clone, Copy)]
enum State {
    Idle = 0,
    /// Threads blocked in `Wait`, first to last.
    Waiting { head: *mut Tcb, tail: *mut Tcb },
    /// Signals nobody has taken: the OR of their badges, possibly 0.
    Active { bits: usize },
}

const _: () = assert!(core::mem::size_of::<Notification>() <= 1 << NOTIFICATION_BITS);

/// Signal `ntfn` with `badge`: wake the first waiter with it, or add it to
/// the pending bits.
///
/// # Safety
/// `ntfn` must be a live notification.
pub unsafe fn signal(ntfn: *mut Notification, badge: usize) {
    // SAFETY: per the caller's guarantee; queued and bound TCBs are live.
    unsafe {
        let n = &mut *ntfn;
        let pending = match n.state {
            State::Idle => badge,
            State::Active { bits } => bits | badge,
            State::Waiting { head, .. } => {
                dequeue(n, head);
                (*head).context.set_result(0, badge);
                (*head).context.set_result(1, 0);
                sched::make_runnable(head);
                return;
            }
        };
        // A bound thread waiting in RECV on an endpoint takes the signal.
        if !n.bound.is_null() && (*n.bound).state == ThreadState::BlockedOnReceive {
            n.state = State::Idle;
            crate::endpoint::wake_bound(n.bound, pending);
        } else {
            n.state = State::Active { bits: pending };
        }
    }
}

/// Take the pending bits for `tcb`'s `Wait`. Returns them if a signal is
/// pending; otherwise blocks `tcb` on the notification and returns None,
/// and the bits arrive in its registers when it is woken.
///
/// # Safety
/// Both must be live.
pub unsafe fn wait(ntfn: *mut Notification, tcb: *mut Tcb) -> Option<usize> {
    // SAFETY: per the caller's guarantee; queued TCBs are live.
    unsafe {
        let n = &mut *ntfn;
        if let Some(bits) = poll(ntfn) {
            return Some(bits);
        }
        sched::block(tcb, ThreadState::BlockedOnNotification, ntfn as usize);
        (*tcb).ipc_next = ptr::null_mut();
        n.state = match n.state {
            State::Waiting { head, tail } => {
                (*tcb).ipc_prev = tail;
                (*tail).ipc_next = tcb;
                State::Waiting { head, tail: tcb }
            }
            _ => {
                (*tcb).ipc_prev = ptr::null_mut();
                State::Waiting { head: tcb, tail: tcb }
            }
        };
        None
    }
}

/// Take the pending bits without blocking: None if no signal is pending.
///
/// # Safety
/// `ntfn` must be a live notification.
pub unsafe fn poll(ntfn: *mut Notification) -> Option<usize> {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let n = &mut *ntfn;
        let State::Active { bits } = n.state else { return None };
        n.state = State::Idle;
        Some(bits)
    }
}

/// Take `tcb` off the waiter queue of the notification it is blocked on.
///
/// # Safety
/// `tcb` must be live and blocked on a live notification.
pub unsafe fn cancel(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let n = &mut *((*tcb).blocked_on as *mut Notification);
        dequeue(n, tcb);
    }
}

/// The last capability to `ntfn` is gone: every waiter's `Wait` is
/// restarted, so it runs again, finds the capability missing and fails
/// cleanly; a bound thread is unbound.
///
/// # Safety
/// `ntfn` must be a live notification.
pub unsafe fn finalise(ntfn: *mut Notification) {
    // SAFETY: per the caller's guarantee; queued and bound TCBs are live.
    unsafe {
        let n = &mut *ntfn;
        while let State::Waiting { head, .. } = n.state {
            dequeue(n, head);
            (*head).context.restart_syscall();
            sched::make_runnable(head);
        }
        n.state = State::Idle;
        if !n.bound.is_null() {
            (*n.bound).bound_notification = ptr::null_mut();
            n.bound = ptr::null_mut();
        }
    }
}

unsafe fn dequeue(n: &mut Notification, tcb: *mut Tcb) {
    let State::Waiting { mut head, mut tail } = n.state else {
        unreachable!("dequeue from a notification nobody waits on");
    };
    // SAFETY: tcb is in n's queue (caller's guarantee); neighbours are live.
    unsafe {
        let t = &mut *tcb;
        match t.ipc_prev.is_null() {
            true => head = t.ipc_next,
            false => (*t.ipc_prev).ipc_next = t.ipc_next,
        }
        match t.ipc_next.is_null() {
            true => tail = t.ipc_prev,
            false => (*t.ipc_next).ipc_prev = t.ipc_prev,
        }
        t.ipc_next = ptr::null_mut();
        t.ipc_prev = ptr::null_mut();
        t.blocked_on = 0;
    }
    n.state = if head.is_null() { State::Idle } else { State::Waiting { head, tail } };
}
