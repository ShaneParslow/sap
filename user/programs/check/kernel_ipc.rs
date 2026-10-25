//! Notifications, endpoints, capability transfer and fault delivery.

use core::sync::atomic::{AtomicUsize, Ordering};
use sap_process::process_slot;
use sap_rt::abi::{attr, error, fault, object, recv_source, reg, rights};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::{ipc, notification, println, sys, tcb, untyped, vspace};
use crate::{Env, expect, slot, untyped_at_least};
use crate::threads::{spawn, spawn_with};

/// Bits received by the waiting thread, how often it woke, and the error
/// that ended it.
static RECEIVED: AtomicUsize = AtomicUsize::new(0);
static WAKEUPS: AtomicUsize = AtomicUsize::new(0);
static WAIT_ERROR: AtomicUsize = AtomicUsize::new(0);

/// Wait on the notification at `ntfn` forever, recording what arrives;
/// when waiting fails, record the error and suspend itself (`me`).
extern "C" fn waiter(ntfn: usize, me: usize) -> ! {
    loop {
        match notification::wait(ntfn) {
            Ok(bits) => {
                RECEIVED.fetch_add(bits, Ordering::Relaxed);
                WAKEUPS.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                WAIT_ERROR.store(e.0, Ordering::Relaxed);
                let _ = tcb::suspend(me);
            }
        }
    }
}

/// Badged signals, polling, a waiting thread, suspension while waiting,
/// revoking one badge, and deleting a notification with a thread waiting.
pub fn check_notifications(info: &Env) {
    let base = info.empty.start + 64;
    let parent = untyped_at_least(info, 16);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let me = process_slot::VSPACE;
    let (ntfn, one, two, one_copy) = (base, base + 1, base + 2, base + 3);

    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, ntfn, 1), Ok(()));
    expect("badge 1", slot(one).mint_from(slot(ntfn), rights::WRITE, 0b01), Ok(()));
    expect("badge 2", slot(two).mint_from(slot(ntfn), rights::WRITE, 0b10), Ok(()));
    expect("re-badge", slot(base + 4).mint_from(slot(one), rights::WRITE, 0b100), Err(error::ILLEGAL_OPERATION));

    // With nobody waiting, signals accumulate and a poll takes them all.
    expect("signal 1", notification::signal(one), Ok(()));
    expect("signal 2", notification::signal(two), Ok(()));
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(Some(0b11)), "poll should see both badges");
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(None), "poll should have cleared the bits");
    // A signal through the unbadged original carries no bits but still
    // counts, and a badged one after it adds its bits.
    expect("unbadged signal", notification::signal(ntfn), Ok(()));
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(Some(0)), "an unbadged signal should be pending");
    expect("unbadged signal again", notification::signal(ntfn), Ok(()));
    expect("signal 2 after it", notification::signal(two), Ok(()));
    assert_eq!(notification::wait(ntfn).map_err(|e| e.0), Ok(0b10), "wait should take both without blocking");
    let denied = notification::wait(one).map_err(|e| e.0);
    assert_eq!(denied, Err(error::INVALID_CAPABILITY), "a send-only capability can't wait");

    // A thread waiting on it: retype, give it a stack, start it.
    let (thread, tables, stack) = (base + 5, base + 6, base + 8);
    expect("retype tcb", untyped::retype(parent, object::TCB, 0, in_root, thread, 1), Ok(()));
    expect("retype tables", untyped::retype(parent, object::PAGE_TABLE, 0, in_root, tables, 2), Ok(()));
    expect("retype stack", untyped::retype(parent, object::FRAME_4K, 0, in_root, stack, 1), Ok(()));
    let stack_va: usize = 0x6000_0000;
    expect("map table", vspace::map_page_table(tables, me, stack_va), Ok(()));
    expect("map table", vspace::map_page_table(tables + 1, me, stack_va), Ok(()));
    expect("map stack", vspace::map_frame(stack, me, stack_va, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    expect("configure", tcb::configure(thread, process_slot::CNODE, me, None), Ok(()));
    expect("priority", tcb::set_priority(thread, process_slot::TCB, 255), Ok(()));
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = (waiter as *const ()).addr();
    regs[reg::SP] = stack_va + 0x1000;
    regs[reg::A0] = ntfn;
    regs[reg::A0 + 1] = thread;
    expect("start", tcb::write_registers(thread, true, &regs), Ok(()));
    sys::yield_now(); // it runs and blocks in wait

    expect("signal 1", notification::signal(one), Ok(()));
    sys::yield_now();
    expect("signal 2", notification::signal(two), Ok(()));
    sys::yield_now();
    assert_eq!(RECEIVED.load(Ordering::Relaxed), 0b11, "the waiter should have received both badges");
    assert_eq!(WAKEUPS.load(Ordering::Relaxed), 2, "one wakeup per signal");

    // Suspended while waiting, it misses nothing: the signal accumulates,
    // and resuming restarts its wait, which takes it.
    expect("suspend", tcb::suspend(thread), Ok(()));
    expect("signal while suspended", notification::signal(one), Ok(()));
    sys::yield_now();
    assert_eq!(WAKEUPS.load(Ordering::Relaxed), 2, "a suspended thread woke");
    expect("resume", tcb::resume(thread), Ok(()));
    sys::yield_now();
    assert_eq!((RECEIVED.load(Ordering::Relaxed), WAKEUPS.load(Ordering::Relaxed)), (0b11 + 0b01, 3), "the restarted wait should take the pending badge");

    // Revoking badge 1 cuts off its copies but nothing else.
    expect("copy badge 1", slot(one_copy).copy_from(slot(one), rights::WRITE), Ok(()));
    expect("revoke badge 1", slot(one).revoke(), Ok(()));
    let gone = notification::signal(one_copy).map_err(|e| e.0);
    assert_eq!(gone, Err(error::FAILED_LOOKUP), "a revoked badge copy still works");
    expect("badge 1 itself survives", notification::signal(one), Ok(()));
    sys::yield_now();

    // Deleting the notification with the thread waiting restarts its wait,
    // which then fails cleanly instead of hanging.
    expect("revoke notification", slot(ntfn).revoke(), Ok(()));
    expect("delete notification", slot(ntfn).delete(), Ok(()));
    sys::yield_now();
    assert_eq!(WAIT_ERROR.load(Ordering::Relaxed), error::FAILED_LOOKUP, "the waiter should see its capability gone");

    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("check: notifications, badges, waiting, suspension and deletion behave");
}

/// Server state, read by the checks.
static SENDS: AtomicUsize = AtomicUsize::new(0);
static SIGNALS: AtomicUsize = AtomicUsize::new(0);
static SERVER_ERROR: AtomicUsize = AtomicUsize::new(0);
const SEND_LABEL: usize = 5;

/// Answer each call on `ep` with its words incremented and the caller's
/// badge as the label; count plain sends and bound-notification signals.
/// When receiving fails, record why and suspend itself (`me`).
extern "C" fn server(ep: usize, reply: usize, me: usize) -> ! {
    let mut words = [0usize; 8];
    let mut received = ipc::recv(ep, reply, &mut words);
    loop {
        received = match received {
            Err(e) => {
                SERVER_ERROR.store(e.0, Ordering::Relaxed);
                let _ = tcb::suspend(me);
                continue;
            }
            Ok(m) if m.source == recv_source::NOTIFICATION => {
                SIGNALS.fetch_add(m.badge, Ordering::Relaxed);
                ipc::recv(ep, reply, &mut words)
            }
            Ok(m) => {
                if m.info.label() == SEND_LABEL {
                    SENDS.fetch_add(1, Ordering::Relaxed);
                }
                let n = m.info.length();
                let mut answer = [0usize; 8];
                for (a, w) in answer.iter_mut().zip(&words[..n]) {
                    *a = w + 1;
                }
                // Replying to a plain send finds no caller and does nothing.
                ipc::reply_recv(ep, reply, m.badge, &answer[..n], &[], &mut words)
            }
        };
    }
}

static LISTENED: AtomicUsize = AtomicUsize::new(0);

/// Receive on `ep` without a reply object, recording each label.
extern "C" fn listener(ep: usize, _: usize, _: usize) -> ! {
    loop {
        if let Ok(m) = ipc::recv(ep, 0, &mut []) {
            LISTENED.store(m.info.label(), Ordering::Relaxed);
        }
    }
}

static CALL_RESULT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Call `ep` once and record the error code it ends with, then stop.
extern "C" fn caller(ep: usize, me: usize, _: usize) -> ! {
    let code = match ipc::call(ep, 3, &[], &[], &mut []) {
        Ok(_) => 0,
        Err(e) => e.0,
    };
    CALL_RESULT.store(code, Ordering::Relaxed);
    loop {
        let _ = tcb::suspend(me);
    }
}

/// Endpoints and reply objects: calls with long messages and badges, plain
/// sends, rights, non-blocking variants, a bound notification waking a
/// receiver, calls nobody can answer, and deleting objects under waiters.
pub fn check_ipc(info: &Env) {
    let base = info.empty.start + 96;
    let parent = untyped_at_least(info, 18);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (ep, reply, badged, read_only, ep2, ntfn) = (base, base + 1, base + 2, base + 3, base + 4, base + 5);
    let region = |k: usize| 0x10_0000_0000 + k * 0x4000_0000;

    expect("retype endpoints", untyped::retype(parent, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("badge 7", slot(badged).mint_from(slot(ep), rights::WRITE, 7), Ok(()));
    expect("read-only copy", slot(read_only).copy_from(slot(ep), rights::READ), Ok(()));
    let server_tcb = spawn(parent, base + 10, region(0), server, [ep, reply, base + 10]);
    sys::yield_now(); // the server runs and waits in RECV

    // A call with six words: two travel through the IPC buffers each way.
    let mut out = [0usize; 8];
    let r = ipc::call(badged, 42, &[1, 2, 3, 4, 5, 6], &[], &mut out).expect("call");
    assert_eq!((r.info.label(), r.info.length(), r.badge), (7, 6, 0), "reply label, length or badge");
    assert_eq!(out[..6], [2, 3, 4, 5, 6, 7], "reply words");

    // A plain send completes without waiting for an answer.
    expect("send", ipc::send(badged, SEND_LABEL, &[10], &[]).map(drop), Ok(()));
    sys::yield_now();
    assert_eq!(SENDS.load(Ordering::Relaxed), 1, "the server didn't count the send");

    // Rights.
    let wrong = |r: Result<ipc::Received, sys::Error>| r.map(drop).map_err(|e| e.0);
    assert_eq!(wrong(ipc::recv(badged, 0, &mut [])), Err(error::INVALID_CAPABILITY), "recv on a send-only cap");
    assert_eq!(wrong(ipc::call(read_only, 1, &[], &[], &mut [])), Err(error::INVALID_CAPABILITY), "call on a receive-only cap");

    // Non-blocking variants on an endpoint nobody uses.
    expect("retype endpoint 2", untyped::retype(parent, object::ENDPOINT, 0, in_root, ep2, 1), Ok(()));
    let nothing = ipc::nb_recv(ep2, 0, &mut []).expect("nb_recv");
    assert_eq!(nothing.source, recv_source::NOTHING, "nb_recv found something");
    expect("nb_send with nobody waiting", ipc::nb_send(ep2, 1, &[], &[]).map(drop), Ok(()));
    assert_eq!(ipc::nb_recv(ep2, 0, &mut []).expect("nb_recv").source, recv_source::NOTHING, "nb_send was kept");

    // A notification bound to the server wakes it from RECV.
    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, ntfn, 1), Ok(()));
    expect("bind", tcb::bind_notification(server_tcb, ntfn), Ok(()));
    expect("bind twice", tcb::bind_notification(server_tcb, ntfn), Err(error::ILLEGAL_OPERATION));
    expect("signal badge", slot(base + 6).mint_from(slot(ntfn), rights::WRITE, 0b100), Ok(()));
    expect("signal", notification::signal(base + 6), Ok(()));
    sys::yield_now();
    assert_eq!(SIGNALS.load(Ordering::Relaxed), 0b100, "the bound notification didn't wake the server");

    // A receiver that lends no reply object can't answer: the caller's
    // message is delivered, and the call ends with NO_REPLY.
    let _listener = spawn(parent, base + 20, region(1), listener, [ep2, 0, 0]);
    sys::yield_now();
    let unanswered = ipc::call(ep2, 9, &[], &[], &mut []).map(drop).map_err(|e| e.0);
    assert_eq!(unanswered, Err(error::NO_REPLY), "a call to a receiver without a reply object");
    sys::yield_now();
    assert_eq!(LISTENED.load(Ordering::Relaxed), 9, "the listener didn't get the message");

    // This thread receives a call and deletes its reply object instead of
    // answering: the caller wakes with NO_REPLY.
    let (ep4, reply4) = (base + 7, base + 8);
    expect("retype endpoint 4", untyped::retype(parent, object::ENDPOINT, 0, in_root, ep4, 1), Ok(()));
    expect("retype reply 4", untyped::retype(parent, object::REPLY, 0, in_root, reply4, 1), Ok(()));
    let _caller = spawn(parent, base + 30, region(2), caller, [ep4, base + 30, 0]);
    sys::yield_now(); // it calls and waits to send
    let got = ipc::recv(ep4, reply4, &mut []).expect("recv the call");
    assert_eq!(got.info.label(), 3, "wrong call received");
    expect("delete the reply object", slot(reply4).delete(), Ok(()));
    sys::yield_now();
    assert_eq!(CALL_RESULT.load(Ordering::Relaxed), error::NO_REPLY, "the caller didn't get NO_REPLY");

    // Deleting the endpoint under the waiting server restarts its RECV,
    // which then fails instead of waiting forever.
    expect("revoke endpoint", slot(ep).revoke(), Ok(()));
    expect("delete endpoint", slot(ep).delete(), Ok(()));
    sys::yield_now();
    assert_eq!(SERVER_ERROR.load(Ordering::Relaxed), error::FAILED_LOOKUP, "the server's RECV didn't fail");

    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("check: endpoints, calls, replies, bound notifications and deletion behave");
}

/// The keeper's receive window and what it last received.
static KEEP_INDEX: AtomicUsize = AtomicUsize::new(0);
static KEEP_COUNT: AtomicUsize = AtomicUsize::new(0);
static KEPT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Receive on `ep` into the window in KEEP_INDEX/KEEP_COUNT and record how
/// many capabilities arrived. A message labelled 2 is answered by moving
/// the first capability received back to the caller.
extern "C" fn keeper(ep: usize, reply: usize, _: usize) -> ! {
    loop {
        ipc::set_receive_window(process_slot::CNODE, KEEP_INDEX.load(Ordering::Relaxed), KEEP_COUNT.load(Ordering::Relaxed));
        let Ok(m) = ipc::recv(ep, reply, &mut []) else { continue };
        KEPT.store(m.info.extra_caps(), Ordering::Relaxed);
        if m.info.label() == 2 {
            let back = [ipc::Give::moved(KEEP_INDEX.load(Ordering::Relaxed))];
            let _ = ipc::reply(reply, 0, &[], &back);
        } else {
            let _ = ipc::reply(reply, 0, &[], &[]);
        }
    }
}

/// Capabilities in messages: copies and moves into a receive window, the
/// grant right, a window too small or already occupied, and replies that
/// carry capabilities (with and without GRANT_REPLY).
pub fn check_cap_transfer(info: &Env) {
    let base = info.empty.start + 160;
    let parent = untyped_at_least(info, 18);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (ep, reply, granting, plain, no_reply_grant, ntfn, frame, spare) =
        (base, base + 1, base + 2, base + 3, base + 4, base + 5, base + 6, base + 7);
    let window = base + 20; // keeper's window: base + 20 .. base + 30
    let mine = base + 40; // init's own window

    expect("retype endpoint", untyped::retype(parent, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, ntfn, 1), Ok(()));
    expect("retype frame", untyped::retype(parent, object::FRAME_4K, 0, in_root, frame, 1), Ok(()));
    expect("retype spare endpoint", untyped::retype(parent, object::ENDPOINT, 0, in_root, spare, 1), Ok(()));
    let all = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;
    expect("granting cap", slot(granting).mint_from(slot(ep), all, 1), Ok(()));
    expect("plain cap", slot(plain).mint_from(slot(ep), rights::WRITE, 2), Ok(()));
    expect("no grant-reply cap", slot(no_reply_grant).mint_from(slot(ep), rights::WRITE | rights::GRANT, 3), Ok(()));
    // The keeper reads its window between receives, and the kernel uses
    // the window set when the keeper blocked; an empty message makes it
    // pick up a new one.
    let set_window = |count: usize| {
        KEEP_INDEX.store(window, Ordering::Relaxed);
        KEEP_COUNT.store(count, Ordering::Relaxed);
        let _ = ipc::send(plain, 1, &[], &[]);
        sys::yield_now();
    };
    let kept = || KEPT.load(Ordering::Relaxed);
    let filled = |s: usize| slot(base + 50).copy_from(slot(s), rights::ALL).map(|_| slot(base + 50).delete());
    let clear = || {
        for i in 0..10 {
            let _ = slot(window + i).delete();
        }
    };
    KEEP_INDEX.store(window, Ordering::Relaxed);
    KEEP_COUNT.store(10, Ordering::Relaxed);
    let _keeper = spawn(parent, base + 60, 0x20_0000_0000, keeper, [ep, reply, 0]);
    sys::yield_now(); // it waits in RECV

    // Two copies and a move arrive in order; the moved capability's slot
    // here is empty afterwards, and the copies work.
    let gives = [ipc::Give::copy(ntfn), ipc::Give::copy(frame), ipc::Give::moved(spare)];
    expect("send three", ipc::send(granting, 1, &[], &gives), Ok(()));
    sys::yield_now();
    assert_eq!(kept(), 3, "three capabilities should have arrived");
    assert!(filled(window + 2).is_ok(), "the moved capability isn't in the window");
    assert!(filled(spare).is_err(), "a moved capability stayed with the sender");
    expect("use a received copy", notification::signal(window), Ok(()));
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(Some(0)), "a received copy should signal");
    clear();

    // Without GRANT nothing travels; the message still does.
    expect("send without grant", ipc::send(plain, 1, &[], &[ipc::Give::copy(ntfn)]), Ok(()));
    sys::yield_now();
    assert_eq!(kept(), 0, "capabilities crossed without GRANT");

    // A window of one takes one; an occupied first slot takes none.
    set_window(1);
    expect("send to a small window", ipc::send(granting, 1, &[], &[ipc::Give::copy(ntfn), ipc::Give::copy(frame)]), Ok(()));
    sys::yield_now();
    assert_eq!(kept(), 1, "a one-slot window should take one capability");
    set_window(10); // slot `window` is still occupied
    expect("send to an occupied window", ipc::send(granting, 1, &[], &[ipc::Give::copy(frame)]), Ok(()));
    sys::yield_now();
    assert_eq!(kept(), 0, "transfer should stop at an occupied slot");
    clear();

    // A reply can carry capabilities back if the caller's capability has
    // GRANT_REPLY: the keeper moves what it received back into this
    // thread's window.
    ipc::set_receive_window(process_slot::CNODE, mine, 4);
    let back = ipc::call(granting, 2, &[], &[ipc::Give::copy(ntfn)], &mut []).expect("call with a capability");
    assert_eq!(back.info.extra_caps(), 1, "the reply should carry the capability back");
    assert!(filled(mine).is_ok(), "the returned capability isn't in this thread's window");
    assert!(filled(window).is_err(), "the keeper kept the capability it moved back");
    let _ = slot(mine).delete();
    let refused = ipc::call(no_reply_grant, 2, &[], &[ipc::Give::copy(ntfn)], &mut []).expect("call without grant-reply");
    assert_eq!(refused.info.extra_caps(), 0, "a reply carried capabilities without GRANT_REPLY");
    clear();

    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("check: capability transfer, grant rights and receive windows behave");
}

/// What the faulting thread read after its handler fixed the fault.
static FAULT_READ: AtomicUsize = AtomicUsize::new(0);

/// Mode 0: read `address` (which faults until a handler maps it) and record
/// the value. Mode 1: run an illegal instruction. Then suspend itself.
extern "C" fn faulter(address: usize, mode: usize, me: usize) -> ! {
    if mode == 0 {
        // SAFETY: deliberately touches an address that may be unmapped; the
        // fault handler maps it before this thread resumes.
        let value = unsafe { core::ptr::read_volatile(mapped::<usize>(address)) };
        FAULT_READ.store(value, Ordering::Relaxed);
    } else {
        // SAFETY: deliberately illegal; the thread never resumes past it.
        unsafe { core::arch::asm!("unimp") };
    }
    loop {
        let _ = tcb::suspend(me);
    }
}

/// Faults delivered over IPC: a page fault fixed by the handler mapping a
/// page and resuming the thread, an illegal instruction left suspended, and
/// a thread without a handler suspended rather than ending the boot.
pub fn check_faults(info: &Env) {
    let base = info.empty.start + 240;
    let parent = untyped_at_least(info, 18);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let me = process_slot::VSPACE;
    let (ep, reply, handler_cap, page) = (base, base + 1, base + 2, base + 3);
    expect("retype endpoint", untyped::retype(parent, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("retype page", untyped::retype(parent, object::FRAME_4K, 0, in_root, page, 1), Ok(()));
    expect("badge the handler cap", slot(handler_cap).mint_from(slot(ep), rights::WRITE, 0x51), Ok(()));

    // A page fault, fixed by the handler. The address is in the faulting
    // thread's own region, whose page tables exist but where nothing is
    // mapped above its stack and IPC buffer.
    let region = 0x30_0000_0000;
    let target = region + 0x8000;
    let t1 = spawn_with(parent, base + 10, region, faulter, [target, 0, base + 10], false);
    expect("set handler", tcb::set_fault_handler(t1, Some(handler_cap)), Ok(()));
    expect("start", tcb::resume(t1), Ok(()));
    let mut words = [0usize; 3];
    let got = ipc::recv(ep, reply, &mut words).expect("receive the fault");
    assert_eq!((got.info.label(), got.badge), (fault::VM, 0x51), "fault kind or badge");
    assert_eq!(words[fault::ADDRESS], target, "fault address");
    let entry = (faulter as *const ()).addr();
    assert!((entry..entry + 0x100).contains(&words[fault::PC]), "fault pc not in faulter");
    // Fix it: map a page there with a value in it, and resume the thread.
    expect("map the page", vspace::map_frame(page, me, target, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: just mapped read-write.
    unsafe { mapped::<usize>(target).write_volatile(0xfa017) };
    expect("resume it", ipc::reply(reply, 0, &[], &[]), Ok(()));
    sys::yield_now();
    assert_eq!(FAULT_READ.load(Ordering::Relaxed), 0xfa017, "the faulting read didn't complete after the fix");

    // An illegal instruction answered with a nonzero label stays stopped,
    // at the instruction.
    let t2 = spawn_with(parent, base + 20, region + 0x4000_0000, faulter, [0, 1, base + 20], false);
    expect("set handler", tcb::set_fault_handler(t2, Some(handler_cap)), Ok(()));
    expect("start", tcb::resume(t2), Ok(()));
    let got = ipc::recv(ep, reply, &mut words).expect("receive the fault");
    assert_eq!(got.info.label(), fault::USER_EXCEPTION, "fault kind");
    expect("refuse it", ipc::reply(reply, 1, &[], &[]), Ok(()));
    sys::yield_now();
    let pc = tcb::read_registers(t2).expect("read registers")[reg::PC];
    assert_eq!(pc, words[fault::PC], "a refused fault should leave the thread at the instruction");

    // Without a handler, the faulting thread is suspended; this thread
    // carries on.
    let t3 = spawn(parent, base + 30, region + 0x8000_0000, faulter, [0, 1, base + 30]);
    sys::yield_now();
    let pc3 = tcb::read_registers(t3).expect("read registers")[reg::PC];
    assert!((entry..entry + 0x100).contains(&pc3), "the unhandled thread isn't stopped in faulter");

    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("check: faults delivered, fixed, refused and unhandled behave");
}
