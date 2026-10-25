---
status: implemented
summary: Messages too large for a ring travel as whole frames whose capabilities the kernel moves to the receiver - the call protocol, why an unmapped frame with no other capability can be read in place, identifying a capability before invoking it, and what QEMU and the D1 board measured (copying wins at every size up to 60 KiB; each page costs six system calls, and on the D1 the two TLB flushes dominate).
updated: 2026-10-20
code: [kernel/arch/riscv/paging.rs, user/lib/ring/transfer.rs, user/programs/peer/main.rs, user/programs/check/channels.rs, user/lib/sys/cnode.rs, user/lib/loader/lib.rs, kernel/invocation.rs, kernel/cspace.rs, kernel/arch/riscv/caps.rs, abi/lib.rs]
verified: 8b9862c
sources: ["[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-10-16-2c1ac53c#m16|§m16]]", "[[2026-10-16-2c1ac53c#m22|§m22]]"]
---

# Page transfer

A message too large for a ring travels as whole 4 KiB frames: the sender writes it into frames it owns, the kernel moves their capabilities to the receiver, and the receiver reads the message in place, knowing nobody else can write it. This is the "transfer large" half of the copy-small, transfer-large rule in [[ipc-channels#Size and streaming]], built in roadmap milestone M5 ([[roadmap]]) as the `transfer` module of `sap-ring` (`user/lib/ring/transfer.rs`). On QEMU and on the D1 board it is slower than copying at every size measured; see [[#What it costs]].

## Protocol

A transfer is a call on the receiver's endpoint, announced by a ring record so that the receiver, which waits on its rings, knows to take it:

1. The sender maps the frames it needs read-write, writes the message (zeroing the rest of the last page, which would otherwise show an older message), and unmaps them (`Sender::call`, `user/lib/ring/transfer.rs:117`).
2. It publishes a record on the ring saying a transfer with this id follows, then calls the receiver's endpoint with the message length and the id as words, moving the frame capabilities with the call (at most 16, the kernel's limit per message, so 64 KiB).
3. The receiver sees the record and receives the call; the capabilities land in its receive window ([[kernel-ipc#Capability transfer]]).
4. It checks each capability and maps it read-only (`Receiver::read`, `user/lib/ring/transfer.rs:184`), lends the bytes to the application, and unmaps them.
5. It replies to the call, moving the frames back (`give_back`), and publishes its answer as an ordinary ring record. Replying first matters: a sender blocked in its call can't drain the ring, so an answer published first could wait forever for space.
6. The sender checks what came back the same way the receiver checked what it got, and reuses the frames.

The echo service takes requests of up to 60 KiB this way (`take_transfer`, `user/programs/peer/main.rs:178`). Plain messages with an id, rather than Cap'n Proto RPC, carry it, as for the rings ([[ipc-rings#Records]]).

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

`SlotRef::identify` (`user/lib/sys/cnode.rs:72`) wraps it. A frame passes the transfer check if it is a 4 KiB frame, exclusive and unmapped (`Frames::check`, `user/lib/ring/transfer.rs:78`). seL4 has only a debug-build `seL4_DebugCapIdentify`, and no query for exclusivity; both are sap additions ([[kernel-sel4-differences]]).

The sender checks returned capabilities the same way, because the receiver could hand back something other than the frames. Anything wrong is deleted and reported as `Lost`.

## What it costs

`measure_transfer` (`user/programs/check/channels.rs:327`) times 64 round trips per size. Each moves `n` bytes to a peer, which reads every byte once and answers with a checksum:
- **ring copy:** check copies the bytes into a ring of 32 data pages, the peer copies them out and sums them;
- **page transfer:** check copies the bytes into its frames, and the peer sums them in place.

On QEMU 11.1 (one CPU, release build, ranges over four boots) and on the Allwinner D1 board ([[hw-isa#Real hardware]]: one C906 at 1 GHz, caches on, release build, two boots, which agreed to within 1%):

| Bytes | QEMU copy | QEMU transfer | D1 copy | D1 transfer |
|---|---|---|---|---|
| 256 | 25–35 µs | 84–105 µs | 17.3–17.8 µs | 86.7–87.6 µs |
| 4,096 | 34–56 µs | 101–141 µs | 42.3–42.4 µs | 112.4–113.1 µs |
| 8,192 | 42–72 µs | 164–209 µs | 69.5–69.8 µs | 184.5–186.3 µs |
| 16,384 | 65–116 µs | 299–391 µs | 119.7–120.0 µs | 331.3–331.4 µs |
| 32,768 | 98–131 µs | 558–658 µs | 220.8–221.2 µs | 609.8–612.0 µs |
| 61,440 | 151–232 µs | 987–1,224 µs | 390.7–391.0 µs | 1,079–1,083 µs |

Copying wins at every size up to the 60 KiB both paths can carry, on both. On the D1 a transfer costs about 45 µs plus about 69 µs per page, and copying about 17 µs plus about 25 µs per page, for two copies and a checksum. The lines diverge, so no larger message would reverse the result as built. QEMU's per-page costs were about 70 µs and 10 µs: emulation flattered copying, but only by a factor of 2.5, and transfer's own cost was about right.

The per-page cost is system calls, and mostly the mapping ones. Each page takes six: on the sender, map, unmap, and identify when it comes back; on the receiver, identify, map and unmap. On top of those, each message takes one call and one reply. On the D1 a yield takes 0.7 µs and an identify 0.9–1.0 µs, but a map plus an unmap takes 16.8–17.1 µs, so the two map–unmap pairs are about half of each page's 69 µs. (Uncached, before the caches were turned on, a yield took 10.3 µs and an identify 18.2 µs.)

**The TLB flush is what makes mapping expensive.** Both map and unmap end with `sfence.vma` for the page and address space (`flush_page`, `kernel/arch/riscv/paging.rs:312`). Three throwaway kernels on the D1 located the cost:
- flushing twice instead of once added 1.4 µs per map plus unmap, so the instruction itself costs about 0.7 µs;
- skipping the flush after map cut a map plus unmap from 17 to 12 µs, and a 60 KiB transfer from 1,081 to 861 µs, still more than twice the copy;
- skipping both flushes broke the checks: the C906 does keep stale translations.

So each flush costs about 5 µs, nearly all of it after the instruction, presumably refilling TLB entries it removed. That suggests the C906 flushes more than the one page named, which is inferred, not confirmed. Either way, a design that changes mappings per page pays this per page, and remapping per message (see [[#Open questions]]) would pay it once.

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
>   - no flush after map: the D1 run without it passed every check and saved a fifth of a transfer's cost, but RISC-V lets a core cache a missing translation unless it has Svvptc, so it is only safe with a fault handler that, on a fault at an address whose entry is now valid, flushes and retries (Linux does this for kernel mappings on cores without Svvptc).
>
>   On the D1 the mapping calls, through their TLB flushes, are about half of each page's cost ([[#What it costs]]), so the options that remove per-page mapping changes matter most; range invocations alone would save the call overhead but not the flushes. CHERI removes the mapping changes altogether ([[ipc-paging-vs-cheri]]).
> - **Building in place.** The sender writes into the frames, but it has to know the message's length before it maps them. Building a Cap'n Proto message directly in the frames needs a size estimate, or mapping all 16 frames.
> - **Messages over 64 KiB:** several calls, larger frames, or a raised capability limit.
> - **A receiver that blocks.** The receiver receives the announced call with a blocking receive, so a sender that publishes the record and never calls stalls that connection's receiver. Binding the receiver's notification to its thread, so that one receive waits for both rings and calls ([[kernel-ipc#Bound notifications]]), would remove that.
> - **Large replies.** Only requests travel by transfer so far. A reply would need the server to send frames back through the reply, or a call in the other direction.
