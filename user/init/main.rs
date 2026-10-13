//! The first user program: the root task. For now it only proves that user
//! mode works; it will become the process manager (M7).

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// The kernel starts the root task with sp at the top of its stack.
global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    call    main
1:  j       1b
"#
);

use sap_abi::syscall::{DEBUG_HALT, DEBUG_PUTCHAR, YIELD};

/// A syscall with one argument. The kernel saves and restores every
/// register except the result registers.
fn syscall(number: usize, arg: usize) -> usize {
    let result;
    // SAFETY: the kernel preserves all registers but a0..a5; these calls
    // only return in a0.
    unsafe {
        asm!("ecall", inlateout("a0") arg => result, in("a7") number,
             lateout("a1") _, lateout("a2") _, lateout("a3") _, lateout("a4") _, lateout("a5") _,
             options(nostack));
    }
    result
}

struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        s.bytes().for_each(|b| {
            syscall(DEBUG_PUTCHAR, b as usize);
        });
        Ok(())
    }
}

fn halt(failure: bool) -> ! {
    syscall(DEBUG_HALT, failure as usize);
    loop {}
}

static GREETING: &str = "hello from user mode";

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    // GREETING is a pointer in .data, so printing it also checks that the
    // loader applied the relocations.
    let _ = writeln!(Console, "init: {GREETING}, main at {:#x}", main as *const () as usize);
    // Yielding with no other thread at this priority comes straight back.
    syscall(YIELD, 0);
    let _ = writeln!(Console, "init: yielded and resumed");
    halt(false)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(Console, "init: panic: {info}");
    halt(true)
}
