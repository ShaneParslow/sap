//! CHERI capabilities in user registers, when the kernel enables them.

use sap_rt::abi::BootInfo;
use sap_rt::{println, sys};

/// The top of user addresses, which user capabilities are bounded to.
const USER_END: usize = 1 << 38;

/// The permission to access system registers.
const ASR: usize = 1 << 16;

/// A capability in a user register survives a system call and timer
/// interrupts, which the kernel saves and restores whole; a register the
/// kernel writes a result into comes back as an integer, with no tag; user
/// capabilities are bounded to user addresses; and DDC is per thread.
pub fn registers(info: &BootInfo) {
    if info.cheri == 0 {
        return;
    }

    through_a_system_call();
    through_interrupts(info.timer_hz);
    bounds();
    ddc_is_per_thread();

    println!(
        "CHERI capabilities survive system calls and interrupts in registers; \
         kernel-written results are integers; DDC and the pc are bounded to user \
         addresses, and DDC is per thread"
    );
}

/// The capability is DDC, which every thread has (the root, for now).
fn through_a_system_call() {
    let kept: usize;
    let written: usize;

    // SAFETY: reads DDC into registers and polls slot 0, which holds
    // nothing; touches no memory. The RVY instructions are raw encodings:
    // `yaddrw` (set the address) and `ytagr` (read the tag).
    unsafe {
        core::arch::asm!(
            "csrr s2, 0x03f",
            "csrr a0, 0x03f",
            ".insn r 0x7b, 0, 0x0b, a0, a0, x0",
            "ecall",
            ".insn r 0x7b, 0, 0x7a, {kept}, s2, x4",
            ".insn r 0x7b, 0, 0x7a, {written}, a0, x4",
            kept = lateout(reg) kept,
            written = lateout(reg) written,
            inout("a7") sap_rt::abi::syscall::POLL => _,
            out("a0") _, out("a1") _, out("a2") _, out("s2") _,
            options(nostack),
        );
    }

    assert_eq!(
        kept, 1,
        "a capability in s2 keeps its tag through a system call"
    );
    assert_eq!(
        written, 0,
        "a result the kernel writes into a0 is an integer"
    );
}

/// Spin through a few timer interrupts (100 Hz) with a capability in s3.
fn through_interrupts(timer_hz: u64) {
    let deadline = sys::time() + timer_hz / 20;
    let survived: usize;

    // SAFETY: registers and the time counter only.
    unsafe {
        core::arch::asm!(
            "csrr s3, 0x03f",
            "1: rdtime {now}",
            "bltu {now}, {deadline}, 1b",
            ".insn r 0x7b, 0, 0x7a, {survived}, s3, x4",
            now = out(reg) _,
            deadline = in(reg) deadline,
            survived = lateout(reg) survived,
            out("s3") _,
            options(nostack, nomem),
        );
    }

    assert_eq!(
        survived, 1,
        "a capability in s3 keeps its tag through timer interrupts"
    );
}

/// DDC and the program counter capability: the user root, bounded to user
/// addresses and without access to system registers. The pc is read with
/// `auipc` in capability mode, between `ymodeswy` and `ymodeswi`.
fn bounds() {
    let ddc_base: usize;
    let ddc_len: usize;
    let ddc_perms: usize;
    let pcc_base: usize;
    let pcc_len: usize;
    let pcc_perms: usize;

    // SAFETY: register-only; capability mode only around one `auipc`.
    unsafe {
        core::arch::asm!(
            "csrr {c}, 0x03f",
            ".insn r 0x7b, 0, 0x7a, {db}, {c}, x0",
            ".insn r 0x7b, 0, 0x7a, {dl}, {c}, x3",
            ".insn r 0x7b, 0, 0x7a, {dp}, {c}, x1",
            ".insn 4, 0x5600007b",
            "auipc {c}, 0",
            ".insn 4, 0x5610007b",
            ".insn r 0x7b, 0, 0x7a, {pb}, {c}, x0",
            ".insn r 0x7b, 0, 0x7a, {pl}, {c}, x3",
            ".insn r 0x7b, 0, 0x7a, {pp}, {c}, x1",
            c = out(reg) _,
            db = lateout(reg) ddc_base, dl = lateout(reg) ddc_len, dp = lateout(reg) ddc_perms,
            pb = lateout(reg) pcc_base, pl = lateout(reg) pcc_len, pp = lateout(reg) pcc_perms,
            options(nostack, nomem),
        );
    }

    assert_eq!(
        (ddc_base, ddc_len),
        (0, USER_END),
        "DDC covers user addresses only"
    );
    assert_eq!(
        (pcc_base, pcc_len),
        (0, USER_END),
        "the pc capability covers user addresses only"
    );
    assert_eq!(ddc_perms & ASR, 0, "DDC can't access system registers");
    assert_eq!(
        pcc_perms & ASR,
        0,
        "the pc capability can't access system registers"
    );
}

/// DDC is the thread's own: narrowed to one page, it stays narrowed through
/// a system call, which the kernel serves through a DDC of its own. Restored
/// before any load or store needs it.
fn ddc_is_per_thread() {
    let narrowed: usize;

    // SAFETY: no loads or stores while DDC is narrowed; capability mode
    // only around the two DDC writes, which need it to write the whole
    // capability; s4 keeps the original DDC across the system call.
    unsafe {
        core::arch::asm!(
            "csrr s4, 0x03f",
            ".insn r 0x7b, 0, 0x0b, {n}, s4, {page}",
            ".insn r 0x7b, 0, 0x1b, {n}, {n}, {len}",
            ".insn 4, 0x5600007b",
            "csrw 0x03f, {n}",
            ".insn 4, 0x5610007b",
            "ecall",
            "csrr {n}, 0x03f",
            ".insn r 0x7b, 0, 0x7a, {narrowed}, {n}, x3",
            ".insn 4, 0x5600007b",
            "csrw 0x03f, s4",
            ".insn 4, 0x5610007b",
            n = out(reg) _,
            page = in(reg) 0x1000usize,
            len = in(reg) 0x1000usize,
            narrowed = lateout(reg) narrowed,
            inout("a7") sap_rt::abi::syscall::YIELD => _,
            out("a0") _, out("a1") _, out("a2") _, out("a3") _,
            out("a4") _, out("a5") _, out("a6") _, out("s4") _,
            options(nostack, nomem),
        );
    }

    assert_eq!(
        narrowed, 0x1000,
        "a thread's DDC is its own, kept across a system call"
    );
}
