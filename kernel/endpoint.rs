//! Endpoints and reply objects: synchronous IPC, as in seL4's MCS
//! configuration.
//!
//! An endpoint is a rendezvous point with one queue, threaded through the
//! waiting threads' TCBs: it holds blocked senders or blocked receivers,
//! never both, since a sender that finds a receiver (or the reverse) hands
//! the message over at once.
//!
//! A reply object is how a CALL gets answered. A receiver lends one to RECV;
//! when the message it receives is a CALL, the caller is linked to the reply
//! object and blocks until someone replies through it. Because the reply
//! right lives in an object rather than in the server's thread, one thread
//! can hold several pending replies, and (later) a caller's scheduling
//! context can follow the reply object back.
//!
//! The message itself moves in `transfer`: message registers in registers,
//! the rest from the sender's IPC buffer to the receiver's.

use core::ptr;

use sap_abi::{error, recv_source, MessageInfo, MAX_MESSAGE_LENGTH, MESSAGE_REGISTERS};

use crate::sched;
use crate::thread::{self, Tcb, ThreadState};

/// log2 of an endpoint's size in memory.
pub const ENDPOINT_BITS: usize = 4;
/// log2 of a reply object's size in memory.
pub const REPLY_BITS: usize = 4;

#[repr(C)]
pub struct Endpoint {
    /// Blocked senders or blocked receivers; the head's state says which.
    head: *mut Tcb,
    tail: *mut Tcb,
}

const _: () = assert!(core::mem::size_of::<Endpoint>() <= 1 << ENDPOINT_BITS);

#[repr(C)]
pub struct Reply {
    /// The caller waiting for an answer through this object.
    caller: *mut Tcb,
    /// The receiver this object is lent to while it waits in RECV.
    receiver: *mut Tcb,
}

const _: () = assert!(core::mem::size_of::<Reply>() <= 1 << REPLY_BITS);

// Result registers of IPC syscalls: a0 badge, a1 message info, a2..a5
// message registers, a6 what woke the receiver, a7 error code.
const BADGE: usize = 0;
const INFO: usize = 1;
const SOURCE: usize = 6;
const ERROR: usize = 7;

/// Set the error result of an IPC syscall on `tcb`.
///
/// # Safety
/// `tcb` must point at a live TCB.
pub unsafe fn set_error(tcb: *mut Tcb, code: usize) {
    // SAFETY: per the caller's guarantee.
    unsafe { (*tcb).context.set_result(ERROR, code) };
}

/// Move `from`'s message to `to`, with `badge`, and complete `to`'s
/// receive: the message registers go register to register, the rest from
/// `from`'s IPC buffer to `to`'s. A message longer than the registers is cut
/// to them if either thread lacks an IPC buffer.
unsafe fn transfer(from: *mut Tcb, to: *mut Tcb, badge: usize) {
    // SAFETY: both are live TCBs (callers' guarantee); their IPC buffers,
    // if valid, are kernel-mapped frames they hold; `ptr::copy` allows the
    // two buffers to overlap if userspace made them the same memory.
    unsafe {
        let info = MessageInfo((*from).context.arg(INFO));
        let mut length = info.length().min(MAX_MESSAGE_LENGTH);
        for i in 0..length.min(MESSAGE_REGISTERS) {
            let word = (*from).context.arg(2 + i);
            (*to).context.set_result(2 + i, word);
        }
        if length > MESSAGE_REGISTERS {
            match (thread::ipc_buffer_ptr(from), thread::ipc_buffer_ptr(to)) {
                (Some(src), Some(dst)) => {
                    let n = length - MESSAGE_REGISTERS;
                    let src = (&raw const (*src).msg).cast::<usize>().add(MESSAGE_REGISTERS);
                    let dst = (&raw mut (*dst).msg).cast::<usize>().add(MESSAGE_REGISTERS);
                    ptr::copy(src, dst, n);
                }
                _ => length = MESSAGE_REGISTERS,
            }
        }
        let c = &mut (*to).context;
        c.set_result(BADGE, badge);
        c.set_result(INFO, MessageInfo::new(info.label(), 0, length).0);
        c.set_result(SOURCE, recv_source::MESSAGE);
        c.set_result(ERROR, error::OK);
    }
}

/// After `sender`'s message was delivered: a CALL links the sender to
/// `reply` and blocks it there; without a reply object it can't be
/// answered, so it continues with NO_REPLY. A plain send completes.
unsafe fn after_delivery(sender: *mut Tcb, call: bool, reply: *mut Reply, sender_was_running: bool) {
    // SAFETY: sender is live; reply is null or live (callers' guarantee).
    unsafe {
        if call && !reply.is_null() {
            (*reply).caller = sender;
            if sender_was_running {
                sched::block(sender, ThreadState::BlockedOnReply, reply as usize);
            } else {
                (*sender).state = ThreadState::BlockedOnReply;
                (*sender).blocked_on = reply as usize;
            }
            return;
        }
        set_error(sender, if call { error::NO_REPLY } else { error::OK });
        if !sender_was_running {
            sched::make_runnable(sender);
        }
    }
}

/// SEND, NB_SEND or the sending half of CALL, by the running `sender`.
///
/// # Safety
/// `ep` must be a live endpoint and `sender` the live, running thread.
pub unsafe fn send(ep: *mut Endpoint, sender: *mut Tcb, badge: usize, call: bool, blocking: bool) {
    // SAFETY: per the caller's guarantee; queued TCBs and lent reply
    // objects are live.
    unsafe {
        let e = &mut *ep;
        let receiver = e.head;
        if !receiver.is_null() && (*receiver).state == ThreadState::BlockedOnReceive {
            dequeue(e, receiver);
            let reply = core::mem::replace(&mut (*receiver).ipc_reply, ptr::null_mut());
            if !reply.is_null() {
                (*reply).receiver = ptr::null_mut();
            }
            transfer(sender, receiver, badge);
            sched::make_runnable(receiver);
            after_delivery(sender, call, reply, true);
        } else if blocking || call {
            (*sender).ipc_badge = badge;
            (*sender).ipc_call = call;
            sched::block(sender, ThreadState::BlockedOnSend, ep as usize);
            enqueue(e, sender);
        } else {
            // NB_SEND with nobody waiting: the message is dropped, as in seL4.
            set_error(sender, error::OK);
        }
    }
}

/// RECV or NB_RECV by the running `receiver`, lending `reply` (or null).
///
/// # Safety
/// `ep` must be a live endpoint, `receiver` the live, running thread, and
/// `reply` null or a live reply object.
pub unsafe fn recv(ep: *mut Endpoint, receiver: *mut Tcb, reply: *mut Reply, blocking: bool) {
    // SAFETY: per the caller's guarantee; queued TCBs are live.
    unsafe {
        // Pending signals on the bound notification come first.
        let bound = (*receiver).bound_notification;
        if !bound.is_null() {
            let bits = crate::notification::poll(bound);
            if bits != 0 {
                deliver_signal(receiver, bits);
                return;
            }
        }
        if !reply.is_null() && !(*reply).caller.is_null() {
            // Lending it again would strand the caller still waiting on it.
            set_error(receiver, error::ILLEGAL_OPERATION);
            return;
        }
        let e = &mut *ep;
        let sender = e.head;
        if !sender.is_null() && (*sender).state == ThreadState::BlockedOnSend {
            dequeue(e, sender);
            transfer(sender, receiver, (*sender).ipc_badge);
            after_delivery(sender, (*sender).ipc_call, reply, false);
        } else if blocking {
            (*receiver).ipc_reply = reply;
            if !reply.is_null() {
                (*reply).receiver = receiver;
            }
            sched::block(receiver, ThreadState::BlockedOnReceive, ep as usize);
            enqueue(e, receiver);
        } else {
            let c = &mut (*receiver).context;
            c.set_result(BADGE, 0);
            c.set_result(INFO, 0);
            c.set_result(SOURCE, recv_source::NOTHING);
            c.set_result(ERROR, error::OK);
        }
    }
}

/// Complete a receive with a signal from the bound notification.
///
/// # Safety
/// `receiver` must be a live TCB, not queued anywhere.
pub unsafe fn deliver_signal(receiver: *mut Tcb, bits: usize) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let c = &mut (*receiver).context;
        c.set_result(BADGE, bits);
        c.set_result(INFO, 0);
        c.set_result(SOURCE, recv_source::NOTIFICATION);
        c.set_result(ERROR, error::OK);
    }
}

/// A signal arrived on `receiver`'s bound notification while it waits in
/// RECV: take it out of the endpoint's queue and wake it with the bits.
///
/// # Safety
/// `receiver` must be live and blocked on receive.
pub unsafe fn wake_bound(receiver: *mut Tcb, bits: usize) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        cancel(receiver);
        (*receiver).state = ThreadState::Inactive;
        deliver_signal(receiver, bits);
        sched::make_runnable(receiver);
    }
}

/// REPLY by `replier` through `reply`: hand the replier's message to the
/// waiting caller and wake it. Nothing happens if no caller waits.
///
/// # Safety
/// Both must be live.
pub unsafe fn reply(reply: *mut Reply, replier: *mut Tcb) {
    // SAFETY: per the caller's guarantee; a linked caller is live.
    unsafe {
        let caller = core::mem::replace(&mut (*reply).caller, ptr::null_mut());
        if caller.is_null() {
            return;
        }
        (*caller).blocked_on = 0;
        transfer(replier, caller, 0);
        sched::make_runnable(caller);
    }
}

/// Take `tcb` out of the queue of the endpoint it is blocked on (sending or
/// receiving), returning any reply object it had lent.
///
/// # Safety
/// `tcb` must be live and blocked on send or receive.
pub unsafe fn cancel(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let e = &mut *((*tcb).blocked_on as *mut Endpoint);
        dequeue(e, tcb);
        let reply = core::mem::replace(&mut (*tcb).ipc_reply, ptr::null_mut());
        if !reply.is_null() {
            (*reply).receiver = ptr::null_mut();
        }
    }
}

/// A caller blocked on reply stops waiting: unlink it from the reply
/// object, so a later REPLY finds nobody.
///
/// # Safety
/// `tcb` must be live and blocked on reply.
pub unsafe fn cancel_reply(tcb: *mut Tcb) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        let reply = (*tcb).blocked_on as *mut Reply;
        (*reply).caller = ptr::null_mut();
        (*tcb).blocked_on = 0;
    }
}

/// The last capability to `ep` is gone: every queued thread's system call
/// is restarted, so it runs again, finds the capability missing and fails.
///
/// # Safety
/// `ep` must be a live endpoint.
pub unsafe fn finalise_endpoint(ep: *mut Endpoint) {
    // SAFETY: per the caller's guarantee; queued TCBs are live.
    unsafe {
        while !(*ep).head.is_null() {
            let tcb = (*ep).head;
            cancel(tcb);
            (*tcb).state = ThreadState::Inactive;
            (*tcb).context.restart_syscall();
            sched::make_runnable(tcb);
        }
    }
}

/// The last capability to `reply` is gone: a caller waiting on it gets
/// NO_REPLY, and a receiver it is lent to loses it.
///
/// # Safety
/// `reply` must be a live reply object.
pub unsafe fn finalise_reply(reply: *mut Reply) {
    // SAFETY: per the caller's guarantee; linked TCBs are live.
    unsafe {
        let caller = core::mem::replace(&mut (*reply).caller, ptr::null_mut());
        if !caller.is_null() {
            (*caller).blocked_on = 0;
            (*caller).state = ThreadState::Inactive;
            set_error(caller, error::NO_REPLY);
            sched::make_runnable(caller);
        }
        let receiver = core::mem::replace(&mut (*reply).receiver, ptr::null_mut());
        if !receiver.is_null() {
            (*receiver).ipc_reply = ptr::null_mut();
        }
    }
}

fn enqueue(e: &mut Endpoint, tcb: *mut Tcb) {
    // SAFETY: tcb is live and in no queue; the tail is live.
    unsafe {
        (*tcb).ipc_prev = e.tail;
        (*tcb).ipc_next = ptr::null_mut();
        if e.tail.is_null() {
            e.head = tcb;
        } else {
            (*e.tail).ipc_next = tcb;
        }
        e.tail = tcb;
    }
}

fn dequeue(e: &mut Endpoint, tcb: *mut Tcb) {
    // SAFETY: tcb is in e's queue; its neighbours are live.
    unsafe {
        let t = &mut *tcb;
        match t.ipc_prev.is_null() {
            true => e.head = t.ipc_next,
            false => (*t.ipc_prev).ipc_next = t.ipc_next,
        }
        match t.ipc_next.is_null() {
            true => e.tail = t.ipc_prev,
            false => (*t.ipc_next).ipc_prev = t.ipc_prev,
        }
        t.ipc_next = ptr::null_mut();
        t.ipc_prev = ptr::null_mut();
        t.blocked_on = 0;
    }
}
