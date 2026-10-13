---
status: reference
summary: WebAssembly's component model and its interface language WIT - typed interfaces, own/borrow resource handles, worlds - which sap considered as its spec language before choosing Cap'n Proto, keeping the ownership and world ideas.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m15|§m15]]", "[[2026-09-27-future-of-computing#m23|§m23]]"]
---

# Wasm component model and WIT

**What it is:** the component model (released as WASI 0.2 in early 2024) defines typed interfaces between Wasm components written in any language. WIT is its interface language: records, variants, resources with `own`/`borrow` handles, async streams and futures (WASI 0.3), and *worlds* declaring a component's imports and exports. The canonical ABI lowers WIT types to Wasm linear memory ([[2026-09-27-future-of-computing#m3|§m3]], [[2026-09-27-future-of-computing#m11|§m11]]).

**Status for sap:** was the proposed spec language for several turns, then dropped for Cap'n Proto: for native use WIT has a type system and a standalone parser (`wit-parser`) but no transport, no RPC and no native bindings ([[2026-09-27-future-of-computing#m23|§m23]]; [[decisions]]).

**What sap keeps:** own/borrow semantics (as Cap'n Proto annotations, [[cap-ownership]]) and worlds (as the bootstrap capability, [[kernel-process-model]]).

**What sap avoids:** the canonical ABI's shared-nothing copy semantics, which call the callee's `realloc` and copy lists into its memory; safe, but the opposite of zero-copy for large data ([[2026-09-27-future-of-computing#m15|§m15]]).

**Binding quality, as of the chat:** Rust excellent; JS (jco) and Python (componentize-py) reasonable; C verbose; others improving ([[2026-09-27-future-of-computing#m13|§m13]]).
