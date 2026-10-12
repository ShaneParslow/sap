//! Kernel console: `print!` and `println!` over the architecture's early console.
//!
//! This is a debugging aid, not the system's console. Terminals, graphics
//! and input are userspace components (see the open question on the user
//! interface stack in the wiki).
//!
//! TODO: once a userspace log service exists (M7), make kernel printing
//! debug-build only, as seL4 does, and send release-build kernel messages
//! to that service.

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
