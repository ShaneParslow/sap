---
status: decided
summary: Cap'n Proto is sap's schema language, wire format and RPC - zero-copy layout, append-only evolution, safe reads of untrusted data, capability RPC with promise pipelining - extended with $own/$borrow annotations rather than forked. Replaced WIT.
updated: 2026-10-11
sources: ["[[2026-09-27-future-of-computing#m5|§m5]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m22|§m22]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m28|§m28]]"]
---

# Cap'n Proto

**Decided:** interfaces are Cap'n Proto schemas, messages use Cap'n Proto encoding, and remote and local RPC follow Cap'n Proto's capability RPC. The human questioned WIT because it has no native transport, RPC or bindings while Cap'n Proto has all three in production; the chat reversed its earlier WIT recommendation, and the human called dropping WIT a big simplification ([[2026-09-27-future-of-computing#m22|§m22]], [[2026-09-27-future-of-computing#m23|§m23]], [[2026-09-27-future-of-computing#m28|§m28]]). The ownership extension below is proposed, not yet adopted.

## Properties sap relies on

([[2026-09-27-future-of-computing#m13|§m13]])
- **The in-memory format is the wire format.** Messages are segments of 64-bit words; structs have a schema-determined fixed layout; pointers are relative offsets, so messages are position-independent. A shared buffer can be read in place with no parse step. This is what makes shared-memory IPC zero-copy ([[ipc-channels]]).
- **Schema evolution by appending.** Fields are numbered; new fields go at the end. Old readers ignore unknown fields; new readers see defaults for missing ones. Defaults are XOR-encoded, so a zeroed field reads as its default ([[iface-versioning]]).
- **Safe reading of untrusted data.** Readers bounds-check each pointer as they follow it, and a traversal limit stops a small message from forcing huge amounts of work ([[ipc-validation]]).
- **Capability RPC.** Interfaces are first-class types; passing an interface reference in a message passes a capability, and the only way to get one is to be given one. Descends from the E language's CapTP, so it is an object-capability design from the start.
- **Promise pipelining.** Calls can be made on the result of a call that hasn't returned: `open(path).read(0, 4096)` costs one round trip. Saves latency over IPC and a lot over a network.
- **Production use:** Cloudflare's workerd. Written by Kenton Varda after protobuf v2, to fix what he saw as protobuf's mistakes ([[2026-09-27-future-of-computing#m5|§m5]]).

## Weaknesses

([[2026-09-27-future-of-computing#m13|§m13]], [[2026-09-27-future-of-computing#m15|§m15]])
- **Messages are append-only arenas.** Every new struct, list or string is allocated at the end of the message; replacing a field leaves the old data as unreachable "orphans", and lists can't be resized. Fine for build-send-discard IPC messages; bad as a long-lived mutable data structure, so don't use it as one.
- **Builder APIs are clunky** in most languages.
- **No own/borrow distinction:** capabilities are reference-counted references. Added below.
- **Implementation quality varies** outside C++ (the reference implementation) and Rust. Check which RPC features capnproto-rust covers before committing ([[2026-09-27-future-of-computing#m23|§m23]]).

## Ownership annotations (proposed)

Cap'n Proto supports custom annotations, so ownership needs no fork of the schema language ([[2026-09-27-future-of-computing#m23|§m23]]):

1. Define `$own` and `$borrow` annotations for interface-typed fields and parameters.
2. Extend capnproto-rust's code generator (a compiler plugin) so `$own` becomes a Rust move and `$borrow` a lifetime-bound reference.
3. Enforce ownership in the transport: the wire carries only an index into the message's capability table, so `own` deletes the sender's table entry on transfer and `borrow` revokes the receiver's entry when the call returns ([[cap-ownership]]).

Only the Rust generator has to change at first, because static checking is a per-language bonus while transport enforcement is universal. The human had suggested forking Cap'n Proto's bindings; the annotation route reaches the same result without a fork.

## Further annotations (proposed)

Network and storage put more facts into schemas, and the human proposed carrying them the same way ([[2026-10-08-22c3947b#m20|§m20]]):
- **`$merge(rule)`** on a struct or field: how concurrent offline updates to a ref holding this type combine: a built-in rule (set union, counter, last-writer-wins only when explicitly chosen), or, for complex cases, deferring to a **merger capability** that receives both versions and their common parent. Without it, a conflict produces a value holding both versions ([[store-distribution]]).

> [!todo] Deferred
> Merge rules are a large piece of work that informs nothing near-term; deferred by the human ([[2026-10-08-22c3947b#m22|§m22]]).
- **`$local`** on a field: meaningful only on one machine; the store and the network bridge refuse values with it set ([[ipc-network-rpc#Decided properties]]).

Like `$own`/`$borrow`, these are enforced by whoever stores or transports the value, so languages whose code generators ignore them still behave correctly.

## What is kept from WIT

WIT (the WebAssembly component model's interface language) contributed ideas that survive as conventions ([[2026-09-27-future-of-computing#m15|§m15]], [[2026-09-27-future-of-computing#m23|§m23]]):
- **Worlds:** a declaration of everything a component imports and exports, which is a declaration of exactly the authority it needs. In Cap'n Proto this is a bootstrap interface that receives its capabilities as parameters ([[kernel-process-model]]).
- **Convenience types** (option, result, flags): Cap'n Proto expresses them as unions or bitfields; only ergonomics differ.
- **own/borrow semantics:** the annotations above.

WIT's canonical ABI was a poor fit anyway: it is shared-nothing by design and copies lists into the callee's memory, the opposite of zero-copy for large data ([[2026-09-27-future-of-computing#m15|§m15]]). See [[wasm-component-model]].

> [!question] Open
> Whether to adopt the annotation design as specified; how intent, conformance tests and failure/idempotency declarations attach to schemas ([[iface-spec-contents]]); whether Cap'n Proto RPC's own message framing is used inside sap's rings or only the encoding ([[ipc-channels]]).
