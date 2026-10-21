---
status: hub
summary: The kernel provides only unforgeable authority, memory mapping, wakeups and scheduling handoff; everything else is a component. sap writes its own kernel, modeled on seL4.
updated: 2026-10-08
---

# Kernel

sap's kernel is a capability microkernel in the seL4 mould. It does the four things nothing else can do and leaves everything else (wire formats, RPC, naming, the object store, drivers) to userspace components (principle 4 in [[vision-principles]]) ([[2026-09-27-future-of-computing#m25|§m25]]):

1. **Authority:** unforgeable capability tables.
2. **Memory:** mapping, unmapping and transferring pages.
3. **Wakeups:** notifications (futex-like doorbells).
4. **Scheduling handoff:** a synchronous caller donates its time to the callee.

Pages are ordered by dependency: the first is a decision the others depend on, then the detailed comparison with seL4 behind it, then what the kernel exposes, what crossing into it costs, how it schedules, how processes are built from all of that, and finally what the RISC-V kernel does today: boot, memory, traps, then the root task.

| Page | Status | Summary |
|---|---|---|
| [[kernel-choice]] | decided | sap writes its own kernel. The seL4 alternative, what it would have given, and the gaps the own kernel must close. |
| [[kernel-sel4-differences]] | exploring | seL4 checked at the source level: one capability per message, device trees, drivers, desktop fit, and whether to sketch userspace on it. |
| [[kernel-objects-and-syscalls]] | proposed | Kernel object types and a seL4-sized syscall set where everything else is an invocation on a kernel-object capability. |
| [[kernel-crossing-costs]] | reference | What mode and address-space switches cost, latency floors, and the three ways to keep crossings off the hot path. |
| [[kernel-scheduling]] | proposed | CPU time as a capability, timeslice donation, adaptive waiting, and preemption awareness. |
| [[kernel-process-model]] | proposed | Processes start with a capability table and a typed bootstrap capability; no environment, argv strings, fork or exec. |
| [[kernel-boot-riscv]] | implemented | OpenSBI to `_start`, the boot page table and jump to the upper half, `riscv_start` and architecture-neutral boot information, `kmain`, the console, log and shutdown. |
| [[kernel-memory-riscv]] | implemented | Sv39 with the kernel in the upper half, the boot page table, per-section permissions, the direct map, free memory and the boot allocator, user address spaces. |
| [[kernel-traps-riscv]] | implemented | Trap entry with the `sscratch` stack switch, the shared return path, dispatch of timer, breakpoints, syscalls and user faults, and SBI timer ticks. |
| [[kernel-threads]] | implemented | Thread control blocks, thread invocations (configure, registers, resume, suspend, priorities), fixed-priority round-robin scheduling, the idle loop, and lock-free kernel globals on one hart. |
| [[kernel-capabilities]] | implemented | Capability representation, slots and CNodes, the derivation tree, address resolution, delete and revoke, `CALL` and CNode operations, and the root task's initial capabilities. |
| [[kernel-ipc]] | implemented | Notifications, badges, endpoints with MCS-style reply objects, bound notifications, blocking, cancellation and restarted system calls; capability transfer and fault delivery to come in M4. |
| [[kernel-root-task]] | implemented | The one program the kernel loads itself: building and embedding `user/init`, loading a position-independent ELF, entering user mode, the temporary debug syscalls, and starting the kernel checks as a program of their own. |

The kernel is Rust (`no_std`); its code is in `kernel/`, with architecture code under `kernel/arch/riscv/`.
