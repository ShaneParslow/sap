---
status: decided
summary: Application code calls a generated interface and never knows the transport (direct call, shared-memory channel or network). Bindings are generated per language and their quality decides adoption. In-process calling conventions and library linking are each language's problem; the OS only identifies code by hash.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m13|§m13]]", "[[2026-09-27-future-of-computing#m14|§m14]]", "[[2026-09-27-future-of-computing#m22|§m22]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Bindings and languages

## Code doesn't see the transport

**Decided.** One interface definition, generated bindings, and a transport chosen when the connection is set up: a direct call inside a trust domain, a shared-memory channel between trust domains, or the network between machines ([[ipc-transports]]). Code written against the interface doesn't know which it is using. The human called this "a really really good way to streamline adoption" that uses the spec's language independence well ([[2026-09-27-future-of-computing#m13|§m13]], [[2026-09-27-future-of-computing#m14|§m14]]).

**If every cross-component call is a message in a defined wire format, the wire format is the ABI.** Each language needs one binding generator, instead of glue for every pair of languages. Fuchsia made the same choice with FIDL ([[2026-09-27-future-of-computing#m11|§m11]]).

## What stays the language's problem

**Decided** ([[2026-09-27-future-of-computing#m28|§m28]], [[2026-09-27-future-of-computing#m29|§m29]]):
- **Calling conventions inside one process** are a language detail. Where generated bindings make direct calls between languages in one process, they use the platform C ABI underneath, generated from the spec so nobody writes headers by hand. The C ABI's mess comes from hand-written, drifting headers, not the register conventions ([[2026-09-27-future-of-computing#m25|§m25]]).
- **Loading and linking libraries** (statically or at load time) is the language's job, including making sure loaded binaries are compatible with the platform and each other. A numerics library will normally be linked by the language, not called over IPC: a function call costs nanoseconds and even a very good IPC round trip costs hundreds of nanoseconds or more ([[2026-09-27-future-of-computing#m13|§m13]]).
- **The OS's only contribution** is content-hash identity for code, so a loader can verify what it loads.

This splits code reuse into two kinds ([[2026-09-27-future-of-computing#m11|§m11]]):
- **Components** communicate across languages and trust boundaries through specs and messages. This is the OS-level, universal part.
- **Libraries** are linked inside a component and shared within a language's ecosystem. They never touch the OS ABI.

## Binding quality

Bindings decide adoption. Wrappers that feel foreign get wrapped again by hand, and then the ecosystem fragments. Each language needs bindings written by someone who knows its idioms, not one generic template applied everywhere (the WinRT lesson) ([[2026-09-27-future-of-computing#m11|§m11]], [[2026-09-27-future-of-computing#m13|§m13]]).

Cap'n Proto has working code generators and runtimes for C++ and Rust and others of varying quality, but its builder APIs are clunky ([[iface-capnp]]). The plan: use the existing generators to prove the model, then generate idiomatic per-language bindings directly ([[2026-09-27-future-of-computing#m15|§m15]]). Rust gets ownership-aware bindings first ([[cap-ownership]]).

Prior art for generating bindings: WinRT language projections, GObject Introspection, Fuchsia FIDL bindings, wit-bindgen, Mozilla UniFFI, Diplomat ([[2026-09-27-future-of-computing#m11|§m11]]).

## Rust and shared memory

In Rust, reading memory through ordinary references (`&[u8]`, `&str`) while another process may write it is undefined behavior regardless of bounds checks. capnproto-rust hands out exactly those references. So generated Rust bindings must only read buffers that are immutable or transferred, and access genuinely shared mutable regions through raw pointers, atomics or volatile reads ([[2026-09-27-future-of-computing#m29|§m29]]; [[ipc-validation]], [[ipc-shared-buffers]]).

> [!question] Open
> The human expects some case will need real in-process FFI across languages beyond what generated direct-call bindings give, and couldn't see how to support it without falling back into the C-ABI mess ([[2026-09-27-future-of-computing#m24|§m24]]). No such case has been identified yet.
