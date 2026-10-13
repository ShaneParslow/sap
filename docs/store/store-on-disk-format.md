---
status: exploring
summary: The store starts in memory; for persistence the proposal is an append-only log of content-addressed blobs plus an index, which makes crash safety nearly free. Git's object and packfile formats are the prior art; compaction and GC come later.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# On-disk format

The human flagged the on-disk binary representation as "one of the easiest things to screw up very badly", while noting there is a lot of prior work ([[2026-09-27-future-of-computing#m28|§m28]]). Nothing is decided.

## Proposed starting point

([[2026-09-27-future-of-computing#m29|§m29]])
- **Start in memory.** The store's interface and semantics can be built and tested before anything touches a disk.
- **Persistence:** an append-only log of content-addressed blobs, plus an index from hash to log offset, plus the current value of each ref.
- **Crash safety almost for free:** nothing is overwritten, so a torn write at the tail is simply discarded on recovery.
- **Later:** compaction and garbage collection of unreachable values.

## Prior art

Git's loose object and packfile formats: content-addressed objects, deltas, an index ([[2026-09-27-future-of-computing#m29|§m29]]). Nix's store and Unison's codebase format are other content-addressed stores worth reading.

## Questions this format must answer

- How ref updates are made atomic and durable (a ref log in the same append-only stream?).
- Where the sturdy-ref side table lives and how it is protected at rest ([[store-sturdy-refs]]).
- Garbage collection: what roots are (refs, live capabilities, leases), and when unreachable values are dropped, given that history is kept by default.
- Hash function and encoding of schema hashes inside values ([[store-data-model]]).
- Whether the block device is reached through a virtio-blk driver component directly or through an intermediate block service ([[hw-devices-and-io]]).
