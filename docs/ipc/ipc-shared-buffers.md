---
status: proposed
summary: Raw shared memory outside messages (graphics, audio, GPU buffers) is a capability to a memory object with a declared layout schema and explicit synchronization; the OS guarantees memory safety and revocation, not that the bytes make sense.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Shared buffers

Some data can't reasonably move as messages: graphics buffers passed between compositor and clients, audio rings, GPU memory. Doing it all as IPC would mean copying whenever data flows in more than one direction. The human concluded raw shared memory between processes is necessary, and proposed keeping meaning at the semantic layer ("this executable opens shared memory using schema `h:…`, an image buffer with this resolution and encoding") even though the hard message structure is lost ([[2026-09-27-future-of-computing#m28|§m28]]). That is how Wayland already works: the buffer travels as a dma-buf file descriptor, and its format, modifier and stride travel in a typed protocol message beside it ([[2026-09-27-future-of-computing#m29|§m29]]).

## Design

([[2026-09-27-future-of-computing#m29|§m29]])
- **A shared region is a capability to a memory object.** The kernel enforces its size, rights (read-only or read-write per holder), lifetime and revocation.
- **Its type can carry a layout schema:** `buffer<h:image-layout>`, with resolution and encoding as typed metadata. The validated message structure is lost; the declared meaning is kept.
- **Synchronization is explicit:** fences signal when a buffer is ready. Wayland has been moving to explicit sync for the same reason.
- **Reading structured data in a shared region** follows the single-fetch rule, or the region uses a Cap'n Proto layout read through generated accessors ([[ipc-validation]]).
- **In Rust,** generated bindings for shared regions use raw pointers or volatile access, never `&[u8]` ([[iface-bindings-and-languages]]).

## What the OS guarantees

The OS guarantees that a hostile peer can't break your memory safety or keep access past revocation. It can't guarantee the bytes make sense. For pixels, a misbehaving writer causes tearing or garbage frames, which is harmless; for structured data, the reader's own discipline applies. So shared buffers are partly an application concern and partly an OS guarantee: the human's framing was that the OS's job here is closer to preventing denial of service than enforcing meaning ([[2026-09-27-future-of-computing#m28|§m28]]).

## Relation to channels and CHERI

Large messages in [[ipc-channels]] use the same mechanism (separate buffer pages granted by capability), but with ownership transfer so the receiver can trust the contents. A shared buffer is the case where both sides keep access. With CHERI, a shared buffer is a bounded capability rather than mapped pages, and can be narrowed to a sub-range or to read-only for a particular holder ([[hw-cheri]]).
