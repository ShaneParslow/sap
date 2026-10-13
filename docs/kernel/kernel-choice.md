---
status: decided
summary: sap writes its own kernel, following seL4's design where useful. Records the seL4 alternative the chat recommended, what it would have given, and the gaps the own kernel must close.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]", "[[2026-09-27-future-of-computing#m30|§m30]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# Own kernel or seL4

**Decided (2026-10-08): sap writes its own kernel** ([[2026-10-08-22c3947b-wiki-setup]]). The design chat had recommended building on seL4; the human chose a custom kernel, consistent with the repository's `kernel/arch/riscv/` and with wanting CHERI from the start, which seL4 mainline lacks. The rest of this page records the alternative and what the own kernel takes from it, so the reasoning isn't lost.

## What is settled

- **Not a Linux prototype.** The human judged that a userspace prototype on Linux would be too detached from the real system ([[2026-09-27-future-of-computing#m28|§m28]]). (The chat had earlier proposed one, built from memfd rings, fds passed over Unix sockets, Landlock and FUSE.)
- **Bare metal on QEMU riscv64** is the starting platform either way ([[hw-isa]]).
- **The novel parts are above the kernel:** the interface spec, capability semantics, IPC conventions, process startup, the object store ([[2026-09-27-future-of-computing#m29|§m29]]).

## Path A: build on seL4

At boot, seL4 hands one program, the root task, capabilities to all untyped memory. Everything else is built in userspace: allocating kernel objects by retyping untyped memory, building capability spaces and address spaces, creating threads, mapping pages ([[2026-09-27-future-of-computing#m31|§m31]]).

**What sap would write:** a process manager (`spawn`, bootstrap capabilities, the process-factory interface); memory and capability-slot allocators; the IPC library (rings, notifications, Cap'n Proto, own/borrow); drivers; the object store, powerbox and compatibility layer.

**What exists to start from:** Microkit (static systems declared in XML, with `notified(channel)` and `protected(channel, msg)` entry points); rust-sel4 (Rust bindings, Microkit support, custom root tasks); sDDF (drivers over SPSC rings with notification doorbells, essentially sap's IPC design applied to drivers); LionsOS (an early OS on Microkit and sDDF). See [[sel4]].

**How well seL4 fits** ([[2026-09-27-future-of-computing#m31|§m31]]):

| Need | seL4 | Fit |
|---|---|---|
| Unforgeable capabilities | per-process capability tables | exact |
| Revoke everything derived from a grant | capability derivation tree | exact |
| CPU time as a capability, donation | MCS scheduling contexts, passive servers | exact |
| Small syscall set, kernel as objects | by design | exact |
| Kernel memory accounting | all kernel memory comes from untyped memory you hand it | better than most |
| Shared rings plus doorbells | frames plus notifications | exact |
| Per-object RPC identity | badged endpoints | good |
| `borrow` | mint child capability, revoke on return | good |
| `own` (move) | IPC transfer copies; sender keeps its copy | **gap** (host-mediated workaround, [[cap-ownership]]) |

**Where it falls short:**
1. Moves aren't a kernel primitive; ownership transfer costs an extra round trip through the object's host.
2. A message delivers one real capability: the receiver names a single receive slot ([[kernel-sel4-differences]]).
3. Capability-slot management is tedious and left to userspace (the most commonly cited pain point).
4. Changing the kernel forfeits its proofs, so anything sap wants that seL4 lacks goes in userspace or an unverified fork.
5. Verification covers specific configurations, not every combination of architecture, multicore and MCS.
6. No CHERI support in mainline, as far as the chat knew.
7. Almost no ecosystem: basic drivers from sDDF/LionsOS, nothing for GPUs, Wi-Fi or desktop. The usual answer is Linux in a VM on seL4's VMM ([[system-legacy-compat]]).

The chat's summary: seL4 is most of the kernel this design wants, and all gaps except CHERI can be handled in userspace at modest cost.

## Path B: write the kernel

Gets native moves, kernel-level own/borrow, any capability count per message, and CHERI compartments designed in from the start. Costs years before IPC and capabilities match seL4's, and no formal verification ([[2026-09-27-future-of-computing#m31|§m31]]). The chat noted that if the goal includes learning to write a kernel, the human's proposed order is right with two changes: add trap handling, the timer, Sv39 page tables and user mode after the console, and build capability tables before IPC, since IPC transfers capabilities ([[2026-09-27-future-of-computing#m29|§m29]]). That order is in [[roadmap]].

## A middle path

Write the kernel but copy seL4's design (object types, derivation tree, MCS, syscall set) as closely as is useful, so the userspace layers could later move onto seL4, or a kernel change could be argued to seL4 upstream with evidence. The chat's framing: "if the design eventually proves that a kernel change is worth it, you'll know exactly which change" ([[2026-09-27-future-of-computing#m31|§m31]]).

## Consequences of the decision

- The roadmap follows the own-kernel order ([[roadmap]]).
- `own` can be a native kernel move rather than host-mediated ([[cap-ownership]]); capability count per message is sap's to choose ([[kernel-objects-and-syscalls]]).
- CHERI compartments can be designed into the kernel ([[hw-cheri]]).
- No formal verification. The middle path above (copy seL4's object model, derivation tree, MCS and syscall set closely) is the default unless a reason to diverge comes up, which keeps seL4's design and literature usable as a reference.
- Legacy drivers need sap's own VMM support later, since seL4's VMM isn't available ([[system-legacy-compat]]).
