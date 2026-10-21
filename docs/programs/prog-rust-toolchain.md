---
status: implemented
summary: How sap's Rust code is built - one pinned nightly for everything, the kernel on the built-in bare-metal target, and user programs on sap's own target (static position-independent executables, core, alloc and compiler_builtins built from source) so no read-only page carries a relocation; why the precompiled core forced text relocations before.
updated: 2026-10-19
sources: ["[[2026-10-08-22c3947b#m30|§m30]]", "[[2026-10-16-2c1ac53c#m4|§m4]]"]
code: [rust-toolchain.toml, Cargo.toml, .cargo/config.toml, user/.cargo/config.toml, user/riscv64gc-unknown-sap.json, user/build_program.rs, kernel/build.rs, user/init/build.rs, user/check/build.rs]
verified: 852c5a9
---

# Rust toolchain

All of sap's code is Rust in one Cargo workspace ([[kernel-boot-riscv#Building and running]]), built by one pinned nightly compiler. The kernel uses the built-in bare-metal target; user programs use a target of sap's own, so that every byte of them, the standard `core` library included, is position-independent. The `std` plan built on top of this is [[prog-rust-std]].

## One pinned nightly

`rust-toolchain.toml` pins `nightly-2026-10-13` with `rust-src` (the library sources `build-std` compiles), `rustfmt`, `clippy`, and the bare-metal target's precompiled libraries for the kernel. rustup installs it on first use. The kernel uses the same compiler as the user programs, so the two never disagree about a compiler-dependent detail; the human chose one nightly for everything ([[2026-10-16-2c1ac53c#m4|§m4]]). The pin moves only deliberately.

## Two targets

| | Kernel | User programs |
|---|---|---|
| Target | `riscv64gc-unknown-none-elf` (built in) | `riscv64gc-unknown-sap` (`user/riscv64gc-unknown-sap.json`) |
| Set by | `.cargo/config.toml` | `user/.cargo/config.toml`, for builds run inside `user/` |
| Relocation model | static, linked at a fixed address by `kernel/arch/riscv/linker.ld` | position-independent (`pie`) |
| `core`, `alloc`, `compiler_builtins` | precompiled | built from source (`build-std`), with `compiler-builtins-mem` for `memcpy` and friends |
| Output | a fixed-address ELF that QEMU loads | a static position-independent ELF with only relative relocations |

The sap target is the bare-metal one (`rustc -Z unstable-options --print target-spec-json --target riscv64gc-unknown-none-elf`) changed in four ways: `"os": "sap"`, `relocation-model` `pie`, static position-independent executables (with static linking as the default), and sap's linker arguments as `pre-link-args`, so each segment starts on its own page and there is no RELRO. Static PIE makes rustc link with `-static -pie --no-dynamic-linker -z text`. The last of those makes the linker refuse a relocation in a read-only segment, so the build itself checks the property below. Custom target specifications and `build-std` are unstable, hence the nightly, and this nightly also wants `json-target-spec` enabled in the config.

## Why `core` had to be rebuilt

The loader puts a position-independent program at any address and adds that address to each stored pointer (a *relative relocation*) before it runs ([[prog-runtime#The loader]], [[kernel-root-task]]). Rust compiles a program's own code that way when asked, but it ships `core` and `compiler_builtins` precompiled with the target's default model, which for the bare-metal target is static. Static code keeps tables of pointers, such as a trait object's vtable, in read-only data with absolute addresses. Linked into a position-independent program, those became relocations in read-only segments (*text relocations*), which the linker refuses by default. The first program to use `core`'s debug formatting hit it.

Until 2026-10-19 the workaround was `-z notext`, which allowed them. Built from source for the sap target, `core` puts such tables in writable data (`.data.rel.ro`, which without RELRO is part of the writable segment), so now:
- **every relocation lands in the writable segment** (all 184 of `init`'s, checked with `readelf`); the read-only and executable segments are the same in every process that runs the program, so they can be mapped straight from the store's frames and shared ([[prog-loading#Code objects and formats]]);
- the image is smaller (`init` went from about 333 KiB to 243 KiB), since `core` is compiled with the program's own settings;
- any crate that needs only `core` and `alloc`, most of the `no_std` ecosystem and Cap'n Proto's runtime included, builds for sap unchanged.

## Building a program from a build script

The kernel embeds the root task, and a program embeds the programs it loads (`init` its boot pack, `check` the peer), each with `include_bytes!`. Both build scripts call one helper, `build_user_program` (`user/build_program.rs:15`), shared through `include!` because a build-dependency crate inside the workspace would also be checked for the bare-metal target. It runs Cargo in release mode from the program's directory, so `user/.cargo/config.toml` applies; drops the outer build's flags and target, which would override that config; gives the program a target directory of its own inside the caller's build output, since sharing the caller's would deadlock on its lock; and makes the caller's build script rerun whenever anything under `user/`, `abi/` or `interfaces/` changes.

Building a user crate any other way, such as rust-analyzer's `cargo check --workspace` from the top, uses the kernel's target. That is fine for checking: the two targets differ only in relocation model and linking.

> [!question] Open
> How the target specification reaches builds of other people's crates (a published file, an upstream tier 3 target); see [[prog-rust-std#Order]].
