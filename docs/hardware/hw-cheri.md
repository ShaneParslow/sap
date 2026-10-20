---
status: decided
summary: CHERI support is wanted from the start as an accelerator, never a correctness requirement. What CHERI is, its standardization status, what it fixes for sap (bounds, unforgeability, sealed handles, cheap compartment calls) and what it doesn't.
updated: 2026-10-19
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
- **Byte-granular sharing.** A message pool can be one flat region with a bounded capability per message, and a read-only capability derived for the receiver. Granting access is passing a pointer instead of mapping pages ([[2026-09-27-future-of-computing#m15|§m15]]). In sap's kernel, frames are page-sized only because page tables are ([[kernel-capabilities]]). A byte-granular memory capability fits the same capability model: derived from a frame, bounded to any byte range, handed to a compartment as a CHERI capability rather than as a mapping. The derivation tree handles that easily; what doesn't come easily is revoking it, since CHERI capabilities can be copied into ordinary memory where the tree can't follow them ([[#Revocation sweeps]], [[2026-10-08-22c3947b#m30|§m30]]).
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

## Keeping the door open

CHERI and paging are not one a subset of the other. CHERI bounds pointers inside an address space; paging separates address spaces and makes revocation of shared memory instant (unmap), which CHERI can't match without sweeps. CHERI hardware still has page tables. So paging work isn't wasted, and CHERI can come later (M10), provided nothing assumes that an integer can become a pointer. Proposed rules until then ([[2026-10-16-2c1ac53c#m8|§m8]]):
- **No pointers in the kernel ABI or in shared memory.** Already true: system calls take capability addresses and words, the kernel reaches the IPC buffer through its frame capability, and rings and Cap'n Proto messages hold indices and offsets.
- **Pointers come from one function, not from casts.** In user code every pointer to memory Rust didn't allocate (the IPC buffer, the boot-info page, mapped frames, rings) comes from `sap_sys::address::mapped`, the one use of exposed provenance ([[prog-runtime#Crates]]). Under CHERI it would derive the pointer from a capability the runtime holds for its address space, so a port changes that function. Code that only needs an address, such as a thread's entry point, takes it with `.addr()`, which exposes nothing.
- **The compiler checks it.** On the pinned nightly, `implicit_provenance_casts` (formerly `fuzzy_provenance_casts` and `lossy_provenance_casts`, behind `#![feature(strict_provenance_lints)]`) flags every `as` cast between integers and pointers, and `integer_to_ptr_transmutes` (on by default) flags transmutes. Every user crate denies both (roadmap I5), so a new cast in user code doesn't compile. The kernel still has 49 casts (the direct map, and object pointers kept as `usize` in capabilities and thread state); a hybrid-mode CHERI kernel tolerates them, a purecap one would not.

What the lints don't see, each a known place to check rather than a scattered risk:
- the explicit exposed-provenance functions (`with_exposed_provenance`, `expose_provenance`), which give untagged pointers under purecap CHERI; `clippy.toml` disallows the first two everywhere but `mapped`, which Clippy reports when it runs (the build doesn't run it);
- assembly, which is rewritten for CHERI's capability registers anyway;
- dependencies, whose lints are capped: as of 2026-10-19 `elf` makes no such casts, `linked_list_allocator` does its address arithmetic on pointers, and `fdt` stores physical addresses as pointers that are never dereferenced;
- pointers stored in integer-typed memory (message words, `AtomicUsize`, byte buffers), whose tag is lost on CHERI when copied as data;
- layouts that assume a pointer is 8 bytes, such as structures shared with assembly; size assertions like the 64-byte capability slot fail at compile time;
- a pointer moved outside its object's bounds, or rebuilt from the wrong base, which compiles and then traps on CHERI.

- **Revocation across trust boundaries stays at page granularity,** between address spaces, where it is cheap on both.

## Platform support

- **Emulation:** CHERI-QEMU covers CHERI-RISC-V and Morello ([[2026-09-27-future-of-computing#m29|§m29]]).
- **Rust (as of October 2026):** CHERI support exists only outside mainline Rust: a Morello compiler from the University of Kent (Rust 1.72.1, with `std` on CheriBSD), and a CHERIoT compiler for embedded RISC-V tracking nightly, where `core` and `alloc` compile and testing is ongoing ([Rust for Morello](https://www.cs.kent.ac.uk/people/staff/mjb211/rust/index.htm), [CHERI Alliance Rust working group](https://cheri-alliance.org/who-we-are/working-groups/rust-cheri/)). No purecap compiler for 64-bit CHERI-RISC-V was found. Since sap's userspace is Rust, purecap user programs wait on a compiler; code built for plain RISC-V runs unchanged in CHERI's hybrid mode, so the kernel and today's programs can run on CHERI-QEMU first.
- **seL4:** no CHERI support in mainline as far as the chat knew; building on seL4 would mean the paging variant only ([[2026-09-27-future-of-computing#m31|§m31]]). This is one of the arguments in [[kernel-choice]].

> [!question] Open
> Which CHERI-RISC-V variant and toolchain to target (the standardization track versus the older Cambridge ISA in CHERI-QEMU), and at what milestone CHERI enters the roadmap. See [[roadmap]].
