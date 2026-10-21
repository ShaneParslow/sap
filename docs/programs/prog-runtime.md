---
status: implemented
summary: The user-side crates every sap program links today - sap-sys (stateless system-call and invocation wrappers), sap-rt (the thread start convention and the process ABI's startup record and bootstrap message, entry point, debug console, panic handler, and a runtime with a slot allocator, notification-backed mutexes and an optional heap for alloc), sap-ring, and sap-loader, the ELF loader the root task uses to start other programs in their own processes.
updated: 2026-10-20
code: [abi/lib.rs, user/rt/process.rs, interfaces/peer.capnp, user/sys/arch/riscv.rs, user/rt/arch/riscv.rs, user/loader/arch/riscv.rs, user/sys/address.rs, clippy.toml, user/rt/lib.rs, user/rt/runtime.rs, user/rt/Cargo.toml, user/sys/lib.rs, user/rt/slots.rs, user/rt/sync.rs, user/rt/heap.rs, user/sys/vspace.rs, user/sys/sys.rs, user/sys/ipc.rs, user/loader/lib.rs, user/ring/lib.rs, user/rt/debug.rs, user/peer/main.rs, user/init/main.rs, user/init/build.rs, kernel/root_task.rs]
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# The user runtime and its crates

sap's user programs build on four `no_std` crates under `user/`, which take every number and layout from the shared `sap-abi` crate, so they can't drift from the kernel ([[kernel-threads#Syscalls]]). `sap-rt` is the runtime, the equivalent of a C runtime; it will grow into the platform layer for Rust's `std` ([[prog-rust-std]]).

## Thread start convention

Every thread starts with:
- `sp` at the top of its stack;
- `tp` pointing at its IPC buffer;
- up to three arguments in `a0`–`a2`.

The kernel follows it for the root task, whose `a0` is the address of the boot-info page (`kernel/root_task.rs:223`), and every thread `sap-rt` starts follows it too. `tp` is free because there is no thread-local storage yet, so it serves as the per-thread pointer to the IPC buffer (`ipc_buffer`, `user/sys/sys.rs:13`); a `TODO` moves it to a real thread-local once the Rust target has TLS.

`_start` calls the program's `sap_main(a0, a1, a2)` (`user/rt/lib.rs`).

## The process ABI

A process started by a launcher, rather than by the kernel, gets the address of its **startup record** in `a0`: a read-only page holding a `Startup` (`abi/lib.rs:430`) and, `BOOTSTRAP_OFFSET` (128) bytes in, the bootstrap message (`abi/lib.rs:450`). The record gives the time counter's frequency, the IPC buffer and stack addresses, an address range for the heap, the CNode's size, and two slot ranges: the imports and the free slots. Its CNode has fixed slots for its runtime capabilities (`process_slot`, `abi/lib.rs:402`): its thread (1), its CNode, guarded so slot numbers are capability addresses (2), its address space (3) and its untyped memory budget (4, empty if it has none). Its imports start at slot 8.

The **bootstrap message** is a Cap'n Proto message in the flat single-segment encoding whose root struct is the program's imports, as [[kernel-process-model]] describes. A capability in it is, for now, the index of an import in an integer field (`TODO` until RPC capability tables, M6). It fits in the rest of the page, at most 3,968 bytes. `sap_rt::process` reads it: `startup(a0)` gives the record (`user/rt/process.rs:13`), `bootstrap` the message bytes (line 21) and `import` the slot of an import index, checked against the range (line 30). A program reads the message in place with ordinary references, which is sound because nobody writes the page once the process runs ([[ipc-validation#Double fetch]]).

## Crates

| Crate | Holds | Relies on |
|---|---|---|
| `sap-sys` (`user/sys/`) | stateless wrappers over the kernel interface, as libsel4 is to seL4: `address` (`mapped`, `user/sys/address.rs:18`, the only way user code turns an address into a pointer; the user crates deny integer-to-pointer casts and `clippy.toml` disallows the exposed-provenance functions elsewhere, [[hw-cheri#Keeping the door open]]), `sys` (the raw `CALL` for kernel invocations, `YIELD`, the IPC buffer), `time` (the `time` counter, `user/sys/sys.rs:35`, read without a system call), `cnode` (including `identify`, which describes a capability without invoking it), `untyped`, `vspace` and `tcb` invocations ([[kernel-capabilities]], [[kernel-threads]]), `notification` (`SIGNAL`, `WAIT`, `POLL`) and `ipc` (`SEND`, `NB_SEND`, `CALL`, `RECV`, `NB_RECV`, `REPLY`, `REPLY_RECV`, with capabilities to give, moved or copied, and the receive window, [[kernel-ipc]]) | nothing but `tp` pointing at the IPC buffer |
| `sap-rt` (`user/rt/`) | the entry point, the panic handler (prints and halts with failure), the `debug` console (`print!`, `println!`), and the runtime: `runtime`, `slots`, `sync` and, behind the `heap` feature (on by default), `heap`; it re-exports `sap-sys`'s modules | resources given to `runtime::start` |
| `sap-ring` (`user/ring/`) | shared-memory rings of variable-length records between processes ([[ipc-rings]]), and `transfer`, which moves messages too large for a ring as frames ([[ipc-page-transfer]]) | `sap-sys` |
| `sap-loader` (`user/loader/`) | the ELF loader, below | `sap-sys`, the `elf` crate |
| `sap-interfaces` (`user/interfaces/`) | Rust bindings for the Cap'n Proto schemas in `interfaces/`, generated at build time ([[iface-capnp]]), and the record kinds of the echo channel | the `capnp` crate; the `capnp` tool to build |

Each crate keeps its architecture-specific code in `arch/riscv.rs`, as the kernel does ([[system-portability]]): in `sap-sys` the system-call instruction, with arguments and results in `a0`–`a7`, and the thread pointer (`user/sys/arch/riscv.rs`), so every wrapper above it is architecture-neutral; in `sap-rt` the `_start` entry; in `sap-loader` the ELF machine and relocation type. Porting the user side means adding those three files and a target specification.

A program that doesn't allocate turns the `heap` feature off, as `user/peer` does, and then carries no allocator; using `alloc` without it fails to link. The split (roadmap I4) keeps state, allocation and policy out of the system-call layer, so anything can build on `sap-sys` alone.

## Slots, locks and the heap

A program hands the runtime what it may use on the program's behalf, once, before it allocates: `runtime::start` with `Resources` (`user/rt/runtime.rs:14`), naming its root CNode, a range of free slots and an untyped region, and with the heap, its address space and an unused address range for the heap to grow into. The runtime retypes two notifications from the untyped region for its locks, then starts the slot allocator and the heap (`start`, line 37). Revoking the untyped region destroys everything the runtime made from it. Only the root task calls it today; a program started by another will get its resources in the typed bootstrap message, and its entry code will start the runtime before `sap_main` (M7).

**Slots** (`user/rt/slots.rs`). A bitmap over the given range of the root CNode, up to 4,096 slots (the root task's whole CNode), handing out single slots or contiguous runs (`alloc_range`, line 45), since retype fills consecutive slots. Slot numbers are capability addresses because every root CNode so far is one guarded level. Freeing a slot is the caller's promise that it is empty. Growing past one CNode with second-level CNodes is a `TODO` for the process manager (M7).

**Locks** (`user/rt/sync.rs`). A `Mutex` in the style of a futex-based one (Drepper's "mutex2"): a word that is unlocked, locked, or locked with possible waiters. An uncontended lock and unlock is one atomic operation each. A thread that finds the mutex taken marks it contended and sleeps on the mutex's notification; an unlock that finds it contended signals (`lock`, line 53). Sleeping rather than spinning matters with priorities: a spinning high-priority thread would never let a lower-priority holder run. Each mutex has its own notification, because with one shared between two mutexes a waiter on one could take the signal meant for a waiter on the other, which would then sleep with its mutex free. The notifications are unbadged, so this relies on unbadged signals being kept ([[kernel-ipc#Notifications]]). Before a mutex has a notification, a contended lock yields in a loop.

**The heap** (`user/rt/heap.rs`). The global allocator behind `alloc` (`Box`, `Vec`, `String`), so `alloc` works in every sap program that has the `heap` feature and starts the runtime. It is a first-fit free list (the `linked_list_allocator` crate, without its spinlock) behind a `Mutex`, over the heap's address range. Nothing is mapped until the first allocation. When the list can't satisfy one, the heap grows at the top by at least 64 KiB (`GROW_MIN`, line 31): it takes a run of slots, retypes that many 4 KiB frames from the untyped region (at most 256 per retype), and maps each, adding a page table only when a mapping fails for lack of one (`vspace::map_frame_with_tables`, `user/sys/vspace.rs:26`) (`grow`, `user/rt/heap.rs:97`). Allocation fails, and `alloc` panics, before `runtime::start` or once the range or the memory runs out.

> [!todo] Not built
> The heap never shrinks: freed memory returns to the free list but stays mapped. The loader still uses its own bump allocator for slots (`Slots`) and adds page tables by retyping one and deleting it if every level exists, which spends untyped memory that only a revoke recovers; both should move to the runtime's allocator and `map_frame_with_tables`.

`init`'s `check_heap` (`user/init/main.rs:1360`) checks that nothing is mapped before the first allocation, growth past 2 MiB (across level-0 page tables), a page-aligned allocation, reuse of freed memory without growth, formatting into a `String`, two threads allocating 40,000 vectors each at once, and a thread that finds a mutex held sleeping until `init` unlocks it. The heap gets the largest untyped region other than the first (`runtime_untyped`, line 45), and the other checks pick theirs around it, since several revoke their region when done.

## The loader

`sap_loader::load` (`user/loader/lib.rs:158`) starts a program in a new process, given an ELF image, untyped memory to build it from, free slots in the caller's CNode, and an unused 1 GiB region of the caller's address space for scratch mappings (`Parent`, line 44), and a `Launch` (line 61): a priority, the size of an untyped budget if any, the caller's capabilities to give as imports with the rights for each, and the bootstrap message. It is the first piece of the process manager (M7, [[prog-loading]]), doing in userspace what the kernel does once for the root task ([[kernel-root-task]]):

1. Retypes a page table and makes it the root of a new address space with an ASID; retypes a CNode (64 slots, `CNODE_RADIX`, line 41) and a TCB.
2. For each page of each loadable segment, retypes a frame, maps it at the scratch address in the caller's space, fills it with the file bytes that fall in that page plus the relative relocations whose targets do, unmaps it, and maps it into the new address space with the segment's permissions (writable and executable together is refused). Page tables are added as needed (`ensure_tables`, line 130), installing whatever levels are missing.
3. Maps a 16 KiB stack and an IPC buffer at the same addresses the root task uses, and the startup page, filled with the record and the bootstrap message, read-only below the IPC buffer (`STARTUP`, line 37). The heap range it gives is 1 GiB at `0x10_0000_0000` (line 39).
4. Configures the thread with a guarded copy of the new CNode (so full-width addresses name its slots directly), the new address space and the IPC buffer; sets its priority and start registers, `a0` at the startup record; leaves it stopped.
5. Fills the process ABI's slots: copies of the thread, the guarded CNode and the address space; the budget, retyped as a new untyped object from the caller's and moved in, since copying untyped memory would mark the caller's full ([[kernel-capabilities#Retyping untyped memory]]); and copies of the imports.

The caller can still copy more capabilities into the process's slots (`Process::give`), adds page tables where it will map more frames later (`Process::map_tables`, `user/loader/lib.rs:105`), and starts it. Everything was retyped from the untyped memory the caller passed, so revoking that untyped memory destroys the whole process: its thread, address space, CNode and pages.

The loader handles exactly what sap programs are built as: static position-independent RISC-V ELF with only relative relocations ([[kernel-root-task#Building it]]). Other formats and interpreters are separate loaders, per [[prog-loading#Code objects and formats]].

## The second program

`user/peer` is a small program the root task loads with `sap_loader::load` to check IPC between address spaces. Its bootstrap message, `PeerImports` in `interfaces/peer.capnp`, picks what it does: the endpoint checks below, or, over a channel of two rings, echoing word records, serving the echo interface, or breaking the ring's rules on purpose ([[ipc-rings#What runs show]]); and it names the endpoint, notifications and reply object it uses by import index. `init` builds the message without the heap and copies it out of Cap'n Proto's single-segment allocator before the builder goes, since that allocator zeroes its buffer then (`peer_bootstrap`, `user/init/main.rs:872`). The peer builds without the heap. `user/init/build.rs` builds it and embeds it, as the kernel embeds `init`, and rebuilds it whenever the peer, `sap-rt` or the ABI crate changes. `check_process` in `init` (`user/init/main.rs:894`) starts the peer with a badged endpoint as its one import and a 64 KiB budget, checks the type of every process ABI slot through the peer's CNode, sets `init` as its fault handler, and starts it. Then it checks:
- the peer's six-word `CALL` arrives with the badge;
- the reply's words reach the peer, which reports their sum back;
- the peer's read of an address mapped only in `init`'s address space faults, and the fault comes back to `init` as a `VM` fault at that address.

Revoking the untyped memory then removes the peer entirely.

> [!todo] Not built
> An untyped allocator; thread-local storage; process exit and supervision; capability fields in the bootstrap message.
