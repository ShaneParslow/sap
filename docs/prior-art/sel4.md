---
status: reference
summary: The formally verified capability microkernel whose design sap's kernel follows, and the main reference for sap's own kernel (building on it was considered and declined) - plus its ecosystem (Microkit, rust-sel4, sDDF, LionsOS).
updated: 2026-10-19
sources: ["[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m29|§m29]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# seL4

A small capability microkernel with machine-checked proofs of functional correctness for specific configurations. Supports RISC-V and Arm ([[2026-09-27-future-of-computing#m29|§m29]]).

**What sap takes:** per-process capability tables; the capability derivation tree (revoking a parent revokes all derived capabilities, [[cap-ownership]]); MCS scheduling contexts and timeslice donation ([[kernel-scheduling]]); a dozen-operation syscall set where everything else is invoking kernel objects ([[kernel-objects-and-syscalls]]); all kernel memory retyped from untyped memory, which gives quotas for free; badged endpoints for per-object identity; the per-thread IPC buffer ([[2026-09-27-future-of-computing#m7|§m7]], [[2026-09-27-future-of-computing#m31|§m31]]).

**Ecosystem** ([[2026-09-27-future-of-computing#m31|§m31]]):
- *Microkit:* static systems declared in XML (components, shared memory regions, channels); components implement `notified(channel)` and `protected(channel, msg)`.
- *rust-sel4:* Rust bindings and runtime, including Microkit support and custom root tasks. Check its current state before committing.
- *sDDF:* the seL4 Device Driver Framework; drivers as components over SPSC rings with notification doorbells.
- *LionsOS:* an early-stage OS from the seL4 group at UNSW, built on Microkit and sDDF.
- *VMM:* runs Linux guests, used for legacy drivers and applications ([[system-legacy-compat]]).

**Where it falls short for sap:** capability transfer copies rather than moves; one capability delivered per message; tedious slot management; kernel changes forfeit verification; no mainline CHERI support. Full analysis in [[kernel-choice]]; a source-level comparison, desktop fit, and whether to sketch on it in [[kernel-sel4-differences]].

## Information-flow proof

Murray et al. proved in 2013 that seL4 can enforce isolation between partitions, including confidentiality: no partition learns anything about another except through channels the configuration allows ([paper](https://sel4.systems/Research/pdfs/sel4-from-general-purpose-to-proof-information-flow-enforcement.pdf), IEEE S&P 2013). The proof needed three restrictions on how seL4 is used:
- **A static partition schedule.** Partitions run in fixed round-robin time slices, with priority scheduling only inside a partition, so which partition runs never depends on what another did. This was a kernel change (seL4's "domain scheduler").
- **No device interrupts except the timer.** The kernel didn't isolate one partition's interrupts from another's: an interrupt for partition A's device arrives, and is handled, while partition B runs, so A's device activity changes the kernel's state and B's execution. The channel isn't user code running in kernel mode (drivers are user processes reached by a notification); it's *when* delivery happens. Rather than change interrupt delivery, the configuration gives no partition an interrupt capability, and drivers poll memory-mapped I/O.
- **Inert copies of every capability that crosses partitions,** kept where nobody can delete them, so that deleting one never makes another the last one. Otherwise, whether a deletion destroys an object would reveal whether another partition still holds a capability to it.

The proof covers storage channels visible in the kernel's abstract model. Timing channels (shared caches, a partition overrunning its slice with a system call just before a switch) are explicitly out of scope. Interrupts aren't harmful in general; they matter only for a system that must prove isolation. If sap wants that for attested subsystems ([[vision-attestation-and-verification]]), the remedy is to deliver a partition's interrupts only while it runs, not to give up interrupts.
