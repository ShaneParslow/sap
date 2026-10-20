---
status: hub
summary: Ordered work items - the kernel decision, the bring-up milestones on RISC-V, and design exercises that can run in parallel.
updated: 2026-10-19
sources: ["[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Roadmap

This is the to-do list. Items are ordered by dependency; each links to the design it implements. When an item is done, its design pages become `implemented` (see [[workflows]], section 6) and the item is marked done here with the commit.

## M0. Kernel path: own kernel (done 2026-10-08)

sap writes its own kernel ([[kernel-choice]]). The seL4 column below is kept for reference, as the closest existing implementation of each milestone.

Tooling: QEMU's `virt` machine loads its bundled OpenSBI by default (`-bios default`; on Arch, `qemu-system-riscv` pulls in `qemu-system-riscv-firmware`), and the kernel is loaded at `0x80200000` with `-kernel`, entered in S-mode. CHERI-QEMU and a CHERI OpenSBI come later, normally built with `cheribuild` (M10).

## Next

The order agreed on 2026-10-19 ([[2026-10-16-2c1ac53c#m10|§m10]]), from the items below:
1. ~~I4, splitting `libsap`~~ (done).
2. ~~I5, provenance hygiene in user code~~ (done).
3. **M5**, Cap'n Proto messages across the rings; M7's bootstrap message is itself a Cap'n Proto message.
4. **M7**, the process manager: programs start their runtime from the bootstrap message, and the loader moves onto the runtime's slot allocator.
5. **I2**, `std`, once thread-local storage, a log service and the process ABI exist.

CHERI stays at M10: the provenance lint and the rules in [[hw-cheri#Keeping the door open]] keep it reachable, and Rust has no CHERI-RISC-V purecap compiler yet ([[hw-cheri#Platform support]]).

## Bring-up milestones

The human's proposed order (console, log, isolation, mapping, IPC, processes, store) with the chat's two amendments: traps, timer, page tables and user mode come right after the console, and capability tables come before IPC because IPC transfers capabilities ([[2026-09-27-future-of-computing#m28|§m28]], [[2026-09-27-future-of-computing#m29|§m29]]).

| # | Milestone | Design | seL4 path |
|---|---|---|---|
| M1 | Boot on QEMU `virt` riscv64; console output through SBI (OpenSBI); kernel log. | [[hw-isa]] | Hello world on Microkit + rust-sel4. |
| M2 | Trap handling, timer interrupts, minimal device-tree parsing (RAM, reservations, timebase), Sv39 page tables, entry to user mode. | [[kernel-objects-and-syscalls]] | Provided. |
| M3 | Capability tables and the first kernel objects: untyped memory, frames, address spaces, threads; invocation via `Call`. | [[cap-model]], [[kernel-objects-and-syscalls]] | Provided. |
| M4 | Notifications (badges, binding to a thread); endpoints with MCS-style reply objects; capability transfer (up to 16, move or copy); fault delivery over IPC; `init` loads a second program, and the two share a ring with notifications as doorbells. | [[ipc-channels]], [[ipc-cap-transfer]] | Microkit system with a shared region and channel. |
| M5 | Cap'n Proto messages across the ring, with single-fetch reads and copy-small/transfer-large. | [[ipc-validation]], [[iface-capnp]] | Same. |
| M6 | Capability transfer with own/borrow semantics; `$own`/`$borrow` annotations and Rust codegen. | [[cap-ownership]] | Host-mediated moves (seL4 copies on transfer). |
| M7 | Dynamic process spawning: a userspace process manager loading static ELF from a boot image, the process ABI, initial capability table, bootstrap capability, process factory as its RPC interface. | [[kernel-process-model]], [[prog-loading]], [[cap-process-authority]] | Own root task; Microkit is static. |
| M8 | A virtio block driver as a component. | [[hw-devices-and-io]] | Can start from sDDF. |
| M9 | Values and refs in memory; then the store over the block driver, as an append-only log plus index. | [[store-data-model]], [[store-on-disk-format]] | Same. |
| M10 | CHERI: build for CHERI-RISC-V under CHERI-QEMU; compartments and per-channel capability pools as a faster transport. | [[hw-cheri]], [[ipc-paging-vs-cheri]] | Not available in mainline seL4. |

## Design exercises (no code dependency)

These can happen at any time and feed the milestones above.

- **D1. Specify one small interface completely** (a clipboard or a key-value store): schema, ownership, failure modes and idempotency, intent, conformance tests. Shows what the spec format is missing ([[2026-09-27-future-of-computing#m11|§m11]]; [[iface-spec-contents]]).
- **D2. Use-case walkthroughs for the object store**, before committing to its design ([[2026-09-27-future-of-computing#m9|§m9]]). Each stresses naming, revocation or performance differently:
  1. sharing a live structured object instead of a screenshot;
  2. an agent reorganizing a photo library under a time and capability budget;
  3. upgrading a library across an incompatible interface version;
  4. a game that needs an attested enclave;
  5. a legacy POSIX build tool running through the compatibility layer.

  Started 2026-10-11 with a different set (roaming preferences, granting a website data, a system agent, device roles, Linux interop) in [[store-use-cases]]; the five above are still to do.
- **D3. Check capnproto-rust's RPC feature coverage** against what M5–M6 need ([[iface-capnp]]).
- **D4. Read Sandstorm's powerbox and sturdy-ref design** before designing the store's grant UI ([[sandstorm]]).
- **D5. Write the kernel interface as a schema:** object types and their methods in Cap'n Proto, the syscall ABI, and the process ABI, with each deliberate difference from seL4 noted. Feeds M2–M7 ([[kernel-sel4-differences]], [[cap-process-authority]], [[prog-loading]]).
- **D6. Read the OCapN specifications** (CapTP, netlayers, sturdy refs, three-party handoff) before designing the network bridge ([[ipc-network-rpc]]).

## Infrastructure

- **I2. A `std` platform layer** for sap: thread-local storage, then `std` once a log service exists ([[prog-rust-std]]). The heap, slot allocator and mutexes it builds on exist ([[prog-libsap#Slots, locks and the heap]]).
- **I3. seL4-style zombie capabilities:** iterative, preemptible deletion, before untrusted programs run ([[kernel-capabilities#Delete, finalise, revoke]]).

## Done

- M0: own kernel (no commit; design decision).
- M1: boots on QEMU `virt`, logs with levels through the SBI debug console, powers off through SBI reset ([[kernel-boot-riscv]]; `a4d43db`, `013eb44`).
- M2 (2026-10-11): traps and timer ([[kernel-traps-riscv]]; `2420fdc`, `2c41a33`); boot information from the device tree and failure exit status (`4629d35`, `2799abb`), made architecture-neutral (`e875153`) ([[kernel-boot-riscv]]); Sv39 paging with the kernel in the upper half (`6325cb4`, [[kernel-memory-riscv]]); user mode with a root task loaded by the kernel (`1be5924`, [[kernel-root-task]]). CSR and SBI access moved to the `riscv` and `sbi-rt` crates (`ff1fdea`).
- M3 (2026-10-13): threads and a priority scheduler ([[kernel-threads]]; `55838c6`); capability spaces, the derivation tree, `CALL` and CNode operations, the root task's initial capabilities (`64cda48`); untyped retype (`0c39997`); page-table, frame and ASID invocations with hardware ASIDs (`e69595e`); thread invocations, so userspace creates and schedules threads ([[kernel-capabilities]]). Shared `sap-abi` crate and `libsap` runtime.
- M4 (2026-10-13): notifications with badges (`ed24f74`); endpoints with MCS-style reply objects and bound notifications (`48f0571`); capability transfer, up to 16 per message (`a7e19d3`); fault delivery (`1b77a5f`); a userspace ELF loader and a second process (`810d575`); shared-memory rings between two processes ([[kernel-ipc]], [[prog-libsap]], [[ipc-rings]]).
- I1 (2026-10-19): one pinned nightly for everything, and sap's own target for user programs with `core`, `alloc` and `compiler_builtins` built from source; `-z notext` removed ([[prog-rust-toolchain]]).
- I4 (2026-10-19): `libsap` split into `sap-sys`, the `libsap` runtime with an optional heap, `sap-ring` and `sap-loader`; `libsap::init` became `runtime::start` ([[prog-libsap#Crates]]).
- I5 (2026-10-19): user code gets pointers only from `sap_sys::address::mapped`; user crates deny `implicit_provenance_casts` and `integer_to_ptr_transmutes`; `clippy.toml` disallows the exposed-provenance functions elsewhere ([[hw-cheri#Keeping the door open]]).
