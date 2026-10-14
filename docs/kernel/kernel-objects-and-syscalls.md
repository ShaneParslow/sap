---
status: proposed
summary: The kernel's object types and a seL4-sized syscall set (Call, Send, Recv, ReplyRecv, Signal, Wait, Yield); everything else is a Call on a capability to a kernel object. Register-based arguments plus a per-thread IPC buffer page.
updated: 2026-10-13
sources: ["[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Kernel objects and syscalls

> [!note] Implemented so far
> Untyped memory, frames, page tables (address spaces), CNodes, threads and `CALL` exist as of M3 ([[kernel-capabilities]], [[kernel-threads]]); endpoints, notifications, the other IPC syscalls and scheduling contexts don't yet. This page remains the design.

The kernel is just another set of objects invoked through capabilities. The design is modeled on seL4 and holds whether sap writes its own kernel or uses seL4 ([[kernel-choice]]).

## Kernel object types

([[2026-09-27-future-of-computing#m7|§m7]], [[2026-09-27-future-of-computing#m25|§m25]])

| Object | Purpose |
|---|---|
| Untyped memory | Raw memory from which all other kernel objects are created ("retyped"). Gives kernel memory accounting and quotas for free. |
| Frame / memory region | Physical memory that can be mapped. Shared rings and buffers are frames mapped into several address spaces. |
| Address space | A page-table root (Sv39 on RISC-V). |
| Capability table | A process's table of capabilities (seL4: CNode / CSpace). |
| Thread | Execution context, bound to an address space, a capability table and a scheduling context. |
| Scheduling context | A budget of CPU time; see [[kernel-scheduling]]. |
| Endpoint | Synchronous IPC rendezvous; carries small messages and capabilities. |
| Notification | Asynchronous doorbell: a word of signal bits a thread can wait on. |
| IRQ handler | Delivers a device interrupt as a notification; acknowledged by invocation. |
| MMIO range / device | A device's register space, for driver components ([[hw-devices-and-io]]). |

There is no process object. A process is a thread, an address space and a capability table assembled by the userspace process manager, whose process-factory interface is how others spawn processes (decided 2026-10-11, [[prog-loading#Who loads]]).

## Syscalls

A very small set, all operating on capabilities ([[2026-09-27-future-of-computing#m29|§m29]]):

| Syscall | Purpose |
|---|---|
| `Call(cap, msg)` | Send and wait for the reply (with timeslice donation, once scheduling contexts exist). |
| `Send` / `NBSend` | Send, blocking or non-blocking. |
| `Recv(ep, reply)` / `NBRecv` | Wait for a message; a caller's reply goes to the reply object passed in. |
| `Reply(reply)` | Answer the caller linked to a reply object. |
| `ReplyRecv(ep, reply)` | Reply and wait for the next message: the server loop in one trap. |
| `Signal` / `Wait` / `Poll` | Notifications: set bits, block until bits are set, or check without blocking. |
| `Yield` | Give up the CPU. |

**Replies use reply objects, as in seL4's MCS configuration** (decided 2026-10-13, [[2026-10-08-22c3947b#m34|§m34]]): a server passes a reply object to `Recv`, and a `Call` that it receives links the caller to that object until the server replies through it. Unlike classic seL4, where the reply right sits in the server's thread, this lets one thread hold several pending replies and is what time donation builds on ([[kernel-scheduling]]). Notifications have their own syscalls rather than reusing `Send`/`Recv` as seL4 does. A user fault is delivered as a message to the thread's fault-handler endpoint, as an implicit `Call`; the handler replies to resume the thread.

**Everything else is a `Call` on a capability to a kernel object:** retyping untyped memory into objects, mapping and unmapping frames, configuring and resuming threads, copying, minting, moving, revoking and deleting capabilities, acknowledging IRQs, binding scheduling contexts.

## Argument passing

Every architecture has a register-based syscall convention, and with a syscall set this small it is enough. Registers beat the stack: faster, and nothing in user memory for the kernel to validate ([[2026-09-27-future-of-computing#m25|§m25]]).

For message contents that don't fit in registers, each thread has an **IPC buffer**: a page at a known address, as in seL4. This is the fixed-address kernel channel the human proposed ([[2026-09-27-future-of-computing#m28|§m28]]). Linux's vDSO is the related precedent for kernel-provided code and data mapped into every process.

A batched submission ring (io_uring-style) for many kernel operations at once, such as mapping a hundred pages, is deferred: start with synchronous calls for control operations and add a ring if batching turns out to matter. A ring needs a doorbell syscall unless the kernel polls ([[2026-09-27-future-of-computing#m29|§m29]]).

## Kernel memory

The kernel has no heap and no fixed-size pools that userspace can exhaust. Every kernel object occupies memory that userspace retyped from untyped memory it holds, so a process that wants more of anything pays for it from its own memory budget. The human asked whether fixed kernel memory would limit capability slots or call queues ([[2026-10-08-22c3947b#m28|§m28]]); under this design neither is limited:

| Resource | Where its memory comes from |
|---|---|
| Capability slots | CNodes, retyped from untyped. A process needing more slots creates another CNode and links it into its capability space. |
| Endpoint queues | None: a sender that finds no receiver blocks, and the queue is a list threaded through the waiting threads' own TCBs. |
| Notifications | One word in the notification object. |
| Rings and buffers | Frames mapped into both processes ([[ipc-channels]]), charged to whoever created the channel. |
| Page tables, TCBs, address spaces | Retyped from untyped. |
| Capabilities in transfer | Slots in the receiver's receive range ([[ipc-cap-transfer]]). |

Running out therefore shows up as backpressure on the process that owns the memory (a full ring, no free slots), never as kernel allocation failure. The fixed costs are set at boot and small: a kernel stack per CPU, the kernel's own page tables, and a small top-level table of address-space identifiers (seL4 has 128 entries, with the pools under it retyped from untyped).

The costs move to userspace instead: allocating slots and tracking memory is the most-cited pain point of programming seL4, so the runtime library needs a slot allocator; and untyped memory is handed out with a watermark, so a region can only be reused after everything created from it is revoked.

## What the kernel does not do

No file system, no global namespace, no bus, no wire format, no RPC semantics. Linux's kdbus put D-Bus semantics in the kernel and was rejected for baking one bus's policy into the kernel; its authors' userspace dbus-broker got most of the gains ([[2026-09-27-future-of-computing#m25|§m25]]; see [[linux-ipc-history]]). Cap'n Proto messages, rings and validation are userspace conventions ([[ipc]]); the kernel's part in IPC is capability transfer, mapping, notifications and donation.

> [!question] Open
> Whether sap's kernel adds a native capability move (seL4 lacks one; [[cap-ownership]]) and how many capabilities one transfer can carry. seL4 delivers one real capability per message ([[kernel-sel4-differences]]); the proposal is batched move-or-copy transfer into a per-connection receive range ([[ipc-cap-transfer]]).

> [!note] Kernel interfaces as schemas
> seL4 defines its kernel-object methods in an interface file and generates the stubs. The proposal for sap is a Cap'n Proto schema, so kernel objects can be declared as typed imports like any other interface ([[cap-process-authority#Runtime capabilities and declared imports]]).
