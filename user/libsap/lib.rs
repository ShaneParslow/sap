//! The runtime for sap user programs: the entry point, a debug console, the
//! panic handler, a slot allocator, locks, and (with the `heap` feature, on
//! by default) a heap for `alloc`. A program provides `sap_main`, and starts
//! the runtime with `runtime::start` before it uses slots, locks or the heap.
//!
//! The system-call wrappers are `sap-sys`, re-exported here. Rings and the
//! loader are crates of their own (`sap-ring`, `sap-loader`).
//!
//! Every thread starts with `sp` at the top of its stack, `tp` pointing at
//! its IPC buffer, and up to three arguments in `a0`..`a2`. For the root
//! task, `a0` is the address of the kernel's boot-info page.

#![no_std]

pub mod debug;
#[cfg(feature = "heap")]
pub mod heap;
pub mod runtime;
pub mod slots;
pub mod sync;

pub use sap_sys::{abi, cnode, ipc, notification, sys, tcb, untyped, vspace};

use core::panic::PanicInfo;

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
