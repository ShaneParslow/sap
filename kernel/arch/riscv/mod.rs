//! RISC-V (RV64GC, S-mode under SBI firmware).

mod entry;
mod sbi;
pub mod timer;
pub mod trap;

use core::sync::atomic::{AtomicU8, Ordering};

const CONSOLE_UNPROBED: u8 = 0;
const CONSOLE_DBCN: u8 = 1;
const CONSOLE_LEGACY: u8 = 2;

/// Which SBI console to use, found on first write. Relaxed is enough: every
/// hart that probes stores the same answer, and nothing else is published
/// through this value.
static CONSOLE: AtomicU8 = AtomicU8::new(CONSOLE_UNPROBED);

pub fn console_write(bytes: &[u8]) {
    let mut kind = CONSOLE.load(Ordering::Relaxed);
    if kind == CONSOLE_UNPROBED {
        kind = if sbi::has_debug_console() { CONSOLE_DBCN } else { CONSOLE_LEGACY };
        CONSOLE.store(kind, Ordering::Relaxed);
    }
    if kind == CONSOLE_DBCN && sbi::debug_console_write(bytes) {
        return;
    }
    bytes.iter().copied().for_each(sbi::legacy_putchar);
}

/// Power off the machine; under QEMU this exits the emulator.
pub fn shutdown(failure: bool) -> ! {
    sbi::shutdown(failure);
    halt()
}

const SSTATUS_SIE: usize = 1 << 1;

pub fn enable_interrupts() {
    // SAFETY: the trap vector is installed before this is called.
    unsafe { core::arch::asm!("csrs sstatus, {}", in(reg) SSTATUS_SIE) };
}

/// Sleep until an interrupt arrives (or return spuriously).
pub fn wait_for_interrupt() {
    // SAFETY: wfi only waits for an interrupt.
    unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
}

pub fn halt() -> ! {
    loop {
        // SAFETY: wfi only waits for an interrupt.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
