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
    arch::shutdown(false)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("panic: {info}");
    arch::shutdown(true)
}
