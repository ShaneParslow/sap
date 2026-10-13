---
status: proposed
summary: Interfaces are identified by the hash of their formal content; version numbers are names for hashes; compatibility between two hashes is checked mechanically (append-only fields plus conformance suites); incompatible versions coexist and are bridged by adapters.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m8|§m8]]", "[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m17|§m17]]"]
---

# Interface versioning

The human named version handling as the root of many of today's problems and something a good spec must get right ([[2026-09-27-future-of-computing#m8|§m8]]). Schema evolution was also flagged as the biggest of the problems where "the old quirks come back" ([[2026-09-27-future-of-computing#m3|§m3]]). The proposal below uses content addressing to make compatibility a checked fact rather than a claim.

## Identity by hash

An interface's identity is the hash of its formal content. Human-readable version numbers are names pointing at hashes. Capabilities are typed by interface hash ([[cap-model]]), and programs declare their imports by hash, so nothing is referenced by name alone ([[2026-09-27-future-of-computing#m9|§m9]]).

## Evolution rules

Cap'n Proto's rules are the baseline ([[iface-capnp]]): fields carry explicit ordinals, new fields are appended, nothing is removed or renumbered, old readers ignore unknown fields, new readers see defaults ([[2026-09-27-future-of-computing#m15|§m15]], [[2026-09-27-future-of-computing#m17|§m17]]). Changes within these rules are shape-compatible by construction.

## Checking compatibility

Version B can replace A when ([[2026-09-27-future-of-computing#m9|§m9]]):
- **shape:** B's schema is an append-only extension of A's (checked mechanically, like `buf breaking` or cargo-semver-checks);
- **behavior:** B passes A's conformance suite ([[2026-09-27-future-of-computing#m11|§m11]]). Shape checks alone let semantic breaks through; conformance tests close most of that gap cheaply.

Where contracts are attached, the full condition is behavioral subtyping: B accepts at least what A accepts (weaker or equal preconditions), guarantees at least what A guarantees (stronger or equal postconditions), and B's protocol can simulate A's. That level is optional, like contracts themselves ([[iface-spec-contents]]).

## Coexistence and adapters

Incompatible versions coexist without conflict, as in Nix and Unison, because each is a different hash. Bridging two incompatible versions takes an **adapter**: a component with its own spec, implementing one hash in terms of the other. AIs can write adapters (and proofs or tests for them); a small trusted checker verifies them. This is proof-carrying code applied to versioning: generation can be unreliable as long as checking isn't (principle 8 in [[vision-principles]]) ([[2026-09-27-future-of-computing#m9|§m9]]).

The trusted core of the whole interface system is that checker: the schema compiler, the compatibility checker, and (where used) the contract verifier. Keep it small.

## Effects elsewhere

- **Object store values** carry a schema hash, so stored data is versioned the same way ([[store-data-model]]). Reading old values through a new schema follows the same append-only rules; incompatible changes need an adapter or a migration.
- **The program IR** needs versioning too; it is the same problem ([[prog-representation]]).

> [!question] Open
> Hashes are tagged with their algorithm so the function can be replaced without breaking stored references (decided 2026-10-11, [[ipc-network-rpc#Decided properties]]). Also open: exactly what is hashed (the canonical schema text, a normalized form, with or without intent and tests), and how human version names are published and resolved. Not discussed.
