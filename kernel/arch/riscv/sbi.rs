//! Calls into the SBI firmware (OpenSBI), through the `sbi-rt` crate.
//!
//! This module only adapts `sbi-rt` to what the kernel needs, so the rest of
//! the arch code doesn't depend on its types.

use sbi_rt::{Physical, Shutdown, NoReason, SystemFailure};

pub fn has_debug_console() -> bool {
    sbi_rt::probe_extension(sbi_rt::Console).is_available()
}

/// Write bytes with the debug console extension. Returns false on SBI error.
/// Firmware reads the bytes by physical address, so `bytes` must be
/// physically contiguous: true for anything in the kernel image or the
/// direct map, which is everything the kernel prints.
pub fn debug_console_write(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while !rest.is_empty() {
        let pa = super::paging::virt_to_phys(rest.as_ptr() as usize);
        let ret = sbi_rt::console_write(Physical::new(rest.len(), pa, 0));
        if ret.is_err() {
            return false;
        }
        rest = &rest[ret.value.min(rest.len())..];
    }
    true
}

/// Deprecated legacy console, for firmware without DBCN.
pub fn legacy_putchar(byte: u8) {
    #[allow(deprecated)]
    sbi_rt::legacy::console_putchar(byte as usize);
}

pub fn shutdown(failure: bool) {
    if failure {
        sbi_rt::system_reset(Shutdown, SystemFailure);
    } else {
        sbi_rt::system_reset(Shutdown, NoReason);
    }
}

/// Request a supervisor timer interrupt when `time` reaches `deadline`;
/// also clears a pending one. `deadline` is in the same units as the `time`
/// CSR (the SBI specification defines it as an absolute `time` value).
///
/// TODO: each call traps into firmware (hundreds of cycles). Hardware with
/// the Sstc extension lets the kernel write `stimecmp` directly; use it when
/// present (pick the path at boot, see hw-isa in the wiki).
pub fn set_timer(deadline: u64) {
    sbi_rt::set_timer(deadline);
}
