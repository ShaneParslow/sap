---
status: implemented
summary: The process factory init serves - SPAWN a program from the boot pack by its BLAKE3 hash with a bootstrap message, imports and untyped memory the client provides; WAIT for it to end and get the memory back; KILL it - with badged capabilities on one endpoint telling clients, processes and handles apart, and every process destroyed by revoking the client's memory.
updated: 2026-10-20
code: [abi/lib.rs, user/init/factory.rs, user/init/main.rs, user/check/main.rs, user/peer/main.rs, interfaces/peer.capnp, interfaces/check.capnp]
sources: ["[[2026-10-20-685b0203#m12|§m12]]"]
---

# The process factory

The process factory is the interface through which a program asks the process manager to start another program, as [[kernel-process-model#Spawning]] describes: no fork, no exec, code named by its hash. `init` serves it (`user/init/factory.rs`), starting programs from its boot pack ([[prog-runtime#The second program]]). It is plain IPC words for now, with the labels and layout in `sap_abi::factory` (`abi/lib.rs:434`); a `TODO` makes it a Cap'n Proto interface once RPC exists (M6).

## The calls

There are three calls, on two kinds of capability. A **factory capability** takes `SPAWN`; the **process handle** it returns takes `WAIT` and `KILL`.

| Call | Request | Reply |
|---|---|---|
| `SPAWN` | words: the program's BLAKE3 hash (4 words, little-endian), the bootstrap message's length in bytes, the message packed into words; capabilities: untyped memory, moved, then the imports | `OK` with the handle; or an error with the memory returned |
| `WAIT` | nothing | how the process ended (`EXITED`, `FAULTED`, `KILLED`) and its code, the exit code or the fault label; carries the memory back |
| `KILL` | nothing | `OK`; a waiting client hears `KILLED` |

`SPAWN` refuses with `BAD_REQUEST` when there is no memory capability, the first capability isn't untyped memory, or the length word claims more bytes than the request carries; with `NO_SUCH_PROGRAM` when the boot pack has no program of that hash; with `FULL` when 16 processes are running (`MAX_PROCESSES`, `user/init/factory.rs:30`); and with `LOAD_FAILED` when the loader returns an error, usually for lack of memory ([[prog-runtime#The loader]]). Every refusal after the memory arrives gives it back; imports are dropped. The bootstrap message is at most 912 bytes, what fits in one message beside the hash.

## Who pays

**The client provides the memory every object of the new process is made from**, as in seL4, where whoever creates an object supplies its untyped memory. The factory moves it into a slot of its own and builds the process from it with `sap-loader`: address space, CNode, thread, pages and page tables. This has three consequences:
- a client can't exhaust `init`'s memory, only its own;
- destroying the process is one revoke of that memory, which deletes every object made from it wherever its capabilities are, including the process's CNode and the imports in it;
- the memory goes back to the client with the answer to `WAIT`, revoked and usable again.

`init` spends its own memory only on bookkeeping: badged capabilities, and reply objects for waiting clients.

## How it works

Everything arrives on `init`'s one endpoint, told apart by badge (`user/init/factory.rs:24`): `FACTORY_BADGE` on factory capabilities, `LAUNCHER_BADGE` plus the process's index on the capability each process holds as its launcher and fault handler ([[prog-runtime#The process ABI]]), and `HANDLE_BADGE` plus the index on its handle. `init`'s loop (`user/init/main.rs:80`) passes every message that isn't `check`'s to `Factory::handle` (`user/init/factory.rs:111`), which returns what to do: reply, answer later, or nothing.

**Spawning** (`spawn`, line 143) checks the request, moves the memory out of the receive window, mints the process's launcher and handle capabilities, loads the image with the imports moved from the window into the new CNode, makes the launcher capability the thread's fault handler and starts it. Each process gets 128 of `init`'s slots for the loader's capabilities (`OBJECT_SLOTS`, line 32). On a load error, revoking the memory removes whatever was built.

**Ending** (`end`, line 210) happens when the process reports `launcher::EXIT`, when it faults (the fault message arrives on the same badged capability), or on `KILL`. It revokes the memory, which destroys the process, revokes and deletes the launcher capability, frees the object slots, and records how it ended. The handle and the memory stay until a client waits.

**Waiting** (`wait`, line 229) answers at once if the process has ended. Otherwise `init` keeps the reply object the call arrived on and receives with a fresh one (`fresh_reply`, line 94); seL4's MCS kernel works the same way, since a reply object is what links a caller to its answer. When the process ends, `answer_waiter` (line 246) replies through the kept object with the memory, then revokes `init`'s original of the handle, which deletes every copy, so the handle stops working. A second `WAIT` on the same process is refused.

## What is checked

`check_factory` (`user/check/main.rs:1603`) starts `peer` through the factory, using the peer's hash that `init` passes in `check`'s bootstrap message:
- a peer that exits with code 7: `WAIT` reports `EXITED` and 7 and returns untyped memory, after which the handle slot is empty, and the returned memory starts the peer again;
- a peer that reads an unmapped address: `FAULTED`, with the label `fault::VM`;
- a peer blocked calling an endpoint nobody answers: `KILL`, then `WAIT` reports `KILLED`;
- refusals: an unknown hash, 16 KiB of memory, an endpoint where memory belongs, no capabilities, and a length word longer than the request, each with the memory back where there was some.

## Limits

> [!todo] Not built
> - The priority of a started process is 255, the highest; the client's scheduling authority should decide it once there are scheduling contexts ([[kernel-scheduling]]).
> - Programs come only from the boot pack, not the store ([[prog-loading#Boot: before the store exists]]).
> - Capabilities in the bootstrap message are import indices, not capability fields (M6).
> - A process's CNode has 64 slots, its stack 16 KiB, and it has no untyped budget of its own; a client can't ask for more.
> - Nobody watches a client that dies holding handles: its processes keep running until killed through another copy of the handle.
