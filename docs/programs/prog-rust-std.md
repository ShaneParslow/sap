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
| threads, mutexes, condition variables | threads retyped from the runtime's quota; mutexes sleep on notifications, as `libsap`'s already do ([[prog-libsap#Slots, locks and the heap]]) |
| time | a time service, or reading the timer counter if user access is enabled |
| standard I/O | the log service; until then the debug console |
| environment variables, process arguments | none: programs receive typed imports, not strings ([[kernel-process-model]]) |
| files, networking | unsupported in `std`; programs use typed interfaces to the store and the network bridge instead |

So a sap `std` would be smaller than a POSIX one by design: programs written against `std::fs` or `std::env` belong in the compatibility layer ([[system-legacy-compat]]), not in native sap.

## Building it today

Building `std` for the sap target with everything unimplemented doesn't work without changing `std` itself. Tried 2026-10-19 with `-Zbuild-std=std,panic_abort` and `#![feature(restricted_std)]`: most of `std` falls back to its "unsupported" platform for an unknown operating system, but about ten platform modules list the systems they support and have no fallback, so the build stops in allocation, random numbers (hash-map keys), thread-locals, I/O errors and the single-threaded locks. Each is a few lines routing `target_os = "sap"` to an implementation, but they are lines in Rust's own library, which means carrying a patched `std` (or upstreaming the target) from the first build. The size of a port, counting non-blank, non-comment lines in this nightly's library: `std` is about 80,000 lines, of which the platform layer for every system together is about 47,000. The Unix-specific part (Linux included) is about 11,000, plus 3,800 of public `std::os::unix` and `std::os::linux` extensions. A small system's port is far smaller: Hermit's is about 1,300 lines (it delegates to a separate system-call crate, as `std` would delegate to `libsap`), Xous's 3,300 and UEFI's 4,300. So a sap port is a few thousand lines, not an overhaul; the cost is carrying it outside Rust until the target is upstream. So `std` waits until there is enough to put behind it: thread-local storage, a log service for standard output, and the process ABI (M7) that tells a program its resources.

## Order

1. A heap allocator in `libsap`, so `alloc` works (needed for Cap'n Proto messages, M5). Built 2026-10-19 ([[prog-libsap#Slots, locks and the heap]]).
2. Thread-local storage, which moves each thread's IPC-buffer pointer out of `tp`, where the RISC-V TLS convention puts the thread pointer ([[prog-libsap#Thread start convention]]).
3. A `std` platform layer once a log service exists.
4. Upstreaming the target to Rust as a tier 3 target when the interface is stable, as Redox, Hermit and Fuchsia did.

> [!question] Open
> Whether `libsap` stays separate from the `std` platform layer or becomes it.
