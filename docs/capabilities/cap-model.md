---
status: proposed
summary: A capability is an unforgeable, transferable, attenuable reference to an object, typed by an interface and carrying rights; the same model appears as CHERI pointers, kernel handles, RPC references and stored sturdy refs.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Capability model

That sap is capability-based is decided; the specific model on this page was proposed in the design chat and has not been contested. It is the reference for every other area.

## Definition

A capability is **an unforgeable, transferable, attenuable reference to an object, typed by an interface, carrying a set of rights** ([[2026-09-27-future-of-computing#m9|§m9]], [[2026-09-27-future-of-computing#m25|§m25]]).

- **Unforgeable:** the only way to get one is to be given one, or to derive it from one you hold. "Only connectivity begets connectivity."
- **Transferable:** it can be passed to another component in a message, either moved or lent ([[cap-ownership]]).
- **Attenuable:** a holder can derive a weaker capability (fewer rights, a narrower interface, a subrange of memory) and pass that on.
- **Typed by an interface:** a capability names an object *as seen through* a specific interface, identified by the interface's hash ([[iface-versioning]]). The same object can be reachable through different interfaces by different holders.
- **Rights:** a subset of the operations the interface allows (read, write, transfer, and so on).

Holding the capability is the whole of the permission. There are no access-control lists consulted at call time, no user ids, and no global names that carry authority ([[vision-principles]], principle 2).

## The four layers

The same model is implemented at four layers, each relying on the one below for unforgeability ([[2026-09-27-future-of-computing#m25|§m25]]):

| Layer | What it references | Representation | Protected by |
|---|---|---|---|
| CHERI | memory ranges; sealed opaque tokens | tagged pointer in a register or memory | hardware tag bits |
| Kernel | memory regions, threads, address spaces, endpoints, notifications, IRQs, MMIO ranges, scheduling contexts, process factories | index into a per-process capability table | the kernel |
| RPC (Cap'n Proto) | objects hosted by some process: files, windows, sockets, devices, store objects | index into a per-connection import/export table, carried over a kernel endpoint (local) or a connection (remote) | the hosting process, plus the kernel layer |
| Store (sturdy ref) | the same objects as RPC, persistently | token in the store's private side table | the store |

How they connect: a sturdy ref is restored into a live RPC capability ([[store-sturdy-refs]]). An RPC capability travels over a kernel endpoint capability. The memory under a message or buffer is mapped as pages, or handed out as CHERI capabilities inside a compartment ([[ipc-paging-vs-cheri]]). The kernel layer is described in [[kernel-objects-and-syscalls]], CHERI in [[hw-cheri]].

## Rules that hold at every layer

**Every capability is either an index into a table the holder can't write, or a tagged pointer the holder can't forge.** Nothing else carries authority ([[2026-09-27-future-of-computing#m25|§m25]]).

**No integer names carry authority across a boundary.** If a caller needs to refer to something a receiver owns, the receiver hands out a capability, not an index into its own tables. A forged or stale capability then fails in the kernel before the receiver's code runs, which removes out-of-bounds indexing into receiver state and confused-deputy bugs ([[2026-09-27-future-of-computing#m19|§m19]]). With CHERI, the receiver can hand out a sealed capability pointing straight at its internal object ([[hw-cheri]]).

**Applications see typed objects.** In application code, a capability at any layer appears as an ordinary object of a generated interface type. The RPC library holds the kernel capabilities privately ([[2026-09-27-future-of-computing#m25|§m25]]).

**Authority follows reachability, not knowledge.** Knowing an object's content hash does not grant access to it; holding a capability to an object grants access to everything reachable from it ([[store-data-model]]).

**Time and memory are capabilities too.** CPU time (scheduling contexts) and memory quotas are granted like any other authority, which is how agents get hard budgets ([[kernel-scheduling]]).

## Lineage

This is the object-capability model of the E language and its CapTP protocol, which Cap'n Proto RPC descends from, combined with seL4-style kernel capabilities and Plan 9's per-process namespaces taken to their conclusion: what a program can see is exactly what it can touch ([[2026-09-27-future-of-computing#m3|§m3]]). See [[sel4]], [[plan9]], [[sandstorm]].

> [!question] Open
> What rights exist beyond read/write/transfer, and whether rights are a fixed set or defined per interface. The chat did not settle this.
