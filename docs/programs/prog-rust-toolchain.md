---
status: proposed
summary: How sap programs are built with Rust - why the precompiled core library forces text relocations in position-independent programs today, what a sap target (custom target spec, pinned nightly, core and alloc built from source as position-independent code) fixes, and the path to a Rust standard library for sap.
updated: 2026-10-13
sources: ["[[2026-10-08-22c3947b#m30|§m30]]"]
---

# Rust toolchain and standard library

sap's user programs are Rust today, built for the bare-metal target `riscv64gc-unknown-none-elf` with the `libsap` runtime ([[kernel-root-task]]). That works, with one workaround, and it can't grow into a full Rust environment without a target of sap's own. The human expects to need one anyway, for the standard library ([[2026-10-08-22c3947b#m30|§m30]]). This page is the proposed plan; nothing beyond the workaround is built.

## The position-independence problem

All sap code is position-independent ([[prog-loading]]): a program is linked to run at any address, and the loader applies *relative relocations* (add the load address to stored pointers) before it runs. Rust compiles a program's own code that way when asked (`-C relocation-model=pie`).

Rust ships `core` and `compiler_builtins` precompiled for each target, built with that target's default relocation model. For `riscv64gc-unknown-none-elf` that default is static, not position-independent, and it shows in data that holds pointers. A trait object's vtable is a table of function addresses. Position-independent code puts such tables in a writable section (`.data.rel.ro`), where the loader can fix the addresses up; static code puts them in read-only data (`.rodata`) with absolute addresses. When a position-independent program links against static `core`, those absolute addresses become relocations into read-only segments, called *text relocations*. The linker refuses them by default, which is what happened as soon as `user/init` used `core`'s debug formatting (its `PadAdapter` vtable).

The workaround is `-z notext`, which allows them. It is safe for sap's loader, which copies the image and applies relocations before mapping anything. It has two costs:
- **No sharing of read-only pages.** The plan is to map a program's read-only segments straight from the store's frames into every process running it ([[prog-loading#Code objects and formats]]). Pages carrying relocations differ per load address, so they can't be shared.
- **A loader that writes read-only data.** Every loader of sap programs, including the process manager's, must apply relocations before mapping, never in place.

## A sap target

The fix is to build `core` (and `alloc`, `compiler_builtins`) from source as position-independent code, which needs a target of sap's own:
- **A target specification** (`riscv64gc-unknown-sap`), a JSON file derived from the bare-metal one (`rustc -Z unstable-options --print target-spec-json --target riscv64gc-unknown-none-elf`), with `relocation-model: pie`, static position-independent executables, and sap's linker arguments.
- **A pinned nightly compiler** (`rust-toolchain.toml`), since custom target specifications and building the standard library from source (`-Z build-std=core,alloc,compiler_builtins`) are both unstable. The kernel can stay on stable or move with it.
- **`libsap` as the target's runtime**, as now.

Then `-z notext` goes away, read-only pages are shareable, and every crate that only needs `core` and `alloc` (most of the `no_std` ecosystem, Cap'n Proto's runtime included) works unchanged.

## The standard library

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

1. The sap target with position-independent `core` and `alloc`, before the process manager (M7) loads programs other than the root task.
2. A `std` platform layer once notifications (M4) and a log service exist.
3. Upstreaming the target to Rust as a tier 3 target when the interface is stable, as Redox, Hermit and Fuchsia did.

> [!question] Open
> Whether the kernel also moves to the pinned nightly; how the target spec is distributed to builds of other people's crates; and whether to keep `libsap` separate from the `std` platform layer or make it that layer.
