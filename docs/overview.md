---
status: hub
summary: What sap is, how its layers fit together, what is decided versus open, and where to start reading.
updated: 2026-10-08
---

# sap

**sap is for sapabilities.** It is a capability-based operating system in which every piece of authority is an unforgeable reference, components talk through formally specified Cap'n Proto interfaces carried over shared-memory rings, and a content-addressed object store replaces the file system. The first target is 64-bit RISC-V on QEMU, with CHERI support planned through CHERI-QEMU.

Nothing is implemented yet. The design comes from one long chat ([[2026-09-27-future-of-computing]], summarized in [[2026-09-27-0ce860ab-future-of-computing]]); its status is tracked page by page.

## Why

If AI makes software nearly free, control of computing moves to whoever holds keys, data and the interfaces everything plugs into. sap's bet is that owner-controlled computing needs a small, standardized, capability-secured core under a periphery that can be endlessly customized and generated ([[vision-owner-control]]). The principles that follow from that ([[vision-principles]]) are applied throughout.

## How the parts fit

```
 applications, agents, legacy (contained)        system/, programs/
 ──────────────────────────────────────────
 object store: values by hash, refs,             store/
   capabilities as stored type, powerbox
 ──────────────────────────────────────────
 interfaces: Cap'n Proto schemas,                interfaces/
   $own/$borrow, hash identity, failure model
 ──────────────────────────────────────────
 transports: direct call | shm rings | network   ipc/
 ──────────────────────────────────────────
 kernel: capability tables, frames,              kernel/
   endpoints, notifications, sched contexts
 ──────────────────────────────────────────
 hardware: RISC-V (+ CHERI), virtio, IOMMU       hardware/

 capabilities run through every layer            capabilities/
```

**Capabilities** ([[capabilities]]) are one model at four layers: CHERI pointers, kernel table entries, Cap'n Proto RPC references, and sturdy refs in the store. Each layer relies on the one below for unforgeability.

**The kernel** ([[kernel]]) does only what nothing else can: unforgeable capability tables, memory mapping, wakeups, and handing a caller's CPU time to the callee. It is sap's own kernel, modeled on seL4.

**Interfaces** ([[interfaces]]) are the universal part. Each is a Cap'n Proto schema with ownership annotations, intent, failure modes and a conformance suite, identified by hash. Behavioral proofs are optional and live in languages that support them.

**IPC** ([[ipc]]) picks a transport per connection: direct calls inside a trust domain, shared-memory rings between domains, Cap'n Proto RPC between machines. Code never sees which. Receivers validate structure at trust boundaries; large messages move by page transfer.

**The graph and the store** ([[store]]): the graph is every object in the system (immutable values by hash, mutable refs reached through capabilities, and live objects), whether transient, persistent or remote; the store is the component holding its persistent part on disk. Stored capabilities become sturdy refs whose tokens programs never see; access is granted through a trusted powerbox.

**Hardware** ([[hardware]]) is deliberately boring: a plain RISC core. CHERI makes the same design faster (byte-granular sharing, call-cost compartment switches, sealed handles) but is never required for correctness.

## What is decided

Capabilities everywhere; Cap'n Proto as schema, wire format and RPC (WIT dropped); shared-memory IPC with transport-agnostic bindings; an object store instead of a file system; interface specs universal, behavior specs on the language side; linking and calling conventions left to languages; RISC-V first with CHERI support; bare metal rather than a Linux prototype; sap's own kernel rather than seL4. Dates and reasons are in [[decisions]].

## What is open

The nearest are how capabilities move in messages (`$own`/`$borrow`) and the ring details, both settled by building M4–M6. Everything unresolved is in [[open-questions]]; the planned work, in order, is in [[roadmap]].

## Reading order

[[vision-principles]] → [[cap-model]] → [[kernel]] → [[iface-spec-contents]] → [[ipc-transports]] → [[store-data-model]]. The full catalog is [[index]]. How the wiki itself works is in [[schema]], [[workflows]] and [[style]].
