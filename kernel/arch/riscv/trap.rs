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

/// One saved register. Without CHERI it is a word. With the `cheri`
/// feature it is a capability's 16 bytes: the address, which is all the
/// kernel's Rust code reads or writes, then the metadata. The tag lives in
/// tag memory: the trap code saves and restores whole capabilities, and
/// any write from Rust clears the tag, so a value the kernel sets reaches
/// user mode as an integer.
#[repr(C)]
#[cfg_attr(feature = "cheri", repr(align(16)))]
#[derive(Default, Clone, Copy)]
pub struct Reg {
    pub value: usize,
    #[cfg(feature = "cheri")]
    meta: usize,
}

impl Reg {
    /// An integer: no capability metadata.
    pub const fn new(value: usize) -> Self {
        Self {
            value,
            #[cfg(feature = "cheri")]
            meta: 0,
        }
    }
}

/// Bytes per saved register.
const SLOT: usize = core::mem::size_of::<Reg>();

/// Saved registers. A thread's user registers live in its TCB in this form;
/// traps from the kernel push one on the kernel stack. The layout must match
/// the assembly below: register `n` at `n * SLOT`, then `sepc`, then
/// `sstatus`.
#[repr(C)]
#[derive(Default)]
pub struct UserContext {
    /// x0..x31; x[2] holds the interrupted sp.
    pub x: [Reg; 32],
    /// The interrupted pc (with CHERI, the program counter capability).
    /// Changing its value keeps the metadata, and the return path rebuilds
    /// the tag from it (`return_to_user`).
    pub sepc: Reg,
    pub sstatus: Reg,
}

const CONTEXT_SIZE: usize = core::mem::size_of::<UserContext>();
const _: () = assert!(CONTEXT_SIZE == 34 * SLOT);

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
        context.x[2] = Reg::new(sp);
        context.x[A0] = Reg::new(arg);
        context.sepc = Reg::new(entry);
        context.reset_status();
        context
    }

    pub fn syscall_number(&self) -> usize {
        self.x[A7].value
    }

    /// Syscall argument `i` (0..6).
    pub fn arg(&self, i: usize) -> usize {
        self.x[A0 + i].value
    }

    /// Set syscall result `i` (0..6), returned in the argument registers.
    pub fn set_result(&mut self, i: usize, value: usize) {
        self.x[A0 + i] = Reg::new(value);
    }

    /// Make the interrupted system call run again when the thread resumes:
    /// the trap handler moved `sepc` past the 4-byte `ecall`, so move it
    /// back.
    pub fn restart_syscall(&mut self) {
        self.sepc.value -= 4;
    }

    /// Register `i` in `sap_abi::reg` order: pc, ra, sp, gp, tp, a0..a7.
    pub fn user_reg(&self, i: usize) -> usize {
        match i {
            0 => self.sepc.value,
            _ => self.x[Self::reg_index(i)].value,
        }
    }

    pub fn set_user_reg(&mut self, i: usize, value: usize) {
        match i {
            // The pc keeps its metadata, as for a syscall's step past ecall.
            0 => self.sepc.value = value,
            _ => self.x[Self::reg_index(i)] = Reg::new(value),
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
        self.sstatus = Reg::new((sstatus & !(SSTATUS_SPP | SSTATUS_FS)) | SSTATUS_SPIE);
    }
}

// RVY instructions, as raw encodings since Rust's assembler doesn't know
// them: switch to capability or integer pointer mode; load and store a
// whole capability; set a capability's address; read its tag; copy it; and
// rebuild a capability's tag from an authorising one.
#[cfg(feature = "cheri")]
global_asm!(
    r#"
    .macro YMODESWY
    .insn 4, 0x5600007b
    .endm
    .macro YMODESWI
    .insn 4, 0x5610007b
    .endm
    .macro LY rd, off, base
    .insn i 0x7b, 1, \rd, \off(\base)
    .endm
    .macro SY rs, off, base
    .insn s 0x7b, 2, \rs, \off(\base)
    .endm
    .macro YADDRW cd, cs, rs
    .insn r 0x7b, 0, 0x0b, \cd, \cs, \rs
    .endm
    .macro YTAGR rd, cs
    .insn r 0x7b, 0, 0x7a, \rd, \cs, x4
    .endm
    .macro YMV cd, cs
    .insn r 0x7b, 0, 0x03, \cd, \cs, x0
    .endm
    .macro YBLD cd, cs1, cs2
    .insn r 0x7b, 0, 0x0f, \cd, \cs1, \cs2
    .endm
    # The default data capability.
    .equ CSR_DDC, 0x03f
"#
);

global_asm!(
    r#"
    .section .text
    .balign 4                       # stvec requires 4-byte alignment
    .globl trap_entry
trap_entry:
    # From user mode, sscratch is the thread's UserContext: swap so sp points
    # at it and sscratch holds the user sp. From the kernel, sscratch is 0:
    # swap back, leaving sp unchanged and sscratch 0.
    #
    # With CHERI the swap happens in capability mode, where CSR accesses move
    # whole capabilities: in integer mode, reading sscratch would keep only
    # the user sp's address. Kernel registers hold no capabilities, so a
    # trap from the kernel goes back to integer mode at once.
.if {cheri}
    YMODESWY
.endif
    csrrw   sp, sscratch, sp
    bnez    sp, .Lfrom_user
    csrrw   sp, sscratch, sp
.if {cheri}
    YMODESWI
.endif

    # --- Trap from the kernel: save a UserContext on the current stack. ---
    addi    sp, sp, -{size}
    # Save every general register except x0 (always zero) and x2 (sp,
    # handled below). `.irp` repeats the line for each n: x1 goes to offset
    # 1*{slot}, x3 to 3*{slot}, and so on, matching UserContext::x[n].
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*{slot}(sp)
    .endr
    addi    t0, sp, {size}          # the interrupted sp
    sd      t0, 2*{slot}(sp)
    csrr    t0, sepc
    sd      t0, 32*{slot}(sp)
    csrr    t0, sstatus
    sd      t0, 33*{slot}(sp)

    mv      a0, sp
    call    kernel_trap

    # Return to the interrupted kernel code.
    ld      t0, 33*{slot}(sp)
    csrw    sstatus, t0
    ld      t0, 32*{slot}(sp)
    csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*{slot}(sp)
    .endr
    ld      sp, 2*{slot}(sp)
    sret

.Lfrom_user:
    # --- Trap from user mode: sp is the thread's UserContext. ---
    # With CHERI, still in capability mode: sp is a capability to the
    # context, and every register and the pc are saved whole.
.if {cheri}
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    SY      x\n, \n*{slot}, sp
    .endr
    # The user sp, parked in sscratch; leave sscratch 0 (kernel running).
    csrrw   t0, sscratch, zero
    SY      t0, 2*{slot}, sp
    csrr    t0, sepc
    SY      t0, 32*{slot}, sp
    csrr    t0, sstatus
    sd      t0, 33*{slot}(sp)
    YMODESWI
.else
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    sd      x\n, \n*{slot}(sp)
    .endr
    csrrw   t0, sscratch, zero
    sd      t0, 2*{slot}(sp)
    csrr    t0, sepc
    sd      t0, 32*{slot}(sp)
    csrr    t0, sstatus
    sd      t0, 33*{slot}(sp)
.endif

    # user_trap(&mut UserContext) on a fresh kernel stack. It never returns;
    # it ends in return_to_user.
    mv      a0, sp
    lla     sp, __boot_stack_top
    call    user_trap
1:  j       1b

    # return_to_user(context: *mut UserContext) -> !
    .globl return_to_user
return_to_user:
.if {cheri}
    # In capability mode, with a capability to the context made from DDC
    # (the kernel's is the root): from now until the next trap, a trap
    # comes from this thread.
    # TODO: bounded capabilities for the context and for user code and data,
    # rather than the root (needed once processes are compartments).
    YMODESWY
    csrr    t1, CSR_DDC
    YADDRW  t1, t1, a0
    csrw    sscratch, t1
    YMV     sp, t1
    ld      t0, 33*{slot}(sp)
    csrw    sstatus, t0
    # The saved pc. If kernel code changed its address, the tag is gone:
    # rebuild it from the metadata, with DDC as the authority; a thread
    # the kernel set up has no metadata, and gets DDC's.
    LY      t0, 32*{slot}, sp
    YTAGR   t2, t0
    bnez    t2, 2f
    ld      t2, 32*{slot}+8(sp)
    bnez    t2, 3f
    YADDRW  t0, t1, t0
    j       2f
3:  YBLD    t0, t1, t0
2:  csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    LY      x\n, \n*{slot}, sp
    .endr
    # The user sp last, since sp addresses the context until now.
    LY      sp, 2*{slot}, sp
    sret
.else
    # From now until the next trap, a trap comes from this thread.
    csrw    sscratch, a0
    mv      sp, a0
    ld      t0, 33*{slot}(sp)
    csrw    sstatus, t0
    ld      t0, 32*{slot}(sp)
    csrw    sepc, t0
    .irp n, 1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    ld      x\n, \n*{slot}(sp)
    .endr
    # The user sp last, since sp addresses the context until now.
    ld      sp, 2*{slot}(sp)
    sret
.endif
"#,
    size = const CONTEXT_SIZE,
    slot = const SLOT,
    cheri = const cfg!(feature = "cheri") as usize,
);

unsafe extern "C" {
    fn trap_entry();
    fn return_to_user(context: *mut UserContext) -> !;
}

/// Point `stvec` at `trap_entry` (direct mode: all traps to one address),
/// and mark the kernel as running (`sscratch` = 0; firmware may leave it
/// set).
pub fn init() {
    #[cfg(feature = "cheri")]
    enable_cheri();
    // SAFETY: nothing else uses sscratch; 0 means "the kernel is running".
    unsafe { riscv::register::sscratch::write(0) };
    let entry = trap_entry as *const () as usize;
    // SAFETY: trap_entry saves and restores all state it touches.
    unsafe { stvec::write(stvec::Stvec::new(entry, stvec::TrapMode::Direct)) };
}

/// `senvcfg.Y`: CHERI instructions and capability state in user mode
/// (bit 9 since RVY 0.9.9).
#[cfg(feature = "cheri")]
const SENVCFG_Y: usize = 1 << 9;

/// Let user mode use CHERI. The bit only sticks if the firmware turned
/// CHERI on for supervisor mode (`menvcfg.Y`), so it doubles as the check
/// that this kernel, which saves capabilities, runs on a CHERI hart.
#[cfg(feature = "cheri")]
fn enable_cheri() {
    let senvcfg: usize;
    // SAFETY: setting senvcfg.Y changes only what user mode may do.
    unsafe {
        core::arch::asm!("csrs senvcfg, {y}", "csrr {v}, senvcfg", y = in(reg) SENVCFG_Y, v = out(reg) senvcfg);
    }
    assert!(senvcfg & SENVCFG_Y != 0, "a kernel built with the cheri feature needs CHERI (RVY) hardware and firmware that enables it");
    crate::info!("trap: CHERI on for user mode; user capabilities are saved whole");
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
            context.sepc.value += 4;
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
            let fault = crate::fault::Fault { kind, pc: context.sepc.value, address: stval::read(), cause: scause::read().code() };
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
            crate::debug!("breakpoint at {:#x}", frame.sepc.value);
            // Resume after the ebreak. This is the standard way to continue
            // from a breakpoint: sepc points at the ebreak itself.
            frame.sepc.value += instruction_len(frame.sepc.value);
        }
        _ => panic!(
            "unhandled trap {cause:?} at sepc {:#x}, stval {:#x}",
            frame.sepc.value,
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
