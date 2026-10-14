//! Supervisor trap entry and dispatch, and return to user mode.
//!
//! `sscratch` tells the entry code where a trap came from. While a user
//! thread runs it holds the address of that thread's saved registers (the
//! `UserContext` at the start of its TCB); while the kernel runs it is 0.
//!
//! - **From user mode**, the entry code saves the user registers straight
//!   into the thread's TCB, as seL4 does, switches to the kernel stack and
//!   calls `user_trap`. The kernel stack holds nothing between entries, so
//!   one stack serves every thread.
//! - **From the kernel** (the idle loop, boot checks, or a kernel bug), it
//!   saves the registers on the current stack and calls `kernel_trap`,
//!   which returns to the interrupted code.
//!
//! The kernel runs with interrupts disabled except in the idle loop, so
//! kernel code is never preempted.
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

/// Saved registers. A thread's user registers live in its TCB in this form;
/// traps from the kernel push one on the kernel stack. The layout must match
/// the assembly below.
#[repr(C)]
#[derive(Default)]
pub struct UserContext {
    /// x0..x31; x[2] holds the interrupted sp.
    pub x: [usize; 32],
    pub sepc: usize,
    pub sstatus: usize,
}

const CONTEXT_SIZE: usize = core::mem::size_of::<UserContext>();
const _: () = assert!(CONTEXT_SIZE == 34 * 8);

/// `sstatus.SPP`: the privilege the trap came from (set = supervisor).
const SSTATUS_SPP: usize = 1 << 8;
/// `sstatus.SPIE`: interrupts are enabled after `sret`.
const SSTATUS_SPIE: usize = 1 << 5;
/// `sstatus.FS`: the floating-point unit's state. Off makes any FP
/// instruction trap.
const SSTATUS_FS: usize = 0b11 << 13;

/// Registers holding syscall arguments and results: a0..a5.
const A0: usize = 10;
/// Register holding the syscall number: a7.
const A7: usize = 17;

impl UserContext {
    /// Registers for a thread that starts at `entry` with stack `sp` and
    /// `arg` in its first argument register, in user mode with interrupts
    /// enabled.
    ///
    /// TODO: the FPU is off for user threads, since the kernel doesn't save
    /// FP registers on a switch; a thread that uses it faults. Save and
    /// restore FP state (lazily, on first use) before user code needs it.
    pub fn new_user(entry: usize, sp: usize, arg: usize) -> Self {
        let mut context = Self::default();
        context.x[2] = sp;
        context.x[A0] = arg;
        context.sepc = entry;
        context.reset_status();
        context
    }

    pub fn syscall_number(&self) -> usize {
        self.x[A7]
    }

    /// Syscall argument `i` (0..6).
    pub fn arg(&self, i: usize) -> usize {
        self.x[A0 + i]
    }

    /// Set syscall result `i` (0..6), returned in the argument registers.
    pub fn set_result(&mut self, i: usize, value: usize) {
        self.x[A0 + i] = value;
    }

    /// Make the interrupted system call run again when the thread resumes:
    /// the trap handler moved `sepc` past the 4-byte `ecall`, so move it
    /// back.
    pub fn restart_syscall(&mut self) {
        self.sepc -= 4;
    }

    /// Register `i` in `sap_abi::reg` order: pc, ra, sp, gp, tp, a0..a7.
    pub fn user_reg(&self, i: usize) -> usize {
        match i {
            0 => self.sepc,
            _ => self.x[Self::reg_index(i)],
        }
    }

    pub fn set_user_reg(&mut self, i: usize, value: usize) {
        match i {
            0 => self.sepc = value,
            _ => self.x[Self::reg_index(i)] = value,
        }
    }

    /// The `x` index of register `i` (1..13) in `sap_abi::reg` order.
    fn reg_index(i: usize) -> usize {
        match i {
            1..=4 => i,           // ra, sp, gp, tp are x1..x4
            5..=12 => A0 + i - 5, // a0..a7 are x10..x17
            _ => panic!("no register {i}"),
        }
    }

    /// Reset the status bits to a fresh user thread's, keeping nothing the
    /// thread could have set: user mode, interrupts on after `sret`, FPU
    /// off.
    pub fn reset_status(&mut self) {
        let sstatus = riscv::register::sstatus::read().bits();
        self.sstatus = (sstatus & !(SSTATUS_SPP | SSTATUS_FS)) | SSTATUS_SPIE;
    }
}

global_asm!(
    r#"
    .section .text
    .balign 4                       # stvec requires 4-byte alignment
    .globl trap_entry
trap_entry:
    # From user mode, sscratch is the thread's UserContext: swap so sp points
    # at it and sscratch holds the user sp. From the kernel, sscratch is 0:
    # swap back, leaving sp unchanged and sscratch 0.
    csrrw   sp, sscratch, sp
    bnez    sp, .Lfrom_user
    csrrw   sp, sscratch, sp

    # --- Trap from the kernel: save a UserContext on the current stack. ---
    addi    sp, sp, -{size}
    # Save every general register except x0 (always zero) and x2 (sp,
    # handled below). `.irp` repeats the line for each n: x1 goes to offset
    # 8, x3 to 24, and so on, matching UserContext::x[n].
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*8(sp)
    .endr
    addi    t0, sp, {size}          # the interrupted sp
    sd      t0, 2*8(sp)
    csrr    t0, sepc
    sd      t0, 32*8(sp)
    csrr    t0, sstatus
    sd      t0, 33*8(sp)

    mv      a0, sp
    call    kernel_trap

    # Return to the interrupted kernel code.
    ld      t0, 33*8(sp)
    csrw    sstatus, t0
    ld      t0, 32*8(sp)
    csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*8(sp)
    .endr
    ld      sp, 2*8(sp)
    sret

.Lfrom_user:
    # --- Trap from user mode: sp is the thread's UserContext. ---
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*8(sp)
    .endr
    # The user sp, parked in sscratch; leave sscratch 0 (kernel running).
    csrrw   t0, sscratch, zero
    sd      t0, 2*8(sp)
    csrr    t0, sepc
    sd      t0, 32*8(sp)
    csrr    t0, sstatus
    sd      t0, 33*8(sp)

    # user_trap(&mut UserContext) on a fresh kernel stack. It never returns;
    # it ends in return_to_user.
    mv      a0, sp
    lla     sp, __boot_stack_top
    call    user_trap
1:  j       1b

    # return_to_user(context: *mut UserContext) -> !
    .globl return_to_user
return_to_user:
    # From now until the next trap, a trap comes from this thread.
    csrw    sscratch, a0
    mv      sp, a0
    ld      t0, 33*8(sp)
    csrw    sstatus, t0
    ld      t0, 32*8(sp)
    csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*8(sp)
    .endr
    # The user sp last, since sp addresses the context until now.
    ld      sp, 2*8(sp)
    sret
"#,
    size = const CONTEXT_SIZE,
);

unsafe extern "C" {
    fn trap_entry();
    fn return_to_user(context: *mut UserContext) -> !;
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

/// Resume user mode with the registers in `context`.
pub fn resume_user(context: &mut UserContext) -> ! {
    // SAFETY: `context` lives in a TCB, which outlives the thread running;
    // return_to_user restores every register from it and srets.
    unsafe { return_to_user(context) }
}

#[unsafe(no_mangle)]
extern "C" fn user_trap(context: &mut UserContext) -> ! {
    let cause = scause::read().cause();
    match cause.try_into::<Interrupt, Exception>() {
        Ok(Trap::Interrupt(Interrupt::SupervisorTimer)) => timer::on_interrupt(),
        Ok(Trap::Exception(Exception::UserEnvCall)) => {
            // Return past the ecall, which is always 4 bytes.
            context.sepc += 4;
            crate::syscall::handle(context);
        }
        other => {
            let kind = match other {
                Ok(Trap::Exception(
                    Exception::InstructionPageFault
                    | Exception::LoadPageFault
                    | Exception::StorePageFault
                    | Exception::InstructionFault
                    | Exception::LoadFault
                    | Exception::StoreFault,
                )) => sap_abi::fault::VM,
                _ => sap_abi::fault::USER_EXCEPTION,
            };
            let fault = crate::fault::Fault { kind, pc: context.sepc, address: stval::read(), cause: scause::read().code() };
            // SAFETY: the current thread is live and running; `context` (its
            // registers) isn't used after this.
            unsafe { crate::fault::deliver(crate::sched::current(), fault) };
        }
    }
    crate::sched::run()
}

#[unsafe(no_mangle)]
extern "C" fn kernel_trap(frame: &mut UserContext) {
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
