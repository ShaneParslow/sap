---
status: hub
summary: How messages move - Cap'n Proto messages in shared-memory rings between trust domains, direct calls inside one, the network between machines - with validation at trust boundaries and bounded resources.
updated: 2026-10-19
---

# IPC

sap's IPC is Cap'n Proto messages in shared-memory channels, with the kernel involved only for setup, capability transfer and wakeups. The spec and wire layout are fixed; the transport is chosen per connection ([[2026-09-27-future-of-computing#m17|§m17]]). Pages follow the data path: which transport, then how a channel carries a message, then what the receiver must check, then shared buffers outside messages, then limits; the last page compares how all of this changes with CHERI.

| Page | Status | Summary |
|---|---|---|
| [[ipc-transports]] | decided | Direct call, shared-memory channel, network: chosen per connection by trust domain. |
| [[ipc-channels]] | proposed | Ring layout, publishing, backpressure, message size, streaming, and copy versus page transfer. |
| [[ipc-rings]] | implemented | The channel as built: rings of variable-length records in producer-written pages, each index on a page only its side writes, bodies copied out (or lent in place from a trusted producer), Cap'n Proto messages as bodies, notifications as doorbells. |
| [[ipc-page-transfer]] | implemented | Messages too large for a ring as frames whose capabilities move to the receiver, which checks each is an unmapped frame with no other capability and reads it in place; slower than copying on QEMU at every size up to 60 KiB. |
| [[ipc-cap-transfer]] | proposed | Endpoints and notifications versus rings, and how capabilities travel with ring messages through a per-connection receive range. |
| [[ipc-validation]] | proposed | What a receiver must still check, double fetch, Rust soundness, and keeping validation cheap. |
| [[ipc-shared-buffers]] | proposed | Raw shared memory (graphics, audio, GPU) as capabilities with a declared layout and explicit sync. |
| [[ipc-resource-limits]] | proposed | Flooding, table exhaustion, pipelining amplification, stalled borrows: the limits that bound each. |
| [[ipc-paging-vs-cheri]] | proposed | The paging and CHERI variants side by side, software isolation as a middle option, and the mixed end state. |
| [[ipc-network-rpc]] | exploring | Between machines: a bridge component, node identity by public key, network sturdy refs, the missing three-party handoff, OCapN, and a web gateway. |

The kernel's side (endpoints, notifications, frames, donation) is in [[kernel-objects-and-syscalls]]; ownership of capabilities carried in messages is in [[cap-ownership]].
