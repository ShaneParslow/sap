//! The kernel's debug console and halt call.
//!
//! TODO: these use debug syscalls; replace with a log service (M7).

use core::fmt::{self, Write};

use sap_abi::syscall::{DEBUG_HALT, DEBUG_PUTCHAR};

use crate::sys::syscall1;

pub struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            syscall1(DEBUG_PUTCHAR, b as usize);
        }
        Ok(())
    }
}

/// Stop the machine, reporting success or failure.
pub fn halt(failure: bool) -> ! {
    syscall1(DEBUG_HALT, failure as usize);
    loop {}
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = Console.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::debug::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::debug::_print(format_args!("{}\n", format_args!($($arg)*))) };
}
