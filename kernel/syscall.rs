//! System calls from user mode. The architecture's trap handler passes the
//! call number and up to six arguments, and returns the result to the caller.
//!
//! TODO: only two temporary debug calls exist, like seL4's debug syscalls.
//! Capability invocations (Call, Send, Recv, ...) replace them in M3, and
//! the debug calls become debug-build only.

/// Print one byte on the kernel console.
pub const DEBUG_PUTCHAR: usize = 0x100;
/// Stop the machine; a nonzero argument reports failure.
pub const DEBUG_HALT: usize = 0x101;

/// Returned for an unknown call number.
const ERROR_UNKNOWN: usize = usize::MAX;

pub fn handle(number: usize, args: [usize; 6]) -> usize {
    match number {
        DEBUG_PUTCHAR => {
            crate::arch::console_write(&[args[0] as u8]);
            0
        }
        DEBUG_HALT => {
            let failure = args[0] != 0;
            crate::info!("root task halted ({})", if failure { "failure" } else { "success" });
            crate::arch::shutdown(failure)
        }
        _ => {
            crate::warn!("syscall: unknown call {number:#x}");
            ERROR_UNKNOWN
        }
    }
}
