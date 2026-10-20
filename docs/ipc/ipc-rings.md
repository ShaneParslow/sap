---
status: implemented
summary: The shared-memory channel as built - each direction a single-producer/single-consumer ring of variable-length records in producer-written data pages, with each side's index on a page only it writes; publication by release store; bodies copied out with volatile reads by default, or lent in place from a trusted producer; every header checked against the ring; wake-up flags with sequentially consistent fences; notifications as doorbells; Cap'n Proto messages as record bodies; and what runs on one CPU showed.
updated: 2026-10-19
code: [user/ring/lib.rs, user/loader/lib.rs, user/peer/main.rs, user/init/main.rs, user/interfaces/lib.rs]
sources: ["[[2026-10-08-22c3947b#m33|§m33]]", "[[2026-10-08-22c3947b#m34|§m34]]", "[[2026-10-16-2c1ac53c#m16|§m16]]", "[[2026-10-16-2c1ac53c#m18|§m18]]"]
---

# Shared-memory rings as built

The implementation of the channel design in [[ipc-channels]], as of roadmap milestone M5 ([[roadmap]]): two processes exchange records through rings in shared frames, with notifications as doorbells, and the kernel is involved only when a side has to sleep. It is the `sap-ring` crate (`user/ring/lib.rs`). Record bodies are bytes; the echo service carries Cap'n Proto messages in them. Messages larger than half a ring travel as transferred frames instead ([[ipc-page-transfer]]).

## Layout

Each direction of a channel is a run of contiguous pages (`Layout`, `user/ring/lib.rs:73`), so that each side writes only what it owns:

| Pages | Hold | Producer maps | Consumer maps |
|---|---|---|---|
| producer page | the producer's index and its "waiting for space" flag | read-write | read-only |
| data pages (a power of two) | the records | read-write | read-only |
| consumer page | the consumer's index and its "waiting for records" flag | read-only | read-write |

The indices count words and only grow; a position in the ring is the index modulo its size. Neither side can move the other's index, so a misbehaving peer can garble record contents but not the other side's bookkeeping, as the design requires ([[ipc-channels#Layout]]).

## Records

A record is a two-word header and a body, padded to an even number of words (`record_words`, `user/ring/lib.rs:157`):
- the first header word holds the body's length in bytes and a **kind**, chosen by the application (0 is reserved);
- the second holds an **id**, which the application uses to match a reply to its request.

Records never wrap around the end of the ring. One that doesn't fit before the end is preceded by a padding record (kind 0) that fills it, which the consumer skips. Even lengths guarantee room for that header. A body is at most half the ring, less a header, so a record always fits after at most one padding record (`max_body`). Plain records with a request id, rather than Cap'n Proto's RPC protocol, were the human's choice for M5 ([[2026-10-16-2c1ac53c#m16|§m16]]).

## Sending

`Producer::send` (`user/ring/lib.rs:213`) waits while the ring lacks room for the record and any padding, writes the header and body, and publishes with a **release store** of its index. The consumer writes its own index, so the producer doesn't trust it: the fill is computed with wrapping arithmetic, and an index that has moved past the producer's reads as a full ring (`user/ring/lib.rs:230`), which is no worse than a consumer that stops reading.

A program builds a Cap'n Proto message in a scratch buffer of its own and sends the segment's bytes. Building directly in the ring would avoid that copy, but Cap'n Proto's no-heap allocator (`SingleSegmentAllocator`) zeroes its whole buffer on creation; reused for every message, it re-zeroes only what the last message wrote, so a scratch buffer and a copy cost about the same as building in place and zeroing later.

## Receiving

`Consumer::recv` (`user/ring/lib.rs:297`) waits for a record, then reads the header **once** and checks it before using it (`next`, `user/ring/lib.rs:333`):
- the producer's index, read with an **acquire load**, may be at most one ring ahead of the consumer's; more means the producer broke the rules (`user/ring/lib.rs:343`);
- the record's length may not exceed the largest body, and the record must lie inside what was published and inside the ring.

A violation returns `RingError::Corrupt`, after which nothing on that ring can be trusted. A record whose body is longer than the caller's buffer is skipped and reported as `TooLong`. Otherwise the body is **copied out with volatile reads** into the caller's buffer, and the record is released with a release store of the consumer's index.

The copy is required because the producer can still write a published record, and in Rust reading memory another process writes through ordinary references is undefined behavior, which also lets the compiler re-load a value it already checked ([[ipc-validation#Double fetch]]). Cap'n Proto then reads the private copy, its checked accessors guarding against malformed contents.

**In place, for a trusted producer.** `Consumer::recv_in_place` (`user/ring/lib.rs:320`) lends the body to a closure without copying and releases the record when the closure returns. It is `unsafe`: the caller promises the producer never writes what it published, which holds only for a producer in the same trust domain ([[ipc-transports]]).

## Waiting

A consumer that finds the ring empty polls briefly (`SPINS`, `user/ring/lib.rs:56`), then sets its "waiting" flag, issues a sequentially consistent fence, and looks once more before sleeping on its notification. A producer, after publishing, fences and signals the consumer's notification only if that flag is set. The fences on both sides ensure that either the producer sees the flag or the consumer sees the new record, so a wake-up can't fall between the consumer's last check and its sleep. The producer waits for space the same way, with the roles swapped. A side signals only when the other has said it is waiting, so neither makes a system call while the other keeps up ([[ipc-channels#Waiting]], [[kernel-scheduling#Waiting]]). A notification's bits accumulate, so a stale signal only causes one extra check.

Each process has one notification for everything it waits for, in both directions. Waking up means "look at your rings".

## Setting one up

Today the root task sets up each channel by hand (`connect_peer` in `user/init/main.rs:939`):
- it retypes two notifications and mints a badged signal capability for each;
- it loads an instance of `user/peer` ([[prog-runtime#The loader]]), telling it the ring size and what to do;
- for each page of both rings it retypes a frame, maps it into its own address space and, through a copy of the capability, into the peer's at an agreed address, writable only on the side that owns the page;
- it gives the peer its notification and the doorbell to `init`'s;
- for page transfers, it gives the peer an endpoint to receive on, a reply object, and capabilities to its own address space and CNode, and adds page tables where transferred frames will be mapped.

Channel creation as one operation is still an open question ([[ipc-channels]]).

## What runs show

- **Word records** (`check_ring`, `user/init/main.rs:1037`): `init` sends 10,000 records of two words, with at most 48 in flight, through one-page rings; the peer (`echo_words`, `user/peer/main.rs:88`) sends each back with its words incremented, and `init` checks order, contents and a checksum. On QEMU with one CPU each side slept about 420 times, roughly once per 24 records, the same as the fixed-slot rings before.
- **The echo service** (`check_echo_service`, `user/init/main.rs:1109`): 300 Cap'n Proto requests of varying size (180 KiB in all, text, number lists and blobs up to 900 bytes) through two-page rings to a peer with no heap (`serve_echo`, `user/peer/main.rs:134`), which answers each with a summarising reply; `init` reads alternate replies copied and in place and checks every field. A request that isn't Cap'n Proto, and one longer than the peer's buffer, each come back as an error record while the peer carries on.
- **A lying peer** (`misbehave`, `user/peer/main.rs:235`): its reply isn't Cap'n Proto, which fails to parse, and it then publishes a header claiming a body longer than what it published, which `init` refuses as `Corrupt`.

On one CPU, polling can't help, because the other side can't run until this one sleeps or its time slice ends. What keeps sleeps rare is batching: the producer fills the window before the consumer runs. With several CPUs, the consumer's polling would catch records while the producer is still writing. The adaptive wait is designed for that case, and it needs measuring once sap runs on more than one hart.

> [!todo] Not built
> Capabilities alongside ring records (they travel over an endpoint for now, [[ipc-cap-transfer]]); creating a channel in one operation; measurement on several CPUs.
