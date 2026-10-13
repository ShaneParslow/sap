---
status: decided
summary: The spec, wire layout and bindings are constant; the transport is picked per connection - direct call within a trust domain, shared-memory channel between domains on one machine, Cap'n Proto RPC over the network between machines.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m14|§m14]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m20|§m20]]"]
---

# Transports

## What is constant

**The spec and the wire layout are fixed; the transport is chosen per connection** ([[2026-09-27-future-of-computing#m17|§m17]]):

1. **Spec:** Cap'n Proto schemas with ownership annotations ([[iface-capnp]]).
2. **Layout:** Cap'n Proto encoding.
3. **Bindings:** generated per language; application code never sees the transport ([[iface-bindings-and-languages]]).
4. **Transport:** one of the three below.

The human's summary of the constant was "Cap'n Proto structs into shared-memory channels"; the chat widened it to include the other two transports. The human adopted both shared-memory IPC and network-as-first-class ([[2026-09-27-future-of-computing#m14|§m14]], [[2026-09-27-future-of-computing#m20|§m20]]).

## The three transports

| Transport | Used between | Validation | Copying |
|---|---|---|---|
| **Direct call** | components in one trust domain | none | none |
| **Shared-memory channel** | trust domains on one machine | structural checks, single-fetch reads ([[ipc-validation]]) | small messages copied into the ring; large ones transferred ([[ipc-channels]]) |
| **Network RPC** | machines | full | serialized onto the connection; Cap'n Proto RPC handles capability passing and pipelining |

Inside one trust domain, components treat each other as "friends" (the human's term): no validation and no permission checks between them, which is the direct-call transport ([[2026-09-27-future-of-computing#m14|§m14]], [[2026-09-27-future-of-computing#m15|§m15]]).

## Trust domains

Where trust-domain boundaries fall decides where validation and isolation are paid for. Proposed default: each independently sourced component is its own domain, and components from the same verified build may be placed together ([[2026-09-27-future-of-computing#m17|§m17]]).

## Why not IPC everywhere

A function call costs nanoseconds and the best IPC round trip costs hundreds of nanoseconds or more, so fine-grained calls in hot loops (a numerics library called a million times) can't go over IPC. Those are either direct calls within a domain or ordinary library calls inside a language ([[2026-09-27-future-of-computing#m13|§m13]]). IPC never fully matches a function call on tiny calls, because a function call shares the caller's core, stack and registers; for batched or large messages it comes within a small constant factor ([[2026-09-27-future-of-computing#m15|§m15]]).

## Failure

All three transports share one failure model: every call can fail and capabilities can break ([[iface-failure-semantics]]). That is what keeps the uniform interface honest.

> [!question] Open
> How a connection's transport is chosen in practice (by the launcher at spawn time? negotiated?). Keeping the network off the local fast path has a proposed answer: a bridge component carries all network traffic, and a remote object is a local capability to a proxy ([[ipc-network-rpc#The bridge component]]). How local connections use endpoints and rings is in [[ipc-cap-transfer]].
