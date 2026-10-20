---
status: proposed
summary: The kernel guarantees a buffer exists, not that its contents match the spec, so receivers validate structure at trust boundaries. Covers what needs checking, double fetch and the single-fetch rule, Rust soundness, and making validation cheap with copy, page transfer and memfd-style sealing.
updated: 2026-10-19
sources: ["[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m14|§m14]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Validation and double fetch

The human's question was why a receiver needs to validate at all if it knows the spec and the kernel guarantees the buffer exists ([[2026-09-27-future-of-computing#m14|§m14]]). The answer: **the kernel guarantees that the buffer exists and how large it is; it cannot guarantee that the bytes inside match the spec, because the sender wrote them.** The spec says what should be there; validation checks what is there ([[2026-09-27-future-of-computing#m15|§m15]]).

Validation happens only at trust boundaries. Within a trust domain ("friends") it is skipped ([[ipc-transports]]).

## What needs checking

Validation cost scales with how much pointer structure a message has. A fixed-size struct with no pointers needs almost nothing. Problems come from variable structure ([[2026-09-27-future-of-computing#m15|§m15]]):
- **Offsets and lengths.** A list is (offset, length). A bad offset or an oversized length makes a trusting receiver read memory it shouldn't, such as its own secrets elsewhere in its address space. Page boundaries don't help: the bad read lands in memory the receiver legitimately has mapped.
- **Variant tags** outside the enum, causing type confusion.
- **Invalid UTF-8** in strings, if the receiver's language assumes valid text.
- **Cycles or deep nesting** that make traversal blow up.
- **Fields the receiver uses to index its own state.** The human's catch ([[2026-09-27-future-of-computing#m18|§m18]]). These are semantic, not structural, so only the receiver can check them; the main defense is not to have them, by using capabilities instead of integer indices ([[cap-model]]), plus spec-level refinements that become generated checks ([[iface-spec-contents]]).

Cap'n Proto readers do the structural part lazily: each getter bounds-checks the pointer it follows against the segment, and a traversal limit caps total work ([[iface-capnp]]). It is generated code, a few compare-and-branch instructions per structural field.

## Double fetch

If the buffer is shared and the sender can still write it, the sender can change a message while the receiver reads it. The receiver checks `len = 100`, then reads `len` again when using it, by which time it is 10⁹. This is a real bug class: Xen's paravirtual drivers had exactly this vulnerability on shared rings ([[2026-09-27-future-of-computing#m15|§m15]]).

**The single-fetch rule:** read every structural field exactly once into a local, and use only the local. Generated bindings can enforce this mechanically. Bulk payload (pixels) can be read in place; if it changes mid-read the receiver gets garbage values, but not memory unsafety, because the offsets and lengths it relies on were captured once.

With Cap'n Proto, each individual access reads a pointer word, checks it, and uses that same value, so each access is safe. What remains at the application level ([[2026-09-27-future-of-computing#m29|§m29]]):
- re-reading a value (check a length, read it again);
- inconsistent views: two fields read at different moments may not agree (a total no longer equal to the sum of its parts).

Whether the C++ and Rust internals are robust to concurrent malicious mutation is to be verified in their code, not assumed ([[2026-09-27-future-of-computing#m25|§m25]]).

**Rust makes this stricter.** Reading memory through `&[u8]` or `&str` while another process writes it is undefined behavior in Rust regardless of bounds checks, and capnproto-rust returns exactly such references (its text accessor validates UTF-8 and returns `&str`). So in Rust, immutable or transferred buffers are required for soundness, not just hygiene ([[2026-09-27-future-of-computing#m29|§m29]]). The risk is concrete, not only formal: given a reference, the compiler may assume the memory can't change and load a checked length a second time instead of keeping it in a register, which is exactly the double fetch the rule forbids, introduced by the compiler. capnproto-rust reads segments with ordinary loads and returns ordinary references, so it is sound only on memory nobody else can write: a private copy, transferred pages, or a trusted peer that never writes what it has published.

The single-fetch rule can still be kept in Rust without the copy, by a reader that loads every word with volatile or atomic accesses, keeps each structural field in a local, and copies out only the text and data a program asks for. That needs a Cap'n Proto reader or code generator of sap's own, so it is worth building only if measurement shows the copy matters ([[ipc-channels#Size and streaming]]).

## Making validation cheap

Cap'n Proto's own documentation notes that eager validation partly defeats the purpose of a zero-copy format; the human asked whether that can be mitigated ([[2026-09-27-future-of-computing#m24|§m24]]). The expensive case is eager O(size) validation or copying of large structured messages from untrusted peers. Three cases cover nearly everything ([[2026-09-27-future-of-computing#m25|§m25]]):

| Case | Handling |
|---|---|
| Small messages | Copy into the ring, then read the private copy. |
| Large messages | Transfer the pages so the sender loses write access; Cap'n Proto's lazy per-access checks then suffice, nothing copied. |
| Opaque blobs (pixels, file contents) | Check bounds only. |

The better approach is to ensure there is no concurrent writer at all. Page transfer is the human's "switch the page over to the receiving process once it's fully written" ([[2026-09-27-future-of-computing#m24|§m24]]).

**Linux precedent: memfd sealing.** The sender writes into a memfd, drops its writable mappings, applies `F_SEAL_WRITE`, and sends the fd; the receiver checks the seals and knows the contents can't change. Sealing was added in 2014 so kdbus and Wayland could share buffers safely with untrusted peers. Seals are one-way, so each buffer is single-use ([[2026-09-27-future-of-computing#m25|§m25]]). sap's equivalent is a kernel page-transfer (or seal) operation on frames.

## With CHERI

Bounds violations trap in hardware instead of leaking; tags and UTF-8 still need software checks; double fetch is not solved, because a sender that hands out a read-only capability keeps its own writable one ([[hw-cheri]]).
