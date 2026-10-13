---
status: proposed
summary: How capabilities move and are lent - own and borrow semantics, protocol rules expressed as handle types, derivation trees, and why enforcement is dynamic in the kernel or host and static only as a per-language bonus.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# Ownership: own and borrow

Every capability passed in a message is either **owned** (moved: the sender loses it) or **borrowed** (lent for the duration of one call). These two modes carry most of what an interface needs to say about protocol, and they are enforced by whoever holds the capability tables, never by trusting the message.

## The two modes

- **`own`:** the capability moves from sender to receiver. After the transfer, the sender's table entry is gone.
- **`borrow`:** the receiver gets a temporary capability that is revoked when the call returns (or is cancelled).

In Cap'n Proto schemas these are expressed as `$own` and `$borrow` annotations on interface-typed fields and parameters, with no change to the schema language ([[iface-capnp]]).

## Protocol as types

"Protocol" means the legal orderings of operations: `open` before `read`, no `read` after `close`. Most of it can be expressed with ownership instead of a separate contract language ([[2026-09-27-future-of-computing#m13|§m13]]):

**Typestate.** Each state gets its own handle type, and a transition consumes the old handle and returns a new one: `open(path) -> own<reader>`, then `close(own<reader>)`. After `close` the caller holds no handle it could call `read` on, so the illegal call can't be written.

**Derivation trees.** Every capability records the capability it was derived from. Revoking a parent revokes everything derived from it: a file handle, a mapped region derived from it, subrange views of that region, all revoked when the file is closed. This is seL4's capability derivation tree and answers the human's proposal to reap children when a parent is invalidated ([[2026-09-27-future-of-computing#m12|§m12]]). See [[cap-revocation]].

**What types can't express:** "must eventually call X" (needs linear rather than affine types; at a process boundary the kernel reclaims a dead process's handles instead), constraints across several handles, counts, and timing. These are left out of the first version; ownership plus typestate plus derivation covers most real protocol bugs.

## Enforcement

**Ownership claims in a message are never trusted.** A message carries only indices into a capability table; the tables live in the kernel or in the object's host, which carries out the transfer ([[2026-09-27-future-of-computing#m23|§m23]]):
- *own:* A asks to send index 3 as owned. The table holder checks that A holds index 3 with transfer rights, removes it from A's table and inserts it into B's.
- *borrow:* the table holder inserts a temporary entry into B's table and removes it when the call completes or is cancelled.

A sender that lies about ownership can only misuse capabilities it already holds; it can't forge new ones or keep one it gave away.

**Static checking is a per-language bonus; dynamic enforcement is universal.** Bindings generated for languages with affine types (Rust) turn `own` into move semantics and `borrow` into lifetimes, so misuse is a compile error. Languages whose generators ignore the annotations still interoperate correctly, because the transport enforces ownership whatever the code claims; they just find mistakes at runtime. This is why only the Rust code generator needs ownership support at first ([[2026-09-27-future-of-computing#m23|§m23]]). Types are checked statically only *within* a component; across a boundary the peer may be malicious or written in another language ([[2026-09-27-future-of-computing#m13|§m13]]).

## Where moves are implemented

Who performs a move depends on the kernel underneath:

| Platform | How `own` is enforced |
|---|---|
| Custom kernel | A kernel move primitive: the table entry is transferred atomically. |
| seL4 | Capability transfer over IPC *copies*, so the sender keeps its copy, and a message delivers one capability ([[kernel-sel4-differences]]). Workaround: the object's host is the authority; to move from A to B it revokes A's derived capability and mints a new one for B. Costs an extra round trip ([[2026-09-27-future-of-computing#m31|§m31]]). |
| Linux | File descriptors can be `dup`ed before "transfer" and can't be revoked, so raw fds can't enforce `own`. Capabilities must be references held by a broker ([[2026-09-27-future-of-computing#m25|§m25]]). |

On seL4, `borrow` fits well: mint a child capability, pass it, revoke it when the call returns. Which row applies depends on [[kernel-choice]].

## Borrow lifetime and hostile callees

The lender must be able to end a borrow early, or a hostile callee could hold it forever. Rule: **the lender cancels the call, which ends the borrow, and the borrower sees a broken-capability error.** The object behind a borrowed capability is reference-counted through the table entry, so it can't be destroyed underneath the borrower while the borrow is live; it can only be revoked ([[2026-09-27-future-of-computing#m23|§m23]]). Memory behind a borrowed buffer is a separate question, covered in [[cap-revocation]].
