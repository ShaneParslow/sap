//! Supervisor trap entry and dispatch.
//!
//! TODO: traps currently only come from the kernel itself, so the entry code
//! saves registers on the current stack. Traps from user mode need an
//! `sscratch` swap to a per-hart kernel stack first (M2, user mode).
//!
//! `stvec` uses direct mode: every trap enters `trap_entry`, and Rust code
//! dispatches on `scause`. Vectored mode would only skip that one dispatch
//! for interrupts (exceptions, including syscalls, always use the base
//! address), so it isn't worth the extra entry stubs yet.

use core::arch::global_asm;

use riscv::interrupt::supervisor::{Exception, Interrupt};
use riscv::interrupt::Trap;
use riscv::register::{scause, stval, stvec};

use super::timer;

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
    .balign 4                       # stvec requires 4-byte alignment
    .globl trap_entry
trap_entry:
    # Make room for a TrapFrame on the interrupted stack.
    addi    sp, sp, -{frame}

    # Save every general register except x0 (always zero) and x2 (sp, which
    # changed above). `.irp` repeats the line for each n: x1 goes to offset
    # 8, x3 to 24, and so on, matching TrapFrame::x[n].
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*8(sp)
    .endr

    # Save the interrupted sp: the current sp plus the frame we pushed.
    addi    t0, sp, {frame}
    sd      t0, 2*8(sp)

    # Save the trap's return address and the status register, which the
    # handler may change (to skip an instruction) and which a nested trap
    # would overwrite.
    csrr    t0, sepc
    sd      t0, 32*8(sp)
    csrr    t0, sstatus
    sd      t0, 33*8(sp)

    # trap_handler(&mut TrapFrame)
    mv      a0, sp
    call    trap_handler

    # Restore in reverse: sepc and sstatus first, while t0 is still free.
    ld      t0, 32*8(sp)
    csrw    sepc, t0
    ld      t0, 33*8(sp)
    csrw    sstatus, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*8(sp)
    .endr

    # Pop the frame and return to sepc in the mode recorded in sstatus.SPP.
    addi    sp, sp, {frame}
    sret
"#,
    frame = const FRAME_SIZE,
);

unsafe extern "C" {
    fn trap_entry();
}

/// Point `stvec` at `trap_entry` (direct mode: all traps to one address).
pub fn init() {
    let entry = trap_entry as *const () as usize;
    // SAFETY: trap_entry saves and restores all state it touches.
    unsafe { stvec::write(stvec::Stvec::new(entry, stvec::TrapMode::Direct)) };
}

#[unsafe(no_mangle)]
extern "C" fn trap_handler(frame: &mut TrapFrame) {
    let cause = scause::read().cause();
    match cause.try_into::<Interrupt, Exception>() {
        Ok(Trap::Interrupt(Interrupt::SupervisorTimer)) => timer::on_interrupt(),
        Ok(Trap::Exception(Exception::Breakpoint)) => {
            crate::debug!("breakpoint at {:#x}", frame.sepc);
            // Resume after the ebreak. This is the standard way to continue
            // from a breakpoint: sepc points at the ebreak itself.
            frame.sepc += instruction_len(frame.sepc);
        }
        _ => panic!(
            "unhandled trap {cause:?} at sepc {:#x}, stval {:#x}",
            frame.sepc,
            stval::read()
        ),
    }
}

/// Length of the instruction at `pc`: 2 if compressed, else 4.
fn instruction_len(pc: usize) -> usize {
    // SAFETY: pc is the address of an instruction that just trapped, in
    // kernel text, which is mapped and readable, so the load can't fault.
    // (Rust assumes loads don't trap; a faulting load here would re-enter
    // this handler in the middle of Rust code that doesn't expect it.)
    let low = unsafe { core::ptr::read(pc as *const u16) };
    if low & 0b11 == 0b11 { 4 } else { 2 }
}

/// Trap into the handler and come back; used as a boot check.
pub fn breakpoint() {
    // SAFETY: the handler skips the ebreak and returns.
    unsafe { riscv::asm::ebreak() };
}
