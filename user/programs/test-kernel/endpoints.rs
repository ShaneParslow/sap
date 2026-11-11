//! Endpoints and reply objects.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, error, object, recv_source, rights};
use sap_rt::{endpoint, notification, println, sys, tcb};

use crate::support::{fails, make, ok, slot, spawn, untyped_at_least};

/// Server state, read by the tests.
static SENDS: AtomicUsize = AtomicUsize::new(0);
static SIGNALS: AtomicUsize = AtomicUsize::new(0);
static SERVER_ERROR: AtomicUsize = AtomicUsize::new(0);
const SEND_LABEL: usize = 5;

/// Answer each call on `ep` with its words incremented and the caller's
/// badge as the label; count plain sends and bound-notification signals.
/// When receiving fails, record why and suspend itself (`me`).
extern "C" fn server(ep: usize, reply: usize, me: usize) -> ! {
    let mut words = [0usize; 8];
    let mut received = endpoint::recv(ep, reply, &mut words);

    loop {
        received = match received {
            Err(e) => {
                SERVER_ERROR.store(e.0, Ordering::Relaxed);
                let _ = tcb::suspend(me);
                continue;
            }

            Ok(m) if m.source == recv_source::NOTIFICATION => {
                SIGNALS.fetch_add(m.badge, Ordering::Relaxed);
                endpoint::recv(ep, reply, &mut words)
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
                endpoint::reply_recv(ep, reply, m.badge, &answer[..n], &[], &mut words)
            }
        };
    }
}

static LISTENED: AtomicUsize = AtomicUsize::new(0);

/// Receive on `ep` without a reply object, recording each label.
extern "C" fn listener(ep: usize, _: usize, _: usize) -> ! {
    loop {
        if let Ok(m) = endpoint::recv(ep, 0, &mut []) {
            LISTENED.store(m.info.label(), Ordering::Relaxed);
        }
    }
}

static CALL_RESULT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Call `ep` once and record the error code it ends with, then stop.
extern "C" fn caller(ep: usize, me: usize, _: usize) -> ! {
    let code = match endpoint::call(ep, 3, &[], &[], &mut []) {
        Ok(_) => 0,
        Err(e) => e.0,
    };
    CALL_RESULT.store(code, Ordering::Relaxed);

    loop {
        let _ = tcb::suspend(me);
    }
}

/// The 1 GiB-aligned region for the `k`th thread these tests start.
fn region(k: usize) -> usize {
    0x10_0000_0000 + k * 0x4000_0000
}

/// Calls with long messages and badges, plain sends, rights, non-blocking
/// variants, a bound notification waking a receiver, calls nobody can
/// answer, and deleting objects under waiters.
pub fn calls(info: &BootInfo) {
    let base = info.empty.start + 96;
    let parent = untyped_at_least(info, 18);
    let ep = base;
    let reply = base + 1;
    let badged = base + 2;
    let read_only = base + 3;
    let quiet = base + 4;
    let ntfn = base + 5;
    let signaller = base + 6;
    let answered = base + 7;
    let answered_reply = base + 8;
    let server_slots = base + 10;
    let listener_slots = base + 20;
    let caller_slots = base + 30;

    make(parent, object::ENDPOINT, ep);
    make(parent, object::REPLY, reply);
    ok(
        "badge 7",
        slot(badged).mint_from(slot(ep), rights::WRITE, 7),
    );
    ok(
        "read-only copy",
        slot(read_only).copy_from(slot(ep), rights::READ),
    );
    let server_tcb = spawn(
        parent,
        server_slots,
        region(0),
        server,
        [ep, reply, server_slots],
    );
    sys::yield_now(); // the server runs and waits in RECV

    // A call with six words: two travel through the IPC buffers each way.
    let mut out = [0usize; 8];
    let r = ok(
        "call",
        endpoint::call(badged, 42, &[1, 2, 3, 4, 5, 6], &[], &mut out),
    );
    assert_eq!(r.info.label(), 7, "reply label");
    assert_eq!(r.info.length(), 6, "reply length");
    assert_eq!(r.badge, 0, "reply badge");
    assert_eq!(out[..6], [2, 3, 4, 5, 6, 7], "reply words");

    // A plain send completes without waiting for an answer.
    ok("send", endpoint::send(badged, SEND_LABEL, &[10], &[]));
    sys::yield_now();
    assert_eq!(
        SENDS.load(Ordering::Relaxed),
        1,
        "the server didn't count the send"
    );

    // Rights.
    let denied = endpoint::recv(badged, 0, &mut []).map(drop);
    fails("recv on a send-only cap", denied, error::INVALID_CAPABILITY);
    let denied = endpoint::call(read_only, 1, &[], &[], &mut []).map(drop);
    fails(
        "call on a receive-only cap",
        denied,
        error::INVALID_CAPABILITY,
    );

    // Non-blocking variants on an endpoint nobody uses.
    make(parent, object::ENDPOINT, quiet);
    let nothing = ok("nb_recv", endpoint::nb_recv(quiet, 0, &mut []));
    assert_eq!(
        nothing.source,
        recv_source::NOTHING,
        "nb_recv found something"
    );
    ok(
        "nb_send with nobody waiting",
        endpoint::nb_send(quiet, 1, &[], &[]),
    );
    let nothing = ok("nb_recv", endpoint::nb_recv(quiet, 0, &mut []));
    assert_eq!(nothing.source, recv_source::NOTHING, "nb_send was kept");

    // A notification bound to the server wakes it from RECV.
    make(parent, object::NOTIFICATION, ntfn);
    ok("bind", tcb::bind_notification(server_tcb, ntfn));
    let twice = tcb::bind_notification(server_tcb, ntfn);
    fails("bind twice", twice, error::ILLEGAL_OPERATION);
    ok(
        "signal badge",
        slot(signaller).mint_from(slot(ntfn), rights::WRITE, 0b100),
    );
    ok("signal", notification::signal(signaller));
    sys::yield_now();
    let signals = SIGNALS.load(Ordering::Relaxed);
    assert_eq!(
        signals, 0b100,
        "the bound notification didn't wake the server"
    );

    // A receiver that lends no reply object can't answer: the caller's
    // message is delivered, and the call ends with NO_REPLY.
    spawn(parent, listener_slots, region(1), listener, [quiet, 0, 0]);
    sys::yield_now();
    let unanswered = endpoint::call(quiet, 9, &[], &[], &mut []).map(drop);
    fails(
        "a call to a receiver without a reply object",
        unanswered,
        error::NO_REPLY,
    );
    sys::yield_now();
    let label = LISTENED.load(Ordering::Relaxed);
    assert_eq!(label, 9, "the listener didn't get the message");

    // This thread receives a call and deletes its reply object instead of
    // answering: the caller wakes with NO_REPLY.
    make(parent, object::ENDPOINT, answered);
    make(parent, object::REPLY, answered_reply);
    spawn(
        parent,
        caller_slots,
        region(2),
        caller,
        [answered, caller_slots, 0],
    );
    sys::yield_now(); // it calls and waits to send
    let got = ok(
        "recv the call",
        endpoint::recv(answered, answered_reply, &mut []),
    );
    assert_eq!(got.info.label(), 3, "wrong call received");
    ok("delete the reply object", slot(answered_reply).delete());
    sys::yield_now();
    let result = CALL_RESULT.load(Ordering::Relaxed);
    assert_eq!(result, error::NO_REPLY, "the caller didn't get NO_REPLY");

    // Deleting the endpoint under the waiting server restarts its RECV,
    // which then fails instead of waiting forever.
    ok("revoke endpoint", slot(ep).revoke());
    ok("delete endpoint", slot(ep).delete());
    sys::yield_now();
    let ended = SERVER_ERROR.load(Ordering::Relaxed);
    assert_eq!(ended, error::FAILED_LOOKUP, "the server's RECV didn't fail");

    ok("revoke", slot(parent).revoke());
    println!("endpoints, calls, replies, bound notifications and deletion behave");
}
