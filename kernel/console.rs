//! Kernel console: `print!` and `println!` over the architecture's early console.

use core::fmt::{self, Write};

struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        crate::arch::console_write(s.as_bytes());
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    // Single hart, no interrupts yet: no locking needed.
    let _ = Console.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::console::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => { $crate::console::_print(format_args!("{}\n", format_args!($($arg)*))) };
}
