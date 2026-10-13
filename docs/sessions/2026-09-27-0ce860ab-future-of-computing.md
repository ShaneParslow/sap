---
status: complete
summary: The founding design chat (claude.ai, 2026-09-27 to 2026-10-08) - from the future of computing under abundant AI down to capabilities, Cap'n Proto IPC, the object store and how to start building.
updated: 2026-10-08
session: 0ce860ab-d23a-4b9e-a4a5-810423df02ae
transcript: "[[2026-09-27-future-of-computing]]"
---

# 2026-09-27 · Future of computing with AI automation

A 32-turn claude.ai conversation held over eleven days, in which the human read and researched between turns. It started as a speculative question and ended at a build plan. Ingested in full on 2026-10-08 ([[2026-10-08-22c3947b-wiki-setup]]).

## Goal

Sketch what hardware, OS, applications and networking look like if AI makes software engineering free, and what must be built now so that future is open and owner-controlled ([[2026-09-27-future-of-computing#m0|§m0]]).

## Outcome

The chat moved through four stages, each narrowing the last:

1. **Landscape (m0–m5).** Power shifts to keys, data, law and compute. The human's thesis, that customization demands a standardized core and proven code beats regeneration, was adopted. Capabilities, a personal object store and typed interfaces emerged as the foundations to build. Side threads: attestation limits, language-specific ISAs (rejected), AI writing assembly (rejected).
2. **Stack shape (m6–m11).** ISA judged least consequential; memory topology and I/O queues sketched; microkernel with rings confirmed; Wasm rejected as the universal program format; formal interface specs with intent identified as the biggest lever; interface spec split from behavior spec, with behavior on the language side.
3. **IPC and capabilities in detail (m12–m25).** Ownership as types, validation and double fetch, paging versus CHERI, the Sun note on distributed computing, closest existing systems, live versus stored capabilities, WIT dropped for Cap'n Proto, the kernel's real responsibilities, process startup, the four-layer capability model, CHERI sweeps.
4. **Getting started (m26–m31).** A Claude Doc was made from m25 (m27). Then: Cap'n Proto double fetch in Rust, a seL4-sized syscall set, sturdy refs by example, raw shared buffers, and the build order. The human ruled out a Linux prototype; the chat recommended building on seL4, then laid out what that would and wouldn't give.

## Decisions

All dated in [[decisions]]. Adopted by the human: reuse over regeneration (m2); interface specs and intent as the universal layer, representation decoupled (m8); interface spec at OS level, behavior specs on the language side (m10); transport-agnostic bindings (m14); network as first class with a caveat (m20); Cap'n Proto instead of WIT (m22, m28); no Linux prototype (m28); linking and calling conventions are language problems (m28); raw shared-memory capabilities needed (m28).

Recommended and not adopted or rejected: build on seL4 (m29, m31; open, [[kernel-choice]]); `$own`/`$borrow` annotations (m23; proposed); design for paging with CHERI as accelerator (m17; consistent with the later RISC-V/CHERI choice).

Reversed during the chat: WIT as spec language (m11–m17, reversed m23); Linux prototype (m9, m15, m17, rejected m28); WIT's canonical ABI (rejected m15).

## Pages changed

Created from this chat: all pages under `vision/`, `capabilities/`, `hardware/`, `kernel/`, `interfaces/`, `ipc/`, `store/`, `programs/`, `system/`, `prior-art/`, and [[overview]], [[roadmap]], [[open-questions]], [[decisions]], [[glossary]].

## Open threads

The kernel decision; ownership annotations; spec attachments for intent and tests; ring details; store naming, GC and on-disk format; the powerbox. Full list in [[open-questions]].

## Notes on the source

- m2's attachment is the human's own long essay on their vision; it is the best single statement of their goals and worth rereading.
- m3 read a memory note about **tagd**, the human's earlier semantic file-tagging daemon in Rust, and suggested evolving it toward the object store. The project has since started fresh as sap; tagd is not otherwise part of this wiki.
- m27 created a Claude Doc ("Capability OS: IPC, Kernel, and Capabilities Design Notes") from m25. Its content is a restructuring of m25 plus a search-terms table, both folded into this wiki; the doc itself was not exported.
- Numbers and project statuses in the chat (CHERI ratification timing, Fuchsia releases, Flow-IPC benchmarks) are as of the chat dates and came partly from web searches made in it.
