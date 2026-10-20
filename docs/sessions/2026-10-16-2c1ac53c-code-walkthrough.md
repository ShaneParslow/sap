---
status: in-progress
summary: A guided walkthrough for the human of the M2-M4 work - seL4's concepts and sap's departures, the code layout, IPC from userspace and one level down, the last session's decisions and their consequences - plus a review of usize in the ABI and of the Cargo layout and build scripts.
updated: 2026-10-16
session: 2c1ac53c-ce0d-4f8d-ad5e-208e97e59a1f
transcript: "[[2026-10-16-2c1ac53c]]"
---

# 2026-10-16 · Code walkthrough and build review

## Goal

The human asked for a walkthrough of what was built in [[2026-10-08-22c3947b-wiki-setup]] (M2-M4): seL4's concepts and sap's departures from them, the large parts of the code (especially `user/libsap` and `abi`), IPC from a program's point of view and one level under it, and the last session's decisions with their consequences. Two review questions: whether the pervasive `usize` is fine, and whether the Cargo layout and build scripts still fit now that there are user programs and a shared ABI crate.

## Outcome
## Decisions
## Pages changed
## Commits
## Open threads

## Running notes

- Session start: `pending.py` reports [[2026-10-08-22c3947b-wiki-setup]] still `in-progress` and its export behind; offered recovery, not done yet.
- Booted at `d9f16dd`: every `init` check passes, 10,000 ring round trips, ~418 sleeps per side.
- Found: `user/init/build.rs` doesn't list `abi/` in `rerun-if-changed`, so an ABI-only change rebuilds `init` but embeds a stale `peer`. Reproduced by touching `abi/lib.rs`: `init`'s ELF was rebuilt, `peer`'s kept its 10-14 timestamp. Not fixed; offered.
- Found: a signal through an unbadged notification capability (badge 0) with nobody waiting is lost, because "pending" is `word != 0` (`kernel/notification.rs`). seL4 keeps an explicit Active state, so its zero-badge signal is retained (`sendSignal`/`receiveSignal`, seL4 `6df0b6e` `src/object/notification.c`). Today's code always signals through badged capabilities, so nothing fails. Not fixed; offered.
- Noted from the transcript: of the M4 choices A-F (§m33), the human chose A, C, D and E (§m34); B (batched move/copy transfer, settled at 16) and F (the deferrals) were taken as agreed because the human didn't object (§m35). Both are logged as decided in [[decisions]].
- Review, `usize`: right for register-sized ABI words (seL4 uses `seL4_Word` the same way). Its costs are elsewhere: one type for capability addresses, badges, labels, rights, slot indices and addresses, so arguments can be swapped without a compile error; integer-to-pointer casts that a CHERI purecap build won't allow; and arithmetic on indices a peer controls (`produced - consumed` in `ring.rs` panics in a debug build if a hostile consumer moves its index past the producer's). Suggested `#[repr(transparent)]` newtypes for capability addresses and rights, explicit `wrapping_`/`checked_` arithmetic on shared-memory values, and pointers or strict-provenance APIs where addresses become pointers.
- Review, build: five separate packages (no workspace), three tracked lockfiles plus two stray untracked ones (`abi/`, `user/libsap/`), a nested Cargo chain kernel → init → peer from build scripts, linker arguments and nested-build code duplicated. Fine for now; it grows one nesting level or one build-script stanza per embedded program. Suggested: fix the `rerun-if-changed` bug now; build user programs from one place once a boot image exists (M7), with a build tool (`xtask`) or Cargo's unstable artifact dependencies; fold the per-crate `.cargo` configs into the sap user target (I1).
- Human: fix the bugs. `3a25859` init's build script watches `abi/`; `1beca7f` notifications get seL4's idle/waiting/active states, POLL reports a pending flag in `a2`, and libsap's `poll` returns `Option` (init's old test had asserted the lost signal); `e69ee1e` the producer's fill uses `wrapping_sub` on the consumer-written index.
- Human: no rust-analyzer on user programs. Cause: no top-level manifest, so rust-analyzer found only `kernel/` and `abi/` (one level deep). `bed195a` makes the repository one workspace (one lockfile, root profiles, kernel default member); target and runner in `.cargo/config.toml`, the PIE flag in `user/.cargo/config.toml` for builds run from `user/`. Stray `abi/` and `libsap/` lockfiles and target dirs removed. Checked: debug and release boot, `cargo check --workspace --all-targets` clean, rust-analyzer now analyses `user/init/main.rs`.
- `11f8ace` re-verified the threads, traps and libsap pages.
