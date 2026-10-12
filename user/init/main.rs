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

// TODO: temporary debug syscalls, matching kernel/syscall.rs. Replaced by
// capability invocations in M3.
const SYS_DEBUG_PUTCHAR: usize = 0x100;
const SYS_DEBUG_HALT: usize = 0x101;

fn syscall(number: usize, arg: usize) {
    // SAFETY: the kernel's debug syscalls take one argument in a0 and
    // clobber only a0.
    unsafe { asm!("ecall", inlateout("a0") arg => _, in("a7") number, options(nostack)) };
}

struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        s.bytes().for_each(|b| syscall(SYS_DEBUG_PUTCHAR, b as usize));
        Ok(())
    }
}

fn halt(failure: bool) -> ! {
    syscall(SYS_DEBUG_HALT, failure as usize);
    loop {}
}

static GREETING: &str = "hello from user mode";

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    // GREETING is a pointer in .data, so printing it also checks that the
    // loader applied the relocations.
    let _ = writeln!(Console, "init: {GREETING}, main at {:#x}", main as *const () as usize);
    halt(false)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(Console, "init: panic: {info}");
    halt(true)
}
