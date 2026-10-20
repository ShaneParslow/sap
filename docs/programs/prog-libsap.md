---
status: implemented
summary: The user-side crates every sap program links today - sap-sys (stateless system-call and invocation wrappers), libsap (the thread start convention, entry point, debug console, panic handler, and a runtime with a slot allocator, notification-backed mutexes and an optional heap for alloc), sap-ring, and sap-loader, the ELF loader the root task uses to start other programs in their own processes.
updated: 2026-10-19
code: [user/sys/address.rs, clippy.toml, user/libsap/lib.rs, user/libsap/runtime.rs, user/libsap/Cargo.toml, user/sys/lib.rs, user/libsap/slots.rs, user/libsap/sync.rs, user/libsap/heap.rs, user/sys/vspace.rs, user/sys/sys.rs, user/sys/ipc.rs, user/loader/lib.rs, user/ring/lib.rs, user/libsap/debug.rs, user/peer/main.rs, user/init/main.rs, user/init/build.rs, kernel/root_task.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# libsap, the user runtime

sap's user programs build on four `no_std` crates under `user/`, which take every number and layout from the shared `sap-abi` crate, so they can't drift from the kernel ([[kernel-threads#Syscalls]]). `libsap` is the runtime, the equivalent of a C runtime; it will grow into the platform layer for Rust's `std` ([[prog-rust-std]]).

## Thread start convention

Every thread starts with:
- `sp` at the top of its stack;
- `tp` pointing at its IPC buffer;
- up to three arguments in `a0`–`a2`.

The kernel follows it for the root task, whose `a0` is the address of the boot-info page (`kernel/root_task.rs:222`), and every thread `libsap` starts follows it too. `tp` is free because there is no thread-local storage yet, so it serves as the per-thread pointer to the IPC buffer (`ipc_buffer`, `user/sys/sys.rs:15`); a `TODO` moves it to a real thread-local once the Rust target has TLS.

`_start` calls the program's `sap_main(a0, a1, a2)` (`user/libsap/lib.rs`). There is no bootstrap message yet: a program started by another gets three words and capabilities in agreed slots, a stand-in for the typed bootstrap of [[kernel-process-model]] (M7).

## Crates

| Crate | Holds | Relies on |
|---|---|---|
| `sap-sys` (`user/sys/`) | stateless wrappers over the kernel interface, as libsel4 is to seL4: `address` (`mapped`, `user/sys/address.rs:18`, the only way user code turns an address into a pointer; the user crates deny integer-to-pointer casts and `clippy.toml` disallows the exposed-provenance functions elsewhere, [[hw-cheri#Keeping the door open]]), `sys` (the raw `CALL` for kernel invocations, `YIELD`, the IPC buffer), `cnode`, `untyped`, `vspace` and `tcb` invocations ([[kernel-capabilities]], [[kernel-threads]]), `notification` (`SIGNAL`, `WAIT`, `POLL`) and `ipc` (`SEND`, `NB_SEND`, `CALL`, `RECV`, `NB_RECV`, `REPLY`, `REPLY_RECV`, with capabilities to give, moved or copied, and the receive window, [[kernel-ipc]]) | nothing but `tp` pointing at the IPC buffer |
| `libsap` (`user/libsap/`) | the entry point, the panic handler (prints and halts with failure), the `debug` console (`print!`, `println!`), and the runtime: `runtime`, `slots`, `sync` and, behind the `heap` feature (on by default), `heap`; it re-exports `sap-sys`'s modules | resources given to `runtime::start` |
| `sap-ring` (`user/ring/`) | shared-memory rings between processes ([[ipc-rings]]) | `sap-sys` |
| `sap-loader` (`user/loader/`) | the ELF loader, below | `sap-sys`, the `elf` crate |

A program that doesn't allocate turns the `heap` feature off, as `user/peer` does, and then carries no allocator; using `alloc` without it fails to link. The split (roadmap I4) keeps state, allocation and policy out of the system-call layer, so anything can build on `sap-sys` alone.

## Slots, locks and the heap

A program hands the runtime what it may use on the program's behalf, once, before it allocates: `runtime::start` with `Resources` (`user/libsap/runtime.rs:14`), naming its root CNode, a range of free slots and an untyped region, and with the heap, its address space and an unused address range for the heap to grow into. The runtime retypes two notifications from the untyped region for its locks, then starts the slot allocator and the heap (`start`, line 37). Revoking the untyped region destroys everything the runtime made from it. Only the root task calls it today; a program started by another will get its resources in the typed bootstrap message, and its entry code will start the runtime before `sap_main` (M7).

**Slots** (`user/libsap/slots.rs`). A bitmap over the given range of the root CNode, up to 4,096 slots (the root task's whole CNode), handing out single slots or contiguous runs (`alloc_range`, line 45), since retype fills consecutive slots. Slot numbers are capability addresses because every root CNode so far is one guarded level. Freeing a slot is the caller's promise that it is empty. Growing past one CNode with second-level CNodes is a `TODO` for the process manager (M7).

**Locks** (`user/libsap/sync.rs`). A `Mutex` in the style of a futex-based one (Drepper's "mutex2"): a word that is unlocked, locked, or locked with possible waiters. An uncontended lock and unlock is one atomic operation each. A thread that finds the mutex taken marks it contended and sleeps on the mutex's notification; an unlock that finds it contended signals (`lock`, line 53). Sleeping rather than spinning matters with priorities: a spinning high-priority thread would never let a lower-priority holder run. Each mutex has its own notification, because with one shared between two mutexes a waiter on one could take the signal meant for a waiter on the other, which would then sleep with its mutex free. The notifications are unbadged, so this relies on unbadged signals being kept ([[kernel-ipc#Notifications]]). Before a mutex has a notification, a contended lock yields in a loop.

**The heap** (`user/libsap/heap.rs`). The global allocator behind `alloc` (`Box`, `Vec`, `String`), so `alloc` works in every sap program that has the `heap` feature and starts the runtime. It is a first-fit free list (the `linked_list_allocator` crate, without its spinlock) behind a `Mutex`, over the heap's address range. Nothing is mapped until the first allocation. When the list can't satisfy one, the heap grows at the top by at least 64 KiB (`GROW_MIN`, line 31): it takes a run of slots, retypes that many 4 KiB frames from the untyped region (at most 256 per retype), and maps each, adding a page table only when a mapping fails for lack of one (`vspace::map_frame_with_tables`, `user/sys/vspace.rs:26`) (`grow`, `user/libsap/heap.rs:97`). Allocation fails, and `alloc` panics, before `runtime::start` or once the range or the memory runs out.

> [!todo] Not built
> The heap never shrinks: freed memory returns to the free list but stays mapped. The loader still uses its own bump allocator for slots (`Slots`) and adds page tables by retyping one and deleting it if every level exists, which spends untyped memory that only a revoke recovers; both should move to the runtime's allocator and `map_frame_with_tables`.

`init`'s `check_heap` (`user/init/main.rs:992`) checks that nothing is mapped before the first allocation, growth past 2 MiB (across level-0 page tables), a page-aligned allocation, reuse of freed memory without growth, formatting into a `String`, two threads allocating 40,000 vectors each at once, and a thread that finds a mutex held sleeping until `init` unlocks it. The heap gets the largest untyped region other than the first (`runtime_untyped`, line 36), and the other checks pick theirs around it, since several revoke their region when done.

## The loader

`sap_loader::load` (`user/loader/lib.rs:131`) starts a program in a new process, given an ELF image, untyped memory to build it from, free slots in the caller's CNode, and an unused 1 GiB region of the caller's address space for scratch mappings (`Parent`, line 37). It is the first piece of the process manager (M7, [[prog-loading]]), doing in userspace what the kernel does once for the root task ([[kernel-root-task]]):

1. Retypes a page table and makes it the root of a new address space with an ASID; retypes a CNode (64 slots, `CNODE_RADIX`, line 34) and a TCB.
2. For each page of each loadable segment, retypes a frame, maps it at the scratch address in the caller's space, fills it with the file bytes that fall in that page plus the relative relocations whose targets do, unmaps it, and maps it into the new address space with the segment's permissions (writable and executable together is refused). Page tables are added as needed (`ensure_tables`, line 102), installing whatever levels are missing.
3. Maps a 16 KiB stack and an IPC buffer at the same addresses the root task uses.
4. Configures the thread with a guarded copy of the new CNode (so full-width addresses name its slots directly), the new address space and the IPC buffer; sets its priority and start registers per the convention; leaves it stopped.

The caller then copies capabilities into the new process's slots (`Process::give`) and starts it. Everything was retyped from the untyped memory the caller passed, so revoking that untyped memory destroys the whole process: its thread, address space, CNode and pages.

The loader handles exactly what sap programs are built as: static position-independent RISC-V ELF with only relative relocations ([[kernel-root-task#Building it]]). Other formats and interpreters are separate loaders, per [[prog-loading#Code objects and formats]].

## The second program

`user/peer` is a small program the root task loads with `sap_loader::load` to check IPC between address spaces. `user/init/build.rs` builds it and embeds it, as the kernel embeds `init`, and rebuilds it whenever the peer, `libsap` or the ABI crate changes. `check_process` in `init` (`user/init/main.rs:824`) gives the peer a badged endpoint capability and its own thread capability, sets `init` as its fault handler, and starts it. Then it checks:
- the peer's six-word `CALL` arrives with the badge;
- the reply's words reach the peer, which reports their sum back;
- the peer's read of an address mapped only in `init`'s address space faults, and the fault comes back to `init` as a `VM` fault at that address.

Revoking the untyped memory then removes the peer entirely.

> [!todo] Not built
> A typed bootstrap message; an untyped allocator; thread-local storage; process exit and supervision.
