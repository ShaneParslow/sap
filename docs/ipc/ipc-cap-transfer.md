---
status: proposed
summary: How the kernel's IPC primitives (endpoints for Send/Recv/Call, notifications for Signal/Wait) relate to shared-memory rings, and how capabilities travel with ring messages - they can't go through shared memory, so the kernel moves them in batches into a per-connection receive range that ring messages refer to.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]"]
---

# Kernel IPC and capability transfer

The human asked how send, receive and signal work, and whether they are separate from IPC ([[2026-10-08-22c3947b#m18|§m18]]). They are not separate: they are the kernel half of IPC, and the shared-memory rings of [[ipc-channels]] are a userspace transport built on them. This page covers how the two halves divide the work, and the one thing rings cannot do on their own, which is carry capabilities. Proposed; nothing here is built.

## Two kernel primitives

**Endpoints** carry synchronous messages: `Send`, `Recv`, `Call` (send and wait for the reply) and `ReplyRecv` ([[kernel-objects-and-syscalls]]). A message is a few words in registers plus the rest in the sender's IPC buffer, copied by the kernel into the receiver's. An endpoint message can carry capabilities, and on `Call` the caller lends its scheduling context to the callee ([[kernel-scheduling]]). The endpoint capability's badge tells the receiver which client or object a message is for.

**Notifications** carry no data: a word of bits that `Signal` ORs in and `Wait` collects. They are doorbells. An interrupt also arrives as a notification ([[hw-devices-and-io]]).

A **ring** is a shared frame mapped into both processes, plus a notification in each direction. The sender writes a message into the ring and signals only if the receiver may be asleep; the kernel is not involved per message ([[ipc-channels]]).

## Which path carries what

| | Endpoint `Call` | Ring |
|---|---|---|
| Kernel entries per message | one round trip | zero while both sides are busy; one `Signal` to wake a sleeper |
| Message size | IPC buffer, about a page | ring slot size, or a transferred buffer for large data |
| Capabilities | carried by the kernel | need the side path below |
| CPU time | caller's, donated | the receiver's own |
| Good for | control calls, setup, low-rate calls with capabilities, latency-sensitive requests | streams, batches, high-rate calls, drivers' data paths |

A connection between two components can use both: the endpoint for setup and occasional synchronous calls, the ring for throughput. The Cap'n Proto RPC layer above sees one connection either way ([[ipc-transports]]).

## Capabilities can't travel through shared memory

A ring is bytes both sides can write. A capability written into it would be the sender's local slot number, meaningless to the receiver and forgeable by anyone who can write the ring. Every capability transfer must go through the kernel, which checks that the sender holds the capability with transfer rights and inserts it into the receiver's table ([[cap-ownership]]).

Options, in order of preference:

1. **Batched transfer into a per-connection receive range (proposed).** The receiver dedicates a range of slots in its capability table to the connection when the connection is set up. The sender makes one syscall, `transfer(conn, [slots], flags)`, which moves or copies the listed capabilities into the next free slots of that range and returns their positions. The sender then writes its ring message, whose Cap'n Proto capability table refers to those positions. Because the transfer happens before the message is published, a receiver that reads the message finds the capabilities already in place. Messages without capabilities, the vast majority, cost no syscall.
2. **Capabilities only on endpoint calls.** Any message carrying capabilities goes over the endpoint instead of the ring. Simplest, and enough for M4–M5, but a ring message can't refer to a capability sent on the endpoint without extra ordering rules.
3. **Kernel-copied channels,** as in Fuchsia: the kernel copies message bytes and handles together. Correct but gives up the zero-copy ring.

Option 2 is the starting point; option 1 replaces it when capability-carrying messages on rings become common (M6). The endpoint path is built: up to 16 capabilities per message, moved or copied, into a receive window the receiver names ([[kernel-ipc#Capability transfer]]). The per-connection receive range for rings is not.

## Details of the proposed transfer

- **Flags per capability:** `move` (the sender's slot is emptied atomically), `copy` (a derived copy; the sender keeps the original), or `borrow` (a derived child capability the sender's runtime revokes when the reply arrives). The kernel doesn't need to know where a call ends: the lender holds the parent and revokes through the derivation tree ([[cap-revocation]]).
- **Batching:** up to 16 capabilities per message, unlike seL4, which delivers one ([[kernel-sel4-differences]]). Decided 2026-10-13 ([[2026-10-08-22c3947b#m34|§m34]]). The receiver can't be flooded, since capabilities only land in the slot range it named; the bound exists because each capability costs kernel work in a call that can't be preempted, so an unbounded count would let one process stall the kernel. Bigger batches go as one CNode holding them, or several messages.
- **Backpressure:** the receive range is finite and counts against the receiver's quotas ([[ipc-resource-limits]]). When it is full the transfer fails and the sender waits, like a full ring. The receiver's runtime frees slots by moving capabilities into its import table's storage or deleting them.
- **Validation:** the receiver checks that every capability index in a message lies in its receive range and that the kernel object type matches the interface the schema expects ([[ipc-validation]]). The type check exists: `CNODE_IDENTIFY` reports what a slot holds without invoking it, which matters because invoking a capability of the wrong type does what that type does; on an endpoint it is an IPC call ([[ipc-page-transfer#Identify before invoking]]).

> [!question] Open
> Whether the receive range is a separate small capability table per connection (cleaner isolation, one more object) or a slice of the process's main table; and whether `transfer` and the doorbell `Signal` should be one syscall.
