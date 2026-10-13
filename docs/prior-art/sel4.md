---
status: reference
summary: The formally verified capability microkernel whose design sap's kernel follows, and the main reference for sap's own kernel (building on it was considered and declined) - plus its ecosystem (Microkit, rust-sel4, sDDF, LionsOS).
updated: 2026-10-08
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
