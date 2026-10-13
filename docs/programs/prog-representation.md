---
status: exploring
summary: The chat's model for programs - a canonical typed IR stored by hash, lowered on the device to native code cached by (IR hash, target hash), with Wasm only as a lowering for crossing trust boundaries - and why Wasm core and raw MLIR each fall short as the canonical form.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m4|§m4]]", "[[2026-09-27-future-of-computing#m5|§m5]]", "[[2026-09-27-future-of-computing#m6|§m6]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m8|§m8]]", "[[2026-09-27-future-of-computing#m9|§m9]]"]
---

# Program representation

The human asked whether programs should be ASTs in the object store or flat binaries such as Wasm, and what that means for performance and the ISA ([[2026-09-27-future-of-computing#m4|§m4]]). Later they concluded the representation matters less than the interface spec and should be decoupled from the rest of the OS, while noting that some language is unavoidable, and shipping raw machine code is how today's ABI and portability mess arose ([[2026-09-27-future-of-computing#m8|§m8]]). Nothing here is decided.

## The layered model

Layers rather than a choice between ASTs and binaries ([[2026-09-27-future-of-computing#m5|§m5]], revised in [[2026-09-27-future-of-computing#m7|§m7]]):

1. **Canonical form:** a typed multi-level IR, content-addressed, stored in the object store, with specs and proofs attached. This is the program's identity and what AIs read, write and verify.
2. **Interfaces:** the component's Cap'n Proto interfaces ([[iface-capnp]]).
3. **Lowering:** a verified compiler on the device produces CPU code, GPU kernels or accelerator programs for the hardware actually present. Results are cached as derived objects keyed by (IR hash, target hash), compiled ahead of time or at install, so nothing runs slower at runtime. This is a Nix derivation: a pure function of its inputs, built once and reused.
4. **Wasm:** one lowering target, for shipping code across a trust boundary to a machine whose compiler you don't trust.

## Why not Wasm as the universal format

The human doubted Wasm suits VLIW-style hardware, complex memory models or accelerators ([[2026-09-27-future-of-computing#m6|§m6]]), and the chat agreed ([[2026-09-27-future-of-computing#m7|§m7]]):
- linear memory only; Wasm 3.0 added 64-bit addressing and multiple memories but no notion of placement or tiers ([[hw-memory-topology]]);
- SIMD fixed at 128 bits, no scalable vectors;
- can't express accelerator code (GPU code uses SPIR-V or PTX).

Wasm does portable sandboxing of CPU code well; it wasn't built for heterogeneous hardware. Its component model's typed-interface ideas survive in sap's interface layer ([[wasm-component-model]]).

## MLIR

MLIR is a framework for defining IRs ("dialects") and lowering between them, not a language; ML is just where it was adopted first. Ordinary languages use it: Flang lowers Fortran through its FIR dialect, ClangIR routes C/C++ through a dialect, Mojo is built on it. A general-purpose pipeline runs source → language dialect (ownership, closures, generics, effects) → generic dialects (functions, structured control flow, arithmetic, memory references) → LLVM dialect → machine code, with accelerator-friendly regions (tensor ops, loop nests) taking a separate path through linalg to GPU or vendor dialects in the same compilation ([[2026-09-27-future-of-computing#m9|§m9]]).

**Problem:** MLIR makes no stability promises; dialects change between releases. It is good compiler infrastructure and a bad archival format. A canonical IR stored in the store must be a frozen, versioned dialect sap controls, which turns the representation question into a versioning question ([[iface-versioning]]).

## Urbit's jets as the pattern and the warning

Urbit's VM, Nock, is tiny and too slow to run directly; performance comes from "jets", native implementations registered against the hash of the Nock function they replace. Semantic definition as ground truth, native code as a cache of it, is the layered model above. Urbit's real bugs came from jets that didn't exactly match their definitions: the cache must be proven equivalent or checked by translation validation, never assumed ([[2026-09-27-future-of-computing#m5|§m5]]; [[urbit]]).

## Known problems

([[2026-09-27-future-of-computing#m7|§m7]])
- **Verified compilers for heterogeneous targets mostly don't exist.** CompCert covers C on a few ISAs; MLIR has no verified backends. The largest research gap in this model.
- **Performance portability:** the same IR won't run well everywhere without per-target tuning (a search well suited to AI, with results cached).
- **Sharing compiled artifacts** across machines needs reproducible builds or proofs, or everyone recompiles everything.
- **Debugging across lowering levels** is hard.
- **The IR itself needs versioning.**

> [!question] Open
> Whether sap defines a canonical IR at all, or only identifies native code and Wasm by hash and leaves representation to languages. The second is consistent with "specify interfaces, not languages" ([[vision-principles]]) and is all the early roadmap needs. How code objects of either kind are loaded is in [[prog-loading]].
