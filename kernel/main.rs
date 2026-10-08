#![no_std]
#![no_main]

mod arch;
mod console;

use core::panic::PanicInfo;

#[unsafe(no_mangle)]
extern "C" fn kmain(hart_id: usize, dtb: usize) -> ! {
    println!("sap: sapabilities online");
    println!("sap: boot hart {hart_id}, device tree at {dtb:#x}");
    arch::shutdown(false)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("sap: panic: {info}");
    arch::shutdown(true)
}
