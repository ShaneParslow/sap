//! Endpoint IPC: send, receive, call and reply.

use core::arch::asm;

use sap_abi::{syscall, MessageInfo, MESSAGE_REGISTERS};

use crate::sys::{ipc_buffer, Error};

/// What a receive returned.
#[derive(Clone, Copy, Debug)]
pub struct Received {
    /// The sender's badge, or the notification bits if `source` says so.
    pub badge: usize,
    /// The sender's label and the number of words delivered.
    pub info: MessageInfo,
    /// What woke the receiver (`sap_abi::recv_source`).
    pub source: usize,
}

/// One IPC system call: `a0` = endpoint, `a1` = info, `a2`..`a5` = the first
/// words (the rest are already in the IPC buffer), `a6` = reply object.
fn raw(number: usize, ep: usize, info: MessageInfo, reply: usize, out: &mut [usize]) -> Result<Received, Error> {
    let mut mr = [0usize; MESSAGE_REGISTERS];
    let buffer = ipc_buffer();
    for (r, w) in mr.iter_mut().zip(buffer.msg.iter()) {
        *r = *w;
    }
    let (badge, info_out, source, code): (usize, usize, usize, usize);
    // SAFETY: the kernel reads and writes a0..a7 and preserves the rest.
    unsafe {
        asm!("ecall",
             inlateout("a0") ep => badge, inlateout("a1") info.0 => info_out,
             inlateout("a2") mr[0] => mr[0], inlateout("a3") mr[1] => mr[1],
             inlateout("a4") mr[2] => mr[2], inlateout("a5") mr[3] => mr[3],
             inlateout("a6") reply => source, inlateout("a7") number => code,
             options(nostack));
    }
    if code != 0 {
        return Err(Error(code));
    }
    let info = MessageInfo(info_out);
    let n = info.length().min(out.len());
    for (i, w) in out.iter_mut().enumerate().take(n) {
        *w = if i < MESSAGE_REGISTERS { mr[i] } else { buffer.msg[i] };
    }
    Ok(Received { badge, info, source })
}

/// Put `words` where the kernel expects them: all in the IPC buffer's
/// message array (the first four are loaded into registers by `raw`).
fn load(words: &[usize]) -> usize {
    let buffer = ipc_buffer();
    buffer.msg[..words.len()].copy_from_slice(words);
    words.len()
}

/// Send, blocking until a receiver takes the message.
pub fn send(ep: usize, label: usize, words: &[usize]) -> Result<(), Error> {
    let n = load(words);
    raw(syscall::SEND, ep, MessageInfo::new(label, 0, n), 0, &mut []).map(drop)
}

/// Send if a receiver is waiting; otherwise the message is dropped.
pub fn nb_send(ep: usize, label: usize, words: &[usize]) -> Result<(), Error> {
    let n = load(words);
    raw(syscall::NB_SEND, ep, MessageInfo::new(label, 0, n), 0, &mut []).map(drop)
}

/// Send and wait for the reply, whose words land in `out`.
pub fn call(ep: usize, label: usize, words: &[usize], out: &mut [usize]) -> Result<Received, Error> {
    let n = load(words);
    raw(syscall::CALL, ep, MessageInfo::new(label, 0, n), 0, out)
}

/// Wait for a message, lending `reply` (0 for none) to answer a call.
pub fn recv(ep: usize, reply: usize, out: &mut [usize]) -> Result<Received, Error> {
    raw(syscall::RECV, ep, MessageInfo(0), reply, out)
}

/// Receive if a message is waiting; otherwise return with
/// `recv_source::NOTHING`.
pub fn nb_recv(ep: usize, reply: usize, out: &mut [usize]) -> Result<Received, Error> {
    raw(syscall::NB_RECV, ep, MessageInfo(0), reply, out)
}

/// Answer the caller waiting on `reply`, if any.
pub fn reply(reply: usize, label: usize, words: &[usize]) -> Result<(), Error> {
    let n = load(words);
    raw(syscall::REPLY, 0, MessageInfo::new(label, 0, n), reply, &mut []).map(drop)
}

/// Reply through `reply`, then wait on `ep` with the same reply object.
pub fn reply_recv(ep: usize, reply: usize, label: usize, words: &[usize], out: &mut [usize]) -> Result<Received, Error> {
    let n = load(words);
    raw(syscall::REPLY_RECV, ep, MessageInfo::new(label, 0, n), reply, out)
}
