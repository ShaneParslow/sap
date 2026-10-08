# sap

sap ("sap is for sapabilities") is a capability-based operating system built around a content-addressed object store and Cap'n Proto messages over shared-memory IPC. First target: RISC-V on QEMU, with CHERI support planned (CHERI-QEMU).

## Layout

- `kernel/` — kernel source (`kernel/arch/riscv/` first).
- `docs/` — the project wiki, an Obsidian vault maintained by the agent. Start at `docs/overview.md` and `docs/index.md`.
- `tools/wiki/` — wiki helpers: `chat2md.py` (export transcripts), `pending.py` (find un-ingested sessions), `lint.py` (check the wiki).

## Working rules

- The wiki is the project's memory. Read the relevant pages before designing or answering, and keep them current as you work.
- Keep a running session summary in `docs/sessions/` from the first substantive point of every session, so a cut-off session still leaves a record. Close out sessions by diffusing them into the wiki.
- Code is the source of truth for current behavior; chats are the source of truth for decisions. Update `implemented` wiki pages in the same commit as the code they describe.
- Run `python3 -I tools/wiki/lint.py` after changing the wiki.

The full rules follow.

@docs/meta/schema.md
@docs/meta/workflows.md
@docs/meta/style.md
