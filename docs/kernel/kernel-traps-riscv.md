---
status: implemented
summary: How the kernel takes traps on RISC-V today - one direct-mode vector that switches to the kernel stack for traps from user mode via sscratch, a shared return path that also enters user mode, a Rust dispatcher for the timer, breakpoints, syscalls and user faults, and periodic timer interrupts through the SBI TIME extension.
updated: 2026-10-11
verified: 1be5924
code: [kernel/arch/riscv/trap.rs, kernel/arch/riscv/timer.rs, kernel/arch/riscv/sbi.rs, kernel/arch/riscv/mod.rs, kernel/main.rs, kernel/syscall.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Traps and timer on RISC-V

Part of roadmap milestone M2 ([[roadmap]]): the kernel takes traps from itself and from user mode, handles them, and returns, and it receives periodic timer interrupts. CSR access and trap causes come from the `riscv` crate.

## Trap entry and return

`trap::init` (`kernel/arch/riscv/trap.rs:122`) sets `sscratch` to 0 (firmware may leave it set) and writes `stvec` in direct mode: every trap enters `trap_entry`. Vectored mode would only skip one dispatch for interrupts, since exceptions, syscalls included, always use the base address, so direct mode stays.

**Where the frame goes.** `sscratch` holds the kernel stack top while user code runs and 0 while the kernel runs. `trap_entry` (`trap.rs:49`) swaps it with `sp` (line 53). Nonzero means the trap came from user mode, and `sp` is now on the kernel stack. Zero means the kernel was running, and swapping back (line 55) leaves `sp` unchanged.

**Saving.** It pushes a `TrapFrame` (`trap.rs:27`): registers x1 and x3–x31, the interrupted stack pointer in the x2 slot (from `sscratch` for a user trap, otherwise the current `sp` plus the frame), then `sepc` and `sstatus`. `sscratch` is left 0 while the kernel runs. The frame is 34 words, and a compile-time assertion checks the Rust layout against the assembly offsets. The assembly is commented line by line.

**Returning** (`trap_return`, `trap.rs:90`). It restores `sstatus`. If the trap is returning to user mode (`sstatus.SPP` clear), it first sets `sscratch` to the kernel stack top (line 98), so the next user trap lands there. Then it restores `sepc` and the registers, the stack pointer last, and executes `sret`.

The same return path enters user mode the first time: `enter_user` (`trap.rs:174`) builds a frame at the top of the kernel stack and jumps to `trap_return` ([[kernel-root-task]]).

On trap entry the hardware clears `sstatus.SIE`, so the handler runs with interrupts disabled; `sret` restores them from `SPIE`. In user mode, supervisor interrupts are always taken.

A `TODO` notes there is one kernel stack, the boot stack, so only one user thread can exist; per-thread kernel stacks or saved contexts arrive with threads as kernel objects (M3).

## Dispatch

`trap_handler` (`trap.rs:131`) reads `scause` through the `riscv` crate and matches on its supervisor `Interrupt` and `Exception` enums:

| Cause | From | Handling |
|---|---|---|
| Supervisor timer interrupt | either | `timer::on_interrupt` |
| Environment call from user mode | user | syscall: `sepc += 4`, number from `a7`, arguments `a0`–`a5`, result in `a0` (`kernel/syscall.rs`) |
| Breakpoint | kernel | logged at debug level; `sepc` advanced past the `ebreak` |
| Anything else | user | logged as a user fault, then shutdown with failure |
| Anything else | kernel | panic with cause, `sepc` and `stval` |

Skipping a breakpoint uses the instruction's real length (`instruction_len`, `trap.rs:196`), since `ebreak` may be the 2-byte compressed `c.ebreak`; the low two bits of the first halfword are `11` only for 4-byte instructions. A user fault ends the boot because the root task is the only user thread; a `TODO` marks delivering faults to a handler over IPC, as seL4 does.

## Timer

`timer::init` (`kernel/arch/riscv/timer.rs:20`) records the timer frequency from the boot information ([[kernel-boot-riscv]]). `timer::start(hz)` (`timer.rs:33`) computes the interval in timer ticks, programs the first deadline with the SBI TIME extension's `set_timer` (`kernel/arch/riscv/sbi.rs:50`), and enables the supervisor timer interrupt in `sie`. Interrupts are taken in the kernel only after `enable_interrupts` (`kernel/arch/riscv/mod.rs:81`) sets `sstatus.SIE`.

Each interrupt increments a tick counter and programs the next deadline, which also clears the pending interrupt. `now` (`timer.rs:16`) reads the `time` CSR, which every application-class core has (Zicntr). Deadlines passed to `set_timer` are absolute values of the same counter, so the two always agree.

`set_timer` traps into firmware on every tick. A `TODO` at `sbi.rs` marks using the Sstc extension, which lets the kernel write `stimecmp` directly, when the hardware has it ([[hw-isa#Handling extension variation]]).

## Boot check

`boot_checks` (`kernel/main.rs:32`) triggers an `ebreak` and confirms execution resumes, then runs the timer at 100 Hz and waits in `wfi` for ten ticks, which takes about 100 ms. The timer keeps running while the root task runs, so user code is interrupted and resumed about every 10 ms.

> [!todo] Not built
> Per-thread kernel stacks or contexts; floating-point state save and restore (user code may use the FPU, and only one user thread exists); external interrupts (PLIC); fault delivery over IPC.
