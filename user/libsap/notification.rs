//! Notifications: signal, wait, poll.

use core::arch::asm;

use sap_abi::syscall;

use crate::sys::Error;

/// A notification syscall: `a0` = capability; returns (a0, a1 error code).
fn call(number: usize, cptr: usize) -> Result<usize, Error> {
    let (bits, code): (usize, usize);
    // SAFETY: the kernel preserves every register except a0..a5.
    unsafe {
        asm!("ecall", inlateout("a0") cptr => bits, lateout("a1") code, in("a7") number,
             lateout("a2") _, lateout("a3") _, lateout("a4") _, lateout("a5") _,
             options(nostack));
    }
    if code == 0 { Ok(bits) } else { Err(Error(code)) }
}

/// OR this capability's badge into the notification, waking a waiter.
pub fn signal(cptr: usize) -> Result<(), Error> {
    call(syscall::SIGNAL, cptr).map(drop)
}

/// Block until some bits are pending, then take them.
pub fn wait(cptr: usize) -> Result<usize, Error> {
    call(syscall::WAIT, cptr)
}

/// Take the pending bits without blocking (0 if none).
pub fn poll(cptr: usize) -> Result<usize, Error> {
    call(syscall::POLL, cptr)
}
