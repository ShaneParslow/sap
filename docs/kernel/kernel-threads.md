---
status: implemented
summary: Threads and scheduling as implemented today - the thread control block (saved user registers, its capability slots, state, priorities), the thread invocations that configure, start, stop and inspect threads, seL4-style fixed-priority round-robin scheduling with time slices, the idle loop, and lock-free kernel globals on one hart.
updated: 2026-10-13
code: [kernel/thread.rs, kernel/sched.rs, kernel/global.rs, kernel/syscall.rs, kernel/invocation.rs, user/libsap/tcb.rs, user/init/main.rs, kernel/arch/riscv/trap.rs, kernel/arch/riscv/mod.rs, kernel/arch/riscv/vspace.rs, kernel/root_task.rs, kernel/main.rs, abi/lib.rs]
verified: 7123956
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Threads and scheduling

Part of roadmap milestone M3 ([[roadmap]]): user threads are kernel objects with their own saved state, created by retyping untyped memory, configured and started through thread capabilities, and scheduled by priority. The design follows seL4 without its MCS extensions ([[kernel-scheduling]] has the target design with scheduling contexts).

## Thread control block

A `Tcb` (`kernel/thread.rs:52`) occupies a block of `1 << TCB_BITS` = 1 KiB (line 19, as in seL4 on RISC-V; a compile-time assertion checks it fits). It holds:
- `context`: the thread's saved user registers, first, so the trap entry code finds them at the TCB's own address ([[kernel-traps-riscv#Trap entry and return]]);
- four capability slots (lines 24–28): the root of its capability space, the root of its address space, its IPC buffer frame, and its fault-handler endpoint ([[kernel-ipc#Fault delivery]]). They hold copies, as in seL4, so deleting the address space or capability space also removes the thread's reference ([[kernel-capabilities]]);
- `state`: `Inactive` (never started, or suspended), `Running`, meaning runnable whether or not it is the thread on the CPU, or blocked on a notification, an endpoint or a reply ([[kernel-ipc#Blocking and cancellation]]), with the object it is blocked on and its links in that object's queue;
- `priority`: 0–255, with 255 the highest (the root task's);
- `mcp`, the maximum controlled priority: the highest priority this thread may grant when used as the authority for setting priorities, as in seL4; 0 for new threads, 255 for the root task;
- `ipc_buffer`: the user address of its IPC buffer, inside the frame in its slot;
- the links of its ready queue.

`Tcb::init` (line 99) zeroes the block, which is an inactive TCB with empty slots. The root task's TCB is placed in a frame from the boot allocator ([[kernel-root-task]]); every other TCB is retyped from untyped memory ([[kernel-capabilities#Retyping untyped memory]]). Deleting the last capability to a TCB takes the thread off the CPU and deletes its slots (`thread::finalise`, line 202).

The syscall path holds `&mut` to a thread's registers, which live inside its TCB, so `thread::cspace` and `thread::ipc_buffer` (lines 116, 125) read single fields through a raw pointer rather than borrowing the whole TCB.

## Thread invocations

A new TCB does nothing until it is configured and started through its capability (`tcb` in `kernel/invocation.rs:185`, labels in `abi/lib.rs`):

| Operation | Effect |
|---|---|
| `TCB_CONFIGURE` | give the thread a capability-space root (a CNode), an address-space root (a page table with an ASID) and optionally an IPC buffer frame and address; the thread keeps derived copies in its slots, replacing any it had. Everything is checked before anything changes. |
| `TCB_WRITE_REGISTERS` | set registers in `sap_abi::reg` order (pc, ra, sp, gp, tp, a0–a7), reset the status bits to a fresh user thread's, and optionally make it runnable |
| `TCB_READ_REGISTERS` | return those registers, the ones beyond the four message registers through the caller's IPC buffer |
| `TCB_RESUME`, `TCB_SUSPEND` | make an inactive thread runnable; or stop it, cancelling and restarting a blocking system call so resuming repeats it |
| `TCB_BIND_NOTIFICATION`, `TCB_SET_FAULT_HANDLER` | bind a notification ([[kernel-ipc#Bound notifications]]), set the fault-handler endpoint ([[kernel-ipc#Fault delivery]]) |
| `TCB_SET_PRIORITY`, `TCB_SET_MC_PRIORITY` | set its priority or maximum controlled priority, at most the authority thread's `mcp` (`RANGE_ERROR` otherwise) |

Reading and writing registers is refused on the calling thread itself (`ILLEGAL_OPERATION`), as in seL4; here the calling thread's registers are also borrowed by the syscall path. The priority rule is seL4's: a thread can't use an authority to grant more priority than that authority was itself granted. `user/libsap/tcb.rs` wraps these.

## Scheduler

`kernel/sched.rs` keeps 256 ready queues, one per priority, and a bitmap of the non-empty ones (lines 13–32). **Every runnable thread is in its queue, including the running one.** The thread that runs is the head of the highest non-empty queue (`highest`, line 168). That keeps the rules short:
- `make_runnable` (line 49) marks a thread `Running` and appends it to its queue; `make_inactive` (line 77) removes it; `set_priority` (line 95) moves a runnable thread to its new priority's queue;
- `tick` (line 114), on every timer interrupt, counts down the current thread's time slice, five ticks or 50 ms (line 15), and when it runs out moves the thread to the tail of its queue;
- `yield_current` (line 126), the `YIELD` syscall, moves it to the tail at once.

Moving a thread to the tail of its own queue is the whole of preemption and round robin. A higher-priority thread always runs first; threads at one priority take turns by time slice.

`run` (line 143) ends every entry from user mode. It picks the highest-priority thread, records it as current (a new thread gets a fresh time slice), switches to the address space its address-space capability names if that isn't already active (`arch::activate_vspace`, which checks the capability is a root whose ASID is still assigned), and resumes it in user mode. A thread whose address space is gone is suspended with a warning; a `TODO` says it should fault to its handler once faults are delivered over IPC. If no thread is runnable, `run` runs the idle loop.

## Idle loop

`arch::idle_once` (`kernel/arch/riscv/mod.rs:95`) enables interrupts, waits with `wfi`, handles whatever arrives through the kernel-trap path, and disables interrupts again. `run` repeats it until a thread is runnable. This and the boot checks are the only places the kernel runs with interrupts enabled; seL4 uses an idle thread for the same purpose.

## Kernel globals

Scheduler state is a `Global` (`kernel/global.rs:13`), a cell that hands out `&mut` without locking. That is sound because there is one hart, kernel code runs with interrupts disabled, and the idle loop touches no globals, so kernel code is never re-entered while it holds a reference. A `TODO` there says to take a big kernel lock on every entry once more harts run, as seL4 does.

## Tests

`user/init` starts a second thread in its own address space, on a stack it maps itself, running a loop that increments a shared counter (`check_threads` in `user/init/main.rs`). It checks that configuring with a non-root address space fails; that an authority can't grant more than its own `mcp`; that registers of the calling thread can't be written; that a priority-100 thread never runs while the priority-255 root task is runnable, even across yields; that at equal priority the counter moves while the root task spins without yielding, which only preemption can cause; that a suspended thread stops; that its registers show it stopped inside the loop, on its stack; that a resumed thread runs again; and that a deleted one never does.

## Syscalls

`sap-abi` (`abi/lib.rs`), a crate shared by the kernel and user programs, defines the syscall numbers and the layouts both sides read, so the two can't drift. The kernel dispatches in `syscall::handle` (`kernel/syscall.rs`): `CALL`, the capability invocation ([[kernel-capabilities#Invocation]]), `YIELD`, and the two debug calls `DEBUG_PUTCHAR` and `DEBUG_HALT`.

> [!todo] Not built
> Scheduling contexts and donation; floating-point state; copying registers between threads; multiple harts.
