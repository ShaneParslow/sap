//! Notifications: a word of pending signal bits and a queue of waiting
//! threads, as in seL4. `Signal` ORs the signaller's badge into the word, or
//! hands it straight to a waiter; `Wait` takes and clears the word, blocking
//! while it is zero; `Poll` takes it without blocking.
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
    /// Pending badge bits, nonzero only while no thread waits.
    word: usize,
    head: *mut Tcb,
    tail: *mut Tcb,
    /// A thread bound to this notification (later: wakes from `Recv`).
    pub bound: *mut Tcb,
}

const _: () = assert!(core::mem::size_of::<Notification>() <= 1 << NOTIFICATION_BITS);

/// Signal `ntfn` with `badge`: wake the first waiter with it, or OR it into
/// the word.
///
/// # Safety
/// `ntfn` must be a live notification.
pub unsafe fn signal(ntfn: *mut Notification, badge: usize) {
    // SAFETY: per the caller's guarantee; queued TCBs are live.
    unsafe {
        let n = &mut *ntfn;
        if n.head.is_null() {
            n.word |= badge;
            return;
        }
        let waiter = n.head;
        dequeue(n, waiter);
        (*waiter).context.set_result(0, badge);
        (*waiter).context.set_result(1, 0);
        sched::make_runnable(waiter);
    }
}

/// Take the pending bits for `tcb`'s `Wait`. Returns them if nonzero;
/// otherwise blocks `tcb` on the notification and returns None, and the
/// bits arrive in its registers when it is woken.
///
/// # Safety
/// Both must be live.
pub unsafe fn wait(ntfn: *mut Notification, tcb: *mut Tcb) -> Option<usize> {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let n = &mut *ntfn;
        if n.word != 0 {
            return Some(core::mem::take(&mut n.word));
        }
        sched::block(tcb, ThreadState::BlockedOnNotification, ntfn as usize);
        (*tcb).ipc_prev = n.tail;
        (*tcb).ipc_next = ptr::null_mut();
        if n.tail.is_null() {
            n.head = tcb;
        } else {
            (*n.tail).ipc_next = tcb;
        }
        n.tail = tcb;
        None
    }
}

/// Take and clear the pending bits without blocking.
///
/// # Safety
/// `ntfn` must be a live notification.
pub unsafe fn poll(ntfn: *mut Notification) -> usize {
    // SAFETY: per the caller's guarantee.
    unsafe { core::mem::take(&mut (*ntfn).word) }
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
        while !n.head.is_null() {
            let waiter = n.head;
            dequeue(n, waiter);
            (*waiter).context.restart_syscall();
            sched::make_runnable(waiter);
        }
        if !n.bound.is_null() {
            (*n.bound).bound_notification = ptr::null_mut();
            n.bound = ptr::null_mut();
        }
    }
}

unsafe fn dequeue(n: &mut Notification, tcb: *mut Tcb) {
    // SAFETY: tcb is in n's queue (caller's guarantee); neighbours are live.
    unsafe {
        let t = &mut *tcb;
        match t.ipc_prev.is_null() {
            true => n.head = t.ipc_next,
            false => (*t.ipc_prev).ipc_next = t.ipc_next,
        }
        match t.ipc_next.is_null() {
            true => n.tail = t.ipc_prev,
            false => (*t.ipc_next).ipc_prev = t.ipc_prev,
        }
        t.ipc_next = ptr::null_mut();
        t.ipc_prev = ptr::null_mut();
        t.blocked_on = 0;
    }
}
