---
status: decided
summary: sap targets RISC-V first (QEMU virt, with CHERI-QEMU available). The case for a plain RISC core over language-specific or VLIW designs, how RISC-V, Arm and x86 compare for a microkernel, and how to use optional extensions without per-target builds.
updated: 2026-10-20
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

## Real hardware

QEMU is fine for correctness but distorts costs. A system call there takes microseconds, while memory copies run at close to host speed, so measurements such as the copy/transfer crossover ([[ipc-page-transfer#What it costs]]) need a real core. The human has one RISC-V board, a MangoPi MQ-Pro with an Allwinner D1 and 1 GiB of memory ([[2026-10-16-2c1ac53c#m22|§m22]]). The unmodified kernel boots on it and every check passes; the board needs only firmware and a device tree. Facts about it, checked on the board on 2026-10-20 ([[2026-10-20-685b0203-ipc-cost-and-cheri]]):
- **Core:** one T-Head C906 at 1 GHz, RV64GC with an MMU (Sv39) and 16 ASID bits. It also has a vector unit that predates the ratified vector extension (version 0.7.1), which standard toolchains don't target.
- **Memory attributes:** the C906's own page-table bits (63–59) only apply when `mxstatus.MAEE` is set, which Allwinner's firmware and U-Boot do and Linux then handles as a vendor erratum. Booted as below, `mxstatus` reads 0, so standard Sv39 entries work and attributes come from the chip's fixed address map (the C906's "sysmap", set when the chip was made; see [openc906's `sysmap.h`](https://github.com/XUANTIE-RV/openc906/blob/main/C906_RTL_FACTORY/gen_rtl/mmu/rtl/sysmap.h) for the format).
- **Caches:** off after the boot ROM (`mhcr` reads 0), and nothing in OpenSBI turns them on. Uncached, a system call takes about 10 µs and copying runs at a few MB/s. An M-mode stub run before OpenSBI turns them on with U-Boot's values (`mcor` 0x70013 to invalidate, `mhcr` 0x11ff, `mhint` 0x16e30c); with them on every check still passes and a system call takes 0.7 µs.
- **Boot:** over USB in the boot ROM's FEL mode with [xfel](https://github.com/xboot/xfel): initialise DRAM, write OpenSBI's `fw_payload` (generic platform, the kernel as payload at 0x40200000, the device tree built in) to 0x40000000, and jump to it. The device tree is Linux's `sun20i-d1-mangopi-mq-pro` plus a `memory` node (0x40000000, 1 GiB) and a CLINT node (`thead,c900-clint` at 0x14000000); without the CLINT node the timer never fires. The console is OpenSBI's (SBI debug console over the 16550-style UART), so the kernel needs no driver.
- **Recovery:** the D1 has no power-off, so a run ends with the core stopped in OpenSBI. Arming the SoC watchdog from FEL before the jump (base 0x020500a0, 16 s, whole-system reset) brings the board back to FEL after every run, with no SD card inserted; that bounds a run to 16 s.
- **One hart,** so it measures single-core costs only, not the multi-CPU polling the rings are designed for ([[ipc-rings#What runs show]]).

> [!todo] Not built
> The board scripts (device-tree additions, firmware build, cache stub, watchdog, FEL load) exist only outside the repository. They are firmware-side; sap's code needed no change.

> [!note] Arm and CHERI
> CHERI-QEMU also emulates Arm's Morello. Whether to add an Arm port later is open; the human wasn't sure of the Arm boot path ([[2026-09-27-future-of-computing#m28|§m28]]).
