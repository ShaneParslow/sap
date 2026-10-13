---
status: implemented
summary: Threads and scheduling as implemented today - the thread control block (saved user registers, state, priority, address space), seL4-style fixed-priority round-robin scheduling with time slices counted in timer ticks, the idle loop, and how kernel globals are kept without locks on one hart.
updated: 2026-10-13
code: [kernel/thread.rs, kernel/sched.rs, kernel/global.rs, kernel/arch/riscv/trap.rs, kernel/arch/riscv/mod.rs, kernel/root_task.rs, kernel/main.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Threads and scheduling

The first part of roadmap milestone M3 ([[roadmap]]): user threads are kernel objects with their own saved state, and the kernel switches between them. Only the root task exists so far; thread capabilities and invocations that create more threads come later in M3. The design follows seL4 without its MCS extensions ([[kernel-scheduling]] has the target design with scheduling contexts).

## Thread control block

A `Tcb` (`kernel/thread.rs:26`) occupies a block of `1 << TCB_BITS` = 1 KiB (line 12, as in seL4 on RISC-V; a compile-time assertion checks it fits). It holds:
- `context`: the thread's saved user registers, first, so the trap entry code finds them at the TCB's own address ([[kernel-traps-riscv#Trap entry and return]]);
- `state`: `Inactive` (never started or suspended) or `Running`, meaning runnable, whether or not it is the thread on the CPU;
- `priority`: 0–255, with 255 the highest (the root task's);
- `vspace_root`: the physical address of the root page table of its address space;
- the links of its ready queue.

A `TODO` on `vspace_root` says to replace it with a capability to the address space held in the TCB, so deleting the address space stops the thread, as seL4 does.

`Tcb::init` (line 52) writes an inactive TCB into memory the caller owns. Today the only caller is the root task's loader, which places the TCB in a frame from the boot allocator ([[kernel-root-task]]); later, TCBs are retyped from untyped memory.

## Scheduler

`kernel/sched.rs` keeps 256 ready queues, one per priority, and a bitmap of the non-empty ones (lines 13–32). **Every runnable thread is in its queue, including the running one.** The thread that runs is the head of the highest non-empty queue (`highest`, line 106). That keeps the rules short:
- `make_runnable` (line 49) marks a thread `Running` and appends it to its queue;
- `tick` (line 59), on every timer interrupt, counts down the current thread's time slice, five ticks or 50 ms (line 15), and when it runs out moves the thread to the tail of its queue;
- `yield_current` (line 71), the `YIELD` syscall, moves it to the tail at once.

Moving a thread to the tail of its own queue is the whole of preemption and round robin. A higher-priority thread always runs first; threads at one priority take turns by time slice.

`run` (line 88) ends every entry from user mode. It picks the highest-priority thread, records it as current (a new thread gets a fresh time slice), switches to its address space if that isn't already active (`arch::activate_vspace`), and resumes it in user mode. If no thread is runnable it runs the idle loop.

## Idle loop

`arch::idle_once` (`kernel/arch/riscv/mod.rs:59`) enables interrupts, waits with `wfi`, handles whatever arrives through the kernel-trap path, and disables interrupts again. `run` repeats it until a thread is runnable. This and the boot checks are the only places the kernel runs with interrupts enabled; seL4 uses an idle thread for the same purpose.

## Kernel globals

Scheduler state is a `Global` (`kernel/global.rs:13`), a cell that hands out `&mut` without locking. That is sound because there is one hart, kernel code runs with interrupts disabled, and the idle loop touches no globals, so kernel code is never re-entered while it holds a reference. A `TODO` there says to take a big kernel lock on every entry once more harts run, as seL4 does.

## Syscalls

`sap-abi` (`abi/lib.rs`), a crate shared by the kernel and user programs, defines the syscall numbers, so the two sides can't drift. The kernel dispatches in `syscall::handle` (`kernel/syscall.rs:17`): `YIELD`, and the two debug calls `DEBUG_PUTCHAR` and `DEBUG_HALT`. `CALL`, the capability invocation, is reserved and returns an error until capability spaces exist.

> [!todo] Not built
> Thread capabilities and TCB invocations (configure, write registers, resume, suspend, set priority); blocking states for IPC (M4); scheduling contexts and donation; per-thread ASIDs; floating-point state; multiple harts.
