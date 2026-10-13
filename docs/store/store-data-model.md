---
status: proposed
summary: The store holds immutable content-addressed values forming a DAG, plus a small set of mutable refs reached only through capabilities. Values reference their schema by hash; operations are get, put, invoke and subscribe; knowing a hash grants nothing, holding a capability grants everything reachable from it.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m21|§m21]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Store data model

This page covers values and refs, the data part of the graph; how they sit alongside live objects, and which backend hosts them, is in [[store-object-graph]].

The human's starting idea: everything is key → value associations (system information, programs, user data), the value type is self-describing, and the open questions were the topology, the operations, how programs work and how permissions are handled ([[2026-09-27-future-of-computing#m2|§m2]]). The answers below are what prior art converged on ([[2026-09-27-future-of-computing#m3|§m3]]).

## Topology: values and refs

- **Values** are immutable and content-addressed: a value's name is the hash of its bytes. Values reference other values by hash, forming a DAG.
- **Refs** are a small set of mutable cells, each pointing at a value. Refs give names and state; they are reached only through capabilities.

Git, Nix, IPFS and Unison all arrived at this split independently. Immutable content-addressed data handles caching, sharing, deduplication and integrity in one step; mutable refs handle naming and change. History comes free: updating a ref leaves the old value in place ([[2026-09-27-future-of-computing#m29|§m29]]).

## Self-describing values

Each value carries the **hash of its schema** rather than an inline description. Overhead stays near zero and any receiver can look up what a value means ([[2026-09-27-future-of-computing#m3|§m3]]). Schemas are Cap'n Proto interfaces/structs identified by hash ([[iface-versioning]]), so stored data evolves by the same rules as messages. Values may contain capability fields; see [[store-sturdy-refs]].

The human's screenshot example is the motivating case: sharing a typed object instead of a rendered image of it ([[2026-09-27-future-of-computing#m2|§m2]]). How sharing works is in [[store-granting-access]].

## Operations

Very few ([[2026-09-27-future-of-computing#m3|§m3]]):
- **get** a value by hash (given authority to reach it);
- **put** a value, returning its hash;
- **invoke**: send a message to a capability (including a ref's interface: read, compare-and-set);
- **subscribe** to changes of a ref.

The use-case walkthroughs add requirements that are proposed but not yet part of the model: schema checks on `put`, atomic compare-and-set across several refs (for an agent's reviewed change), cheap branches of a set of refs with typed diffs, and `subscribe` delivering the new value's hash ([[store-use-cases#Requirements collected]]).

Programs are then objects that receive messages (the actor model); E, Spritely Goblins and Cap'n Proto RPC land on roughly this set. The store's own interface is a Cap'n Proto interface like any other.

## Authority follows reachability

**Hashes are names, not authority.** Knowing a hash does not let you read the object: low-entropy data can be guessed from its hash, so treating hashes as credentials leaks information. Instead, holding a capability to an object lets you read everything reachable from it down the DAG, as a readable Git tree gives you its blobs. This is "only connectivity begets connectivity" applied to storage. Writes always go through capabilities to mutable refs ([[2026-09-27-future-of-computing#m21|§m21]]).

## Control plane and data plane

Typed, capability-checked objects carry metadata, interfaces and coordination, where overhead doesn't matter. Bulk data (tensors, video, buffers) is stored as large typed blobs in zero-copy layouts, and an accelerator is handed a capability to a blob and reads it directly. This is the human's "accelerators exposed through the object store" ([[2026-09-27-future-of-computing#m3|§m3]]; [[ipc-shared-buffers]], [[hw-memory-topology]]).

## Hard problems

From [[2026-09-27-future-of-computing#m3|§m3]], each with where it is handled:
- schema evolution ([[iface-versioning]]);
- capability revocation ([[cap-revocation]]);
- garbage collection of a shared object graph (open; [[store-on-disk-format]]);
- human-meaningful naming on top of hashes (open);
- legacy compatibility ([[system-legacy-compat]]).

> [!question] Open
> Topology of refs (a tree rooted at the user's root ref, or a general graph) and naming. Use cases are being walked through in [[store-use-cases]]; distribution across machines is in [[store-distribution]].
