---
status: decided
summary: The design principles every area applies - standard core, least authority, mechanism in the kernel, honest failure, hardware as accelerator, specs over languages - each with how firmly it is held.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m10|§m10]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Design principles

These are the rules the technical design keeps returning to. They are ordered from most fundamental to most specific. Each says where it came from and whether the human has adopted it (*adopted*) or it was argued in the chat and not contested (*accepted*). When a design question comes up, check the answer against these first.

## 1. A standard core, generated periphery

Data types, interfaces and verified components are standardized and long-lived. Presentation and glue are generated per user. Customization depends on firm interfaces rather than competing with them. *Adopted*: this is the human's thesis ([[2026-09-27-future-of-computing#m2|§m2]], [[2026-09-27-future-of-computing#m3|§m3]]). See [[vision-owner-control]].

## 2. Least authority through object capabilities

Holding a reference is the permission. There is no ambient authority: no global namespace to walk, no user-id checks, no environment inherited by default. A program can reach exactly what it was given. The namespace and the permission system are the same thing. *Adopted*: it is the project's premise ([[2026-09-27-future-of-computing#m3|§m3]]). See [[cap-model]].

## 3. Specify interfaces, not languages

The OS defines the interfaces between components, formally and in a language-neutral way, and stays out of how any language works inside a component. sap is "the glue between all languages and all programs", not a new language. Behavior specifications and proofs belong to languages that support them. *Adopted*: the human's position in [[2026-09-27-future-of-computing#m10|§m10]], reaffirmed in [[2026-09-27-future-of-computing#m28|§m28]] for linking and calling conventions. See [[iface-spec-contents]].

## 4. Mechanism in the kernel, policy in components

The kernel does only what nothing else can: unforgeable authority, memory mapping, wakeups, scheduling handoff. Wire formats, RPC, buses and naming live in user space. Linux's rejected kernel buses are the cautionary example. *Accepted* ([[2026-09-27-future-of-computing#m25|§m25]]). See [[kernel]].

## 5. Design for distribution; local is the special case

Every call may fail, interfaces declare their failure modes, and nothing crosses a boundary except values and capabilities. Code written to survive the network then works on every transport. A crashed driver on one machine is the same partial failure as a lost peer on the network. *Adopted*, with the human's condition that the network must not get in the way of local performance ([[2026-09-27-future-of-computing#m19|§m19]], [[2026-09-27-future-of-computing#m20|§m20]]). See [[iface-failure-semantics]].

## 6. Hardware accelerates; it is never required for correctness

Design for paging, which runs everywhere. CHERI and similar features make the same semantics faster when present. The spec, the wire layout and application code don't change with the hardware; only the transport and kernel do. *Accepted* ([[2026-09-27-future-of-computing#m7|§m7]], [[2026-09-27-future-of-computing#m17|§m17]]); the human wants CHERI support from the start, which is consistent with it ([[sessions/2026-10-08-22c3947b-wiki-setup|2026-10-08 session]]). See [[ipc-paging-vs-cheri]].

## 7. Let whoever has the information decide

Expose control where the program can predict behavior (DMA, scratchpads, accelerator queues); hide it where behavior depends on runtime state (caches, branch prediction, page replacement); where both hold part of the information, the program states intent and the system acts on observation. This is how the chat read the lesson of RISC. *Accepted* ([[2026-09-27-future-of-computing#m7|§m7]]). See [[hw-isa]] and [[hw-memory-topology]].

## 8. Generation may be unreliable if checking is not

Untrusted generators (AI, unaudited compilers) are acceptable when a small trusted checker verifies their output: proof-carrying code, translation validation, conformance tests. Keep the trusted checker small. *Accepted* ([[2026-09-27-future-of-computing#m5|§m5]], [[2026-09-27-future-of-computing#m9|§m9]]). See [[prog-toolchain-and-verification]].
