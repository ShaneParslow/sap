---
status: proposed
summary: Nothing in a message is taken on faith, and every resource a peer can consume is bounded - ring size, capability-table quotas, pipelined-call caps, borrow timeouts, traversal limits, and CPU/memory budgets.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m22|§m22]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Resource limits

The human flagged security as underrated in IPC designs: Cap'n Proto's documentation discusses amplification attacks, ownership claims in messages could be false, and "you can't take much on faith" ([[2026-09-27-future-of-computing#m22|§m22]]). Most libraries in this space solve performance and ignore the trust boundary; Flow-IPC's model, for instance, is a shared arena between processes that trust each other ([[flow-ipc]]). This page is the inventory of what a hostile peer can exhaust and what bounds it.

## Inside a message

Cap'n Proto's own mitigations: a **traversal limit** (total data a reader will follow) and a **nesting-depth limit**, so a small crafted message can't force huge work ([[2026-09-27-future-of-computing#m23|§m23]]). Ownership claims inside a message are never trusted; the table holder performs transfers ([[cap-ownership]]).

## Around messages

The rest of the attack surface surrounds messages ([[2026-09-27-future-of-computing#m23|§m23]]):

| Attack | Bound |
|---|---|
| Message flooding | Bounded rings: a full ring is natural backpressure ([[ipc-channels]]) |
| Unbounded queues on the receiving side | Never queue without bound; refuse or block instead ([[2026-09-27-future-of-computing#m25|§m25]]) |
| Capability-table exhaustion | Per-domain table quotas |
| Unbounded pipelined calls queued on unresolved promises | A cap on queued calls per promise |
| Callee stalls forever while holding a borrow | Caller cancellation (ends the borrow) and timeouts ([[cap-revocation]]) |
| Memory and CPU exhaustion | Budget capabilities: scheduling contexts and memory quotas ([[kernel-scheduling]]) |

Kernel memory accounting falls out of the seL4-style model: all kernel objects are created from untyped memory a component was given, so a component can't make the kernel allocate on its behalf beyond its quota ([[kernel-objects-and-syscalls]]).

> [!question] Open
> Default values for every limit above, and how a component reports or recovers from hitting one (error types in the failure model, [[iface-failure-semantics]]).
