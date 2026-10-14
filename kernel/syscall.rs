//! System calls from user mode. The architecture's trap handler passes the
//! interrupted thread's registers; arguments and results are in the
//! argument registers, numbered by `UserContext::arg` and `set_result`.
//!
//! `CALL` invokes a capability (`invocation.rs`). The other IPC calls
//! (Send, Recv, ReplyRecv, Signal, Wait) arrive with endpoints and
//! notifications (M4).
//!
//! TODO: make the debug calls debug-build only once a userspace log service
//! exists (M7).

use sap_abi::syscall::{CALL, DEBUG_HALT, DEBUG_PUTCHAR, POLL, SIGNAL, WAIT, YIELD};
use sap_abi::{error, rights};

use crate::arch::UserContext;
use crate::cap::Cap;
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
        // SAFETY: the current thread is the live TCB holding `context`.
        CALL => unsafe { crate::invocation::call(crate::sched::current(), context) },
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
