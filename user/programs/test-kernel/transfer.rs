//! Capabilities in messages.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, object, rights};
use sap_rt::endpoint::Give;
use sap_rt::{endpoint, notification, println, sys};

use crate::support::{CNODE, make, ok, slot, spawn, untyped_at_least};

/// The keeper's receive window and what it last received.
static KEEP_INDEX: AtomicUsize = AtomicUsize::new(0);
static KEEP_COUNT: AtomicUsize = AtomicUsize::new(0);
static KEPT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// How many slots the keeper's window has at most.
const WINDOW_SLOTS: usize = 10;

/// A message with this label is answered by moving the first capability
/// received back to the caller.
const RETURN_IT: usize = 2;

/// Receive on `ep` into the window in KEEP_INDEX/KEEP_COUNT and record how
/// many capabilities arrived.
extern "C" fn keeper(ep: usize, reply: usize, _: usize) -> ! {
    loop {
        let index = KEEP_INDEX.load(Ordering::Relaxed);
        let count = KEEP_COUNT.load(Ordering::Relaxed);
        endpoint::set_receive_window(CNODE, index, count);

        let Ok(m) = endpoint::recv(ep, reply, &mut []) else {
            continue;
        };
        KEPT.store(m.info.extra_caps(), Ordering::Relaxed);

        if m.info.label() == RETURN_IT {
            let back = [Give::moved(index)];
            let _ = endpoint::reply(reply, 0, &[], &back);
        } else {
            let _ = endpoint::reply(reply, 0, &[], &[]);
        }
    }
}

fn kept() -> usize {
    KEPT.load(Ordering::Relaxed)
}

/// Whether slot `index` holds a capability.
fn filled(index: usize) -> bool {
    slot(index).identify().is_ok()
}

/// Empty the keeper's window, from slot `window` on.
fn clear(window: usize) {
    for i in 0..WINDOW_SLOTS {
        let _ = slot(window + i).delete();
    }
}

/// Copies and moves into a receive window, the grant right, a window too
/// small or already occupied, and replies that carry capabilities (with and
/// without GRANT_REPLY).
pub fn capabilities(info: &BootInfo) {
    let base = info.empty.start + 160;
    let parent = untyped_at_least(info, 18);
    let ep = base;
    let reply = base + 1;
    let granting = base + 2;
    let plain = base + 3;
    let no_reply_grant = base + 4;
    let ntfn = base + 5;
    let frame = base + 6;
    let spare = base + 7;
    let window = base + 20; // the keeper's: base + 20 .. base + 30
    let mine = base + 40; // this thread's own
    let keeper_slots = base + 60;

    make(parent, object::ENDPOINT, ep);
    make(parent, object::REPLY, reply);
    make(parent, object::NOTIFICATION, ntfn);
    make(parent, object::FRAME_4K, frame);
    make(parent, object::ENDPOINT, spare);

    let all = rights::WRITE | rights::GRANT | rights::GRANT_REPLY;
    let write_grant = rights::WRITE | rights::GRANT;
    ok("granting cap", slot(granting).mint_from(slot(ep), all, 1));
    ok(
        "plain cap",
        slot(plain).mint_from(slot(ep), rights::WRITE, 2),
    );
    ok(
        "no grant-reply cap",
        slot(no_reply_grant).mint_from(slot(ep), write_grant, 3),
    );

    // The keeper reads its window between receives, and the kernel uses
    // the window set when the keeper blocked; an empty message makes it
    // pick up a new one.
    let set_window = |count: usize| {
        KEEP_INDEX.store(window, Ordering::Relaxed);
        KEEP_COUNT.store(count, Ordering::Relaxed);
        let _ = endpoint::send(plain, 1, &[], &[]);
        sys::yield_now();
    };

    KEEP_INDEX.store(window, Ordering::Relaxed);
    KEEP_COUNT.store(WINDOW_SLOTS, Ordering::Relaxed);
    spawn(parent, keeper_slots, 0x20_0000_0000, keeper, [ep, reply, 0]);
    sys::yield_now(); // it waits in RECV

    // Two copies and a move arrive in order; the moved capability's slot
    // here is empty afterwards, and the copies work.
    let gives = [Give::copy(ntfn), Give::copy(frame), Give::moved(spare)];
    ok("send three", endpoint::send(granting, 1, &[], &gives));
    sys::yield_now();
    assert_eq!(kept(), 3, "three capabilities should have arrived");
    assert!(
        filled(window + 2),
        "the moved capability isn't in the window"
    );
    assert!(!filled(spare), "a moved capability stayed with the sender");
    ok("use a received copy", notification::signal(window));
    let polled = ok("poll", notification::poll(ntfn));
    assert_eq!(polled, Some(0), "a received copy should signal");
    clear(window);

    // Without GRANT nothing travels; the message still does.
    ok(
        "send without grant",
        endpoint::send(plain, 1, &[], &[Give::copy(ntfn)]),
    );
    sys::yield_now();
    assert_eq!(kept(), 0, "capabilities crossed without GRANT");

    // A window of one takes one; an occupied first slot takes none.
    set_window(1);
    let two = [Give::copy(ntfn), Give::copy(frame)];
    ok(
        "send to a small window",
        endpoint::send(granting, 1, &[], &two),
    );
    sys::yield_now();
    assert_eq!(kept(), 1, "a one-slot window should take one capability");

    set_window(WINDOW_SLOTS); // slot `window` is still occupied
    let one = [Give::copy(frame)];
    ok(
        "send to an occupied window",
        endpoint::send(granting, 1, &[], &one),
    );
    sys::yield_now();
    assert_eq!(kept(), 0, "transfer should stop at an occupied slot");
    clear(window);

    // A reply can carry capabilities back if the caller's capability has
    // GRANT_REPLY: the keeper moves what it received back into this
    // thread's window.
    endpoint::set_receive_window(CNODE, mine, 4);
    let give = [Give::copy(ntfn)];
    let call = endpoint::call(granting, RETURN_IT, &[], &give, &mut []);
    let back = ok("call with a capability", call);
    assert_eq!(
        back.info.extra_caps(),
        1,
        "the reply should carry the capability back"
    );
    assert!(
        filled(mine),
        "the returned capability isn't in this thread's window"
    );
    assert!(
        !filled(window),
        "the keeper kept the capability it moved back"
    );
    let _ = slot(mine).delete();

    let call = endpoint::call(no_reply_grant, RETURN_IT, &[], &give, &mut []);
    let refused = ok("call without grant-reply", call);
    assert_eq!(
        refused.info.extra_caps(),
        0,
        "a reply carried capabilities without GRANT_REPLY"
    );
    clear(window);

    ok("revoke", slot(parent).revoke());
    println!("capability transfer, grant rights and receive windows behave");
}
