//! Text output and the halt call. `print!` and `println!` write to the log
//! (`interfaces/log.capnp`) once the program has one (`set_log`), and to
//! the kernel's debug console before that or if the log fails.
//!
//! TODO: programs without a log (the root task, processes the factory
//! starts) still use the debug syscalls; make those debug-build only once
//! every program gets a log (I6).

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicUsize, Ordering};

use capnp::message::{Builder, SingleSegmentAllocator};
use sap_interfaces::log_capnp::log::write_params;
use sap_interfaces::{methods, words};
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

/// Room for a `write` call's params of one chunk: the root pointer, the
/// text's pointer, and the text with its terminating zero.
const PARAMS_WORDS: usize = 2 + (CHUNK + 1).div_ceil(8);

impl Log {
    fn flush(&mut self) {
        // The chunk holds whole characters only (`write_str`).
        let text = core::str::from_utf8(&self.chunk[..self.len]).unwrap_or("?");
        self.len = 0;
        if !Self::write(self.log, text) {
            let _ = Console.write_str(text);
        }
    }

    /// Write `text` to the log; whether it was written. Builds the message
    /// on the stack: nothing in printing allocates.
    fn write(log: usize, text: &str) -> bool {
        let mut scratch = [0u64; PARAMS_WORDS];
        // SAFETY: any bytes are valid `u8`s, and the slice covers exactly
        // the scratch words, which it borrows.
        let bytes = unsafe { core::slice::from_raw_parts_mut(scratch.as_mut_ptr().cast::<u8>(), PARAMS_WORDS * 8) };
        let mut allocator = SingleSegmentAllocator::new(bytes);
        let mut message = Builder::new(&mut allocator);
        message.init_root::<write_params::Builder>().set_text(text);
        let mut buffer = [0usize; PARAMS_WORDS];
        let Some(n) = words::pack(&message, &mut buffer) else { return false };
        matches!(ipc::call(log, methods::log::WRITE, &buffer[..n], &[], &mut []), Ok(reply) if reply.info.label() == 0)
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
