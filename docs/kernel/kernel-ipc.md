---
status: implemented
summary: Kernel IPC as implemented today - notifications (Signal, Wait, Poll), badges and their place in the derivation tree, endpoints with seL4 MCS-style reply objects (Send, Recv, Call, Reply, ReplyRecv and non-blocking variants), notifications bound to threads, blocking, cancellation and restarted system calls. Capability transfer and fault delivery follow in M4.
updated: 2026-10-13
code: [kernel/notification.rs, kernel/endpoint.rs, kernel/syscall.rs, kernel/invocation.rs, kernel/thread.rs, kernel/sched.rs, kernel/cap.rs, kernel/cspace.rs, kernel/untyped.rs, kernel/arch/riscv/trap.rs, abi/lib.rs, user/libsap/notification.rs, user/libsap/ipc.rs, user/libsap/sys.rs, user/init/main.rs]
sources: ["[[2026-10-08-22c3947b#m33|§m33]]", "[[2026-10-08-22c3947b#m34|§m34]]"]
---

# Kernel IPC

Roadmap milestone M4 ([[roadmap]]) adds the kernel's two IPC objects, following seL4 with the divergences decided for sap ([[decisions]]): notifications, which carry no data, and endpoints, which carry messages and capabilities between threads. The design is [[kernel-objects-and-syscalls]] and [[ipc-cap-transfer]]; this page describes what is built: notifications and endpoints, so far without capabilities in messages.

## Notifications

A `Notification` (`kernel/notification.rs:18`) is a word of pending badge bits and a queue of waiting threads, in 32 bytes (`NOTIFICATION_BITS`, line 15), retyped from untyped memory like any object. Three syscalls act on a notification capability (`abi/lib.rs:55`, handled in `notification_call`, `kernel/syscall.rs:47`):

| Syscall | Needs right | Effect |
|---|---|---|
| `SIGNAL` | write | OR the capability's badge into the word; if a thread is waiting, hand the badge straight to the first one and wake it instead (`signal`, `kernel/notification.rs:35`) |
| `WAIT` | read | if the word is nonzero, return it and clear it; otherwise block until a signal arrives (`wait`, `kernel/notification.rs:62`) |
| `POLL` | read | return and clear the word without blocking (`kernel/notification.rs:86`) |

Results come back in `a0` (the bits) and `a1` (an error code). Separate syscalls rather than seL4's reuse of `Send` and `Recv` on notifications was the human's choice ([[2026-10-08-22c3947b#m34|§m34]]).

The waiter queue is threaded through the waiting threads' TCBs (`ipc_next`/`ipc_prev`), so a notification costs the same however many threads wait on it ([[kernel-objects-and-syscalls#Kernel memory]]).

## Badges

A badge is a word stamped on a capability when it is minted (`CNODE_MINT` with the badge as data). Each signaller gets its own badge, so a waiter learns who signalled from the bits it receives. As in seL4:
- an unbadged capability can be badged once; re-badging is `ILLEGAL_OPERATION`;
- a copy with a *new* badge is revocable and marked "first badged" in its slot (`first_badged`, `kernel/cspace.rs:40`);
- copies of that capability with the *same* badge become its children, while copies badged afresh don't (`is_parent_of`, line 233). Revoking a badged capability therefore cuts off exactly the holders of that badge, and revoking the unbadged original cuts off every badge.

## Endpoints

An `Endpoint` (`kernel/endpoint.rs:32`, 16 bytes) is a rendezvous point with one queue of blocked threads, threaded through their TCBs. It holds blocked senders or blocked receivers, never both: a sender that finds a receiver waiting (or the reverse) hands the message over at once. Endpoint capabilities carry a badge, like notification capabilities, and rights: write to send or call, read to receive, grant (unused until capabilities travel in messages).

| Syscall (`abi/lib.rs:35`) | Effect |
|---|---|
| `SEND` | deliver to a waiting receiver, or block until one arrives (`send`, `kernel/endpoint.rs:127`) |
| `NB_SEND` | deliver if a receiver waits; otherwise drop the message, as in seL4 |
| `CALL` | send, then wait for the reply; on a kernel object it is an invocation instead ([[kernel-capabilities#Invocation]]) |
| `RECV` | take a waiting sender's message, or block until one arrives (`recv`, line 159) |
| `NB_RECV` | take a message if one waits; otherwise return with nothing |
| `REPLY` | answer the caller waiting on a reply object, if any (`reply`, line 234) |
| `REPLY_RECV` | `REPLY`, then `RECV` with the same reply object: a server loop in one trap |

**Registers.** Arguments are as for invocations: `a0` the endpoint, `a1` the message info, `a2`–`a5` message words 0–3, and the rest in the IPC buffer; `a6` names a reply object for the calls that take one. A received message arrives as `a0` the sender's badge, `a1` the message info (the sender's label and the length delivered), `a2`–`a5` the first words, the rest in the receiver's IPC buffer, and `a6` what woke the receiver (`recv_source`, `abi/lib.rs:94`). Because `a1` carries the sender's own label, IPC errors come back separately, in `a7`. Dispatch is `ipc` in `kernel/syscall.rs:94`.

**The message** moves in `transfer` (`kernel/endpoint.rs:70`): registers to registers, words 4 and up from the sender's IPC buffer to the receiver's, copied with `ptr::copy` in case userspace made the two buffers the same memory. A message longer than the registers is cut to them if either thread lacks an IPC buffer. The receiver's message info reports the length actually delivered.

## Reply objects

Replies follow seL4's MCS configuration, as decided ([[2026-10-08-22c3947b#m34|§m34]]). A receiver lends a `Reply` object (`kernel/endpoint.rs:41`, 16 bytes, retyped like any object) to `RECV`. When the message it receives is a `CALL`, the caller is linked to the reply object and blocks in `BlockedOnReply` until someone replies through it (`after_delivery`, line 103). The reply right lives in the object rather than in the server's thread, so a thread can hold several pending calls in several reply objects, and time donation can later follow the reply back to the caller ([[kernel-scheduling]]).

The edge cases:
- **No reply object.** A `CALL` received by a thread that lent none is delivered, and the caller continues at once with `NO_REPLY` (`abi/lib.rs:90`).
- **A reply object already holding a caller.** Lending it to `RECV` again is refused (`ILLEGAL_OPERATION`), since that would strand the caller. seL4 instead unlinks the old caller.
- **Deleting a reply object.** A caller waiting on it wakes with `NO_REPLY`; a receiver it is lent to loses it (`finalise_reply`, line 301).
- **`REPLY` with nobody waiting** does nothing, which lets a server's first `REPLY_RECV`, or one after a plain `SEND`, simply receive.
- **Replies carry no badge:** `a0` is 0.

## Bound notifications

`TCB_BIND_NOTIFICATION` (`kernel/invocation.rs:267`) binds a notification to a thread, one each way. A signal on it while the thread waits in `RECV` on any endpoint takes the thread out of the endpoint's queue and wakes it with the bits in `a0` and `a6` = `NOTIFICATION` (`wake_bound`, `kernel/endpoint.rs:219`). Bits pending when the thread enters `RECV` are delivered first. This lets one server thread wait for both calls and doorbells, which shared-memory rings need ([[ipc-channels]]).

## Blocking and cancellation

A blocked thread is in one of four states (`kernel/thread.rs:39`): blocked on a notification, on sending or receiving through an endpoint, or on a reply. It is off the ready queue (`sched::block`, `kernel/sched.rs:64`) and, except when waiting for a reply, on the object's queue, with the object recorded in the TCB. Whatever ends the wait other than its normal completion has to undo that (`cancel_blocking`, `kernel/thread.rs:141`): leave the notification's or endpoint's queue, return a lent reply object, or unlink from the reply object a caller waits on (`cancel` and `cancel_reply`, `kernel/endpoint.rs:252`, 269):
- **Suspending** a blocked thread cancels the wait and **restarts the system call**: the program counter is moved back over the `ecall` (`UserContext::restart_syscall`, `kernel/arch/riscv/trap.rs:93`), so resuming the thread repeats its `WAIT`, as seL4 does (`thread::suspend`, `kernel/thread.rs:160`). Nothing is lost: signals sent meanwhile accumulate in the word, and the repeated `WAIT` takes them.
- **Deleting the last capability to the notification or endpoint** restarts every waiter's call (`finalise`, `kernel/notification.rs:109`; `finalise_endpoint`, `kernel/endpoint.rs:283`). The repeated call then fails cleanly with `FAILED_LOOKUP` instead of blocking forever on an object that is gone. A caller suspended while waiting for a reply has its `CALL` restarted too, so resuming it sends the call again, as in seL4.
- **Deleting the thread** cancels its wait and unbinds any notification bound to it (`thread::finalise`, line 189).

`RESUME` (and `WRITE_REGISTERS` with the resume flag) only acts on an inactive thread (`thread::resume`, line 175), so it can't make a blocked thread runnable while it is still on a queue.



## Per-thread IPC buffers in userspace

Several threads of one program each need their own IPC buffer. `libsap` keeps each thread's buffer address in the thread-pointer register `tp`, which nothing else uses yet since there is no thread-local storage (`set_ipc_buffer`, `user/libsap/sys.rs:12`); whoever starts a thread sets `tp` with its other registers. A `TODO` there says to move to a real thread-local once the sap Rust target has TLS ([[prog-rust-toolchain]]).

## Tests

`user/init` (`check_notifications`) checks: minting two badges and refusing a re-badge; two signals with nobody waiting accumulating, so one poll returns both bits and the next returns none; a send-only capability being refused a `WAIT`; a second thread blocked in `WAIT` waking once per signal with the right badge; a signal sent while that thread is suspended being taken by its restarted `WAIT` after `RESUME`; revoking one badge deleting its copies while the badge itself keeps working; and deleting the notification with the thread waiting, after which its restarted `WAIT` reports `FAILED_LOOKUP`.

`check_ipc` uses a `spawn` helper (`user/init/main.rs:420`) to start threads with their own stack and IPC buffer, and checks:
- a server thread answering with `REPLY_RECV`: a six-word `CALL` through a badged capability comes back with the badge as the label and every word incremented, so two words crossed the IPC buffers each way, and the reply carries no badge;
- a plain `SEND` completing without an answer, counted by the server;
- `RECV` on a send-only capability and `CALL` on a receive-only one refused;
- `NB_RECV` finding nothing on an idle endpoint, and `NB_SEND` with nobody waiting dropping the message;
- binding a notification to the server, binding twice refused, and a signal waking the server out of `RECV`;
- a `CALL` to a receiver that lent no reply object being delivered and ending with `NO_REPLY`;
- `init` receiving a call itself and deleting its reply object, after which the caller wakes with `NO_REPLY`;
- deleting the endpoint under the waiting server, whose restarted `RECV` then reports `FAILED_LOOKUP`.

> [!todo] Not built
> Capability transfer in messages (up to 16, move or copy), fault delivery, the two-process ring demo, the IPC fast path, and scheduling contexts donated on `CALL`.
