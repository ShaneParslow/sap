//! Supervisor trap entry and dispatch.
//!
//! Traps currently only come from the kernel itself, so the entry code saves
//! registers on the current stack. Traps from user mode will need an
//! `sscratch` swap to a kernel stack (M2, user mode).

use core::arch::{asm, global_asm};

/// Registers saved by `trap_entry`; layout must match the assembly below.
#[repr(C)]
pub struct TrapFrame {
    /// x0..x31; x[2] holds the interrupted sp.
    pub x: [usize; 32],
    pub sepc: usize,
    pub sstatus: usize,
}

const FRAME_SIZE: usize = core::mem::size_of::<TrapFrame>();
const _: () = assert!(FRAME_SIZE == 34 * 8);

global_asm!(
    r#"
    .section .text
    .balign 4
    .globl trap_entry
trap_entry:
    addi    sp, sp, -{frame}
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*8(sp)
    .endr
    addi    t0, sp, {frame}
    sd      t0, 2*8(sp)
    csrr    t0, sepc
    sd      t0, 32*8(sp)
    csrr    t0, sstatus
    sd      t0, 33*8(sp)

    mv      a0, sp
    call    trap_handler

    ld      t0, 32*8(sp)
    csrw    sepc, t0
    ld      t0, 33*8(sp)
    csrw    sstatus, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*8(sp)
    .endr
    addi    sp, sp, {frame}
    sret
"#,
    frame = const FRAME_SIZE,
);

unsafe extern "C" {
    fn trap_entry();
}

const INTERRUPT_BIT: usize = 1 << (usize::BITS - 1);
const EXC_BREAKPOINT: usize = 3;

/// Point `stvec` at `trap_entry` (direct mode: all traps to one address).
pub fn init() {
    let entry = trap_entry as *const () as usize;
    debug_assert!(entry % 4 == 0);
    // SAFETY: trap_entry saves and restores all state it touches.
    unsafe { asm!("csrw stvec, {}", in(reg) entry) };
}

#[unsafe(no_mangle)]
extern "C" fn trap_handler(frame: &mut TrapFrame) {
    let scause: usize;
    let stval: usize;
    // SAFETY: reading CSRs has no side effects.
    unsafe {
        asm!("csrr {}, scause", out(reg) scause);
        asm!("csrr {}, stval", out(reg) stval);
    }
    let code = scause & !INTERRUPT_BIT;

    if scause & INTERRUPT_BIT != 0 {
        panic!("unexpected interrupt {code} (sepc {:#x})", frame.sepc);
    }

    match code {
        EXC_BREAKPOINT => {
            crate::debug!("breakpoint at {:#x}", frame.sepc);
            frame.sepc += instruction_len(frame.sepc);
        }
        _ => panic!(
            "unhandled exception {code} at sepc {:#x}, stval {stval:#x}",
            frame.sepc
        ),
    }
}

/// Length of the instruction at `pc`: 2 if compressed, else 4.
fn instruction_len(pc: usize) -> usize {
    // SAFETY: pc is the address of an instruction that just trapped, in kernel text.
    let low = unsafe { core::ptr::read_volatile(pc as *const u16) };
    if low & 0b11 == 0b11 { 4 } else { 2 }
}

/// Trap into the handler and come back; used as a boot check.
pub fn breakpoint() {
    // SAFETY: the handler skips the ebreak and returns.
    unsafe { asm!("ebreak") };
}
