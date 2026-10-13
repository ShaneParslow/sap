---
status: hub
summary: Ordered work items - the kernel decision, the bring-up milestones on RISC-V, and design exercises that can run in parallel.
updated: 2026-10-11
sources: ["[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Roadmap

This is the to-do list. Items are ordered by dependency; each links to the design it implements. When an item is done, its design pages become `implemented` (see [[workflows]], section 6) and the item is marked done here with the commit.

## M0. Kernel path: own kernel (done 2026-10-08)

sap writes its own kernel ([[kernel-choice]]). The seL4 column below is kept for reference, as the closest existing implementation of each milestone.

Tooling: QEMU's `virt` machine loads its bundled OpenSBI by default (`-bios default`; on Arch, `qemu-system-riscv` pulls in `qemu-system-riscv-firmware`), and the kernel is loaded at `0x80200000` with `-kernel`, entered in S-mode. CHERI-QEMU and a CHERI OpenSBI come later, normally built with `cheribuild` (M10).

## Bring-up milestones

The human's proposed order (console, log, isolation, mapping, IPC, processes, store) with the chat's two amendments: traps, timer, page tables and user mode come right after the console, and capability tables come before IPC because IPC transfers capabilities ([[2026-09-27-future-of-computing#m28|§m28]], [[2026-09-27-future-of-computing#m29|§m29]]).

| # | Milestone | Design | seL4 path |
|---|---|---|---|
| M1 | Boot on QEMU `virt` riscv64; console output through SBI (OpenSBI); kernel log. | [[hw-isa]] | Hello world on Microkit + rust-sel4. |
| M2 | Trap handling, timer interrupts, minimal device-tree parsing (RAM, reservations, timebase), Sv39 page tables, entry to user mode. | [[kernel-objects-and-syscalls]] | Provided. |
| M3 | Capability tables and the first kernel objects: untyped memory, frames, address spaces, threads; invocation via `Call`. | [[cap-model]], [[kernel-objects-and-syscalls]] | Provided. |
| M4 | Endpoints and notifications; two isolated components sharing a ring in a shared frame, with notifications as doorbells. | [[ipc-channels]], [[ipc-cap-transfer]] | Microkit system with a shared region and channel. |
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

## Done

- M0: own kernel (no commit; design decision).
- M1: boots on QEMU `virt`, logs with levels through the SBI debug console, powers off through SBI reset ([[kernel-boot-riscv]]; `a4d43db`, `013eb44`).
- M2 (2026-10-11): traps and timer ([[kernel-traps-riscv]]; `2420fdc`, `2c41a33`); boot information from the device tree and failure exit status (`4629d35`, `2799abb`), made architecture-neutral (`e875153`) ([[kernel-boot-riscv]]); Sv39 paging with the kernel in the upper half (`6325cb4`, [[kernel-memory-riscv]]); user mode with a root task loaded by the kernel (`1be5924`, [[kernel-root-task]]). CSR and SBI access moved to the `riscv` and `sbi-rt` crates (`ff1fdea`).
