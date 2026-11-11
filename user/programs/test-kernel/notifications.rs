//! Notifications: badges, polling, waiting, suspension and deletion.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, error, object, reg, rights};
use sap_rt::{notification, println, sys, tcb};

use crate::support::{
    CNODE, TCB, VSPACE, fails, make, make_many, map_data, map_tables, ok, slot, untyped_at_least,
};

/// Bits received by the waiting thread, how often it woke, and the error
/// that ended it.
static RECEIVED: AtomicUsize = AtomicUsize::new(0);
static WAKEUPS: AtomicUsize = AtomicUsize::new(0);
static WAIT_ERROR: AtomicUsize = AtomicUsize::new(0);

const STACK_VA: usize = 0x6000_0000;

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

fn received() -> usize {
    RECEIVED.load(Ordering::Relaxed)
}

fn wakeups() -> usize {
    WAKEUPS.load(Ordering::Relaxed)
}

/// Badged signals, polling, a waiting thread, suspension while waiting,
/// revoking one badge, and deleting a notification with a thread waiting.
pub fn signalling(info: &BootInfo) {
    let base = info.empty.start + 64;
    let parent = untyped_at_least(info, 16);
    let ntfn = base;
    let one = base + 1;
    let two = base + 2;
    let one_copy = base + 3;
    let rebadged = base + 4;
    let thread = base + 5;
    let tables = base + 6;
    let stack = base + 8;

    make(parent, object::NOTIFICATION, ntfn);
    ok(
        "badge 1",
        slot(one).mint_from(slot(ntfn), rights::WRITE, 0b01),
    );
    ok(
        "badge 2",
        slot(two).mint_from(slot(ntfn), rights::WRITE, 0b10),
    );
    let again = slot(rebadged).mint_from(slot(one), rights::WRITE, 0b100);
    fails("re-badge", again, error::ILLEGAL_OPERATION);

    // With nobody waiting, signals accumulate and a poll takes them all.
    ok("signal 1", notification::signal(one));
    ok("signal 2", notification::signal(two));
    let polled = ok("poll", notification::poll(ntfn));
    assert_eq!(polled, Some(0b11), "poll should see both badges");
    let polled = ok("poll", notification::poll(ntfn));
    assert_eq!(polled, None, "poll should have cleared the bits");

    // A signal through the unbadged original carries no bits but still
    // counts, and a badged one after it adds its bits.
    ok("unbadged signal", notification::signal(ntfn));
    let polled = ok("poll", notification::poll(ntfn));
    assert_eq!(polled, Some(0), "an unbadged signal should be pending");
    ok("unbadged signal again", notification::signal(ntfn));
    ok("signal 2 after it", notification::signal(two));
    let bits = ok("wait", notification::wait(ntfn));
    assert_eq!(bits, 0b10, "wait should take both without blocking");

    let denied = notification::wait(one);
    fails(
        "a send-only capability can't wait",
        denied,
        error::INVALID_CAPABILITY,
    );

    // A thread waiting on it: retype, give it a stack, start it.
    make(parent, object::TCB, thread);
    make_many(parent, object::PAGE_TABLE, tables, 2);
    make(parent, object::FRAME_4K, stack);
    map_tables(tables, STACK_VA);
    map_data(stack, STACK_VA);
    ok("configure", tcb::configure(thread, CNODE, VSPACE, None));
    ok("priority", tcb::set_priority(thread, TCB, 255));

    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = (waiter as *const ()).addr();
    regs[reg::SP] = STACK_VA + 0x1000;
    regs[reg::A0] = ntfn;
    regs[reg::A0 + 1] = thread;
    ok("start", tcb::write_registers(thread, true, &regs));
    sys::yield_now(); // it runs and blocks in wait

    ok("signal 1", notification::signal(one));
    sys::yield_now();
    ok("signal 2", notification::signal(two));
    sys::yield_now();
    assert_eq!(
        received(),
        0b11,
        "the waiter should have received both badges"
    );
    assert_eq!(wakeups(), 2, "one wakeup per signal");

    // Suspended while waiting, it misses nothing: the signal accumulates,
    // and resuming restarts its wait, which takes it.
    ok("suspend", tcb::suspend(thread));
    ok("signal while suspended", notification::signal(one));
    sys::yield_now();
    assert_eq!(wakeups(), 2, "a suspended thread woke");
    ok("resume", tcb::resume(thread));
    sys::yield_now();
    assert_eq!(
        received(),
        0b11 + 0b01,
        "the restarted wait should take the pending badge"
    );
    assert_eq!(wakeups(), 3, "the restarted wait should wake once");

    // Revoking badge 1 cuts off its copies but nothing else.
    ok(
        "copy badge 1",
        slot(one_copy).copy_from(slot(one), rights::WRITE),
    );
    ok("revoke badge 1", slot(one).revoke());
    let gone = notification::signal(one_copy);
    fails(
        "a revoked badge copy still works",
        gone,
        error::FAILED_LOOKUP,
    );
    ok("badge 1 itself survives", notification::signal(one));
    sys::yield_now();

    // Deleting the notification with the thread waiting restarts its wait,
    // which then fails cleanly instead of hanging.
    ok("revoke notification", slot(ntfn).revoke());
    ok("delete notification", slot(ntfn).delete());
    sys::yield_now();
    let ended = WAIT_ERROR.load(Ordering::Relaxed);
    assert_eq!(
        ended,
        error::FAILED_LOOKUP,
        "the waiter should see its capability gone"
    );

    ok("revoke", slot(parent).revoke());
    println!("notifications, badges, waiting, suspension and deletion behave");
}
