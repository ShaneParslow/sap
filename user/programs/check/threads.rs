//! Threads and scheduling, and the helpers the other checks start threads
//! with.

use core::sync::atomic::{AtomicUsize, Ordering};
use sap_process::process_slot;
use sap_rt::abi::{attr, error, object, reg, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{println, sys, tcb, untyped, vspace};
use crate::{Env, expect, slot, untyped_at_least};

/// Counted up by the second thread; both threads share this address space.
static COUNTER: AtomicUsize = AtomicUsize::new(0);

extern "C" fn worker(step: usize) -> ! {
    loop {
        COUNTER.fetch_add(step, Ordering::Relaxed);
    }
}

/// Spin, without yielding, until the counter moves or `limit` iterations
/// pass. Returns whether it moved.
fn counter_moves(limit: usize) -> bool {
    let start = COUNTER.load(Ordering::Relaxed);
    (0..limit).any(|_| COUNTER.load(Ordering::Relaxed) != start)
}

/// Start a second thread in this address space and check priorities,
/// preemption, suspension, register access and deletion.
pub fn check_threads(info: &Env) {
    let base = info.empty.start + 48;
    let parent = untyped_at_least(info, 16);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (me, my_tcb) = (process_slot::VSPACE, process_slot::TCB);

    // The thread, two page tables and a 16 KiB stack at 0x5000_0000.
    let (thread, tables, stack) = (base, base + 1, base + 3);
    expect("retype tcb", untyped::retype(parent, object::TCB, 0, in_root, thread, 1), Ok(()));
    expect("retype tables", untyped::retype(parent, object::PAGE_TABLE, 0, in_root, tables, 2), Ok(()));
    expect("retype stack", untyped::retype(parent, object::FRAME_4K, 0, in_root, stack, 4), Ok(()));
    let stack_va: usize = 0x5000_0000;
    expect("map table", vspace::map_page_table(tables, me, stack_va), Ok(()));
    expect("map table", vspace::map_page_table(tables + 1, me, stack_va), Ok(()));
    for i in 0..4 {
        let rw = rights::READ | rights::WRITE;
        expect("map stack", vspace::map_frame(stack + i, me, stack_va + i * 0x1000, rw, attr::EXECUTE_NEVER), Ok(()));
    }

    expect("configure with a non-root", tcb::configure(thread, process_slot::CNODE, tables, None), Err(error::INVALID_CAPABILITY));
    expect("configure", tcb::configure(thread, process_slot::CNODE, me, None), Ok(()));
    expect("priority above authority", tcb::set_priority(thread, thread, 255), Err(error::RANGE_ERROR));
    expect("priority 100", tcb::set_priority(thread, my_tcb, 100), Ok(()));
    expect("registers of self", tcb::write_registers(my_tcb, false, &[0; reg::COUNT]), Err(error::ILLEGAL_OPERATION));

    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = (worker as *const ()).addr();
    regs[reg::SP] = stack_va + 0x4000;
    regs[reg::A0] = 1;
    expect("start", tcb::write_registers(thread, true, &regs), Ok(()));

    // A lower-priority thread never runs while this one is runnable, even
    // across yields.
    for _ in 0..5 {
        sys::yield_now();
    }
    assert_eq!(COUNTER.load(Ordering::Relaxed), 0, "a lower-priority thread ran");

    // At equal priority the threads share the CPU by time slice: this one
    // never yields, so the counter can only move through preemption.
    expect("priority 255", tcb::set_priority(thread, my_tcb, 255), Ok(()));
    assert!(counter_moves(1 << 30), "the other thread never ran");

    // Suspended, it stops.
    expect("suspend", tcb::suspend(thread), Ok(()));
    let stopped = COUNTER.load(Ordering::Relaxed);
    for _ in 0..5 {
        sys::yield_now();
    }
    assert_eq!(COUNTER.load(Ordering::Relaxed), stopped, "a suspended thread ran");

    // Its registers show where it stopped: inside `worker`, on its stack.
    let regs = tcb::read_registers(thread).expect("read registers");
    let (pc, sp) = (regs[reg::PC], regs[reg::SP]);
    let entry = (worker as *const ()).addr();
    assert!((entry..entry + 0x100).contains(&pc), "pc {pc:#x} not in worker at {entry:#x}");
    assert!((stack_va..=stack_va + 0x4000).contains(&sp), "sp {sp:#x} not on its stack");

    // Resumed, it runs again; deleted, it is gone for good.
    expect("resume", tcb::resume(thread), Ok(()));
    assert!(counter_moves(1 << 30), "the resumed thread never ran");
    expect("delete", slot(thread).delete(), Ok(()));
    let after = COUNTER.load(Ordering::Relaxed);
    for _ in 0..5 {
        sys::yield_now();
    }
    assert_eq!(COUNTER.load(Ordering::Relaxed), after, "a deleted thread ran");
    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("threads, priorities, preemption, suspension and deletion behave");
}

/// Start a thread in this address space at `entry(args)`, with a 4 KiB
/// stack and an IPC buffer in a fresh 1 GiB-aligned region at `va`. Uses
/// six slots from `slots` and memory from `untyped`. Returns the TCB slot.
/// The thread starts at once unless `start` is false.
pub fn spawn_with(untyped: usize, slots: usize, va: usize, entry: extern "C" fn(usize, usize, usize) -> !, args: [usize; 3], start: bool) -> usize {
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (thread, tables, frames) = (slots, slots + 1, slots + 3);
    let me = process_slot::VSPACE;
    let rw = rights::READ | rights::WRITE;
    expect("spawn: tcb", untyped::retype(untyped, object::TCB, 0, in_root, thread, 1), Ok(()));
    expect("spawn: tables", untyped::retype(untyped, object::PAGE_TABLE, 0, in_root, tables, 2), Ok(()));
    expect("spawn: frames", untyped::retype(untyped, object::FRAME_4K, 0, in_root, frames, 2), Ok(()));
    expect("spawn: map", vspace::map_page_table(tables, me, va), Ok(()));
    expect("spawn: map", vspace::map_page_table(tables + 1, me, va), Ok(()));
    expect("spawn: stack", vspace::map_frame(frames, me, va, rw, attr::EXECUTE_NEVER), Ok(()));
    let buffer = va + 0x2000;
    expect("spawn: buffer", vspace::map_frame(frames + 1, me, buffer, rw, attr::EXECUTE_NEVER), Ok(()));
    expect("spawn: configure", tcb::configure(thread, process_slot::CNODE, me, Some((frames + 1, buffer))), Ok(()));
    expect("spawn: priority", tcb::set_priority(thread, process_slot::TCB, 255), Ok(()));
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = (entry as *const ()).addr();
    regs[reg::SP] = va + 0x1000;
    regs[reg::TP] = buffer;
    regs[reg::A0..reg::A0 + 3].copy_from_slice(&args);
    expect("spawn: registers", tcb::write_registers(thread, start, &regs), Ok(()));
    thread
}

pub fn spawn(untyped: usize, slots: usize, va: usize, entry: extern "C" fn(usize, usize, usize) -> !, args: [usize; 3]) -> usize {
    spawn_with(untyped, slots, va, entry, args, true)
}
