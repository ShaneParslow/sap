---
status: hub
summary: External systems sap borrows from or measures itself against, one page each - what it is, what sap takes, what sap avoids.
updated: 2026-10-08
---

# Prior art

The chat's summary of where sap sits: **roughly Plan 9 with typed interfaces and capabilities, using Fuchsia's architecture**, with seL4 as the kernel model and Sandstorm as the closest precedent for the store ([[2026-09-27-future-of-computing#m19|§m19]]). Ordered from kernel-level systems to whole OSes to narrower libraries and languages.

| System | Relevance |
|---|---|
| [[sel4]] | The kernel model: capabilities, derivation tree, MCS scheduling. A candidate base ([[kernel-choice]]). |
| [[fuchsia]] | Closest overall architecture: handles, typed IDL, per-component namespaces, drivers as components. |
| [[genode]] | Capability component framework with a usable desktop; closest in spirit to user sovereignty. |
| [[plan9]] | Everything a server speaking one protocol, per-process namespaces, `cpu` servers. Untyped. |
| [[sandstorm]] | Cap'n Proto, sturdy refs and a powerbox in a real product. Closest to the store layer. |
| [[cloudabi]] | Capability-based process startup with typed arguments. |
| [[urbit]] | Canonical semantics with a native cache ("jets"), and what goes wrong without verification. |
| [[wasm-component-model]] | WIT and worlds: typed interfaces between components. Source of the ownership and world ideas. |
| [[linux-ipc-history]] | kdbus, bus1, dbus-broker, varlink, memfd sealing: what Linux tried and what stuck. |
| [[flow-ipc]] | Zero-copy shared-memory IPC with Cap'n Proto on Linux; evidence of demand and of the trust gap. |
| [[bend]] | A language with required proofs of declared laws; the language side of the spec split. |
