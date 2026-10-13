---
status: decided
summary: sap targets RISC-V first (QEMU virt, with CHERI-QEMU available). The case for a plain RISC core over language-specific or VLIW designs, how RISC-V, Arm and x86 compare for a microkernel, and how to use optional extensions without per-target builds.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m5|§m5]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Instruction set

**Decision:** sap's first target is 64-bit RISC-V on QEMU's `virt` machine. The human chose RISC-V because QEMU has CHERI-RISC-V support ([[hw-cheri]]); the chat's own reasons for it were the simpler privileged spec compared with Arm, a console and timer through SBI (OpenSBI), and virtio peripherals ([[2026-09-27-future-of-computing#m29|§m29]]). Nothing in the design depends on the ISA beyond the kernel's architecture layer (`kernel/arch/riscv/`).

## Why an ordinary RISC core

The human asked whether the ISA should rise to meet the language agents write, as Lisp machines did, or whether AIs should write assembly directly. The chat concluded no on both counts ([[2026-09-27-future-of-computing#m5|§m5]]):

- **Energy and time go to data movement, not decode.** A DRAM access costs two to three orders of magnitude more energy than an arithmetic operation, and ISA differences add up to a few percent. Language-specific instructions target the cheap part of the chip.
- **Volume economics.** Lisp machines were faster at tag checks and GC support in 1980; compilers then made those cheap on commodity chips, which improved every process generation. Azul's Java hardware followed the same arc.
- **What does beat generic hardware:** accelerators for workload structure (GPUs, matrix units), small targeted extensions (Arm's FJCVTZS for JavaScript, memory tagging, CHERI), and things software can't do cheaply, mainly protection and isolation.

So the novelty goes into protection (CHERI) and data movement (capability-addressed memory and accelerators), and the compute core stays boring.

Middle layers (IRs, compilers) were never mainly for humans: they let deterministic, checkable algorithms optimize, let one program target many microarchitectures, and let proofs be carried to the binary. An LLM emitting assembly would be doing compilation through a process nobody can verify. See [[prog-toolchain-and-verification]].

## Why not VLIW

The human suggested the push toward compiler control might favor VLIW. It fails on general-purpose code for the reason Itanium did: static scheduling assumes latencies are known at compile time, but whether a load takes 4 cycles or 300 depends on runtime data that no compiler, however smart, has. VLIW belongs where latency is predictable: DSPs, throughput cores, systolic matrix units ([[2026-09-27-future-of-computing#m7|§m7]]). This is principle 7 in [[vision-principles]]: the decision goes to whoever has the information.

## RISC-V, Arm and x86 for a microkernel

Microkernels cross privilege and address-space boundaries often, so architecture details matter ([[2026-09-27-future-of-computing#m9|§m9]]):

| | Strengths | Weaknesses |
|---|---|---|
| Arm | Mature ASIDs (address-space switches without TLB flushes); clean exception levels; pKVM in production; MTE, and POE coming | More complex privileged architecture |
| RISC-V | Simple trap model; ASIDs in the spec; SBI firmware interface; CHERI-QEMU | Implementations vary; high-performance cores still maturing |
| x86 | Ubiquitous | Heaviest Spectre/Meltdown mitigation burden on kernel entry; FRED should help on new hardware |

The chat rated Arm the best current fit for a microkernel. That doesn't change the design, only how much batching is needed ([[kernel-crossing-costs]]); RISC-V was chosen for CHERI emulation and simplicity.

## Handling extension variation

RISC-V cores differ in which optional extensions they implement (Zbb bit manipulation, Sstc timers, Svpbmt memory types, Zicboz cache-block zeroing, vector). The goal is to use what a machine has without per-target builds and without a branch on every use. The human proposed compiling each variant and, at boot, patching a jump to the best one so the unused paths are "fused off" ([[2026-10-08-22c3947b-wiki-setup]]). That technique exists under several names:

- **Kernel alternatives:** Linux patches instruction sequences at boot according to detected CPU features or errata (`arch/x86/kernel/alternative.c`; on RISC-V the `ALTERNATIVE` macros, used for Zbb, Svpbmt, Zicbom and vendor errata).
- **Static keys / jump labels:** a `nop` that is patched into a jump (or back) when a rarely-changing condition flips; the fast path costs nothing.
- **Function multiversioning:** the compiler emits one clone per feature set for selected functions only, and a resolver picks one once (GCC/Clang `target_clones`, resolved through ELF `ifunc`; the Rust `multiversion` crate). Only the functions that differ are duplicated, which is the deduplication the human expected.

Constraints for sap: patch before kernel text is mapped read-only and execute-only, then `fence.i` on every hart that may have fetched the old code. Choose feature sets per function rather than compiling every combination, which grows exponentially. This works for instruction-level differences only. **CHERI can't be handled this way:** it changes pointer width and the ABI, so it needs a separate build.

> [!question] Open
> Whether and when sap needs this. Nothing in M1–M2 does; it becomes worth it once a hot path benefits from an optional extension.

> [!note] Arm and CHERI
> CHERI-QEMU also emulates Arm's Morello. Whether to add an Arm port later is open; the human wasn't sure of the Arm boot path ([[2026-09-27-future-of-computing#m28|§m28]]).
