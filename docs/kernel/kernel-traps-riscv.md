---
status: implemented
summary: How the kernel takes traps on RISC-V today - one direct-mode vector that saves user registers straight into the current thread's TCB (found through sscratch) and kernel-mode traps on the stack, separate Rust handlers for each, a return path that resumes any thread, and periodic timer interrupts through the SBI TIME extension.
updated: 2026-10-13
code: [kernel/arch/riscv/trap.rs, kernel/arch/riscv/timer.rs, kernel/arch/riscv/sbi.rs, kernel/arch/riscv/mod.rs, kernel/main.rs, kernel/syscall.rs, kernel/sched.rs]
verified: a7e19d3
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Traps and timer on RISC-V

Part of roadmap milestone M2 ([[roadmap]]): the kernel takes traps from itself and from user mode, handles them, and returns, and it receives periodic timer interrupts. CSR access and trap causes come from the `riscv` crate.

## Trap entry and return

`trap::init` (`kernel/arch/riscv/trap.rs:220`) sets `sscratch` to 0 (firmware may leave it set) and writes `stvec` in direct mode: every trap enters `trap_entry` (line 135). Vectored mode would only skip one dispatch for interrupts, since exceptions, syscalls included, always use the base address, so direct mode stays.

`sscratch` tells the entry code where a trap came from. While a user thread runs it holds the address of that thread's saved registers, the `UserContext` (`trap.rs:36`) at the start of its thread control block ([[kernel-threads]]); while the kernel runs it is 0. `trap_entry` swaps it with `sp` and branches on the result.

**From user mode** (`.Lfrom_user`, `trap.rs:172`): `sp` now points at the thread's `UserContext`. The code saves x1 and x3–x31 there, the user stack pointer from `sscratch` (line 171, which also resets `sscratch` to 0), then `sepc` and `sstatus`. It switches to the top of the kernel stack (line 188) and calls `user_trap`, which never returns. Because user registers live in the TCB, the kernel stack holds nothing between entries, and one stack serves every thread, as in seL4.

**From the kernel**: the code pushes a `UserContext` on the current stack, calls `kernel_trap`, restores, and returns to the interrupted kernel code. This happens in the idle loop and the boot checks, the only places the kernel enables interrupts, and on kernel bugs.

**Returning to user mode** (`return_to_user`, `trap.rs:194`): given any thread's `UserContext`, it sets `sscratch` to it (line 196), so the thread's next trap saves into its own TCB, then restores `sstatus`, `sepc` and all registers, the stack pointer last, and executes `sret`. The scheduler calls it, through `arch::return_to_user`, to resume whichever thread it picks.

`UserContext` is 34 words, checked by a compile-time assertion against the assembly offsets, and the assembly is commented line by line. It also gives generic code an architecture-neutral view of syscalls: `syscall_number` (`a7`), `arg(i)` and `set_result(i)` (`a0`–`a5`) (lines 76–86). `UserContext::new_user` (line 67) builds a new thread's registers: user mode, interrupts on after `sret`, and the FPU off. A `TODO` there notes the kernel doesn't save floating-point registers on a switch, so a thread that uses the FPU faults until lazy FP save and restore exists.

On trap entry the hardware clears `sstatus.SIE`, so kernel code always runs with interrupts disabled; it is never preempted. In user mode, supervisor interrupts are always taken.

## Dispatch

`user_trap` (`trap.rs:236`) handles traps from user mode, then calls the scheduler, which picks the thread to resume ([[kernel-threads#Scheduler]]):

| Cause | Handling |
|---|---|
| Supervisor timer interrupt | `timer::on_interrupt`, which also counts the tick against the current thread's time slice |
| Environment call | syscall: `sepc += 4`, then `syscall::handle` with the thread's registers |
| Anything else | a user fault, delivered to the thread's fault handler over IPC ([[kernel-ipc#Fault delivery]]) |


`kernel_trap` (`trap.rs:267`) handles the timer, breakpoints (logged, and `sepc` advanced past the `ebreak` using the instruction's real length, since `ebreak` may be the 2-byte `c.ebreak`), and panics on anything else.

## Timer

`timer::init` (`kernel/arch/riscv/timer.rs:20`) records the timer frequency from the boot information ([[kernel-boot-riscv]]). `kmain` starts the timer at 100 Hz (`TICK_HZ`, `kernel/main.rs:45`) and it runs from then on: `timer::start(hz)` (`timer.rs:33`) computes the interval in timer ticks, programs the first deadline with the SBI TIME extension's `set_timer` (`kernel/arch/riscv/sbi.rs:50`), and enables the supervisor timer interrupt in `sie`.

Each interrupt increments a tick counter, programs the next deadline, which also clears the pending interrupt, and calls `sched::tick` (`timer.rs:56`). `now` (`timer.rs:16`) reads the `time` CSR, which every application-class core has (Zicntr). Deadlines passed to `set_timer` are absolute values of the same counter, so the two always agree.

`set_timer` traps into firmware on every tick. A `TODO` at `sbi.rs` marks using the Sstc extension, which lets the kernel write `stimecmp` directly, when the hardware has it ([[hw-isa#Handling extension variation]]).

## Boot check

`boot_checks` (`kernel/main.rs:48`) triggers an `ebreak` and confirms execution resumes, then waits in the idle loop (`arch::idle_once`, `kernel/arch/riscv/mod.rs:95`) for ten ticks, which takes about 100 ms.

> [!todo] Not built
> Floating-point state save and restore; external interrupts (PLIC); fault delivery over IPC.
