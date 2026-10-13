//! The runtime for sap user programs: the entry point, a debug console, the
//! panic handler, and wrappers for system calls and kernel-object
//! invocations. A program provides `sap_main`.
//!
//! TODO: one thread per program for now: the IPC buffer pointer is global.

#![no_std]

pub mod cnode;
pub mod debug;
pub mod sys;
pub mod untyped;
pub mod vspace;

use core::panic::PanicInfo;

pub use sap_abi as abi;
use sap_abi::BootInfo;

// The kernel starts a thread with sp at the top of its stack and its first
// argument in a0; for the root task that is the boot-info address.
core::arch::global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    call    sap_start
1:  j       1b
"#
);

unsafe extern "Rust" {
    /// The program's entry point.
    fn sap_main(boot_info: &'static BootInfo) -> !;
}

#[unsafe(no_mangle)]
extern "C" fn sap_start(boot_info: &'static BootInfo) -> ! {
    sys::set_ipc_buffer(boot_info.ipc_buffer);
    // SAFETY: the program defines sap_main.
    unsafe { sap_main(boot_info) }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {info}");
    debug::halt(true)
}
