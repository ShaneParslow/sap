//! The binary interface between the kernel and user programs: syscall
//! numbers, invocation labels, error codes and the layouts of structures
//! both sides read. The kernel and every user program depend on this crate,
//! so the two can't drift apart.
//!
//! TODO: generate this from a Cap'n Proto schema of the kernel's objects
//! (roadmap D5), so other languages get the same definitions.

#![no_std]

/// System call numbers, passed in `a7` on RISC-V.
pub mod syscall {
    /// Invoke a capability and wait for the reply.
    pub const CALL: usize = 0;
    /// Give up the rest of the time slice.
    pub const YIELD: usize = 6;

    /// TODO: debug-only calls, like seL4's; compile them out of release
    /// kernels once a userspace log service exists (M7).
    /// Print one byte on the kernel console.
    pub const DEBUG_PUTCHAR: usize = 0x100;
    /// Stop the machine; a nonzero argument reports failure.
    pub const DEBUG_HALT: usize = 0x101;
}
