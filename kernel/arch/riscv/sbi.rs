//! Calls into the SBI firmware (OpenSBI). See the RISC-V SBI specification.

use core::arch::asm;

const EID_BASE: usize = 0x10;
const EID_LEGACY_PUTCHAR: usize = 0x01;
const EID_DBCN: usize = 0x4442_434E; // "DBCN": debug console
const EID_SRST: usize = 0x5352_5354; // "SRST": system reset
const EID_TIME: usize = 0x5449_4D45; // "TIME"

const BASE_PROBE_EXTENSION: usize = 3;
const DBCN_WRITE: usize = 0;
const SRST_RESET: usize = 0;
const TIME_SET_TIMER: usize = 0;

const SRST_TYPE_SHUTDOWN: usize = 0;
const SRST_REASON_NONE: usize = 0;
const SRST_REASON_FAILURE: usize = 1;

/// SBI calls return (error, value); error 0 is success.
fn ecall(eid: usize, fid: usize, a0: usize, a1: usize, a2: usize) -> (isize, usize) {
    let (error, value);
    // SAFETY: the SBI calling convention preserves every register except a0 and a1.
    unsafe {
        asm!(
            "ecall",
            inlateout("a0") a0 => error,
            inlateout("a1") a1 => value,
            in("a2") a2,
            in("a6") fid,
            in("a7") eid,
            options(nostack),
        );
    }
    (error, value)
}

pub fn probe_extension(eid: usize) -> bool {
    ecall(EID_BASE, BASE_PROBE_EXTENSION, eid, 0, 0).1 != 0
}

pub fn has_debug_console() -> bool {
    probe_extension(EID_DBCN)
}

/// Write bytes with the debug console extension. The MMU is off, so the
/// slice's address is its physical address. Returns false on SBI error.
pub fn debug_console_write(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while !rest.is_empty() {
        let addr = rest.as_ptr() as usize;
        let (error, written) = ecall(EID_DBCN, DBCN_WRITE, rest.len(), addr, 0);
        if error != 0 {
            return false;
        }
        rest = &rest[written.min(rest.len())..];
    }
    true
}

/// Deprecated legacy console, for firmware without DBCN.
pub fn legacy_putchar(byte: u8) {
    ecall(EID_LEGACY_PUTCHAR, 0, byte as usize, 0, 0);
}

pub fn shutdown(failure: bool) {
    let reason = if failure { SRST_REASON_FAILURE } else { SRST_REASON_NONE };
    ecall(EID_SRST, SRST_RESET, SRST_TYPE_SHUTDOWN, reason, 0);
}

/// Request a supervisor timer interrupt when `time` reaches `deadline`;
/// also clears a pending one.
pub fn set_timer(deadline: u64) {
    ecall(EID_TIME, TIME_SET_TIMER, deadline as usize, 0, 0);
}
