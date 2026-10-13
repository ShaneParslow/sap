//! System calls from user mode. The architecture's trap handler passes the
//! interrupted thread's registers; arguments and results are in the
//! argument registers, numbered by `UserContext::arg` and `set_result`.
//!
//! TODO: capability invocations (`CALL`) arrive with capability spaces (M3).
//! The debug calls become debug-build only once a userspace log service
//! exists (M7).

use sap_abi::syscall::{DEBUG_HALT, DEBUG_PUTCHAR, YIELD};

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
        number => {
            crate::warn!("syscall: unknown or unimplemented call {number:#x}");
            context.set_result(0, ERROR_UNKNOWN);
        }
    }
}
