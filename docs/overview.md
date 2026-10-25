---
status: hub
summary: What sap is, how its layers fit together, what is decided versus open, and where to start reading.
updated: 2026-10-24
---

# sap

**sap is for sapabilities.** It is a capability-based operating system in which every piece of authority is an unforgeable reference, components talk through formally specified Cap'n Proto interfaces carried over shared-memory rings, and a content-addressed object store replaces the file system. It runs on 64-bit RISC-V, on QEMU and on one real board, and on CHERI-RISC-V under emulation.

The design began in one long chat ([[2026-09-27-future-of-computing]], summarized in [[2026-09-27-0ce860ab-future-of-computing]]) and is refined as it is built; its status is tracked page by page.

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

## What is built

The kernel and the first userspace, as of October 2026 ([[roadmap#Done]]). The kernel boots on QEMU's `virt` machine and, unchanged, on an Allwinner D1 board ([[hw-d1-board]]). It has capability tables, untyped memory, address spaces, threads, notifications, and endpoints with capability transfer and fault delivery ([[kernel]]). `init`, the root task, is the process manager: it serves a process factory that starts programs from a boot pack by hash, built from memory the client provides ([[prog-process-factory]]). Processes exchange Cap'n Proto messages over shared-memory rings, with page transfer for large ones ([[ipc-rings]], [[ipc-page-transfer]]). Built with its `cheri` feature, the kernel runs on emulated CHERI-RISC-V and keeps user capabilities whole ([[hw-cheri]]).

Still design: capabilities inside Cap'n Proto messages and their ownership, interface specs beyond schemas, the object store, and everything above them.

## What is decided

Capabilities everywhere; Cap'n Proto as schema, wire format and RPC (WIT dropped); shared-memory IPC with transport-agnostic bindings; an object store instead of a file system; interface specs universal, behavior specs on the language side; linking and calling conventions left to languages; RISC-V first with CHERI support; bare metal rather than a Linux prototype; sap's own kernel rather than seL4; a userspace process manager whose process factory is its interface, with code named by its BLAKE3 hash; CHERI on the RVY draft of the RISC-V standard. Dates and reasons are in [[decisions]].

## What is open

The nearest are capabilities inside Cap'n Proto messages, with `$own`/`$borrow` (M6), and how to move large messages cheaply, since page transfer as built loses to copying at every size ([[ipc-page-transfer]]). Everything unresolved is in [[open-questions]]; the planned work, in order, is in [[roadmap]].

## Reading order

[[vision-principles]] → [[cap-model]] → [[kernel]] → [[iface-spec-contents]] → [[ipc-transports]] → [[store-data-model]]. The full catalog is [[index]]. How the wiki itself works is in [[schema]], [[workflows]] and [[style]].
