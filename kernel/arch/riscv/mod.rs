//! RISC-V (RV64GC, S-mode under SBI firmware).

mod entry;
mod sbi;
mod test_device;
pub mod timer;
pub mod trap;

use core::sync::atomic::{AtomicU8, Ordering};

use crate::devicetree;

/// Called by the entry code with OpenSBI's arguments. Turns them into the
/// architecture-neutral `BootInfo`, sets up traps and the timer, and enters
/// the generic kernel.
#[unsafe(no_mangle)]
extern "C" fn riscv_start(hart_id: usize, dtb: usize) -> ! {
    // Find QEMU's test device first, so that a panic later in boot still
    // exits the emulator with a failure status.
    if let Some(address) = devicetree::find_device(dtb, &["sifive,test0"]) {
        test_device::init(address);
    }
    let boot = devicetree::parse(dtb, hart_id);
    trap::init();
    timer::init(boot.timer_hz);
    crate::kmain(boot)
}

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
    test_device::exit(failure, 1);
    sbi::shutdown(failure);
    halt()
}

pub fn enable_interrupts() {
    // SAFETY: the trap vector is installed before this is called.
    unsafe { riscv::register::sstatus::set_sie() };
}

/// Sleep until an interrupt arrives (or return spuriously).
pub fn wait_for_interrupt() {
    riscv::asm::wfi();
}

pub fn halt() -> ! {
    loop {
        riscv::asm::wfi();
    }
}
