//! RISC-V: `ecall` with arguments and results in `a0`..`a7`.

use core::arch::asm;

/// Argument and result registers of a system call, in order (`a0`..`a6`).
/// The call number goes in the register after them (`a7`), which some calls
/// use to return an error code.
pub type Registers = [usize; 7];

/// Make system call `number` with `regs` as its arguments; on return
/// `regs` holds the results. Returns what the kernel left in the number
/// register.
///
/// # Safety
/// The call must be one whose effects on this program's memory the caller
/// accounts for (most touch none, or only the IPC buffer).
#[inline(always)]
pub unsafe fn syscall(number: usize, regs: &mut Registers) -> usize {
    let last: usize;
    // SAFETY: the kernel reads and writes a0..a7 and preserves every other
    // register (the caller's guarantee covers memory).
    unsafe {
        asm!("ecall",
             inlateout("a0") regs[0] => regs[0], inlateout("a1") regs[1] => regs[1],
             inlateout("a2") regs[2] => regs[2], inlateout("a3") regs[3] => regs[3],
             inlateout("a4") regs[4] => regs[4], inlateout("a5") regs[5] => regs[5],
             inlateout("a6") regs[6] => regs[6], inlateout("a7") number => last,
             options(nostack));
    }
    last
}

/// The time counter (`rdtime`), in ticks of `BootInfo::timer_hz`.
#[inline(always)]
pub fn time() -> u64 {
    let value: u64;
    // SAFETY: reading a counter the kernel lets user mode read.
    unsafe { asm!("rdtime {}", out(reg) value, options(nomem, nostack)) };
    value
}

/// The thread pointer register (`tp`), which holds the IPC buffer's
/// address (the thread start convention).
#[inline(always)]
pub fn thread_pointer() -> usize {
    let value: usize;
    // SAFETY: reading a register.
    unsafe { asm!("mv {}, tp", out(reg) value, options(nomem, nostack)) };
    value
}
