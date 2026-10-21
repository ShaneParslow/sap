---
status: implemented
summary: Messages too large for a ring travel as whole frames whose capabilities the kernel moves to the receiver - the call protocol, why an unmapped frame with no other capability can be read in place, identifying a capability before invoking it, and what QEMU measured (copying wins at every size up to 60 KiB while each page costs six system calls).
updated: 2026-10-19
code: [user/ring/transfer.rs, user/peer/main.rs, user/init/main.rs, user/sys/cnode.rs, user/loader/lib.rs, kernel/invocation.rs, kernel/cspace.rs, kernel/arch/riscv/caps.rs, abi/lib.rs]
sources: ["[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-10-16-2c1ac53c#m16|§m16]]", "[[2026-10-16-2c1ac53c#m22|§m22]]"]
---

# Page transfer

A message too large for a ring travels as whole 4 KiB frames: the sender writes it into frames it owns, the kernel moves their capabilities to the receiver, and the receiver reads the message in place, knowing nobody else can write it. This is the "transfer large" half of the copy-small, transfer-large rule in [[ipc-channels#Size and streaming]], built in roadmap milestone M5 ([[roadmap]]) as the `transfer` module of `sap-ring` (`user/ring/transfer.rs`). On QEMU it is slower than copying at every size measured; see [[#What it costs]].

## Protocol

A transfer is a call on the receiver's endpoint, announced by a ring record so that the receiver, which waits on its rings, knows to take it:

1. The sender maps the frames it needs read-write, writes the message (zeroing the rest of the last page, which would otherwise show an older message), and unmaps them (`Sender::call`, `user/ring/transfer.rs:117`).
2. It publishes a record on the ring saying a transfer with this id follows, then calls the receiver's endpoint with the message length and the id as words, moving the frame capabilities with the call (at most 16, the kernel's limit per message, so 64 KiB).
3. The receiver sees the record and receives the call; the capabilities land in its receive window ([[kernel-ipc#Capability transfer]]).
4. It checks each capability and maps it read-only (`Receiver::read`, `user/ring/transfer.rs:184`), lends the bytes to the application, and unmaps them.
5. It replies to the call, moving the frames back (`give_back`), and publishes its answer as an ordinary ring record. Replying first matters: a sender blocked in its call can't drain the ring, so an answer published first could wait forever for space.
6. The sender checks what came back the same way the receiver checked what it got, and reuses the frames.

The echo service takes requests of up to 60 KiB this way (`take_transfer`, `user/peer/main.rs:171`). Plain messages with an id, rather than Cap'n Proto RPC, carry it, as for the rings ([[ipc-rings#Records]]).

## Why the receiver can read in place

Reading through ordinary references is sound only for memory nobody else writes while it is read ([[ipc-validation#Double fetch]]). The receiver establishes that for each frame with three facts:
- **It is an unmapped 4 KiB frame.** A frame capability records its one mapping, and moving it keeps the mapping, so a sender could move a frame it still has mapped writable. The capability must be unmapped.
- **Its capability is the only one to the frame.** The capabilities to one object form one contiguous run in the derivation list, because copies are inserted next to their source, so the kernel answers this by comparing the capability's two neighbours (`is_final`, `kernel/cspace.rs:274`, which it already uses to decide when to destroy an object). That holds only if untyped memory can't hand out the same memory twice: copying untyped memory marks the original full, as seL4 does ([[kernel-capabilities#Retyping untyped memory]]); before that rule existed, a sender could have kept a copy of the untyped memory and retyped a second, unrelated frame over the same bytes.
- **The receiver then maps it itself, read-only.** That mapping is now the frame's only one.

Checking once is enough. A new capability to the frame can only be derived from an existing one, and the receiver holds the only one, so none can appear while it reads.

The check trusts two kinds of holder, both of which already have power over the receiver:
- whoever holds a capability to the receiver's own CNode, normally its parent, could copy a frame capability out after the check;
- whoever holds the untyped memory the frames came from, normally the sender's parent, can revoke them. That unmaps them, and the receiver then faults rather than reading changed memory, which is a denial of service.

Device DMA bypasses capabilities until an IOMMU is configured ([[hw-devices-and-io]]). The same argument is what [[vision-attestation-and-verification#Many verifiers, fine-grained claims]] needs.

## Identify before invoking

Invoking a capability performs its type's operation, and on an endpoint `CALL` is an IPC call. A receiver that tried to map a capability it was sent, which turned out to be an endpoint, would call that endpoint and block. So a received capability is never invoked before its type is known. `CNODE_IDENTIFY` (`kernel/invocation.rs:152`), a CNode invocation addressed through the receiver's own CNode, reports what a slot holds without invoking it. It returns:
- the object type, as `object` numbers (`Cap::object_type`, `kernel/cap.rs:120`);
- `EXCLUSIVE`: no other capability names the object. It is never set for untyped memory, which other untyped capabilities can cover;
- `MAPPED`: a frame or page table that is mapped (`ArchCap::is_mapped`, `kernel/arch/riscv/caps.rs:75`).

`SlotRef::identify` (`user/sys/cnode.rs:72`) wraps it. A frame passes the transfer check if it is a 4 KiB frame, exclusive and unmapped (`Frames::check`, `user/ring/transfer.rs:78`). seL4 has only a debug-build `seL4_DebugCapIdentify`, and no query for exclusivity; both are sap additions ([[kernel-sel4-differences]]).

The sender checks returned capabilities the same way, because the receiver could hand back something other than the frames. Anything wrong is deleted and reported as `Lost`.

## What it costs

`measure_transfer` (`user/init/main.rs:1270`) times 64 round trips per size. Each moves `n` bytes to a peer, which reads every byte once and answers with a checksum:
- **ring copy:** init copies the bytes into a ring of 32 data pages, the peer copies them out and sums them;
- **page transfer:** init copies the bytes into its frames, and the peer sums them in place.

QEMU 11.1 on one CPU, release build, ranges over four boots:

| Bytes | Ring copy | Page transfer |
|---|---|---|
| 256 | 25–35 µs | 84–105 µs |
| 4,096 | 34–56 µs | 101–141 µs |
| 8,192 | 42–72 µs | 164–209 µs |
| 16,384 | 65–116 µs | 299–391 µs |
| 32,768 | 98–131 µs | 558–658 µs |
| 61,440 | 151–232 µs | 987–1,224 µs |

Copying wins at every size up to the 60 KiB both paths can carry. A transfer costs about 90 µs plus about 70 µs per page. Copying costs about 30 µs plus about 10 µs per page, for two copies and a checksum.

The per-page cost is system calls. Each page takes six: on the sender, map, unmap, and identify when it comes back; on the receiver, identify, map and unmap. On top of those, each message takes one call and one reply. In the same runs, an identify took 8–14 µs and a map plus unmap 26–42 µs.

These are QEMU's proportions, not hardware's. Under emulation a system call costs microseconds and copying runs at close to host speed, so the measurement exaggerates the cost of transfer relative to copying. It does not locate the crossover on hardware ([[hw-isa#Real hardware]]).

What does carry over is the shape: as built, the cost of a transfer grows with the number of pages, the same way copying does. On hardware a system call costs on the order of hundreds of nanoseconds and copying a page about as much or less, though neither was measured here. Transfer is unlikely to win until its cost is per message rather than per page. The options for that are listed under [[#Open questions]].

## Alternatives

- **Everything by page transfer.** Raised as a way to speed things up ([[2026-10-16-2c1ac53c#m16|§m16]]). The measurement says the opposite at the sizes most messages have: per-page system calls cost more than copying, and a ring needs no system call at all while both sides keep up ([[ipc-rings#Waiting]]).
- **One frame mapped into both processes.** This is what the rings already do. The sender can then keep writing, so a receiver in Rust must copy out or use a volatile reader ([[ipc-validation#Double fetch]]); it suits a trusted peer, not an untrusted one.
- **A grant-table scheme, as in Xen,** in which the receiver maps pages the sender grants by reference. It needs the same "the sender can't write it now" guarantee, and Xen doesn't give it: the granting domain keeps its own mapping. Ownership transfer is the stronger form.
- **Leaving frames with the receiver** instead of returning them. That spends the sender's memory on every message, since untyped memory is only reclaimed by revoking it, so the frames go back.

## Open questions

> [!question] Open
> - **A per-message cost.** Six system calls per page is the main cost ([[2026-10-16-2c1ac53c#m26|§m26]]). In order of ambition, the options are:
>   - range invocations: identify-and-map 16 frames in one call, which also checks exclusivity, and unmap them in one. That makes it four calls per message plus the call and reply.
>   - remapping as part of the IPC: frames named in a call are unmapped from the sender, checked, and mapped read-only into a window the receiver named, and the reply maps them back. That leaves only the call and the reply, plus the TLB flush that unmapping needs. It is L4's model, in which IPC carries map and grant items for flexpages; seL4 dropped it for capability transfer.
>   - 2 MiB frames for large messages.
>
>   Which of these is worth the kernel complexity depends on hardware numbers. CHERI removes the mapping changes altogether ([[ipc-paging-vs-cheri]]).
> - **Building in place.** The sender writes into the frames, but it has to know the message's length before it maps them. Building a Cap'n Proto message directly in the frames needs a size estimate, or mapping all 16 frames.
> - **Messages over 64 KiB:** several calls, larger frames, or a raised capability limit.
> - **A receiver that blocks.** The receiver receives the announced call with a blocking receive, so a sender that publishes the record and never calls stalls that connection's receiver. Binding the receiver's notification to its thread, so that one receive waits for both rings and calls ([[kernel-ipc#Bound notifications]]), would remove that.
> - **Large replies.** Only requests travel by transfer so far. A reply would need the server to send frames back through the reply, or a call in the other direction.
