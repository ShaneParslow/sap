---
status: implemented
summary: The first shared-memory channel as built - a pair of single-producer/single-consumer rings between two processes, each direction a producer-owned data page and a consumer-owned control page, publication by release store, wake-up flags with sequentially consistent fences, notifications as doorbells, and what 10,000 round trips on one CPU showed.
updated: 2026-10-13
code: [user/libsap/ring.rs, user/libsap/process.rs, user/peer/main.rs, user/init/main.rs]
sources: ["[[2026-10-08-22c3947b#m33|§m33]]", "[[2026-10-08-22c3947b#m34|§m34]]"]
---

# Shared-memory rings as built

The first implementation of the channel design in [[ipc-channels]], completing roadmap milestone M4 ([[roadmap]]): two processes exchange messages through rings in shared frames, with notifications as doorbells, and the kernel is involved only when a side has to sleep. It lives in `libsap` (`user/libsap/ring.rs`). Messages are still at most seven words; Cap'n Proto messages and large buffers are M5.

## Layout

Each direction of a channel is two pages, so that each side writes only what it owns:

| Page | Holds | Producer maps | Consumer maps |
|---|---|---|---|
| data | the producer's index, its "waiting for space" flag, and 63 slots of 64 bytes (`SLOTS`, `user/libsap/ring.rs:35`) | read-write | read-only |
| control | the consumer's index and its "waiting for messages" flag | read-only | read-write |

A slot is a length word and up to seven message words. Neither side can move the other's index, so a misbehaving peer can garble message contents but not the ring's bookkeeping on the other side. This follows the design's rule that the consumer index sits on a page the reader writes and the writer only reads ([[ipc-channels#Layout]]).

## Sending and receiving

`Producer::send` (`user/libsap/ring.rs:90`) waits while the ring is full, writes the slot, and publishes it with a **release store** of its index. `Consumer::recv` (line 134) reads the producer's index with an **acquire load**, copies the slot out (reading the length word once and bounding it, so a producer rewriting the slot can't change what was checked; the single-fetch rule of [[ipc-validation]]), and releases the slot with a release store of its own index.

**Sleeping and waking without losing a signal.** A consumer that finds the ring empty polls briefly (`SPINS`, line 37), then sets its "waiting" flag, issues a sequentially consistent fence, and looks once more before sleeping on its notification. A producer, after publishing, fences and signals the consumer's notification only if that flag is set. The fences on both sides ensure that either the producer sees the flag or the consumer sees the new message, so a wake-up can't fall between the consumer's last check and its sleep. The producer waits for space the same way, with the roles swapped. A side signals only when the other has said it is waiting, so neither makes a system call while the other keeps up ([[ipc-channels#Waiting]], [[kernel-scheduling#Waiting]]). A notification's bits accumulate, so a stale signal only causes one extra check.

Each process has one notification for everything it waits for, in both directions. Waking up means "look at your rings".

## Setting one up

Today the root task sets up the channel by hand (`check_ring` in `user/init/main.rs:876`):
- it retypes four frames and two notifications, and mints a badged signal capability for each notification;
- it loads a second instance of `user/peer` ([[prog-libsap#The loader]]);
- it maps the four frames into its own address space and, through copies of their capabilities, into the peer's with the complementary rights (`Process::map_frame`);
- it gives the peer its notification and the doorbell to `init`'s.

The peer's `echo_ring` (`user/peer/main.rs:44`) finds the rings at an agreed address. Channel creation as one operation is still an open question ([[ipc-channels]]).

## What a run shows

`init` sends 10,000 two-word messages with at most 48 in flight, the peer sends each back with its words incremented, and `init` checks order, contents and a checksum. On QEMU with one CPU, each side slept about 420 times: roughly once per 24 messages.

On one CPU, polling can't help, because the other side can't run until this one sleeps or its time slice ends. What keeps sleeps rare is batching: the producer fills the window before the consumer runs. With several CPUs, the consumer's polling would catch messages while the producer is still writing. The adaptive wait is designed for that case, and it needs measuring once sap runs on more than one hart.

> [!todo] Not built
> Cap'n Proto messages in the rings, and large payloads as transferred buffers (M5); capabilities alongside ring messages (they travel over an endpoint for now, [[ipc-cap-transfer]]); creating a channel in one operation; measurement on several CPUs.
