#![no_std]
#![no_main]

mod arch;
mod console;
mod log;

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
extern "C" fn kmain(hart_id: usize, dtb: usize) -> ! {
    info!("sap: sapabilities online");
    info!("boot hart {hart_id}, device tree at {dtb:#x}");

    arch::trap::init();
    boot_checks();

    info!("boot checks passed");
    arch::shutdown(false)
}

/// Exercise what exists so far, so `cargo run` shows it working.
fn boot_checks() {
    arch::trap::breakpoint();
    info!("trap: breakpoint handled and returned");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("panic: {info}");
    arch::shutdown(true)
}
