---
status: proposed
summary: The proposed Rust standard library for sap - std's platform layer mapped onto sap (heap from the runtime's untyped quota, threads, notifications for waiting, a log service for standard I/O), the parts left unsupported on purpose (files, environment, arguments), and the order of work.
updated: 2026-10-19
sources: ["[[2026-10-08-22c3947b#m30|§m30]]"]
---

# A Rust standard library for sap

sap's Rust programs build for sap's own target with `core` and `alloc` from source ([[prog-rust-toolchain]]), on the `libsap` runtime ([[prog-libsap]]). A full `std` is the next layer, roadmap item I2 ([[roadmap]]). This page is the proposed plan; none of it is built.

## The platform layer

Rust's `std` is `core` and `alloc` plus a platform layer: allocation, threads, synchronization, time, standard I/O, files, networking, environment, process arguments. Each operating system implements that layer, and any part can return "unsupported", as Rust's UEFI and `wasm32-unknown-unknown` targets do.

For sap the mapping is mostly direct, with a few deliberate absences:

| `std` area | sap |
|---|---|
| allocation | a heap in memory the runtime maps from its untyped quota ([[cap-process-authority#Runtime capabilities and declared imports]]) |
| threads, mutexes, condition variables | threads retyped from the runtime's quota; futex-style waiting needs notifications (M4) |
| time | a time service, or reading the timer counter if user access is enabled |
| standard I/O | the log service; until then the debug console |
| environment variables, process arguments | none: programs receive typed imports, not strings ([[kernel-process-model]]) |
| files, networking | unsupported in `std`; programs use typed interfaces to the store and the network bridge instead |

So a sap `std` would be smaller than a POSIX one by design: programs written against `std::fs` or `std::env` belong in the compatibility layer ([[system-legacy-compat]]), not in native sap.

## Order

1. A heap allocator in `libsap`, so `alloc` works (needed for Cap'n Proto messages, M5).
2. Thread-local storage, which moves each thread's IPC-buffer pointer out of `tp`, where the RISC-V TLS convention puts the thread pointer ([[prog-libsap#Thread start convention]]).
3. A `std` platform layer once a log service exists.
4. Upstreaming the target to Rust as a tier 3 target when the interface is stable, as Redox, Hermit and Fuchsia did.

> [!question] Open
> Whether `libsap` stays separate from the `std` platform layer or becomes it.
