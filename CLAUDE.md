# sap

sap ("sap is for sapabilities") is a capability-based operating system built around a content-addressed object store and Cap'n Proto messages over shared-memory IPC. First target: RISC-V on QEMU, with CHERI support (RVY draft, CHERI-QEMU).

## Layout

- `kernel/` — kernel source (`kernel/arch/riscv/` first).
- `abi/` — the kernel's binary interface, shared by the kernel and user code.
- `user/` — userspace: runtime libraries in `user/lib/`, programs in `user/programs/`.
- `interfaces/` — Cap'n Proto schemas: `system/` (the system's interfaces, and `component.capnp`, how a program defines itself as a component) and `test/` (interfaces only the checks use). Each program's own schema, its bootstrap struct with its declarations, is `user/programs/<name>/<name>.capnp`.
- `boot/` — boot requests: what the resolver runs, chosen with `SAP_BOOT=<name>` (a file here, or a program's name; `check` by default).
- `tools/board/` — building and booting on real boards (`d1/`: the MangoPi MQ-Pro over USB).
- `tools/manifests/` — host-only crate for init's build: reads programs' declarations (`interfaces/system/component.capnp` annotations) into manifests for the boot pack, and the boot request into the resolver's bootstrap. Excluded from the workspace.
- `toolchain/` — the CHERI toolchain and xfel as submodules, built by `toolchain/build.sh` into `toolchain/out/`.

## Notes and design

Design notes, decisions, the code map and session journals live in `~/src/sap-docs`, a separate local git repository (not pushed; not part of this repo). Its README says how to use it; the two files below are loaded every session.

@~/src/sap-docs/README.md
@~/src/sap-docs/now.md

## Code rules

- Mark unfinished or temporary work in code with a `TODO:` comment saying what is missing and when it is needed (for example `// TODO: switch to a kernel stack via sscratch for user-mode traps (M2)`), so `grep -rn TODO kernel/ user/` finds it. Don't leave it implicit in prose like "not yet" or "for now".
- Commits are feature-sized: one feature per commit, each one building and booting. Split mixed work before committing.
- When a change alters behavior that `~/src/sap-docs/code-map.md` describes, update that entry (and commit sap-docs separately).
- Name deliberately: one name per concept, meaning the same thing in code, schemas, comments and sap-docs (`glossary.md`), and never a word already used for something else. When a better name appears, or a name has drifted from what the thing does, rename it everywhere (code, schemas, comments, docs) instead of living with it; the same goes for structure that no longer fits. Renames and refactors are cheap now and only get dearer.
- Write code to be read and referenced by a person, not to be compact:
  - Keep lines within 100 columns, in code and comments.
  - Separate the logical steps of a function with blank lines; a run of more than about eight lines with no break usually needs one. Put a blank line before a comment that introduces a new step.
  - One binding per `let`, unless the values really are a pair (`let (slot, cap) = ...`). Don't pack unrelated slots or constants into one tuple.
  - Prefer a named local or a small helper to a long nested expression, and factor out setup that repeats, in checks as much as anywhere.
  - In a `match` or a long `if` chain, give any arm with more than a line or two room to breathe.
- The goal is code that meets rustfmt's default rules, reached mostly by hand. `cargo fmt --all -- --check` reports what is left. Don't run `cargo fmt` over existing code; bring each file in line as it is reviewed. New code should meet the rules from the start.
