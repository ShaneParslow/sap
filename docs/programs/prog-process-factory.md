---
status: implemented
summary: The process factory init serves, specified as Cap'n Proto interfaces (interfaces/process.capnp) and carried over plain IPC until RPC - spawn a program from the boot pack by its BLAKE3 hash with a bootstrap message, imports and untyped memory the client provides; wait for it to end and get the memory back; kill it - with badged capabilities on one endpoint telling clients, processes and handles apart, and every process destroyed by revoking the client's memory.
updated: 2026-10-24
code: [interfaces/process.capnp, user/lib/interfaces/lib.rs, user/lib/interfaces/words.rs, user/lib/interfaces/build.rs, user/programs/init/factory.rs, user/programs/init/main.rs, user/programs/check/processes.rs, user/programs/peer/main.rs, interfaces/peer.capnp, interfaces/check.capnp]
sources: ["[[2026-10-20-685b0203#m12|§m12]]", "[[2026-10-22-e547b584-debrief]]"]
---

# The process factory

The process factory is the interface through which a program asks the process manager to start another program, as [[kernel-process-model#Spawning]] describes: no fork, no exec, code named by its hash. `init` serves it (`user/programs/init/factory.rs`), starting programs from its boot pack ([[prog-runtime#The second program]]).

## The schema

The interface is a Cap'n Proto schema, `interfaces/process.capnp`, without ownership annotations yet (the human's choice, [[2026-10-22-e547b584-debrief]]). It has two interfaces. A **factory capability** is a `ProcessFactory`, with one method, `spawn` (`interfaces/process.capnp:29`); the **process handle** it returns is a `Process`, with `wait` and `kill` (`interfaces/process.capnp:44`, `interfaces/process.capnp:48`):

| Method | Params | Results |
|---|---|---|
| `spawn` | `program`, the BLAKE3 hash of the image (32 bytes); `bootstrap`, the program's bootstrap message; `memory`, untyped memory, moved; `imports`, moved in order | `handle`, a `Process` |
| `wait` | none | `end`, a union of `exited` (the exit code), `faulted` (the fault label) and `killed`; `memory`, given back |
| `kill` | none | none; a waiting client hears `killed` |

Every reply's label is a `Status` (`interfaces/process.capnp:64`). `spawn` is refused with `badRequest` when there is no memory capability, the first capability isn't untyped memory, or the request isn't a well-formed params struct with a 32-byte hash; with `noSuchProgram` when the boot pack has no program of that hash; with `full` when 16 processes are running (`MAX_PROCESSES`, `user/programs/init/factory.rs:40`); and with `loadFailed` when the loader returns an error, usually for lack of memory ([[prog-runtime#The loader]]). Every refusal after the memory arrives gives it back; imports are dropped. The comments in the schema say all of this, since the schema language has no place for failure modes or capability order yet.

## On the wire, until RPC

Cap'n Proto's own RPC doesn't run over sap's IPC yet (M6), so a call is one IPC message, as the schema's header comment specifies:
- the label is the method's ordinal. `sap_interfaces::methods` has a constant per method (`process_factory::SPAWN`, `process::WAIT`, `process::KILL`), generated from the schema compiler's output by the build script (`write_methods`, `user/lib/interfaces/build.rs:32`), since capnpc keeps ordinals inside its RPC code;
- the words are the params or results struct as a single-segment message without a segment table, packed by `sap_interfaces::words` (`pack`, `user/lib/interfaces/words.rs:22`);
- capability fields stay null; the capabilities travel beside the message by kernel transfer, in the order of the fields that name them (memory, then imports; the handle; the memory back).

The words of a message limit a `spawn` request to 120 words, which leaves 888 bytes for the bootstrap message after the struct, its pointers and the hash.

Writing the schema turned up four things about capnpc-rust ([[iface-capnp#Coverage]]): code for an `interface` needs the `capnp/alloc` feature, so `sap-interfaces` compiles `process_capnp` only with its `alloc` feature and `peer`, which has no heap, goes without; that code assumes `std`'s prelude, so the build script qualifies `Box` and `to_string` (`qualify_alloc`, `user/lib/interfaces/build.rs:73`); `List(Capability)` is unsupported, so imports are a list of `Import` structs (`interfaces/process.capnp:35`); and with `alloc` on, a message builder keeps its segment list in a `Vec`, so building any message allocates.

## Who pays

**The client provides the memory every object of the new process is made from**, as in seL4, where whoever creates an object supplies its untyped memory. The factory moves it into a slot of its own and builds the process from it with `sap-loader`: address space, CNode, thread, pages and page tables. This has three consequences:
- a client can't exhaust `init`'s memory, only its own;
- destroying the process is one revoke of that memory, which deletes every object made from it wherever its capabilities are, including the process's CNode and the imports in it;
- the memory goes back to the client with the answer to `wait`, revoked and usable again.

`init` spends its own memory only on bookkeeping: badged capabilities, and reply objects for waiting clients.

## How it works

Everything arrives on `init`'s one endpoint, told apart by badge (`user/programs/init/factory.rs:34`): `FACTORY_BADGE` on factory capabilities, `LAUNCHER_BADGE` plus the process's index on the capability each process holds as its launcher and fault handler ([[prog-runtime#The process ABI]]), and `HANDLE_BADGE` plus the index on its handle. `init`'s loop (`user/programs/init/main.rs:81`) passes every message that isn't `check`'s to `Factory::handle` (`user/programs/init/factory.rs:136`), which dispatches on badge and method ordinal and returns what to do: reply, answer later, or nothing.

**Spawning** (`spawn`, line 168) checks the request, reading the params struct with `spawn_request` (line 288), moves the memory out of the receive window, mints the process's launcher and handle capabilities, loads the image with the imports moved from the window into the new CNode, makes the launcher capability the thread's fault handler and starts it. Each process gets 128 of `init`'s slots for the loader's capabilities (`OBJECT_SLOTS`, line 42). On a load error, revoking the memory removes whatever was built.

**Ending** (`end`, line 231) happens when the process reports `launcher::EXIT`, when it faults (the fault message arrives on the same badged capability), or on `kill`. It revokes the memory, which destroys the process, revokes and deletes the launcher capability, frees the object slots, and records how it ended. The handle and the memory stay until a client waits.

**Waiting** (`wait`, line 250) answers at once if the process has ended. Otherwise `init` keeps the reply object the call arrived on and receives with a fresh one (`fresh_reply`, line 117); seL4's MCS kernel works the same way, since a reply object is what links a caller to its answer. When the process ends, `answer_waiter` (line 267) replies through the kept object with the results struct (`wait_reply`, line 298) and the memory, then revokes `init`'s original of the handle, which deletes every copy, so the handle stops working. A second `wait` on the same process is refused.

## What is checked

`check_factory` (`user/programs/check/processes.rs:205`) starts `peer` through the factory as a client of the schema (`factory_spawn`, `user/programs/check/processes.rs:147`; `factory_wait`, `user/programs/check/processes.rs:183`), using the peer's hash that `init` passes in `check`'s bootstrap message:
- a peer that exits with code 7: `wait` reports `exited` with 7 and returns untyped memory, after which the handle slot is empty, and the returned memory starts the peer again;
- a peer that reads an unmapped address: `faulted`, with the label `fault::VM`;
- a peer blocked calling an endpoint nobody answers: `kill`, then `wait` reports `killed`;
- refusals: an unknown hash, 16 KiB of memory, an endpoint where memory belongs, no capabilities, words that aren't a message (a root pointer past their end), and a 31-byte hash, each with the memory back where there was some.

## Limits

> [!todo] Not built
> - The priority of a started process is 255, the highest; the client's scheduling authority should decide it once there are scheduling contexts ([[kernel-scheduling]]).
> - Programs come only from the boot pack, not the store ([[prog-loading#Boot: before the store exists]]).
> - Capabilities in the bootstrap message are import indices, and the schema's capability fields are left null, until M6.
> - The schema has no ownership annotations: that `spawn` moves the memory and `wait` uses up the handle is in its comments only ([[cap-ownership]]).
> - A process's CNode has 64 slots, its stack 16 KiB, and it has no untyped budget of its own; a client can't ask for more.
> - Nobody watches a client that dies holding handles: its processes keep running until killed through another copy of the handle.
