---
status: implemented
summary: Kernel IPC as implemented today - notifications (Signal, Wait, Poll), badges and their place in the derivation tree, endpoints with seL4 MCS-style reply objects (Send, Recv, Call, Reply, ReplyRecv and non-blocking variants), notifications bound to threads, capability transfer (up to 16 per message, moved or copied into a receive window), fault delivery to fault-handler endpoints, and blocking, cancellation and restarted system calls.
updated: 2026-10-19
code: [kernel/notification.rs, kernel/endpoint.rs, kernel/fault.rs, kernel/syscall.rs, kernel/invocation.rs, kernel/thread.rs, kernel/sched.rs, kernel/cap.rs, kernel/cspace.rs, kernel/untyped.rs, kernel/arch/riscv/trap.rs, abi/lib.rs, user/sys/notification.rs, user/sys/ipc.rs, user/sys/sys.rs, user/init/main.rs]
sources: ["[[2026-10-08-22c3947b#m33|§m33]]", "[[2026-10-08-22c3947b#m34|§m34]]"]
---

# Kernel IPC

Roadmap milestone M4 ([[roadmap]]) adds the kernel's two IPC objects, following seL4 with the divergences decided for sap ([[decisions]]): notifications, which carry no data, and endpoints, which carry messages and capabilities between threads. The design is [[kernel-objects-and-syscalls]] and [[ipc-cap-transfer]]; this page describes what is built: notifications, endpoints and capability transfer.

## Notifications

A `Notification` (`kernel/notification.rs:22`) is in one of three states, as in seL4 (`State`, line 34): **idle**; **waiting**, with a queue of threads blocked in `WAIT`; or **active**, holding the OR of the badges of signals nobody has taken yet. Waiters and pending signals never coexist, because a signal that finds a waiter goes straight to it. Active is a state of its own rather than "the bits are nonzero", so a signal through an unbadged capability (badge 0) is still pending. With the thread it may be bound to, a notification takes 32 bytes (`NOTIFICATION_BITS`, line 19), retyped from untyped memory like any object. Three syscalls act on a notification capability (`abi/lib.rs:56`, handled in `notification_call`, `kernel/syscall.rs:48`):

| Syscall | Needs right | Effect |
|---|---|---|
| `SIGNAL` | write | if a thread is waiting, hand it the capability's badge and wake it; otherwise OR the badge into the pending bits (`signal`, `kernel/notification.rs:49`) |
| `WAIT` | read | if a signal is pending, take the bits; otherwise block until one arrives (`wait`, `kernel/notification.rs:80`) |
| `POLL` | read | take the pending bits without blocking, with `a2` saying whether a signal was pending, since an unbadged signal leaves no bits (`poll`, `kernel/notification.rs:108`) |

Results come back in `a0` (the bits) and `a1` (an error code). Separate syscalls rather than seL4's reuse of `Send` and `Recv` on notifications was the human's choice ([[2026-10-08-22c3947b#m34|§m34]]).

The waiter queue is threaded through the waiting threads' TCBs (`ipc_next`/`ipc_prev`), so a notification costs the same however many threads wait on it ([[kernel-objects-and-syscalls#Kernel memory]]).

## Badges

A badge is a word stamped on a capability when it is minted (`CNODE_MINT` with the badge as data). Each signaller gets its own badge, so a waiter learns who signalled from the bits it receives. As in seL4:
- an unbadged capability can be badged once; re-badging is `ILLEGAL_OPERATION`;
- a copy with a *new* badge is revocable and marked "first badged" in its slot (`first_badged`, `kernel/cspace.rs:40`);
- copies of that capability with the *same* badge become its children, while copies badged afresh don't (`is_parent_of`, line 242). Revoking a badged capability therefore cuts off exactly the holders of that badge, and revoking the unbadged original cuts off every badge.

## Endpoints

An `Endpoint` (`kernel/endpoint.rs:35`, 16 bytes) is a rendezvous point with one queue of blocked threads, threaded through their TCBs. It holds blocked senders or blocked receivers, never both: a sender that finds a receiver waiting (or the reverse) hands the message over at once. Endpoint capabilities carry a badge, like notification capabilities, and rights: write to send or call, read to receive, grant to send capabilities, and grant-reply to let replies to one's calls carry capabilities.

| Syscall (`abi/lib.rs:35`) | Effect |
|---|---|
| `SEND` | deliver to a waiting receiver, or block until one arrives (`endpoint::send`, line 207) |
| `NB_SEND` | deliver if a receiver waits; otherwise drop the message, as in seL4 |
| `CALL` | send, then wait for the reply; on a kernel object it is an invocation instead ([[kernel-capabilities#Invocation]]) |
| `RECV` | take a waiting sender's message, or block until one arrives (`recv`, line 241) |
| `NB_RECV` | take a message if one waits; otherwise return with nothing |
| `REPLY` | answer the caller waiting on a reply object, if any (`reply`, line 315) |
| `REPLY_RECV` | `REPLY`, then `RECV` with the same reply object: a server loop in one trap |

**Registers.** Arguments are as for invocations: `a0` the endpoint, `a1` the message info, `a2`–`a5` message words 0–3, and the rest in the IPC buffer; `a6` names a reply object for the calls that take one. A received message arrives as `a0` the sender's badge, `a1` the message info (the sender's label and the length delivered), `a2`–`a5` the first words, the rest in the receiver's IPC buffer, and `a6` what woke the receiver (`recv_source`, `abi/lib.rs:97`). Because `a1` carries the sender's own label, IPC errors come back separately, in `a7`. Dispatch is `ipc` in `kernel/syscall.rs:99`.

**The message** moves in `transfer` (`endpoint::transfer`, line 77): registers to registers, words 4 and up from the sender's IPC buffer to the receiver's, copied with `ptr::copy` in case userspace made the two buffers the same memory. A message longer than the registers is cut to them if either thread lacks an IPC buffer. The receiver's message info reports the length actually delivered.

## Capability transfer

A message carries up to 16 capabilities (`MAX_EXTRA_CAPS`, `abi/lib.rs:284`), the decided divergence from seL4's one copied capability ([[ipc-cap-transfer]]). The sender lists their addresses in its IPC buffer, with a bit per capability saying whether it is **moved** (the sender's slot ends empty and the capability keeps its place in the derivation tree) or **copied** (a derived copy, by the same rules as `CNODE_COPY`) (`caps_move`, `abi/lib.rs:299`). The receiver names a **receive window** in its own IPC buffer: a CNode and a range of its slots. `transfer_caps` (`kernel/endpoint.rs:126`) places the capabilities in the window's slots in order and stops at the first one that doesn't resolve or can't be derived, or the first slot that is full or out of range. The receiver's message info reports how many arrived.

- **The sender needs the endpoint capability's grant right.** Without it the message is delivered and no capabilities travel, as in seL4.
- **Replies carry capabilities only if the caller's capability had grant-reply,** recorded in the reply object when the call is linked (`Reply`, line 44), as in seL4.
- **The receiver can't be flooded:** nothing lands outside its window.
- **The bound of 16** exists because each capability costs a lookup, a derive and an insert in a call that can't be preempted ([[ipc-cap-transfer#Details of the proposed transfer]]).
- The window is read when the message is delivered, so a receiver blocked in `RECV` gets the window it set before blocking.

seL4 "unwraps" a capability to the very endpoint a message travels on into a badge; sap doesn't.

## Reply objects

Replies follow seL4's MCS configuration, as decided ([[2026-10-08-22c3947b#m34|§m34]]). A receiver lends a `Reply` object (`kernel/endpoint.rs:44`, 16 bytes, retyped like any object) to `RECV`. When the message it receives is a `CALL`, the caller is linked to the reply object and blocks in `BlockedOnReply` until someone replies through it (`after_delivery`, line 163). The reply right lives in the object rather than in the server's thread, so a thread can hold several pending calls in several reply objects, and time donation can later follow the reply back to the caller ([[kernel-scheduling]]).

The edge cases:
- **No reply object.** A `CALL` received by a thread that lent none is delivered, and the caller continues at once with `NO_REPLY` (`abi/lib.rs:93`).
- **A reply object already holding a caller.** Lending it to `RECV` again is refused (`ILLEGAL_OPERATION`), since that would strand the caller. seL4 instead unlinks the old caller.
- **Deleting a reply object.** A caller waiting on it wakes with `NO_REPLY`; a receiver it is lent to loses it (`finalise_reply`, line 391).
- **`REPLY` with nobody waiting** does nothing, which lets a server's first `REPLY_RECV`, or one after a plain `SEND`, simply receive.
- **Replies carry no badge:** `a0` is 0.

## Bound notifications

`TCB_BIND_NOTIFICATION` (`kernel/invocation.rs:267`) binds a notification to a thread, one each way. A signal on it while the thread waits in `RECV` on any endpoint takes the thread out of the endpoint's queue and wakes it with the bits in `a0` and `a6` = `NOTIFICATION` (`wake_bound`, `kernel/endpoint.rs:300`). Bits pending when the thread enters `RECV` are delivered first. This lets one server thread wait for both calls and doorbells, which shared-memory rings need ([[ipc-channels]]).

## Blocking and cancellation

A blocked thread is in one of four states (`kernel/thread.rs:41`): blocked on a notification, on sending or receiving through an endpoint, or on a reply. It is off the ready queue (`sched::block`, `kernel/sched.rs:64`) and, except when waiting for a reply, on the object's queue, with the object recorded in the TCB. Whatever ends the wait other than its normal completion has to undo that (`cancel_blocking`, `kernel/thread.rs:150`): leave the notification's or endpoint's queue, return a lent reply object, or unlink from the reply object a caller waits on (`cancel` and `cancel_reply`, `kernel/endpoint.rs:338`, 269):
- **Suspending** a blocked thread cancels the wait and **restarts the system call**: the program counter is moved back over the `ecall` (`UserContext::restart_syscall`, `kernel/arch/riscv/trap.rs:93`), so resuming the thread repeats its `WAIT`, as seL4 does (`thread::suspend`, `kernel/thread.rs:169`). Nothing is lost: signals sent meanwhile accumulate in the word, and the repeated `WAIT` takes them.
- **Deleting the last capability to the notification or endpoint** restarts every waiter's call (`finalise`, `kernel/notification.rs:136`; `finalise_endpoint`, `kernel/endpoint.rs:369`). The repeated call then fails cleanly with `FAILED_LOOKUP` instead of blocking forever on an object that is gone. A caller suspended while waiting for a reply has its `CALL` restarted too, so resuming it sends the call again, as in seL4.
- **Deleting the thread** cancels its wait and unbinds any notification bound to it (`thread::finalise`, line 202).

`RESUME` (and `WRITE_REGISTERS` with the resume flag) only acts on an inactive thread (`thread::resume`, line 188), so it can't make a blocked thread runnable while it is still on a queue.



## Fault delivery

A thread's faults go to the endpoint in its fault slot (`FAULT_SLOT`, `kernel/thread.rs:28`), set with `TCB_SET_FAULT_HANDLER` (`kernel/invocation.rs:285`), which stores a derived copy as `TCB_CONFIGURE` does. This replaces the earlier rule that any user fault ended the boot. As in seL4, the faulting thread makes an **implicit `CALL`** to that endpoint (`fault::deliver`, `kernel/fault.rs:61`):
- **The message** is built from the recorded fault (`Fault`, line 22), never from the thread's registers: the label is the fault kind (`VM` for page and access faults, `USER_EXCEPTION` for the rest), and the words are the faulting pc, the faulting address and the cause code (`abi/lib.rs:189`). The badge is the handler capability's, so one handler can serve many threads.
- **The reply** doesn't touch the thread's registers (`fault::answered`, line 93). Label 0 resumes the thread at the faulting instruction, so a handler can fix the cause (map a page, say) and retry; any other label leaves the thread suspended.
- **A fault that can't be answered** (the receiver lent no reply object, or the reply object was deleted) leaves the thread suspended, instead of returning `NO_REPLY` into registers it doesn't expect to change.
- **Without a handler** the thread is suspended with a warning. The root task has nobody to report to, so a fault there still ends the boot with a failure, which is what makes test runs fail loudly.
- **Suspending a thread that is delivering a fault** drops the fault rather than restarting a system call, so resuming it re-runs the faulting instruction and faults again.

## Per-thread IPC buffers in userspace

Several threads of one program each need their own IPC buffer. Every thread starts with the thread-pointer register `tp` pointing at its IPC buffer, which nothing else uses yet since there is no thread-local storage; the kernel sets it for the root task, and whoever starts any other thread sets it with its other registers. `sap-sys` reads it (`ipc_buffer`, `user/sys/sys.rs:13`). A `TODO` there says to move to a real thread-local once the sap Rust target has TLS ([[prog-runtime]], [[prog-rust-toolchain]]).

## Tests

`user/init` (`check_notifications`) checks: minting two badges and refusing a re-badge; two signals with nobody waiting accumulating, so one poll returns both bits and the next finds nothing pending; a signal through the unbadged original being pending with no bits, and a `WAIT` after an unbadged and a badged signal returning at once with the badged bits; a send-only capability being refused a `WAIT`; a second thread blocked in `WAIT` waking once per signal with the right badge; a signal sent while that thread is suspended being taken by its restarted `WAIT` after `RESUME`; revoking one badge deleting its copies while the badge itself keeps working; and deleting the notification with the thread waiting, after which its restarted `WAIT` reports `FAILED_LOOKUP`.

`check_ipc` uses a `spawn` helper (`spawn_with`) (`user/init/main.rs:460`) to start threads with their own stack and IPC buffer, and checks:
- a server thread answering with `REPLY_RECV`: a six-word `CALL` through a badged capability comes back with the badge as the label and every word incremented, so two words crossed the IPC buffers each way, and the reply carries no badge;
- a plain `SEND` completing without an answer, counted by the server;
- `RECV` on a send-only capability and `CALL` on a receive-only one refused;
- `NB_RECV` finding nothing on an idle endpoint, and `NB_SEND` with nobody waiting dropping the message;
- binding a notification to the server, binding twice refused, and a signal waking the server out of `RECV`;
- a `CALL` to a receiver that lent no reply object being delivered and ending with `NO_REPLY`;
- `init` receiving a call itself and deleting its reply object, after which the caller wakes with `NO_REPLY`;
- deleting the endpoint under the waiting server, whose restarted `RECV` then reports `FAILED_LOOKUP`.

`check_cap_transfer` uses a "keeper" thread that receives into a window and counts what arrives:
- two copies and a move arrive in order; the moved capability is gone from the sender, and a received copy works;
- a message sent through a capability without grant arrives with no capabilities;
- a one-slot window takes one of two capabilities, and a window whose first slot is occupied takes none;
- a `CALL` through a capability with grant-reply gets a capability moved back in its reply, into `init`'s own window; through one without grant-reply, the reply arrives with none.

> [!todo] Not built
> The IPC fast path, scheduling contexts donated on `CALL`, timeout faults, and handlers writing the faulting thread's registers in their reply.

`check_faults` checks that a thread reading unmapped memory reports a `VM` fault with the right address, pc and badge; that its handler (`init`) can map a page there, write a value and reply 0, after which the thread's read completes with that value; that an illegal instruction answered with a nonzero label leaves the thread stopped at the instruction; and that a thread without a handler is suspended inside the faulting function while `init` carries on.
