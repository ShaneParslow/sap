---
status: hub
summary: The interface layer is the universal part of sap - language-neutral specs written as Cap'n Proto schemas with ownership annotations, versioned by hash, with generated bindings and honest failure semantics.
updated: 2026-10-08
---

# Interfaces

The human identified formal, machine-readable interface specifications, with intent attached, as the biggest single lever in the design: the one thing that has to be universal ([[2026-09-27-future-of-computing#m8|§m8]]). Every boundary in sap (kernel, drivers, object store, applications) is described this way. Pages go from what a spec contains, to the concrete format, to how specs evolve, to how languages use them, to what calls promise.

| Page | Status | Summary |
|---|---|---|
| [[iface-spec-contents]] | decided | What an interface spec contains (types, operations, protocol, failure modes, intent, identity, conformance tests) and what it deliberately leaves to languages. |
| [[iface-capnp]] | decided | Cap'n Proto is the schema language, wire format and RPC. Why it replaced WIT, its properties and weaknesses, and the `$own`/`$borrow` annotations. |
| [[iface-versioning]] | proposed | Interfaces identified by hash, append-only evolution, mechanical compatibility checks, conformance suites, adapters. |
| [[iface-bindings-and-languages]] | decided | Code calls interfaces and never sees the transport; bindings are generated per language; linking and calling conventions are each language's problem. |
| [[iface-failure-semantics]] | proposed | Every call can fail; idempotency is declared; concurrency follows E's event-loop model. The Sun "Note on Distributed Computing" as the basis. |

The interface layer sits on top of [[cap-model]] (capabilities are a type in every spec) and underneath [[ipc]] (which carries the messages).
