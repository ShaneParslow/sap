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

use sap_abi::syscall::{CALL, DEBUG_HALT, DEBUG_PUTCHAR, YIELD};

use crate::arch::UserContext;

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
        number => {
            crate::warn!("syscall: unknown or unimplemented call {number:#x}");
            context.set_result(0, ERROR_UNKNOWN);
        }
    }
}
