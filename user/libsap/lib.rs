//! The runtime for sap user programs: the entry point, a debug console, the
//! panic handler, wrappers for system calls and kernel-object invocations,
//! and a loader for starting other programs. A program provides `sap_main`.
//!
//! Every thread starts with `sp` at the top of its stack, `tp` pointing at
//! its IPC buffer, and up to three arguments in `a0`..`a2`. For the root
//! task, `a0` is the address of the kernel's boot-info page.

#![no_std]

pub mod cnode;
pub mod debug;
pub mod ipc;
pub mod notification;
pub mod process;
pub mod sys;
pub mod tcb;
pub mod untyped;
pub mod vspace;

use core::panic::PanicInfo;

pub use sap_abi as abi;

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
    /// The program's entry point, with the thread's three start arguments.
    fn sap_main(a0: usize, a1: usize, a2: usize) -> !;
}

#[unsafe(no_mangle)]
extern "C" fn sap_start(a0: usize, a1: usize, a2: usize) -> ! {
    // SAFETY: the program defines sap_main.
    unsafe { sap_main(a0, a1, a2) }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {info}");
    debug::halt(true)
}
