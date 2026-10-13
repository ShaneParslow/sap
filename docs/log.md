---
status: log
summary: Append-only chronological log of wiki operations - ingests, sessions, lint passes. Grep with `grep "^## \[" log.md`.
updated: 2026-10-11
---

# Log

Append new entries at the bottom. Each starts `## [YYYY-MM-DD] <kind> | <title>`, where kind is `setup`, `ingest`, `session`, `lint` or `query`.

## [2026-10-08] setup | Wiki created

Schema, workflows and style written ([[schema]], [[workflows]], [[style]]); `tools/wiki/` (chat2md, pending, lint) and the SessionStart hook added. `docs/llm-wiki.md` moved to `raw/references/`. Session: [[2026-10-08-22c3947b-wiki-setup]].

## [2026-10-08] ingest | Future of computing with AI automation (claude.ai, 2026-09-27 to 2026-10-08)

Full 32-turn chat exported to [[2026-09-27-future-of-computing]] and diffused into every area: vision, capabilities, hardware, kernel, interfaces, IPC, store, programs, system, prior art, plus [[roadmap]], [[open-questions]], [[decisions]], [[glossary]]. Summary: [[2026-09-27-0ce860ab-future-of-computing]].

## [2026-10-11] session | Design pass: process authority, capability transfer, loading, network, store distribution, seL4

Design questions from the human ([[2026-10-08-22c3947b#m18|§m18]]) answered as proposals. New: [[kernel-sel4-differences]], [[cap-process-authority]], [[ipc-cap-transfer]], [[prog-loading]], [[ipc-network-rpc]], [[store-distribution]], [[store-use-cases]]. Corrected the seL4 capability-per-message limit (one, not a few) after reading seL4 `6df0b6e`. Impact sweep across store, kernel, hardware, IPC, interfaces, programs and system pages; roadmap gained D5 and D6. Session: [[2026-10-08-22c3947b-wiki-setup]].

## [2026-10-11] session | Decisions on seL4, process factory, network properties; the store as the object graph

Recorded the human's decisions from [[2026-10-08-22c3947b#m20|§m20]] in [[decisions]]. New [[store-object-graph]]; "store" now means the whole object graph and the disk component is the persistence service (swept through store, capability, roadmap and overview pages). Address spaces reopened in [[prog-loading]]; `$merge` and `$local` annotations proposed in [[iface-capnp]].

## [2026-10-11] session | Graph inspection, on-demand start, package registry

Filed [[2026-10-08-22c3947b#m22|§m22]]: [[store-object-graph#Inspecting the graph]], [[kernel-process-model#Starting on demand]], new [[system-package-registry]]; merge rules deferred in [[iface-capnp]]; position independence decided.

## [2026-10-11] session | Composition; graph/store/home naming

New [[prog-composition]] from [[2026-10-08-22c3947b#m24|§m24]]. Renamed "persistence service" to store, "store" (whole) to graph, "authoritative node" to home node across live pages.

## [2026-10-11] session | M2 complete: paging and user mode

New [[kernel-memory-riscv]] and [[kernel-root-task]]; rewrote [[kernel-boot-riscv]] and [[kernel-traps-riscv]] (verified `1be5924`); roadmap M2 done; user-interface stack added to [[open-questions]].
