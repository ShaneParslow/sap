//! The runtime (sap-rt): its heap and mutexes.

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};
use sap_process::process_slot;
use sap_rt::abi::{object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{account, heap, notification, println, slots, sync, sys, tcb, untyped};
use crate::{Env, expect, slot, untyped_at_least};
use crate::threads::spawn;

const HEAP_ROUNDS: usize = 40_000;
static HEAP_SUMS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];

/// Allocate and free vectors of varying length, and record their total.
extern "C" fn heap_worker(which: usize, done: usize, me: usize) -> ! {
    let mut total = 0;
    for i in 0..HEAP_ROUNDS {
        let v: Vec<usize> = core::hint::black_box((0..i % 64 + 1).map(|k| k + which).collect());
        total += v.iter().sum::<usize>();
    }
    HEAP_SUMS[which].store(total, Ordering::Relaxed);
    let _ = notification::signal(done);
    loop {
        let _ = tcb::suspend(me);
    }
}

static HANDOFF: sync::Mutex<usize> = sync::Mutex::new(0);

/// Take `HANDOFF` (sleeping while init holds it), set it, and say so.
extern "C" fn handoff_worker(done: usize, me: usize, _: usize) -> ! {
    *HANDOFF.lock() = 7;
    let _ = notification::signal(done);
    loop {
        let _ = tcb::suspend(me);
    }
}

/// The heap: nothing mapped before the first allocation, growth across page
/// tables and chunks of the account, charged to it, alignment, reuse of
/// freed memory, and two threads allocating at once through the heap's
/// lock.
pub fn check_heap(info: &Env) {
    assert_eq!(heap::mapped(), 0, "nothing should be mapped before the first allocation");
    let before = account::usage(info.account).expect("check's account").used;
    // black_box: the compiler may otherwise remove an allocation it sees
    // freed unused.
    let boxed = core::hint::black_box(Box::new(41usize));
    assert_eq!(*boxed + 1, 42);
    let first = heap::mapped();
    assert!(first >= 64 * 1024 && first % 4096 == 0, "the first growth should map at least 64 KiB");

    // About 2.3 MiB: more than one level-0 page table covers.
    let big: Vec<usize> = core::hint::black_box((0..300_000).collect());
    assert_eq!(big.iter().sum::<usize>(), 300_000 * 299_999 / 2);
    assert!(heap::mapped() > 2 << 20, "the heap should have grown past 2 MiB");
    let charged = account::usage(info.account).expect("check's account").used - before;
    assert!(charged >= heap::mapped() as u64 - 64 * 1024, "the heap's growth is charged to the account ({charged} bytes)");
    let layout = Layout::from_size_align(100, 4096).unwrap();
    // SAFETY: a non-zero size; freed with the same layout.
    let aligned = core::hint::black_box(unsafe { alloc::alloc::alloc(layout) });
    assert!(!aligned.is_null() && aligned.addr() % 4096 == 0, "a page-aligned allocation");
    // SAFETY: allocated above with this layout.
    unsafe { alloc::alloc::dealloc(aligned, layout) };
    assert_eq!(format!("{} and {}", "text", 2), "text and 2");
    let grown = heap::mapped();
    drop(big);
    let again: Vec<usize> = core::hint::black_box((0..300_000).collect());
    assert_eq!(heap::mapped(), grown, "freed memory should be reused");
    drop(again);

    // Two threads allocating at once. They share init's priority, so the
    // timer interleaves them, sometimes while one holds the lock.
    let parent = untyped_at_least(info, 18);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let done = slots::alloc_range(3).expect("slots");
    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, done, 1), Ok(()));
    for which in 0..2 {
        let badged = done + 1 + which;
        expect("badge", slot(badged).mint_from(slot(done), rights::WRITE, 1 << which), Ok(()));
        let thread_slots = slots::alloc_range(6).expect("slots");
        spawn(parent, thread_slots, 0x0c_0000_0000 + which * 0x4000_0000, heap_worker, [which, badged, thread_slots]);
    }
    let mut finished = 0;
    while finished != 0b11 {
        finished |= notification::wait(done).expect("wait for the workers");
    }
    for which in 0..2 {
        let expected: usize = (0..HEAP_ROUNDS).map(|i| (i % 64 + 1) * (i % 64) / 2 + (i % 64 + 1) * which).sum();
        assert_eq!(HEAP_SUMS[which].load(Ordering::Relaxed), expected, "worker {which}'s total");
    }

    // A thread that finds a mutex held sleeps on its notification, and the
    // unlock wakes it.
    let (lock_ntfn, handoff_done) = (slots::alloc().expect("slot"), slots::alloc().expect("slot"));
    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, lock_ntfn, 1), Ok(()));
    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, handoff_done, 1), Ok(()));
    HANDOFF.set_notification(lock_ntfn);
    let held = HANDOFF.lock();
    let before = sync::sleeps();
    let thread_slots = slots::alloc_range(6).expect("slots");
    spawn(parent, thread_slots, 0x0e_0000_0000, handoff_worker, [handoff_done, thread_slots, 0]);
    while sync::sleeps() == before {
        sys::yield_now();
    }
    assert_eq!(*held, 0, "the waiting thread got in while init held the lock");
    drop(held);
    expect("handoff", notification::wait(handoff_done).map(drop), Ok(()));
    assert_eq!(*HANDOFF.lock(), 7, "the woken thread should have taken the lock");
    println!("heap grows, aligns, reuses memory and serves two threads ({} KiB mapped); a mutex sleeps and wakes", heap::mapped() / 1024);
}
