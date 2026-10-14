---
status: implemented
summary: The user-side runtime every sap program links today - the thread start convention, the entry point, a debug console and panic handler, wrappers for system calls and kernel-object invocations, IPC with capability transfer, and the ELF loader the root task uses to start other programs in their own processes.
updated: 2026-10-13
code: [user/libsap/lib.rs, user/libsap/sys.rs, user/libsap/ipc.rs, user/libsap/process.rs, user/libsap/debug.rs, user/peer/main.rs, user/init/main.rs, user/init/build.rs, kernel/root_task.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# libsap, the user runtime

`libsap` (`user/libsap/`) is the Rust library every sap program builds on today: the equivalent of a C runtime plus thin wrappers over the kernel's interface. It is `no_std` and uses the shared `sap-abi` crate for every number and layout, so it can't drift from the kernel ([[kernel-threads#Syscalls]]). It will grow into the platform layer for Rust's `std` ([[prog-rust-toolchain#The standard library]]).

## Thread start convention

Every thread starts with:
- `sp` at the top of its stack;
- `tp` pointing at its IPC buffer;
- up to three arguments in `a0`–`a2`.

The kernel follows it for the root task, whose `a0` is the address of the boot-info page (`kernel/root_task.rs:222`), and every thread `libsap` starts follows it too. `tp` is free because there is no thread-local storage yet, so it serves as the per-thread pointer to the IPC buffer (`ipc_buffer`, `user/libsap/sys.rs:15`); a `TODO` moves it to a real thread-local once the Rust target has TLS.

`_start` calls the program's `sap_main(a0, a1, a2)` (`user/libsap/lib.rs`). There is no bootstrap message yet: a program started by another gets three words and capabilities in agreed slots, a stand-in for the typed bootstrap of [[kernel-process-model]] (M7).

## What it wraps

| Module | Wraps |
|---|---|
| `sys` | the raw `CALL` for kernel invocations, `YIELD`, and the IPC buffer |
| `cnode`, `untyped`, `vspace`, `tcb` | invocations on CNodes, untyped memory, page tables, frames, ASID pools and threads ([[kernel-capabilities]], [[kernel-threads]]) |
| `notification` | `SIGNAL`, `WAIT`, `POLL` |
| `ipc` | `SEND`, `NB_SEND`, `CALL`, `RECV`, `NB_RECV`, `REPLY`, `REPLY_RECV`, with capabilities to give (`Give`, moved or copied) and the receive window ([[kernel-ipc]]) |
| `debug` | the debug console (`print!`, `println!`) and halt, over the debug syscalls |
| `process` | the loader, below |

The panic handler prints the message and halts the machine with failure.

## The loader

`process::load` (`user/libsap/process.rs:121`) starts a program in a new process, given an ELF image, untyped memory to build it from, free slots in the caller's CNode, and an unused 1 GiB region of the caller's address space for scratch mappings (`Parent`, line 35). It is the first piece of the process manager (M7, [[prog-loading]]), doing in userspace what the kernel does once for the root task ([[kernel-root-task]]):

1. Retypes a page table and makes it the root of a new address space with an ASID; retypes a CNode (64 slots, `CNODE_RADIX`, line 32) and a TCB.
2. For each page of each loadable segment, retypes a frame, maps it at the scratch address in the caller's space, fills it with the file bytes that fall in that page plus the relative relocations whose targets do, unmaps it, and maps it into the new address space with the segment's permissions (writable and executable together is refused). Page tables are added as needed (`ensure_tables`, line 92), installing whatever levels are missing.
3. Maps a 16 KiB stack and an IPC buffer at the same addresses the root task uses.
4. Configures the thread with a guarded copy of the new CNode (so full-width addresses name its slots directly), the new address space and the IPC buffer; sets its priority and start registers per the convention; leaves it stopped.

The caller then copies capabilities into the new process's slots (`Process::give`) and starts it. Everything was retyped from the untyped memory the caller passed, so revoking that untyped memory destroys the whole process: its thread, address space, CNode and pages.

The loader handles exactly what sap programs are built as: static position-independent RISC-V ELF with only relative relocations ([[kernel-root-task#Building it]]). Other formats and interpreters are separate loaders, per [[prog-loading#Code objects and formats]].

## The second program

`user/peer` is a small program the root task loads with `process::load` to check IPC between address spaces. `user/init/build.rs` builds it and embeds it, as the kernel embeds `init`. `check_process` in `init` (`user/init/main.rs:810`) gives the peer a badged endpoint capability and its own thread capability, sets `init` as its fault handler, and starts it. Then it checks:
- the peer's six-word `CALL` arrives with the badge;
- the reply's words reach the peer, which reports their sum back;
- the peer's read of an address mapped only in `init`'s address space faults, and the fault comes back to `init` as a `VM` fault at that address.

Revoking the untyped memory then removes the peer entirely.

> [!todo] Not built
> A typed bootstrap message; a slot allocator and untyped allocator (today's callers hand out slot numbers by hand); thread-local storage; process exit and supervision.
