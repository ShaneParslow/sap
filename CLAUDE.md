# sap

sap ("sap is for sapabilities") is a capability-based operating system built around a content-addressed object store and Cap'n Proto messages over shared-memory IPC. First target: RISC-V on QEMU, with CHERI support (RVY draft, CHERI-QEMU).

## Layout

- `kernel/` — kernel source (`kernel/arch/riscv/` first).
- `abi/` — the kernel's binary interface, shared by the kernel and user code.
- `user/` — userspace: runtime libraries in `user/lib/`, programs in `user/programs/`.
- `interfaces/` — Cap'n Proto schemas.
- `tools/board/` — building and booting on real boards (`d1/`: the MangoPi MQ-Pro over USB).
- `toolchain/` — the CHERI toolchain and xfel as submodules, built by `toolchain/build.sh` into `toolchain/out/`.

## Notes and design

Design notes, decisions, the code map and session journals live in `~/src/sap-docs`, a separate local git repository (not pushed; not part of this repo). Its README says how to use it; the two files below are loaded every session.

@~/src/sap-docs/README.md
@~/src/sap-docs/now.md

## Code rules

- Mark unfinished or temporary work in code with a `TODO:` comment saying what is missing and when it is needed (for example `// TODO: switch to a kernel stack via sscratch for user-mode traps (M2)`), so `grep -rn TODO kernel/ user/` finds it. Don't leave it implicit in prose like "not yet" or "for now".
- Commits are feature-sized: one feature per commit, each one building and booting. Split mixed work before committing.
- When a change alters behavior that `~/src/sap-docs/code-map.md` describes, update that entry (and commit sap-docs separately).
