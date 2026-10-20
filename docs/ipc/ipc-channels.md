---
status: proposed
summary: A channel is two single-producer/single-consumer rings in shared memory, one per direction. Covers slot layout, atomic publish, backpressure when full, message size limits, streaming with credits, and the copy-small/transfer-large rule.
updated: 2026-10-19
sources: ["[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m14|§m14]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Channels and rings

A channel connects two trust domains on one machine. That shared-memory channels carry sap's messages is decided; the layout below is the chat's proposal. Notifications (doorbells) and frame mapping come from the kernel ([[kernel-objects-and-syscalls]]); everything else is a userspace convention.


> [!note] Built
> Built as described here, with the details settled by building ([[ipc-rings]]): each direction's indices on pages of their own, records of a two-word header (length, kind, an id matching replies to requests; no sequence number or capability count yet) and an inline body of up to half the ring, padding instead of wrapping, and bodies copied out on receive. Large payloads travel as transferred frames ([[ipc-page-transfer]]); on QEMU, copying beats transfer at every size up to 60 KiB, because each transferred page costs six system calls.

## Layout

Each direction is a separate **single-producer/single-consumer ring** ([[2026-09-27-future-of-computing#m25|§m25]]):
- **Data pages:** mapped read-write by the writer and read-only by the reader.
- **Consumer index:** on a separate cache line or page that the reader can write and the writer can only read.
- **Slots:** a fixed-size header (length, sequence number, count of capability-table indices) followed by an inline body for small messages, or a reference to separate buffer pages for large ones.

One shared region per channel, mapped only into its two ends, so the number of mappings grows linearly with connections rather than with combinations of permissions. The human had worried that paging would need a page set per combination of permission namespaces; per-channel regions avoid that ([[2026-09-27-future-of-computing#m14|§m14]], [[2026-09-27-future-of-computing#m15|§m15]]).

## Publishing

The writer fills a slot, then does a **release-store** of the producer index; the reader does an **acquire-load**. A message is either fully published or not visible at all. This is the human's "mark each message complete, then the receiver reads" ([[2026-09-27-future-of-computing#m14|§m14]]). It handles ordering for a cooperative writer; it does not stop a hostile writer from continuing to modify a published slot, which is the double-fetch problem in [[ipc-validation]].

An "invalid" flag works for cooperative teardown. Against a hostile peer, revoking access means unmapping ([[cap-revocation]]).

## When the ring is full

**Backpressure.** Never drop silently, never grow without bound ([[2026-09-27-future-of-computing#m25|§m25]]):
- a synchronous writer blocks, with a timeout;
- an asynchronous writer gets a would-block result and a notification when space frees.

Unbounded receive-side queues are a memory-exhaustion attack ([[ipc-resource-limits]]).

## Size and streaming

- A single message is bounded by the ring's slot and segment size.
- **Small messages are copied into the ring.** A copy of a few hundred bytes is trivial ([[2026-09-27-future-of-computing#m13|§m13]]).
- **Large payloads travel as separate buffers transferred by ownership:** their pages are granted to the receiver, and the sender loses write access. Xen's grant tables work this way ([[2026-09-27-future-of-computing#m15|§m15]]).
- **Unbounded data is a stream of chunks with credit-based flow control.** Cap'n Proto's streaming calls already work this way.

The copy/transfer crossover point is per-platform and needs measuring ([[2026-09-27-future-of-computing#m17|§m17]], [[2026-09-27-future-of-computing#m25|§m25]]). On QEMU there is none below 64 KiB: transfer as built costs system calls per page, and copying is cheaper at every size ([[ipc-page-transfer#What it costs]]). Hardware numbers are still needed. Nobody serializes anything in either case: the Cap'n Proto layout is the message.

## Waiting

Adaptive: the reader polls while the channel is busy and sleeps on a notification when idle ([[kernel-scheduling]]).

## Capabilities in messages

A slot's header counts the capability-table indices the message carries. The indices refer to the sender's table; the transfer (move or borrow) is carried out by the kernel or the object's host, never by trusting the message ([[cap-ownership]]).

> [!question] Open
> Slot size and ring size defaults; whether Cap'n Proto RPC's own framing (call/return/finish messages, question and answer tables) runs inside the ring unchanged or is replaced by a leaner local protocol; how the two directions of a channel and its doorbells are created in one operation. To settle by building milestone M5 in [[roadmap]].
