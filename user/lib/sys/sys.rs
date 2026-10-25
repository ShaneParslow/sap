//! System calls and the IPC buffer.

use sap_abi::{syscall, IpcBuffer, MessageInfo, MAX_EXTRA_CAPS, MESSAGE_REGISTERS};

/// This thread's IPC buffer.
///
/// Each thread keeps its IPC buffer's address in the thread-pointer
/// register `tp`, which nothing else uses yet (there is no thread-local
/// storage). Whoever starts a thread sets it with the other start
/// registers; the kernel does so for the root task.
///
/// TODO: move to a real thread-local once the sap Rust target has TLS.
pub fn ipc_buffer() -> &'static mut IpcBuffer {
    let address = crate::arch::thread_pointer();
    assert!(address != 0, "no IPC buffer");
    // SAFETY: the kernel mapped an IPC buffer here for this thread, and only
    // this thread uses it.
    unsafe { &mut *crate::address::mapped::<IpcBuffer>(address) }
}

/// A syscall with one argument, returning one word.
pub fn syscall1(number: usize, arg: usize) -> usize {
    let mut regs = [arg, 0, 0, 0, 0, 0, 0];
    // SAFETY: the calls made this way (YIELD, the debug calls) touch no
    // memory of this program.
    unsafe { crate::arch::syscall(number, &mut regs) };
    regs[0]
}

pub fn yield_now() {
    syscall1(syscall::YIELD, 0);
}

/// The time counter, in ticks of `BootInfo::timer_hz`. No system call.
pub fn time() -> u64 {
    crate::arch::time()
}

/// Invoke capability `cptr` with `label`, message words `words` and extra
/// capabilities `caps`. Returns the reply's message info and its first
/// result words.
pub fn call(cptr: usize, label: usize, words: &[usize], caps: &[usize]) -> (MessageInfo, [usize; MESSAGE_REGISTERS]) {
    assert!(caps.len() <= MAX_EXTRA_CAPS);
    // Only touch the IPC buffer when the message needs it, so threads
    // without one can still make short invocations.
    if words.len() > MESSAGE_REGISTERS || !caps.is_empty() {
        let buffer = ipc_buffer();
        for (i, &w) in words.iter().enumerate().skip(MESSAGE_REGISTERS) {
            buffer.msg[i] = w;
        }
        buffer.caps[..caps.len()].copy_from_slice(caps);
    }
    // The capability, the info, then the first message words.
    let mut regs = [cptr, MessageInfo::new(label, caps.len(), words.len()).0, 0, 0, 0, 0, 0];
    for (r, &w) in regs[2..2 + MESSAGE_REGISTERS].iter_mut().zip(words) {
        *r = w;
    }
    // SAFETY: an invocation writes only result registers and the IPC
    // buffer, which the caller reads after this returns.
    unsafe { crate::arch::syscall(syscall::CALL, &mut regs) };
    let mut results = [0; MESSAGE_REGISTERS];
    results.copy_from_slice(&regs[2..2 + MESSAGE_REGISTERS]);
    (MessageInfo(regs[1]), results)
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
