---
status: in-progress
summary: Whether page transfer's per-page cost dooms the no-write guarantee for large messages, what CHERI offers for moving ownership, and whether CHERI should come before M10.
updated: 2026-10-20
session: 685b0203-610e-4a35-9891-a6d55f52b28d
transcript: "[[2026-10-20-685b0203]]"
---

# 2026-10-20 · IPC cost and CHERI

## Goal

Decide what to do about page transfer losing to copying (six system calls per page), with the D1 measurement blocked until a USB-serial adapter is available; and whether CHERI should move earlier.

## Running notes

- Session start: `pending.py` reports [[2026-10-08-22c3947b-wiki-setup]] and [[2026-10-16-2c1ac53c-code-walkthrough]] still `in-progress`; told the human, not recovered yet.
- Human: six calls per transfer makes the IPC concept look doomed; asked whether to drop the no-write guarantee, whether CHERI helps, and whether CHERI has an atomic transfer to another thread. Keeping the copy (safe) / volatile (trusted or bulk) split for now.
- Answered: six per page is how transfer was built from generic invocations, not its floor; the floor on paging is call + reply + page-table edits + a TLB flush, and with several harts a shootdown. Proposed three tiers: copy structure (small), bulk bytes in shared buffers read through a volatile byte type (memory-safe, not consistent), transfer only for large structured messages from untrusted peers. Not decided.
- CHERI has no move instruction. Filed in [[ipc-paging-vs-cheri#Moving ownership]]: linear capabilities (experimental in CHERI ISAv7, StkTokens; no hardware, not in the RISC-V standard) and CHERIoT-style lending of non-global capabilities, which can make a sender unable to keep its write access if a trusted owner lends it the buffer only for the length of a call.
- Human: everything here is userspace, so alternatives can be tried in parallel; wondering whether CHERI should come sooner despite poor Rust support. Suggested a time-boxed spike (kernel on CHERI-QEMU in hybrid mode, then a hand-written compartment transport) without moving purecap Rust userspace earlier. Awaiting the human.
- (2026-10-20) Human: boot on CHERI, on a branch; asked whether to target RVY (the draft standard) through a QEMU fork, or Morello. Found: upstream QEMU has no CHERI; RVY draft is at 0.9.10 (riscv/riscv-cheri release 2026-10-14); cheribuild's `rvy-qemu`, `rvy-llvm` and `cheri-alliance-opensbi` targets build forks from Capabilities-Limited (QEMU 7.1 base, branch `user/qwattash/rvfi-fixes-0910`, updated 2026-10-17); CHERI-Alliance/qemu main has only v9 (`riscv64xcheri`) and the 0.9.3 draft (`riscv64cheristd`); cheribuild marks RVY hybrid as unsupported by the toolchain (purecap only). Recommended RVY over Morello (no new arch port, standards track, CHERI-seL4 also targets it).
- Branch `cheri` created. Cloned the RVY QEMU fork to `~/src/cheri-qemu`; QEMU 7.1's meson needs `distutils`, gone in Python 3.14, so it builds with a venv that has setuptools.
- Read the fork's reset: harts reset with root PCC/DDC and the Y enables (`menvcfg.Y`, `senvcfg.Y`) clear, so an unmodified kernel and the non-CHERI bundled OpenSBI should run as plain RV64. Using capabilities in S-mode needs a CHERI-aware OpenSBI, since a plain one saves and restores registers as integers and would strip tags on every M-mode trap.
- Built `qemu-system-riscv64y` (RVY 0.9.10). It defaults to a CHERI OpenSBI (`opensbi-riscv64cheristd-generic-fw_jump.bin`) it doesn't ship; with Arch's plain OpenSBI 1.8.1 (`-bios /usr/share/qemu/opensbi-riscv64-generic-fw_dynamic.bin`) the unmodified release kernel boots and passes every `init` check; the copy/transfer numbers match `qemu-system-riscv64`. Filed the emulator facts in [[hw-cheri#Platform support]].
- Human: build LLVM; asked for an AUR package or a PKGBUILD in `~/pkg`. No AUR package exists for any CHERI tool. Wrote `~/pkg/cheri-rvy-llvm/PKGBUILD` (fork commit `457ae0c`, LLVM 20, clang+lld, RISC-V only, assertions on). First put under `/opt`; the human questioned it, and it now installs to `/usr/lib/cheri-rvy-llvm`, as Arch's `clang20` uses `/usr/lib/llvm20` (target-prefixed names are a GCC convention; LLVM's libraries, resource directory and `ld.lld` would collide in `/usr`). RVY is chosen by `-march=rv64y…`, not by the triple. cmake isn't installed, so the build waits on the human.
- Human: what about the process manager (M7) while LLVM builds; the interface-declaration and hashing tangle. Proposed a cut (below in chat): M7 without schema hashing or code objects, building on what `sap-loader` already does.
- (§m12) Human: keep the toolchain in the repository as submodules (two machines), on the `cheri` branch. `fd90573` (cheri): the three RVY forks as shallow submodules in `toolchain/` and `toolchain/build.sh` (llvm, qemu, opensbi into `toolchain/out/`). `git submodule add --depth 1 -b` failed to check out a non-default branch, so each was cloned shallow and then added. The `~/pkg` PKGBUILD is dropped. On `master` the toolchain directories are excluded locally (`.git/info/exclude`), with a copy of `build.sh`, so builds continue while M7 work happens there.
- (§m12) Human decided M7: `init` becomes the process manager with its checks split into a spawned program; bootstrap capabilities as slot indices until RPC; BLAKE3 for code. Logged in [[decisions]], [[roadmap]], [[prog-loading]], [[kernel-process-model]]. CHERI spike recorded in [[roadmap#Next]] and the variant question closed in [[hw-cheri]].
