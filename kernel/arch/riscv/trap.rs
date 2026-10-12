//! Supervisor trap entry and dispatch, and the first entry to user mode.
//!
//! `sscratch` tells the entry code where the trap came from: it holds the
//! kernel stack top while a user thread runs, and 0 while the kernel runs.
//! A trap from user mode swaps it with `sp`, so the frame goes on the kernel
//! stack; a trap from the kernel finds 0 and stays on the current stack.
//!
//! TODO: there is one kernel stack (the boot stack), so only one user thread
//! can exist. Per-thread kernel stacks or per-thread saved contexts come
//! with threads as kernel objects (M3).
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

/// `sstatus.SPP`: the privilege the trap came from (set = supervisor).
const SSTATUS_SPP: usize = 1 << 8;
/// `sstatus.SPIE`: interrupts are enabled after `sret`.
const SSTATUS_SPIE: usize = 1 << 5;
/// `sstatus.FS` = Initial: user code may use the FPU.
const SSTATUS_FS_INITIAL: usize = 1 << 13;

global_asm!(
    r#"
    .section .text
    .balign 4                       # stvec requires 4-byte alignment
    .globl trap_entry
trap_entry:
    # From user mode, sscratch is the kernel stack top: swap so sp is on the
    # kernel stack and sscratch holds the user sp. From the kernel,
    # sscratch is 0: swap back, leaving sp unchanged and sscratch 0.
    csrrw   sp, sscratch, sp
    bnez    sp, 1f
    csrrw   sp, sscratch, sp
1:
    # Make room for a TrapFrame.
    addi    sp, sp, -{frame}

    # Save every general register except x0 (always zero) and x2 (sp,
    # handled below). `.irp` repeats the line for each n: x1 goes to offset
    # 8, x3 to 24, and so on, matching TrapFrame::x[n].
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*8(sp)
    .endr

    # The interrupted sp: the user sp parked in sscratch, or, from the
    # kernel, the current sp plus the frame. Leave sscratch 0: the kernel is
    # running now.
    csrrw   t0, sscratch, zero
    bnez    t0, 2f
    addi    t0, sp, {frame}
2:  sd      t0, 2*8(sp)

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

    # Return path, also used to enter user mode the first time (sp points
    # at a prepared frame).
    .globl trap_return
trap_return:
    # Restore sstatus. If returning to user mode (SPP clear), set sscratch
    # to the kernel stack top so the next trap from user mode lands there.
    ld      t0, 33*8(sp)
    csrw    sstatus, t0
    andi    t1, t0, {spp}
    bnez    t1, 3f
    addi    t1, sp, {frame}
    csrw    sscratch, t1
3:
    ld      t0, 32*8(sp)
    csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*8(sp)
    .endr

    # Restore the interrupted sp last (user or kernel), then return to sepc
    # in the mode recorded in sstatus.SPP.
    ld      sp, 2*8(sp)
    sret
"#,
    frame = const FRAME_SIZE,
    spp = const SSTATUS_SPP,
);

unsafe extern "C" {
    fn trap_entry();
}

/// Point `stvec` at `trap_entry` (direct mode: all traps to one address),
/// and mark the kernel as running (`sscratch` = 0; firmware may leave it
/// set).
pub fn init() {
    // SAFETY: nothing else uses sscratch; 0 means "the kernel is running".
    unsafe { riscv::register::sscratch::write(0) };
    let entry = trap_entry as *const () as usize;
    // SAFETY: trap_entry saves and restores all state it touches.
    unsafe { stvec::write(stvec::Stvec::new(entry, stvec::TrapMode::Direct)) };
}

#[unsafe(no_mangle)]
extern "C" fn trap_handler(frame: &mut TrapFrame) {
    let cause = scause::read().cause();
    let from_user = frame.sstatus & SSTATUS_SPP == 0;
    match cause.try_into::<Interrupt, Exception>() {
        Ok(Trap::Interrupt(Interrupt::SupervisorTimer)) => timer::on_interrupt(),
        Ok(Trap::Exception(Exception::UserEnvCall)) => {
            // Return past the ecall (always 4 bytes) with the result in a0.
            frame.sepc += 4;
            let args = [frame.x[10], frame.x[11], frame.x[12], frame.x[13], frame.x[14], frame.x[15]];
            frame.x[10] = crate::syscall::handle(frame.x[17], args);
        }
        Ok(Trap::Exception(Exception::Breakpoint)) if !from_user => {
            crate::debug!("breakpoint at {:#x}", frame.sepc);
            // Resume after the ebreak. This is the standard way to continue
            // from a breakpoint: sepc points at the ebreak itself.
            frame.sepc += instruction_len(frame.sepc);
        }
        _ if from_user => {
            // TODO: deliver user faults to the thread's fault handler over
            // IPC, as seL4 does (M3/M4). For now the only user thread is the
            // root task, and a fault in it ends the boot.
            crate::error!(
                "user fault: {cause:?} at pc {:#x}, address {:#x}",
                frame.sepc,
                stval::read()
            );
            super::shutdown(true)
        }
        _ => panic!(
            "unhandled trap {cause:?} at sepc {:#x}, stval {:#x}",
            frame.sepc,
            stval::read()
        ),
    }
}

unsafe extern "C" {
    static __boot_stack_top: u8;
}

/// Enter user mode at `entry` with stack pointer `sp`, in the address space
/// that is currently active. Never returns: later traps from user mode start
/// again at the top of the kernel stack.
pub fn enter_user(entry: usize, sp: usize) -> ! {
    let stack_top = &raw const __boot_stack_top as usize;
    let frame = (stack_top - FRAME_SIZE) as *mut TrapFrame;
    let sstatus = riscv::register::sstatus::read().bits();
    let user = TrapFrame {
        x: {
            let mut x = [0; 32];
            x[2] = sp;
            x
        },
        sepc: entry,
        sstatus: (sstatus & !SSTATUS_SPP) | SSTATUS_SPIE | SSTATUS_FS_INITIAL,
    };
    // SAFETY: the frame sits at the top of the kernel stack, below which
    // nothing live remains once we jump; trap_return restores it and srets.
    unsafe {
        frame.write(user);
        core::arch::asm!("mv sp, {0}", "j trap_return", in(reg) frame, options(noreturn));
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
