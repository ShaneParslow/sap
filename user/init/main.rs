//! The first user program: the root task. It will become the process
//! manager (M7); for now it checks the kernel features as they arrive.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicUsize, Ordering};

use libsap::abi::{attr, error, fault, init_slot, object, recv_source, reg, rights, BootInfo};
use libsap::cnode::SlotRef;
use libsap::sys::Error;
use libsap::process::{self, Parent, Slots};
use libsap::{debug, ipc, notification, println, sys, tcb, untyped, vspace};

/// Slot `i` of the root CNode.
fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

#[unsafe(no_mangle)]
fn sap_main(boot_info: usize, _: usize, _: usize) -> ! {
    // SAFETY: the kernel passes the address of the read-only boot-info page.
    let info = unsafe { &*(boot_info as *const BootInfo) };
    report(info);
    check_cnode_operations(info);
    check_retype(info);
    check_mapping(info);
    check_threads(info);
    check_notifications(info);
    check_ipc(info);
    check_cap_transfer(info);
    check_faults(info);
    check_process(info);
    println!("init: all checks passed");
    debug::halt(false)
}

fn report(info: &BootInfo) {
    let untyped = &info.untyped_list[..info.untyped.len()];
    let bytes: usize = untyped.iter().map(|u| 1usize << u.size_bits).sum();
    let largest = untyped.iter().map(|u| u.size_bits).max().unwrap_or(0);
    println!(
        "init: {} image frames, {} page tables, {} untyped ({} MiB, largest 2^{}), slots {}..{} free",
        info.image_frames.len(),
        info.image_paging.len(),
        untyped.len(),
        bytes >> 20,
        largest,
        info.empty.start,
        info.empty.end
    );
}

fn expect(what: &str, result: Result<(), Error>, wanted: Result<(), usize>) {
    let got = result.map_err(|e| e.0);
    if got != wanted {
        panic!("{what}: got {got:?}, wanted {wanted:?}");
    }
}

/// Copy, move, delete and revoke capabilities, and check the errors for
/// misuse.
fn check_cnode_operations(info: &BootInfo) {
    let (a, b, c) = (slot(info.empty.start), slot(info.empty.start + 1), slot(info.empty.start + 2));
    let tcb = slot(init_slot::TCB);
    let untyped = slot(info.untyped.start);

    expect("copy tcb", a.copy_from(tcb, rights::ALL), Ok(()));
    expect("copy into occupied slot", a.copy_from(tcb, rights::ALL), Err(error::DELETE_FIRST));
    expect("copy the copy", b.copy_from(a, rights::ALL), Ok(()));
    expect("move", c.move_from(b), Ok(()));
    expect("copy from emptied slot", b.copy_from(b, rights::ALL), Err(error::FAILED_LOOKUP));
    expect("delete", c.delete(), Ok(()));
    expect("delete", a.delete(), Ok(()));
    expect("delete empty slot", a.delete(), Ok(()));

    // A copy of untyped memory is its child: revoking the original deletes
    // it.
    expect("copy untyped", a.copy_from(untyped, rights::ALL), Ok(()));
    expect("copy untyped copy", b.copy_from(a, rights::ALL), Ok(()));
    expect("revoke untyped", untyped.revoke(), Ok(()));
    expect("revoked copy is gone", c.copy_from(a, rights::ALL), Err(error::FAILED_LOOKUP));
    expect("revoked grandchild is gone", c.copy_from(b, rights::ALL), Err(error::FAILED_LOOKUP));

    // Addresses that don't resolve.
    expect("slot out of range", a.copy_from(slot(1 << info.cnode_radix), rights::ALL), Err(error::FAILED_LOOKUP));
    expect("wrong depth", a.copy_from(SlotRef { depth: 8, ..tcb }, rights::ALL), Err(error::FAILED_LOOKUP));
    println!("init: CNode copy, move, delete and revoke behave");
}

/// Retype untyped memory into objects, run it out, and get it back by
/// revoking.
fn check_retype(info: &BootInfo) {
    let empty = info.empty.start + 8;
    // A 64 KiB untyped region carved from the first big enough block, so
    // running out is quick.
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 16)
        .expect("no untyped region of 64 KiB");
    let parent = info.untyped.start + index;
    let region = empty;
    // Depth 0 names the root CNode itself as the destination.
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    expect("carve 64 KiB", untyped::retype(parent, object::UNTYPED, 16, in_root, region, 1), Ok(()));

    // A CNode with 16 slots, a TCB, a page table and four frames.
    let cnode = region + 1;
    expect("retype cnode", untyped::retype(region, object::CNODE, 4, in_root, cnode, 1), Ok(()));
    expect("retype tcb", untyped::retype(region, object::TCB, 0, in_root, cnode + 1, 1), Ok(()));
    expect("retype page table", untyped::retype(region, object::PAGE_TABLE, 0, in_root, cnode + 2, 1), Ok(()));
    expect("retype frames", untyped::retype(region, object::FRAME_4K, 0, in_root, cnode + 3, 4), Ok(()));
    expect("retype into occupied slot", untyped::retype(region, object::FRAME_4K, 0, in_root, cnode, 1), Err(error::DELETE_FIRST));
    expect("retype unknown type", untyped::retype(region, 99, 0, in_root, cnode + 7, 1), Err(error::INVALID_ARGUMENT));
    expect("copy untyped with children", slot(cnode + 7).copy_from(slot(region), rights::ALL), Err(error::REVOKE_FIRST));

    // Capabilities can go into the new CNode, addressed through it: its
    // capability is at slot `cnode` of the root, and its own slots take 4
    // bits.
    let inner = SlotRef { root: cnode, index: 3, depth: 4 };
    expect("copy into the new cnode", inner.copy_from(slot(init_slot::TCB), rights::ALL), Ok(()));

    // Fill the rest of the 64 KiB with frames. The CNode (1 KiB) and TCB
    // (1 KiB) share the first page; the page table and four frames take
    // five more; ten pages remain.
    expect("fill with frames", untyped::retype(region, object::FRAME_4K, 0, in_root, cnode + 7, 10), Ok(()));
    expect("out of memory", untyped::retype(region, object::FRAME_4K, 0, in_root, cnode + 17, 1), Err(error::NOT_ENOUGH_MEMORY));

    // Revoking the region deletes every object made from it, including the
    // copy inside the new CNode, and the memory can be used again.
    expect("revoke region", slot(region).revoke(), Ok(()));
    expect("objects are gone", slot(cnode + 17).copy_from(slot(cnode), rights::ALL), Err(error::FAILED_LOOKUP));
    expect("reuse after revoke", untyped::retype(region, object::FRAME_4K, 0, in_root, cnode, 16), Ok(()));
    expect("revoke again", slot(region).revoke(), Ok(()));
    expect("revoke the carve", slot(parent).revoke(), Ok(()));
    println!("init: untyped retype, exhaustion and reuse after revoke behave");
}

/// Map page tables and frames into this address space and a new one, read
/// and write through the mappings, and check the misuse errors.
fn check_mapping(info: &BootInfo) {
    let base = info.empty.start + 32;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 22)
        .expect("no untyped region of 4 MiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let me = init_slot::VSPACE;

    // Three page tables (one per level below the root, and a spare), two
    // 4 KiB frames and a 2 MiB frame. The 2 MiB frame goes first so it is
    // aligned at the start of the region.
    let (big, l1, l0, spare, f1, f2) = (base, base + 1, base + 2, base + 3, base + 4, base + 5);
    expect("retype 2 MiB frame", untyped::retype(parent, object::FRAME_2M, 0, in_root, big, 1), Ok(()));
    expect("retype page tables", untyped::retype(parent, object::PAGE_TABLE, 0, in_root, l1, 3), Ok(()));
    expect("retype frames", untyped::retype(parent, object::FRAME_4K, 0, in_root, f1, 2), Ok(()));

    // 1 GiB up: nothing of init's is mapped there, so both tables are new.
    let va: usize = 0x4000_0000;
    let rw = rights::READ | rights::WRITE;
    expect("frame without tables", vspace::map_frame(f1, me, va, rw, attr::EXECUTE_NEVER), Err(error::FAILED_LOOKUP));
    expect("map level-1 table", vspace::map_page_table(l1, me, va), Ok(()));
    expect("map level-0 table", vspace::map_page_table(l0, me, va), Ok(()));
    expect("every level has a table", vspace::map_page_table(spare, me, va), Err(error::DELETE_FIRST));
    expect("map frame", vspace::map_frame(f1, me, va, rw, attr::EXECUTE_NEVER), Ok(()));
    expect("map frame twice", vspace::map_frame(f1, me, va + 0x1000, rw, 0), Err(error::INVALID_CAPABILITY));
    expect("map over a mapping", vspace::map_frame(f2, me, va, rw, 0), Err(error::DELETE_FIRST));
    expect("unaligned", vspace::map_frame(f2, me, va + 1, rw, 0), Err(error::ALIGNMENT_ERROR));
    expect("kernel address", vspace::map_frame(f2, me, 0xffff_ffc0_0000_0000, rw, 0), Err(error::INVALID_ARGUMENT));
    expect("write-only", vspace::map_frame(f2, me, va + 0x1000, rights::WRITE, 0), Err(error::INVALID_ARGUMENT));

    // Write through the mapping, move the frame to another address
    // read-only, and read the value back there.
    let word = va as *mut u64;
    // SAFETY: the frame was just mapped read-write at va.
    unsafe { word.write_volatile(0x5a9_cafe) };
    expect("unmap frame", vspace::unmap_frame(f1), Ok(()));
    expect("remap read-only", vspace::map_frame(f1, me, va + 0x3000, rights::READ, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: mapped read-only at va + 0x3000.
    let back = unsafe { ((va + 0x3000) as *const u64).read_volatile() };
    assert_eq!(back, 0x5a9_cafe, "value didn't survive remapping");

    // A 2 MiB frame goes straight into the level-1 table.
    let big_va = va + 0x20_0000;
    expect("map 2 MiB frame", vspace::map_frame(big, me, big_va, rw, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: mapped read-write.
    unsafe { ((big_va + 0x1f_fff8) as *mut u64).write_volatile(7) };
    let address = vspace::frame_address(big).expect("frame address");
    let region = info.untyped_list[index].paddr;
    assert!(address == region, "2 MiB frame at {address:#x}, region at {region:#x}");

    // A new address space: an unmapped page table becomes a root with an
    // ASID, and mappings go into it without touching this one.
    let root = base + 6;
    expect("retype root", untyped::retype(parent, object::PAGE_TABLE, 0, in_root, root, 3), Ok(()));
    expect("map into non-root", vspace::map_page_table(base + 7, root, va), Err(error::INVALID_CAPABILITY));
    expect("assign asid", vspace::assign_asid(init_slot::ASID_POOL, root), Ok(()));
    expect("assign twice", vspace::assign_asid(init_slot::ASID_POOL, root), Err(error::INVALID_CAPABILITY));
    expect("map table in new space", vspace::map_page_table(base + 7, root, va), Ok(()));
    expect("map table in new space", vspace::map_page_table(base + 8, root, va), Ok(()));
    expect("map frame in new space", vspace::map_frame(f2, root, va, rw, 0), Ok(()));

    // Unmapping a page table detaches what is mapped through it: the frame
    // mapped through it must be unmapped and can't be mapped again until a
    // table is back.
    expect("unmap level-0 table", vspace::unmap_page_table(l0), Ok(()));
    expect("unmap frame", vspace::unmap_frame(f1), Ok(()));
    expect("frame needs the table again", vspace::map_frame(f1, me, va, rw, 0), Err(error::FAILED_LOOKUP));
    expect("map the spare table", vspace::map_page_table(spare, me, va), Ok(()));
    expect("map frame again", vspace::map_frame(f1, me, va, rights::READ, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: mapped read-only; a fresh table, the same frame.
    let again = unsafe { (va as *const u64).read_volatile() };
    assert_eq!(again, 0x5a9_cafe, "value didn't survive a new table");

    // Revoking the region deletes every object: frames and tables are
    // unmapped, and the new address space's ASID is freed.
    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("init: page tables, frames, ASIDs and unmapping behave");
}

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
fn check_threads(info: &BootInfo) {
    let base = info.empty.start + 48;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 16)
        .expect("no untyped region of 64 KiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let (me, my_tcb) = (init_slot::VSPACE, init_slot::TCB);

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

    expect("configure with a non-root", tcb::configure(thread, init_slot::CNODE, tables, None), Err(error::INVALID_CAPABILITY));
    expect("configure", tcb::configure(thread, init_slot::CNODE, me, None), Ok(()));
    expect("priority above authority", tcb::set_priority(thread, thread, 255), Err(error::RANGE_ERROR));
    expect("priority 100", tcb::set_priority(thread, my_tcb, 100), Ok(()));
    expect("registers of self", tcb::write_registers(my_tcb, false, &[0; reg::COUNT]), Err(error::ILLEGAL_OPERATION));

    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = worker as *const () as usize;
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
    let entry = worker as *const () as usize;
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
    println!("init: threads, priorities, preemption, suspension and deletion behave");
}

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
fn check_notifications(info: &BootInfo) {
    let base = info.empty.start + 64;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 16)
        .expect("no untyped region of 64 KiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let me = init_slot::VSPACE;
    let (ntfn, one, two, one_copy) = (base, base + 1, base + 2, base + 3);

    expect("retype notification", untyped::retype(parent, object::NOTIFICATION, 0, in_root, ntfn, 1), Ok(()));
    expect("badge 1", slot(one).mint_from(slot(ntfn), rights::WRITE, 0b01), Ok(()));
    expect("badge 2", slot(two).mint_from(slot(ntfn), rights::WRITE, 0b10), Ok(()));
    expect("re-badge", slot(base + 4).mint_from(slot(one), rights::WRITE, 0b100), Err(error::ILLEGAL_OPERATION));

    // With nobody waiting, signals accumulate and a poll takes them all.
    expect("signal 1", notification::signal(one), Ok(()));
    expect("signal 2", notification::signal(two), Ok(()));
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(0b11), "poll should see both badges");
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(0), "poll should have cleared the word");
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
    expect("configure", tcb::configure(thread, init_slot::CNODE, me, None), Ok(()));
    expect("priority", tcb::set_priority(thread, init_slot::TCB, 255), Ok(()));
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = waiter as *const () as usize;
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
    println!("init: notifications, badges, waiting, suspension and deletion behave");
}

/// Start a thread in this address space at `entry(args)`, with a 4 KiB
/// stack and an IPC buffer in a fresh 1 GiB-aligned region at `va`. Uses
/// six slots from `slots` and memory from `untyped`. Returns the TCB slot.
/// The thread starts at once unless `start` is false.
fn spawn_with(untyped: usize, slots: usize, va: usize, entry: extern "C" fn(usize, usize, usize) -> !, args: [usize; 3], start: bool) -> usize {
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let (thread, tables, frames) = (slots, slots + 1, slots + 3);
    let me = init_slot::VSPACE;
    let rw = rights::READ | rights::WRITE;
    expect("spawn: tcb", untyped::retype(untyped, object::TCB, 0, in_root, thread, 1), Ok(()));
    expect("spawn: tables", untyped::retype(untyped, object::PAGE_TABLE, 0, in_root, tables, 2), Ok(()));
    expect("spawn: frames", untyped::retype(untyped, object::FRAME_4K, 0, in_root, frames, 2), Ok(()));
    expect("spawn: map", vspace::map_page_table(tables, me, va), Ok(()));
    expect("spawn: map", vspace::map_page_table(tables + 1, me, va), Ok(()));
    expect("spawn: stack", vspace::map_frame(frames, me, va, rw, attr::EXECUTE_NEVER), Ok(()));
    let buffer = va + 0x2000;
    expect("spawn: buffer", vspace::map_frame(frames + 1, me, buffer, rw, attr::EXECUTE_NEVER), Ok(()));
    expect("spawn: configure", tcb::configure(thread, init_slot::CNODE, me, Some((frames + 1, buffer))), Ok(()));
    expect("spawn: priority", tcb::set_priority(thread, init_slot::TCB, 255), Ok(()));
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = entry as *const () as usize;
    regs[reg::SP] = va + 0x1000;
    regs[reg::TP] = buffer;
    regs[reg::A0..reg::A0 + 3].copy_from_slice(&args);
    expect("spawn: registers", tcb::write_registers(thread, start, &regs), Ok(()));
    thread
}

fn spawn(untyped: usize, slots: usize, va: usize, entry: extern "C" fn(usize, usize, usize) -> !, args: [usize; 3]) -> usize {
    spawn_with(untyped, slots, va, entry, args, true)
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
fn check_ipc(info: &BootInfo) {
    let base = info.empty.start + 96;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 18)
        .expect("no untyped region of 256 KiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
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
    println!("init: endpoints, calls, replies, bound notifications and deletion behave");
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
        ipc::set_receive_window(init_slot::CNODE, KEEP_INDEX.load(Ordering::Relaxed), KEEP_COUNT.load(Ordering::Relaxed));
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
fn check_cap_transfer(info: &BootInfo) {
    let base = info.empty.start + 160;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 18)
        .expect("no untyped region of 256 KiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
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
    assert_eq!(notification::poll(ntfn).map_err(|e| e.0), Ok(0), "unbadged copy signals 0");
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
    ipc::set_receive_window(init_slot::CNODE, mine, 4);
    let back = ipc::call(granting, 2, &[], &[ipc::Give::copy(ntfn)], &mut []).expect("call with a capability");
    assert_eq!(back.info.extra_caps(), 1, "the reply should carry the capability back");
    assert!(filled(mine).is_ok(), "the returned capability isn't in this thread's window");
    assert!(filled(window).is_err(), "the keeper kept the capability it moved back");
    let _ = slot(mine).delete();
    let refused = ipc::call(no_reply_grant, 2, &[], &[ipc::Give::copy(ntfn)], &mut []).expect("call without grant-reply");
    assert_eq!(refused.info.extra_caps(), 0, "a reply carried capabilities without GRANT_REPLY");
    clear();

    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("init: capability transfer, grant rights and receive windows behave");
}

/// What the faulting thread read after its handler fixed the fault.
static FAULT_READ: AtomicUsize = AtomicUsize::new(0);

/// Mode 0: read `address` (which faults until a handler maps it) and record
/// the value. Mode 1: run an illegal instruction. Then suspend itself.
extern "C" fn faulter(address: usize, mode: usize, me: usize) -> ! {
    if mode == 0 {
        // SAFETY: deliberately touches an address that may be unmapped; the
        // fault handler maps it before this thread resumes.
        let value = unsafe { core::ptr::read_volatile(address as *const usize) };
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
fn check_faults(info: &BootInfo) {
    let base = info.empty.start + 240;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 18)
        .expect("no untyped region of 256 KiB");
    let parent = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let me = init_slot::VSPACE;
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
    let entry = faulter as *const () as usize;
    assert!((entry..entry + 0x100).contains(&words[fault::PC]), "fault pc not in faulter");
    // Fix it: map a page there with a value in it, and resume the thread.
    expect("map the page", vspace::map_frame(page, me, target, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: just mapped read-write.
    unsafe { (target as *mut usize).write_volatile(0xfa017) };
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
    println!("init: faults delivered, fixed, refused and unhandled behave");
}

/// The second program, built by this crate's build script.
static PEER: &[u8] = include_bytes!(env!("PEER_ELF"));

/// Load the peer into a process of its own and talk to it: a call across
/// address spaces, and its attempt to read this process's memory, which
/// faults and is reported here.
fn check_process(info: &BootInfo) {
    let base = info.empty.start + 300;
    let (index, _) = info.untyped_list[..info.untyped.len()]
        .iter()
        .enumerate()
        .find(|(_, u)| u.size_bits >= 20)
        .expect("no untyped region of 1 MiB");
    let parent_untyped = info.untyped.start + index;
    let in_root = SlotRef { root: init_slot::CNODE, index: 0, depth: 0 };
    let (ep, reply, badged) = (base, base + 1, base + 2);
    expect("retype endpoint", untyped::retype(parent_untyped, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent_untyped, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("badge", slot(badged).mint_from(slot(ep), rights::WRITE | rights::GRANT, 0x9ee), Ok(()));

    // The scratch region is where this process fills pages; the peer will
    // try to read it.
    let scratch = 0x38_0000_0000;
    let parent = Parent {
        cnode: init_slot::CNODE,
        vspace: init_slot::VSPACE,
        asid_pool: init_slot::ASID_POOL,
        authority: init_slot::TCB,
        untyped: parent_untyped,
        scratch,
    };
    let mut slots = Slots { next: base + 3, end: base + 200 };
    // The peer's slot 1 is its endpoint, slot 2 its own thread.
    let peer = process::load(PEER, &parent, &mut slots, 255, [1, 2, 0]);
    peer.give(&parent, 1, badged, rights::WRITE | rights::GRANT);
    peer.give(&parent, 2, peer.tcb, rights::ALL);
    expect("fault handler", tcb::set_fault_handler(peer.tcb, Some(badged)), Ok(()));
    // Something mapped at the scratch address, so the peer's read would
    // succeed if it could see this address space.
    expect("scratch page", untyped::retype(parent_untyped, object::FRAME_4K, 0, in_root, slots.next, 1), Ok(()));
    let marker = slots.next;
    expect("map scratch", vspace::map_frame(marker, init_slot::VSPACE, scratch, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    peer.start();

    let mut words = [0usize; 8];
    let first = ipc::recv(ep, reply, &mut words).expect("receive the peer's call");
    assert_eq!((first.badge, first.info.label(), first.info.length()), (0x9ee, 1, 6), "the peer's call");
    assert_eq!(words[..6], [1, 2, 3, 4, 5, 6], "the peer's words");
    let doubled = [2, 4, 6, 8, 10, 12];
    let second = ipc::reply_recv(ep, reply, 0, &doubled, &[], &mut words).expect("receive the peer's report");
    assert_eq!((second.info.label(), words[0]), (2, 42), "the peer should report the sum of the reply");
    let fault_msg = ipc::reply_recv(ep, reply, 0, &[], &[], &mut words).expect("receive the peer's fault");
    assert_eq!((fault_msg.info.label(), words[fault::ADDRESS]), (fault::VM, scratch), "the peer's read should fault");
    expect("leave it suspended", ipc::reply(reply, 1, &[], &[]), Ok(()));

    expect("unmap scratch", vspace::unmap_frame(marker), Ok(()));
    expect("revoke", slot(parent_untyped).revoke(), Ok(()));
    println!("init: a second process, calls between address spaces and isolation behave");
}
