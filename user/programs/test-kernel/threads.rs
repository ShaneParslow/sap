//! Threads and scheduling.

use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{BootInfo, error, object, reg};
use sap_rt::{println, sys, tcb};

use crate::support::{
    CNODE, TCB, VSPACE, fails, make, make_many, map_data, map_tables, ok, slot, untyped_at_least,
};

/// Counted up by the second thread; both threads share this address space.
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Spins this long, at most, waiting for the other thread to run.
const SPIN_LIMIT: usize = 1 << 30;

const STACK_VA: usize = 0x5000_0000;
const STACK_PAGES: usize = 4;
const STACK_TOP: usize = STACK_VA + STACK_PAGES * 0x1000;

extern "C" fn worker(step: usize) -> ! {
    loop {
        COUNTER.fetch_add(step, Ordering::Relaxed);
    }
}

fn count() -> usize {
    COUNTER.load(Ordering::Relaxed)
}

/// Spin, without yielding, until the counter moves or the limit passes.
/// Returns whether it moved.
fn counter_moves() -> bool {
    let start = count();
    (0..SPIN_LIMIT).any(|_| count() != start)
}

fn yield_a_few_times() {
    for _ in 0..5 {
        sys::yield_now();
    }
}

/// Start a second thread in this address space and check priorities,
/// preemption, suspension, register access and deletion.
pub fn scheduling(info: &BootInfo) {
    let base = info.empty.start + 48;
    let parent = untyped_at_least(info, 16);
    let thread = base;
    let tables = base + 1;
    let stack = base + 3;

    // The thread, two page tables and a 16 KiB stack.
    make(parent, object::TCB, thread);
    make_many(parent, object::PAGE_TABLE, tables, 2);
    make_many(parent, object::FRAME_4K, stack, STACK_PAGES);
    map_tables(tables, STACK_VA);
    for page in 0..STACK_PAGES {
        map_data(stack + page, STACK_VA + page * 0x1000);
    }

    let non_root = tcb::configure(thread, CNODE, tables, None);
    fails(
        "configure with a non-root",
        non_root,
        error::INVALID_CAPABILITY,
    );
    ok("configure", tcb::configure(thread, CNODE, VSPACE, None));

    let above = tcb::set_priority(thread, thread, 255);
    fails("priority above authority", above, error::RANGE_ERROR);
    ok("priority 100", tcb::set_priority(thread, TCB, 100));

    let of_self = tcb::write_registers(TCB, false, &[0; reg::COUNT]);
    fails("registers of self", of_self, error::ILLEGAL_OPERATION);

    let entry = (worker as *const ()).addr();
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = entry;
    regs[reg::SP] = STACK_TOP;
    regs[reg::A0] = 1;
    ok("start", tcb::write_registers(thread, true, &regs));

    // A lower-priority thread never runs while this one is runnable, even
    // across yields.
    yield_a_few_times();
    assert_eq!(count(), 0, "a lower-priority thread ran");

    // At equal priority the threads share the CPU by time slice: this one
    // never yields, so the counter can only move through preemption.
    ok("priority 255", tcb::set_priority(thread, TCB, 255));
    assert!(counter_moves(), "the other thread never ran");

    // Suspended, it stops.
    ok("suspend", tcb::suspend(thread));
    let stopped = count();
    yield_a_few_times();
    assert_eq!(count(), stopped, "a suspended thread ran");

    // Its registers show where it stopped: inside `worker`, on its stack.
    let regs = ok("read registers", tcb::read_registers(thread));
    let pc = regs[reg::PC];
    let sp = regs[reg::SP];
    assert!(
        (entry..entry + 0x100).contains(&pc),
        "pc {pc:#x} not in worker at {entry:#x}"
    );
    assert!(
        (STACK_VA..=STACK_TOP).contains(&sp),
        "sp {sp:#x} not on its stack"
    );

    // Resumed, it runs again; deleted, it is gone for good.
    ok("resume", tcb::resume(thread));
    assert!(counter_moves(), "the resumed thread never ran");
    ok("delete", slot(thread).delete());
    let after = count();
    yield_a_few_times();
    assert_eq!(count(), after, "a deleted thread ran");

    ok("revoke", slot(parent).revoke());
    println!("threads, priorities, preemption, suspension and deletion behave");
}
