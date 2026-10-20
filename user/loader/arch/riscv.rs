//! RISC-V: the ELF machine and the one relocation type sap programs have.

/// The ELF machine of programs this loader runs.
pub const MACHINE: u16 = elf::abi::EM_RISCV;
/// A relative relocation: the load address plus the addend.
pub const RELATIVE: u32 = elf::abi::R_RISCV_RELATIVE;
