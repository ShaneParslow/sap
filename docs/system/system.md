---
status: hub
summary: Whole-system concerns that sit across the layers - running legacy software and drivers, porting to other architectures, and using remote machines without giving up authority.
updated: 2026-10-08
---

# System

Concerns that involve every layer at once and were discussed only briefly. All are exploratory; none is on the early roadmap, but all constrain the design, so they are recorded here rather than left implicit.

| Page | Status | Summary |
|---|---|---|
| [[system-legacy-compat]] | exploring | POSIX and Linux run as contained components behind typed interfaces, never as the foundation. Linux in a VM for drivers sap lacks. |
| [[system-portability]] | exploring | What is architecture-specific, why a microkernel quarantines x86's complexity, where the real cost is (drivers), and a proposed order for ports. |
| [[system-package-registry]] | exploring | A community registry of interfaces and implementations fetched into the store by hash, authority visible before install, the agent as installer, and a name directory. |
| [[system-remote-compute]] | exploring | Network transport as first class; Plan 9's `cpu` model, where computation moves to a remote machine but authority stays with the user. |
