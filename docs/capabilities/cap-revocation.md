---
status: proposed
summary: How authority is taken back - derivation-tree revocation, caretakers for delegated access, cancelling borrows, leases instead of distributed GC - and what revoking memory costs on paging versus CHERI.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m21|§m21]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Revocation and lifetime

Granting authority is easy in a capability system; taking it back is the hard part. The chat flagged revocation as one of the problems where "the old quirks come back" ([[2026-09-27-future-of-computing#m3|§m3]]). This page covers the mechanisms in the order a capability's life reaches them.

## Mechanisms

**Derivation-tree revocation.** Every capability records its parent; revoking a capability revokes everything derived from it ([[cap-ownership]]). This handles the common case of tearing down a resource and everything handed out from it.

**Caretakers for delegation.** When A wants to give B access that A can later cut off without affecting its own access, A gives B a capability to a caretaker: an indirection object that forwards calls and that A can disable. Membranes generalize this to whole object graphs reached through the caretaker. The chat noted these are fiddly but necessary ([[2026-09-27-future-of-computing#m3|§m3]], [[2026-09-27-future-of-computing#m21|§m21]]).

**Ending borrows.** A borrow ends when the call returns, or early when the lender cancels the call; the borrower then sees a broken capability ([[2026-09-27-future-of-computing#m23|§m23]]).

**Process death.** When a process dies, the kernel reclaims all its table entries, which releases everything it owned or borrowed. This is what covers "must eventually release" obligations that types can't enforce.

**Stored capabilities: expiry and leases.** A sturdy ref is revoked by deleting its entry in the store's side table. Stored capabilities carry expiry or leases. sap does **not** attempt distributed garbage collection of capabilities: it is a known hard problem, and Cap'n Proto doesn't do it either ([[2026-09-27-future-of-computing#m21|§m21]]). See [[store-sturdy-refs]].

## Revoking memory

Revoking access to memory costs very different amounts on the two isolation mechanisms ([[2026-09-27-future-of-computing#m17|§m17]]):

| | Paging | CHERI |
|---|---|---|
| How | Unmap the pages from the holder. | Quarantine the freed memory until a sweep invalidates every outstanding capability to it. |
| Cost | Page-table update plus a TLB shootdown (interrupting other cores to flush cached mappings). The main pain point of the paging design. | A memory sweep; research (Cornucopia, CheriBSD) keeps reducing it. |

On paging, a borrower that keeps a mapping after the owner unmaps keeps the *old* pages alive, so there is no use-after-free. The remaining risk is the owner reusing a buffer while the borrower is still reading it: a data race that yields garbage values, not a memory-safety violation ([[2026-09-27-future-of-computing#m23|§m23]]).

On CHERI in a single address space, freed memory reallocated to another component while a borrower still holds a capability to it would leak that component's data. That is CHERI's temporal-safety problem, and it is a **trust-boundary** problem, not a use-after-free bug: the borrower may be keeping the capability deliberately, and the lender's own type system says nothing about the borrower ([[2026-09-27-future-of-computing#m25|§m25]]). For IPC it is avoided by design with per-channel buffer pools, so no sweep is needed on the message path; see [[hw-cheri]].

## On Linux

File descriptors can't be revoked and can be duplicated before being sent, so a Linux prototype can't use raw fds as revocable capabilities; a broker holding the real references would be needed ([[2026-09-27-future-of-computing#m25|§m25]]). The human has since ruled out a Linux prototype ([[2026-09-27-future-of-computing#m28|§m28]]), so this matters only for [[system-legacy-compat]].

> [!question] Open
> Whether sap's kernel needs revocation beyond the derivation tree (for example, revoking one delegated copy without a caretaker), and what the user-facing model for revoking a share looks like. See [[open-questions]].
