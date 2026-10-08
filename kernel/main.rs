#![no_std]
#![no_main]

mod arch;
mod console;
mod devicetree;
mod log;

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
extern "C" fn kmain(hart_id: usize, dtb: usize) -> ! {
    info!("sap: sapabilities online");
    info!("boot hart {hart_id}, device tree at {dtb:#x}");

    let boot = devicetree::parse(dtb);
    boot.log();

    arch::trap::init();
    arch::timer::init(boot.timebase_hz);
    if let Some(address) = boot.test_device {
        arch::test_device::init(address);
    }
    boot_checks();

    info!("boot checks passed");
    arch::shutdown(false)
}

/// Exercise what exists so far, so `cargo run` shows it working.
fn boot_checks() {
    arch::trap::breakpoint();
    info!("trap: breakpoint handled and returned");

    arch::timer::start(100);
    arch::enable_interrupts();
    let start = arch::timer::now();
    while arch::timer::ticks() < 10 {
        arch::wait_for_interrupt();
    }
    let elapsed_us = arch::timer::to_micros(arch::timer::now() - start);
    info!("timer: 10 ticks at 100 Hz in {elapsed_us} us");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("panic: {info}");
    arch::shutdown(true)
}
