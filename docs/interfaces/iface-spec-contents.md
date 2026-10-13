---
status: decided
summary: An interface spec is the language-neutral contract between components - types (with capabilities as owned or borrowed), operations, protocol via ownership, failure modes, intent, hash identity and a conformance suite. Behavioral contracts and proofs are optional attachments consumed by languages that support them.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m8|§m8]]", "[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m10|§m10]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m19|§m19]]"]
---

# What an interface spec contains

**Decided:** the OS-level spec describes interfaces in a language-neutral way and stops there. Behavior specifications (contracts, proofs) live on the language side, as optional extras. The human's reasoning: you must tolerate languages that can't express behavior specs, so the universal part has to be the part every language can consume; sap is "the glue between all languages and all programs", not a new language ([[2026-09-27-future-of-computing#m10|§m10]]). The spec describes interfaces but shouldn't drive their design; that comes from what is optimal, ergonomic and portable for the operation itself.

The concrete format is Cap'n Proto schemas plus annotations ([[iface-capnp]]); this page is about content, not syntax.

## The universal part

Every interface spec contains ([[2026-09-27-future-of-computing#m11|§m11]], [[2026-09-27-future-of-computing#m17|§m17]], [[2026-09-27-future-of-computing#m19|§m19]]):

1. **Types:** primitives, records, variants, lists, and capabilities marked as owned or borrowed ([[cap-ownership]]).
2. **Operations**, including streaming ones.
3. **Protocol:** legal orderings and when a handle is consumed, expressed through ownership and typestate rather than a separate contract language ([[cap-ownership#Protocol as types]]).
4. **Failure modes and retry semantics:** what errors each operation can produce, and whether it is idempotent or at-most-once ([[iface-failure-semantics]]).
5. **Intent:** rationale, examples, non-goals and edge-case reasoning in natural language, attached to specific items. Non-normative: the formal parts define correctness and intent explains them. Where AIs find the two disagreeing, that is flagged as a bug ([[2026-09-27-future-of-computing#m9|§m9]]).
6. **Identity and versioning:** the interface's hash, and checked compatibility relations to other hashes ([[iface-versioning]]).
7. **A conformance test suite:** the one form of behavior spec that is fully language-neutral, because it runs against any implementation over IPC ([[2026-09-27-future-of-computing#m11|§m11]]).

Ownership and protocol are the small amount of behavior that has to be in the universal part, because every caller needs them regardless of language: most FFI and IPC bugs are "who frees this", "is this handle still valid", "can I call read before open". The IPC layer enforces them at runtime without proofs ([[2026-09-27-future-of-computing#m11|§m11]]).

**Design rules for interfaces** ([[2026-09-27-future-of-computing#m19|§m19]]):
- No integer names that carry authority: refer to receiver-owned things by capability ([[cap-model]]).
- Static bounds go in the spec as refinements (`u8 where < 16`) that become generated checks; bounds that depend on runtime state are checked by the receiver.

## The optional part

Contracts (pre/postconditions, invariants, refinement types like "returns sorted"), cost bounds, and proofs are **attachments keyed to the interface hash**. Languages that can use them (Dafny, Verus, Bend, F\*) consume them; Bend, for example, could compile an interface's contracts into laws its compiler requires proofs for. Every other language ignores them ([[2026-09-27-future-of-computing#m11|§m11]]).

When claims are attached, each is labeled with how it is enforced: *proved*, *tested*, *monitored at runtime*, or *stated only*. Runtime monitoring is cheap here because every cross-component call already passes a typed boundary ([[2026-09-27-future-of-computing#m9|§m9]]).

## High-level spec and low-level ABI

The human noticed two levels being conflated: a high-level spec (intent, types, operations) and a low-level one (calling convention, layout). The resolution: the high-level spec is lowered to a concrete ABI by fixed, deterministic rules, and nobody writes the low-level form by hand. For cross-component calls the ABI *is* the wire format, so calling conventions become internal to each language ([[2026-09-27-future-of-computing#m11|§m11]]; [[iface-bindings-and-languages]]).

## Alternatives considered

- **A new specification language with full formal semantics.** The human judged it beyond reach and likely to run into endless problems if made fully verifiable ([[2026-09-27-future-of-computing#m10|§m10]]).
- **Mandatory proofs.** Intent, types and protocol cover most composition problems; conformance tests close most of the remaining semantic-compatibility gap cheaply; proofs are for where tests aren't enough ([[2026-09-27-future-of-computing#m11|§m11]]).
- **WIT as the spec language.** Superseded by Cap'n Proto; see [[iface-capnp]] and [[decisions]].

Prior art for each part: shape (WIT, Cap'n Proto, Protobuf, Fuchsia FIDL, AWS Smithy), behavior (Dafny, Verus, F\*, Liquid Haskell, TLA+, session types), compatibility checking (cargo-semver-checks, `buf breaking`, which check shape only) ([[2026-09-27-future-of-computing#m9|§m9]]).

> [!question] Open
> How intent and conformance tests are physically attached to a Cap'n Proto schema (annotations, sidecar objects in the store keyed by hash, or both), and how the suite is run. The chat's suggested first exercise: write a complete spec for one small interface (a clipboard or key-value store) and see what breaks. See [[roadmap]].
