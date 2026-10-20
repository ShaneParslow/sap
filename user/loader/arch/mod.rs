//! Architecture layer: what the loader accepts and how it relocates.

#[cfg(target_arch = "riscv64")]
mod riscv;
#[cfg(target_arch = "riscv64")]
pub use riscv::*;
