//! Architecture layer: the system-call instruction and the thread pointer.
//! Everything above this module is architecture-neutral.

#[cfg(target_arch = "riscv64")]
mod riscv;
#[cfg(target_arch = "riscv64")]
pub use riscv::*;
