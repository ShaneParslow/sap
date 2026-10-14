//! System calls from user mode. The architecture's trap handler passes the
//! interrupted thread's registers; arguments and results are in the
//! argument registers, numbered by `UserContext::arg` and `set_result`.
//!
//! `CALL` on a kernel object invokes it (`invocation.rs`); on an endpoint it
//! is IPC, like SEND, RECV, REPLY and REPLY_RECV (`endpoint.rs`). SIGNAL,
//! WAIT and POLL act on notifications (`notification.rs`).
//!
//! TODO: make the debug calls debug-build only once a userspace log service
//! exists (M7).

use sap_abi::syscall::{CALL, DEBUG_HALT, DEBUG_PUTCHAR, NB_RECV, NB_SEND, POLL, RECV, REPLY, REPLY_RECV, SEND, SIGNAL, WAIT, YIELD};
use sap_abi::{error, rights};

use crate::arch::UserContext;
use crate::cap::Cap;
use crate::endpoint::{self, Endpoint, Reply};
use crate::notification::{self, Notification};
use crate::{cspace, sched, thread};

/// Returned in the first result register for an unknown or unimplemented
/// call.
const ERROR_UNKNOWN: usize = usize::MAX;

pub fn handle(context: &mut UserContext) {
    match context.syscall_number() {
        DEBUG_PUTCHAR => crate::arch::console_write(&[context.arg(0) as u8]),
        DEBUG_HALT => {
            let failure = context.arg(0) != 0;
            crate::info!("halted by user ({})", if failure { "failure" } else { "success" });
            crate::arch::shutdown(failure)
        }
        YIELD => crate::sched::yield_current(),
        CALL if is_endpoint(context.arg(0)) => ipc(CALL, context),
        // SAFETY: the current thread is the live TCB holding `context`.
        CALL => unsafe { crate::invocation::call(crate::sched::current(), context) },
        number @ (SEND | NB_SEND | RECV | NB_RECV | REPLY | REPLY_RECV) => ipc(number, context),
        number @ (SIGNAL | WAIT | POLL) => notification_call(number, context),
        number => {
            crate::warn!("syscall: unknown or unimplemented call {number:#x}");
            context.set_result(0, ERROR_UNKNOWN);
        }
    }
}

/// SIGNAL, WAIT and POLL. Results: `a0` = badge bits, `a1` = error code.
fn notification_call(number: usize, context: &mut UserContext) {
    let tcb = sched::current();
    // SAFETY: the current thread is live; one field.
    let root = unsafe { thread::cspace(tcb) };
    let (ptr, cap_rights, badge) = match cspace::lookup_cap(root, context.arg(0)) {
        Ok((_, Cap::Notification { ptr, rights, badge })) => (ptr as *mut Notification, rights as usize, badge),
        Ok(_) => return fail(context, error::INVALID_CAPABILITY),
        Err(_) => return fail(context, error::FAILED_LOOKUP),
    };
    let needed = if number == SIGNAL { rights::WRITE } else { rights::READ };
    if cap_rights & needed == 0 {
        return fail(context, error::INVALID_CAPABILITY);
    }
    // SAFETY: a notification capability names a live notification; the
    // current thread is live.
    let bits = unsafe {
        match number {
            SIGNAL => {
                notification::signal(ptr, badge);
                Some(0)
            }
            WAIT => notification::wait(ptr, tcb),
            _ => Some(notification::poll(ptr)),
        }
    };
    // A blocked WAIT gets its results when it is woken.
    if let Some(bits) = bits {
        context.set_result(0, bits);
        context.set_result(1, error::OK);
    }
}

fn fail(context: &mut UserContext, code: usize) {
    context.set_result(0, 0);
    context.set_result(1, code);
}

/// Whether `cptr` names an endpoint in the current thread's space.
fn is_endpoint(cptr: usize) -> bool {
    // SAFETY: the current thread is live; one field.
    let root = unsafe { thread::cspace(sched::current()) };
    matches!(cspace::lookup_cap(root, cptr), Ok((_, Cap::Endpoint { .. })))
}

/// Endpoint IPC: CALL (on an endpoint), SEND, NB_SEND, RECV, NB_RECV, REPLY,
/// REPLY_RECV. `a0` = endpoint, `a6` = reply object (0 for none); results,
/// including the `a7` error code, are written by `endpoint.rs`.
fn ipc(number: usize, context: &mut UserContext) {
    let (ep_cptr, reply_cptr) = (context.arg(0), context.arg(6));
    let tcb = sched::current();
    // From here on the registers are reached through `tcb`, since the IPC
    // code also writes other threads' registers.
    let _ = context;
    // SAFETY: the current thread is live; one field.
    let root = unsafe { thread::cspace(tcb) };
    let fail = |code| {
        // SAFETY: tcb is live.
        unsafe {
            (*tcb).context.set_result(6, sap_abi::recv_source::NOTHING);
            endpoint::set_error(tcb, code);
        }
    };

    // The reply object, for the calls that take one.
    let mut reply: *mut Reply = core::ptr::null_mut();
    if matches!(number, RECV | NB_RECV | REPLY | REPLY_RECV) && reply_cptr != 0 {
        match cspace::lookup_cap(root, reply_cptr) {
            Ok((_, Cap::Reply { ptr })) => reply = ptr as *mut Reply,
            Ok(_) => return fail(error::INVALID_CAPABILITY),
            Err(_) => return fail(error::FAILED_LOOKUP),
        }
    }
    if matches!(number, REPLY | REPLY_RECV) && !reply.is_null() {
        // SAFETY: reply and tcb are live.
        unsafe { endpoint::reply(reply, tcb) };
    }
    if number == REPLY {
        // SAFETY: tcb is live.
        unsafe { endpoint::set_error(tcb, error::OK) };
        return;
    }

    let (ptr, cap_rights, badge) = match cspace::lookup_cap(root, ep_cptr) {
        Ok((_, Cap::Endpoint { ptr, rights, badge })) => (ptr as *mut Endpoint, rights as usize, badge),
        Ok(_) => return fail(error::INVALID_CAPABILITY),
        Err(_) => return fail(error::FAILED_LOOKUP),
    };
    let receiving = matches!(number, RECV | NB_RECV | REPLY_RECV);
    let needed = if receiving { rights::READ } else { rights::WRITE };
    if cap_rights & needed == 0 {
        return fail(error::INVALID_CAPABILITY);
    }
    // SAFETY: the endpoint, reply object and current thread are live, and
    // the current thread is running.
    unsafe {
        if receiving {
            endpoint::recv(ptr, tcb, reply, number != NB_RECV);
        } else {
            let grants = endpoint::Rights {
                grant: cap_rights & rights::GRANT != 0,
                grant_reply: cap_rights & rights::GRANT_REPLY != 0,
            };
            endpoint::send(ptr, tcb, badge, grants, number == CALL, number == SEND);
        }
    }
}
