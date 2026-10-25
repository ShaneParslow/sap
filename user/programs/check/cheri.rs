//! CHERI capabilities in user mode, when the kernel enables them.

use alloc::boxed::Box;
use sap_rt::{println, sys};
use crate::Env;

/// With CHERI: a capability in a user register survives a system call and
/// timer interrupts, which the kernel saves and restores whole, and a
/// register the kernel writes a result into comes back as an integer, with
/// no tag. The capability is DDC, which every thread has (the root, for
/// now).
pub fn check_cheri(info: &Env) {
    if !info.cheri {
        return;
    }
    let (kept, written): (usize, usize);
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
    assert_eq!(kept, 1, "a capability in s2 keeps its tag through a system call");
    assert_eq!(written, 0, "a result the kernel writes into a0 is an integer");

    // Spin through a few timer interrupts (100 Hz) with a capability in s3.
    let deadline = sys::time() + info.timer_hz / 20;
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
    assert_eq!(survived, 1, "a capability in s3 keeps its tag through timer interrupts");

    // DDC and the program counter capability: the user root, bounded to
    // user addresses and without access to system registers. The pc is read
    // with `auipc` in capability mode, between `ymodeswy` and `ymodeswi`.
    let (ddc_base, ddc_len, ddc_perms, pcc_base, pcc_len, pcc_perms): (usize, usize, usize, usize, usize, usize);
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
    const USER_END: usize = 1 << 38;
    const ASR: usize = 1 << 16;
    assert_eq!((ddc_base, ddc_len), (0, USER_END), "DDC covers user addresses only");
    assert_eq!((pcc_base, pcc_len), (0, USER_END), "the pc capability covers user addresses only");
    assert_eq!((ddc_perms & ASR, pcc_perms & ASR), (0, 0), "user capabilities can't access system registers");

    // DDC is the thread's own: narrowed to one page, it stays narrowed
    // through a system call, which the kernel serves through a DDC of its
    // own. Restored before any load or store needs it.
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
            out("a0") _, out("a1") _, out("a2") _, out("a3") _, out("a4") _, out("a5") _, out("a6") _,
            out("s4") _,
            options(nostack, nomem),
        );
    }
    assert_eq!(narrowed, 0x1000, "a thread's DDC is its own, kept across a system call");

    // Capabilities in memory: the loader and the heap map private pages
    // with `attr::CAPABILITIES`, so DDC stored to the stack or the heap
    // with `sy` comes back tagged with `ly`.
    #[repr(C, align(16))]
    struct Slot([usize; 2]);
    let mut on_stack = Slot([0; 2]);
    let mut on_heap = Box::new(Slot([0; 2]));
    for (slot, place) in [(&mut on_stack as *mut Slot, "the stack"), (&mut *on_heap as *mut Slot, "the heap")] {
        let tag: usize;
        // SAFETY: `slot` is 16 aligned, writable bytes this function owns.
        unsafe {
            core::arch::asm!(
                "csrr {c}, 0x03f",
                ".insn s 0x7b, 2, {c}, 0({p})",
                ".insn i 0x7b, 1, {c}, 0({p})",
                ".insn r 0x7b, 0, 0x7a, {tag}, {c}, x4",
                c = out(reg) _,
                p = in(reg) slot,
                tag = lateout(reg) tag,
                options(nostack),
            );
        }
        assert_eq!(tag, 1, "a capability stored to {place} keeps its tag");
    }
    println!("check: CHERI capabilities survive system calls and interrupts in registers, and loads and stores on the stack and heap; kernel-written results are integers; DDC and the pc are bounded to user addresses, and DDC is per thread");
}
