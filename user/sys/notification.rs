//! Notifications: signal, wait, poll.

use sap_abi::syscall;

use crate::sys::Error;

/// A notification syscall: `a0` = capability; returns (a0, a2) or the
/// error code in a1.
fn call(number: usize, cptr: usize) -> Result<(usize, usize), Error> {
    let mut regs = [cptr, 0, 0, 0, 0, 0, 0];
    // SAFETY: notification calls touch no memory of this program.
    unsafe { crate::arch::syscall(number, &mut regs) };
    let (bits, code, pending) = (regs[0], regs[1], regs[2]);
    if code == 0 { Ok((bits, pending)) } else { Err(Error(code)) }
}

/// Hand this capability's badge to a waiter, or leave it pending.
pub fn signal(cptr: usize) -> Result<(), Error> {
    call(syscall::SIGNAL, cptr).map(drop)
}

/// Block until a signal is pending, then take the pending bits (0 if every
/// signal came through an unbadged capability).
pub fn wait(cptr: usize) -> Result<usize, Error> {
    call(syscall::WAIT, cptr).map(|(bits, _)| bits)
}

/// Take the pending bits without blocking: None if no signal is pending.
pub fn poll(cptr: usize) -> Result<Option<usize>, Error> {
    call(syscall::POLL, cptr).map(|(bits, pending)| (pending != 0).then_some(bits))
}
