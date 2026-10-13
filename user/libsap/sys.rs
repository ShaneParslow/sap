//! System calls and the IPC buffer.

use core::arch::asm;
use core::sync::atomic::{AtomicUsize, Ordering};

use sap_abi::{syscall, IpcBuffer, MessageInfo, MAX_EXTRA_CAPS, MESSAGE_REGISTERS};

static IPC_BUFFER: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn set_ipc_buffer(address: usize) {
    IPC_BUFFER.store(address, Ordering::Relaxed);
}

/// This thread's IPC buffer.
pub fn ipc_buffer() -> &'static mut IpcBuffer {
    let address = IPC_BUFFER.load(Ordering::Relaxed);
    assert!(address != 0, "no IPC buffer");
    // SAFETY: the kernel mapped a page-aligned IPC buffer here for this
    // thread, and only this thread uses it.
    unsafe { &mut *(address as *mut IpcBuffer) }
}

/// A syscall with one argument, returning one word.
pub fn syscall1(number: usize, arg: usize) -> usize {
    let result;
    // SAFETY: the kernel preserves every register except a0..a5.
    unsafe {
        asm!("ecall", inlateout("a0") arg => result, in("a7") number,
             lateout("a1") _, lateout("a2") _, lateout("a3") _, lateout("a4") _, lateout("a5") _,
             options(nostack));
    }
    result
}

pub fn yield_now() {
    syscall1(syscall::YIELD, 0);
}

/// Invoke capability `cptr` with `label`, message words `words` and extra
/// capabilities `caps`. Returns the reply's message info and its first
/// result words.
pub fn call(cptr: usize, label: usize, words: &[usize], caps: &[usize]) -> (MessageInfo, [usize; MESSAGE_REGISTERS]) {
    assert!(caps.len() <= MAX_EXTRA_CAPS);
    let buffer = ipc_buffer();
    for (i, &w) in words.iter().enumerate().skip(MESSAGE_REGISTERS) {
        buffer.msg[i] = w;
    }
    buffer.caps[..caps.len()].copy_from_slice(caps);
    let mut mr = [0; MESSAGE_REGISTERS];
    for (r, &w) in mr.iter_mut().zip(words) {
        *r = w;
    }
    let info = MessageInfo::new(label, caps.len(), words.len());
    let reply: usize;
    // SAFETY: CALL takes the capability in a0, the info in a1 and message
    // registers in a2..a5, and returns in the same registers.
    unsafe {
        asm!("ecall",
             inlateout("a0") cptr => _, inlateout("a1") info.0 => reply,
             inlateout("a2") mr[0] => mr[0], inlateout("a3") mr[1] => mr[1],
             inlateout("a4") mr[2] => mr[2], inlateout("a5") mr[3] => mr[3],
             in("a7") syscall::CALL, options(nostack));
    }
    (MessageInfo(reply), mr)
}

/// An invocation's error code (`sap_abi::error`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Error(pub usize);

/// `call`, turning a nonzero label into an error.
pub fn invoke(cptr: usize, label: usize, words: &[usize], caps: &[usize]) -> Result<[usize; MESSAGE_REGISTERS], Error> {
    let (info, results) = call(cptr, label, words, caps);
    match info.label() {
        0 => Ok(results),
        code => Err(Error(code)),
    }
}
