---
status: hub
summary: How programs are loaded, represented and built - loading is proposed concretely; representation is deliberately decoupled from the rest of the OS and still exploratory.
updated: 2026-10-20
---

# Programs

The chat spent several turns on what programs should look like (ASTs in the store, Wasm, native code, MLIR) before the human concluded it is not the most important question and should be decoupled from the other OS components: the interface spec is what has to be universal ([[2026-09-27-future-of-computing#m8|§m8]]). Loading is concrete and proposed; representation and toolchain are exploratory. What *is* settled about programs lives elsewhere: they are identified by content hash ([[kernel-process-model]]), and languages own their own linking and calling conventions ([[iface-bindings-and-languages]]).

| Page | Status | Summary |
|---|---|---|
| [[prog-runtime]] | implemented | The user-side crates: stateless system-call wrappers (`sap-sys`), the runtime with slots, mutexes and an optional heap (`sap-rt`), rings, and the ELF loader. |
| [[prog-process-factory]] | implemented | The factory `init` serves: spawn a boot-pack program by hash with the client's memory, wait for it and get the memory back, or kill it. |
| [[prog-loading]] | proposed | How code becomes a process: a userspace process manager, static ELF natively and loader components for other formats, a small per-architecture process ABI, per-process address spaces, and a boot image. |
| [[prog-rust-toolchain]] | implemented | One pinned nightly; the kernel on the bare-metal target and user programs on sap's own, with `core` and `alloc` built from source so no read-only page carries a relocation. |
| [[prog-rust-std]] | proposed | A `std` for sap: the platform layer mapped onto sap's runtime, the parts left out on purpose, and the order of work. |
| [[prog-composition]] | exploring | Wiring and starting programs: resolution at launch, shared or per-client instances, eager/lazy/manual start, compositions as data, factory imports as the escape hatch, and edge cases. |
| [[prog-representation]] | exploring | A canonical typed IR in the store, lowered to native code cached by (IR hash, target hash); Wasm as one lowering target; MLIR as infrastructure. |
| [[prog-toolchain-and-verification]] | exploring | Why AI should write specs and source, not assembly; verified deterministic lowering; AI-written optimizations as checked objects; where Bend fits. |
