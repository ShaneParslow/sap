---
status: decided
summary: CHERI support is wanted from the start as an accelerator, never a correctness requirement. What CHERI is, its standardization status, what it fixes for sap (bounds, unforgeability, sealed handles, cheap compartment calls) and what it doesn't.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# CHERI

**Decision:** sap supports CHERI from the beginning, which is why the first target is RISC-V under CHERI-QEMU. CHERI is an accelerator: the design runs on plain paging, and CHERI makes the same semantics cheaper where it exists (principle 6 in [[vision-principles]]). How the two variants differ end to end is in [[ipc-paging-vs-cheri]].

## What it is

CHERI turns pointers into hardware capabilities: each pointer carries bounds and permissions, and a hidden tag bit that makes it unforgeable. A capability can only be derived from another by narrowing it. Pointers become 128 bits ([[2026-09-27-future-of-computing#m3|§m3]], [[2026-09-27-future-of-computing#m7|§m7]]). It is the hardware form of the capability model in [[cap-model]]: a CHERI capability in a register, a kernel capability, and a Cap'n Proto capability over the network are the same idea at three scales ([[2026-09-27-future-of-computing#m5|§m5]]).

**Status (as of the chat, October 2026):** on track to become a RISC-V standard. As of April 2026, Codasip's chief architect targeted ratification in Q2 2026 and expected Q3 to be more likely; whether it has happened since is unknown. Existing RISC-V binaries run unchanged, a full LLVM port exists, and Arm (Morello), Microsoft, Google, Codasip, lowRISC and academic groups have built CHERI processors. Expect it in embedded (CHERIoT) and possibly servers first; no mainstream phone or desktop CPU is announced ([[2026-09-27-future-of-computing#m13|§m13]]).

**Cost:** 128-bit pointers add memory footprint and cache pressure. Morello's overhead figures are hard to interpret because it was an unoptimized prototype ([[2026-09-27-future-of-computing#m7|§m7]]).

## What it gives sap

- **Cheap compartments.** Components share one address space and are isolated by capabilities, so a cross-compartment call goes through sealed entry capabilities at about the cost of a function call, with no page-table switch or TLB flush. This is CHERI's main value here: microkernel IPC nearly as cheap as a call ([[2026-09-27-future-of-computing#m7|§m7]], [[2026-09-27-future-of-computing#m17|§m17]]).
- **Byte-granular sharing.** A message pool can be one flat region with a bounded capability per message, and a read-only capability derived for the receiver. Granting access is passing a pointer instead of mapping pages ([[2026-09-27-future-of-computing#m15|§m15]]).
- **Bounds in hardware.** A malicious offset that points past a message's bounds traps instead of leaking data ([[ipc-validation]]).
- **Sealed handles.** A receiver can give out a sealed capability pointing directly at its internal object. The holder can neither dereference nor forge it, only hand it back; the receiver unseals it with its own key and has a valid pointer with no table lookup or bounds check. This makes "integer handles into receiver internals" a bug class the hardware rules out, and the chat rated it CHERI's most valuable contribution to the design ([[2026-09-27-future-of-computing#m23|§m23]]).

## What it doesn't fix

- **Temporal safety across trust boundaries.** See below.
- **Semantic validation:** variant tags, UTF-8, contracts still need software checks.
- **Denial of service and amplification** ([[ipc-resource-limits]]).
- **Double fetch.** A sender can hand out a read-only capability but keeps its own writable one ([[2026-09-27-future-of-computing#m23|§m23]]).
- **Paging.** CHERI doesn't replace the MMU; CHERI systems still have page tables ([[2026-09-27-future-of-computing#m25|§m25]]).

## Revocation sweeps

Inside a compartment written in a memory-safe language, use-after-free can't happen, so the compartment's own memory needs no sweeps. The problem is lending across a trust boundary: if A lends B a capability to A's memory, then frees and reuses that memory, B still holds a valid capability, and it may be keeping it deliberately. Fixing that in general means quarantining freed memory until a sweep invalidates every outstanding capability to it (the Cornucopia approach; CheriBSD implements it) ([[2026-09-27-future-of-computing#m23|§m23]], [[2026-09-27-future-of-computing#m25|§m25]]).

**For IPC, sap avoids sweeps by design:** memory shared with B only ever holds data meant for B. Each channel has its own buffer pool, and buffers are reused only within that channel. B could already reach everything in the pool, so reuse leaks nothing. Sweeps matter only for general-purpose heaps shared across trust domains and for temporal safety of C/C++; they are not a blocker for this design ([[2026-09-27-future-of-computing#m25|§m25]]).

**Page transfer still has a role.** For large messages from untrusted peers, transferring pages (the sender loses write access) is what guarantees immutability; CHERI capabilities are for fine-grained sharing among components that trust each other ([[2026-09-27-future-of-computing#m25|§m25]]).

## Platform support

- **Emulation:** CHERI-QEMU covers CHERI-RISC-V and Morello ([[2026-09-27-future-of-computing#m29|§m29]]).
- **seL4:** no CHERI support in mainline as far as the chat knew; building on seL4 would mean the paging variant only ([[2026-09-27-future-of-computing#m31|§m31]]). This is one of the arguments in [[kernel-choice]].

> [!question] Open
> Which CHERI-RISC-V variant and toolchain to target (the standardization track versus the older Cambridge ISA in CHERI-QEMU), and at what milestone CHERI enters the roadmap. See [[roadmap]].
