//! Text output and the halt call. `print!` and `println!` write to the log
//! (`interfaces/log.capnp`) once the program has one (`set_log`), and to
//! the kernel's debug console before that or if the log fails.
//!
//! TODO: programs without a log (the root task, processes the factory
//! starts) still use the debug syscalls; make those debug-build only once
//! every program gets a log (I6).

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicUsize, Ordering};

use sap_interfaces::methods;
use sap_sys::abi::syscall::{DEBUG_HALT, DEBUG_PUTCHAR};
use sap_sys::ipc;
use sap_sys::sys::syscall1;

/// The capability to the log; 0 (the null slot) for none.
static LOG: AtomicUsize = AtomicUsize::new(0);

/// Send output to the log at capability `log` from now on.
pub fn set_log(log: usize) {
    LOG.store(log, Ordering::Relaxed);
}

/// The kernel's debug console.
pub struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            syscall1(DEBUG_PUTCHAR, b as usize);
        }
        Ok(())
    }
}

/// The most text one call to the log carries; longer output takes several.
/// Small, since threads with small stacks print too.
const CHUNK: usize = 128;

/// Output gathered into chunks, each written to the log in one call.
struct Log {
    log: usize,
    chunk: [u8; CHUNK],
    len: usize,
}

#[doc(hidden)]
/// The words of an `Output` message of one chunk: its root pointer, the
/// text's pointer, and the text with its terminating zero.
pub const OUTPUT_WORDS: usize = 2 + (CHUNK + 1).div_ceil(8);

/// Encode `log_messages.capnp`'s `Output { text }` into `words`, as
/// `sap_interfaces::words::pack` would put a builder's message; the number
/// of words. By hand, since building a message with capnp allocates when
/// its `alloc` feature is on, and printing must not: the heap prints, and
/// `check` checks that the heap starts empty. `check` compares the encoding
/// with capnp's.
#[doc(hidden)]
pub fn encode_output(text: &str, words: &mut [usize; OUTPUT_WORDS]) -> usize {
    let bytes = &text.as_bytes()[..text.len().min(CHUNK)];
    // The root: a struct pointer to the next word, no data, one pointer.
    words[0] = 1 << 48;
    // The text: a list pointer to the word after it, of bytes, with the
    // terminating zero.
    words[1] = 1 | (2 << 32) | ((bytes.len() + 1) << 35);
    let n = 2 + (bytes.len() + 1).div_ceil(8);
    words[2..n].fill(0);
    for (i, &byte) in bytes.iter().enumerate() {
        words[2 + i / 8] |= (byte as usize) << (8 * (i % 8));
    }
    n
}

impl Log {
    fn flush(&mut self) {
        // The chunk holds whole characters only (`write_str`).
        let text = core::str::from_utf8(&self.chunk[..self.len]).unwrap_or("?");
        self.len = 0;
        if !Self::write(self.log, text) {
            let _ = Console.write_str(text);
        }
    }

    /// Write `text` to the log; whether it was written.
    fn write(log: usize, text: &str) -> bool {
        let mut words = [0; OUTPUT_WORDS];
        let n = encode_output(text, &mut words);
        matches!(ipc::call(log, methods::log::WRITE, &words[..n], &[], &mut []), Ok(reply) if reply.info.label() == 0)
    }
}

impl Write for Log {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.len + c.len_utf8() > CHUNK {
                self.flush();
            }
            self.len += c.encode_utf8(&mut self.chunk[self.len..]).len();
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
    let log = LOG.load(Ordering::Relaxed);
    if log == 0 {
        let _ = Console.write_fmt(args);
        return;
    }
    let mut out = Log { log, chunk: [0; CHUNK], len: 0 };
    let _ = out.write_fmt(args);
    if out.len > 0 {
        out.flush();
    }
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
