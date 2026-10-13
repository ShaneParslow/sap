---
status: decided
summary: The graph is every object in the system - transient, persistent and remote alike - with the store (disk) and the network as backends hosting parts of it. Each object has one host; the kernel makes references between hosts unforgeable but never sees the graph itself.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m20|§m20]]", "[[2026-10-08-22c3947b#m22|§m22]]", "[[2026-10-08-22c3947b#m23|§m23]]"]
---

# The graph

**Decided (2026-10-11):** the **graph** is every object in the system, not only what is on disk. Persistent objects are the part of the graph backed by a block device; remote objects are the part reached over the network ([[2026-10-08-22c3947b#m20|§m20]]). The **store** is the userspace component that hosts the persistent part, one host among many. (The human first used "store" for the whole graph; the names were settled as graph and store in [[2026-10-08-22c3947b#m24|§m24]].) The kernel boundary below is proposed.

## What the graph contains

| Node | What it is | Example |
|---|---|---|
| Value | Immutable, named by the hash of its bytes, typed by a schema hash | a photo, a config struct, a code object, a message |
| Ref | A mutable cell pointing at a value; supports read, compare-and-set, subscribe | `prefs/keybinds`, a document's current version |
| Live object | An object with behavior, reached only by invoking its interface | a window, a socket, a running agent, a block device |

Edges are capability fields (authority to reach a ref or a live object) and hash references between values ([[store-data-model]]). Every node has exactly one **host**: the process that implements it. Refs and live objects are hosted by whoever created them; values can be held by anyone, since a copy is identical by definition.

The graph's operations are therefore not a separate API next to IPC: `get` and `put` act on values, ref operations are calls on a ref's interface, and `invoke` is an ordinary RPC call ([[ipc-transports]]).

## Backends

Where a node lives is a property of its host, not of the node's type:

| Backend | Host | What it holds |
|---|---|---|
| Memory | any process | transient values, refs and live objects; gone when the host exits |
| Disk | the store, over a block device driver | values in an append-only log, persistent refs, the sturdy-ref side table ([[store-on-disk-format]], [[store-sturdy-refs]]) |
| Network | the network bridge, as proxies for objects on other machines | nothing locally except caches ([[ipc-network-rpc]]) |

**Persisting** a transient value means handing it to the store; its hash doesn't change, so nothing that refers to it changes. A ref becomes persistent by having a persistent host. A live object is never saved: if its host is persistent, a sturdy ref to it can be restored later and the host recreates the object.

The home node of a ref ([[store-distribution]]) is the same idea one level up: a ref's host lives on one machine, and that machine orders its updates.

## Where the kernel is involved

Capabilities derive from objects, so the kernel is involved in every reference that crosses a process boundary: the reference bottoms out in a kernel capability (an endpoint, or a channel's frames and notifications), and only the kernel can create or transfer one. What the kernel does not see is the graph: no hashes, values, refs or schemas. Derivation is tracked at two levels:

- **The kernel** tracks kernel objects and capabilities to hosts, with seL4's derivation tree. Deleting a client's connection capability cuts it off from everything that host serves it.
- **Each host** tracks its own objects and the capabilities derived from them, in the import/export tables of each connection. Revoking one object means the host drops it, and every holder sees a broken capability ([[cap-revocation]]).

A host that wants the kernel to enforce per-object identity and revocation can mint a separately badged endpoint capability per object, as seL4 servers do. That costs a kernel table entry per object, paid from the host's memory quota.

The alternative, the kernel owning the graph as KeyKOS and EROS did with a kernel-managed single-level store, was rejected earlier: it is all-or-nothing about persistence, handles network peers and devices poorly, and puts policy in the kernel ([[store-sturdy-refs#Why a userspace store]], [[vision-principles]]).

## Inspecting the graph

The human asked whether the kernel not owning the graph hurts debugging, given that dumping the whole graph should be possible ([[2026-10-08-22c3947b#m22|§m22]]). It doesn't, because a dump is a traversal, and traversal works across hosts:
- **The kernel's view** is the skeleton: every capability table, so which process holds connections to which hosts. A debug build of the kernel can dump it directly; seL4 has similar debug calls.
- **Each host's view** fills in the objects. The language runtime already keeps every connection's import and export tables, so it can answer a standard introspection interface with no work from the program: which objects it hosts, their interface types, and which capabilities each holds. Values are self-describing (schema hash), so the dump is typed and pretty-printable rather than raw bytes.
- **A whole-graph dump** starts at a root capability and asks each host it reaches. A host that is hung or hostile can't be made to answer, but the process manager holds its address space and capability table, so it can freeze the process and read it, the equivalent of a core dump.

Introspection is itself authority, and comes in two strengths:
- **The whole-system view is debug-only.** The human proposed a shim that debug builds insert between the kernel and the first process ([[2026-10-08-22c3947b#m23|§m23]]). The kernel hands the shim every initial capability; the shim keeps copies, starts the process manager, and holds its capability table and address space. Since the process manager holds the capability tables, address spaces and threads of every process it spawns, the shim can reach every process's table and memory from there, together with debug-only kernel calls that dump the capability skeleton. The shim sits outside the process manager, so it still works when the process manager is the thing that's broken. Release builds leave the shim out, so no capability that can see everything exists.
- **Capability-gated introspection stays in release builds.** "Show me everything this program can reach" is an owner-control feature, not a debugging aid ([[vision-owner-control]]). The process manager can answer it from the program's capability table, and a host's introspection interface can be granted to the user's tools like any other capability. Fuchsia's Inspect, structured diagnostics that components publish in production, is the closest existing design.

> [!question] Open
> Whether processes share an in-memory value cache, so transient values are deduplicated by hash across processes and passed as read-only frames; and how a tree of transient refs is promoted to persistent in one step.
