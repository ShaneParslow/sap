#![no_std]
#![no_main]

mod arch;
mod boot;
mod console;
mod devicetree;
mod log;
mod mm;

use core::panic::PanicInfo;

use boot::BootInfo;

/// The architecture-neutral kernel, entered by the architecture code once
/// traps and the timer are set up.
pub fn kmain(boot: BootInfo) -> ! {
    info!("sap: sapabilities online");
    boot.log();
    mm::log_free(&boot.free);
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

/// TODO: print a backtrace in debug builds by walking frame pointers (needs
/// `-C force-frame-pointers=yes`). Panics abort; there is no unwinding.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("panic: {info}");
    arch::shutdown(true)
}
