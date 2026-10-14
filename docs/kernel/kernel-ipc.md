---
status: implemented
summary: Kernel IPC as implemented today - notifications (a word of badge bits and a waiter queue threaded through TCBs, with Signal, Wait and Poll), badges and their place in the derivation tree, blocking and its cancellation, and restarted system calls. Endpoints, reply objects, capability transfer and fault delivery follow in M4.
updated: 2026-10-13
code: [kernel/notification.rs, kernel/syscall.rs, kernel/thread.rs, kernel/sched.rs, kernel/cap.rs, kernel/cspace.rs, kernel/untyped.rs, kernel/arch/riscv/trap.rs, abi/lib.rs, user/libsap/notification.rs, user/init/main.rs]
sources: ["[[2026-10-08-22c3947b#m33|§m33]]", "[[2026-10-08-22c3947b#m34|§m34]]"]
---

# Kernel IPC

Roadmap milestone M4 ([[roadmap]]) adds the kernel's two IPC objects, following seL4 with the divergences decided for sap ([[decisions]]): notifications, which carry no data, and endpoints, which carry messages and capabilities between threads. The design is [[kernel-objects-and-syscalls]] and [[ipc-cap-transfer]]; this page describes what is built. So far that is notifications.

## Notifications

A `Notification` (`kernel/notification.rs:18`) is a word of pending badge bits and a queue of waiting threads, in 32 bytes (`NOTIFICATION_BITS`, line 15), retyped from untyped memory like any object. Three syscalls act on a notification capability (`abi/lib.rs:28`, handled in `notification_call`, `kernel/syscall.rs:44`):

| Syscall | Needs right | Effect |
|---|---|---|
| `SIGNAL` | write | OR the capability's badge into the word; if a thread is waiting, hand the badge straight to the first one and wake it instead (`signal`, `notification.rs:34`) |
| `WAIT` | read | if the word is nonzero, return it and clear it; otherwise block until a signal arrives (`wait`, `notification.rs:56`) |
| `POLL` | read | return and clear the word without blocking (`notification.rs:80`) |

Results come back in `a0` (the bits) and `a1` (an error code). Separate syscalls rather than seL4's reuse of `Send` and `Recv` on notifications was the human's choice ([[2026-10-08-22c3947b#m34|§m34]]).

The waiter queue is threaded through the waiting threads' TCBs (`ipc_next`/`ipc_prev`), so a notification costs the same however many threads wait on it ([[kernel-objects-and-syscalls#Kernel memory]]).

## Badges

A badge is a word stamped on a capability when it is minted (`CNODE_MINT` with the badge as data). Each signaller gets its own badge, so a waiter learns who signalled from the bits it receives. As in seL4:
- an unbadged capability can be badged once; re-badging is `ILLEGAL_OPERATION`;
- a copy with a *new* badge is revocable and marked "first badged" in its slot (`first_badged`, `kernel/cspace.rs:40`);
- copies of that capability with the *same* badge become its children, while copies badged afresh don't (`is_parent_of`, line 233). Revoking a badged capability therefore cuts off exactly the holders of that badge, and revoking the unbadged original cuts off every badge.

## Blocking and cancellation

A thread that waits becomes `BlockedOnNotification` (`kernel/thread.rs:39`): off the ready queue (`sched::block`, `kernel/sched.rs:64`), on the notification's queue, with the notification recorded in the TCB. Whatever ends the wait other than a signal has to take it off that queue (`cancel_blocking`, `kernel/thread.rs:118`):
- **Suspending** a blocked thread cancels the wait and **restarts the system call**: the program counter is moved back over the `ecall` (`UserContext::restart_syscall`, `kernel/arch/riscv/trap.rs:93`), so resuming the thread repeats its `WAIT`, as seL4 does (`thread::suspend`, `kernel/thread.rs:135`). Nothing is lost: signals sent meanwhile accumulate in the word, and the repeated `WAIT` takes them.
- **Deleting the last capability to the notification** restarts every waiter's call (`finalise`, `kernel/notification.rs:103`). The repeated `WAIT` then fails cleanly with `FAILED_LOOKUP` instead of blocking forever on an object that is gone.
- **Deleting the thread** cancels its wait and unbinds any notification bound to it (`thread::finalise`, line 164).

`RESUME` (and `WRITE_REGISTERS` with the resume flag) only acts on an inactive thread (`thread::resume`, line 150), so it can't make a blocked thread runnable while it is still on a queue.

A thread can have a bound notification (`bound_notification`, line 65). Binding and the interplay with `Recv` on an endpoint arrive with endpoints.

## Tests

`user/init` (`check_notifications`) checks: minting two badges and refusing a re-badge; two signals with nobody waiting accumulating, so one poll returns both bits and the next returns none; a send-only capability being refused a `WAIT`; a second thread blocked in `WAIT` waking once per signal with the right badge; a signal sent while that thread is suspended being taken by its restarted `WAIT` after `RESUME`; revoking one badge deleting its copies while the badge itself keeps working; and deleting the notification with the thread waiting, after which its restarted `WAIT` reports `FAILED_LOOKUP`.

> [!todo] Not built
> Endpoints with MCS-style reply objects (`Send`, `Recv`, `Call`, `Reply`, `ReplyRecv`), binding notifications to threads, capability transfer (up to 16, move or copy), fault delivery, and the two-process ring demo.
