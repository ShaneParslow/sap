#![no_std]
#![no_main]

mod arch;
mod boot;
mod cap;
mod console;
mod cspace;
mod devicetree;
mod endpoint;
mod fault;
mod global;
mod invocation;
mod log;
mod mm;
mod notification;
mod root_task;
mod sched;
mod syscall;
mod thread;
mod untyped;

use core::panic::PanicInfo;

use boot::BootInfo;
use mm::BootAllocator;

/// The architecture-neutral kernel, entered by the architecture code once
/// traps, paging and the timer are set up. `alloc` holds the memory not
/// used by the kernel itself.
pub fn kmain(boot: BootInfo, alloc: BootAllocator) -> ! {
    info!("sap: sapabilities online");
    boot.log();
    mm::log_free(alloc.free());
    arch::timer::start(TICK_HZ);
    boot_checks();
    info!("boot checks passed");

    root_task::start(alloc);
    sched::run()
}

/// Timer interrupts per second; the scheduler's time slices are counted in
/// these ticks.
const TICK_HZ: u64 = 100;

/// Exercise what exists so far, so `cargo run` shows it working.
fn boot_checks() {
    arch::trap::breakpoint();
    info!("trap: breakpoint handled and returned");

    let first = arch::timer::ticks();
    let start = arch::timer::now();
    while arch::timer::ticks() < first + 10 {
        arch::idle_once();
    }
    let elapsed_us = arch::timer::to_micros(arch::timer::now() - start);
    info!("timer: 10 ticks at {TICK_HZ} Hz in {elapsed_us} us");
}

/// TODO: print a backtrace in debug builds by walking frame pointers (needs
/// `-C force-frame-pointers=yes`). Panics abort; there is no unwinding.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("panic: {info}");
    arch::shutdown(true)
}
