//! The kernel checks: the program the root task starts at boot to check
//! the kernel's features as they arrive. Its bootstrap message
//! (`CheckImports` in `interfaces/check.capnp`) gives it the authority the
//! root task holds at boot: untyped memory and the ASID pool. It reports
//! success by exiting with code 0; a failed check panics, which halts the
//! machine.

#![no_std]
#![no_main]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};

use sap_rt::abi::{attr, error, fault, identify, object, process_slot, recv_source, reg, rights, SlotRange, Startup, UntypedDesc, MAX_BOOT_UNTYPED};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::sys::Error;
use sap_rt::runtime::{self, Resources};
use sap_rt::{heap, ipc, notification, println, process, slots, sync, sys, tcb, untyped, vspace};
use capnp::message::{Builder, Reader, ReaderOptions, SegmentArray, SingleSegmentAllocator};
use sap_interfaces::check_capnp::check_imports;
use sap_interfaces::echo;
use sap_interfaces::echo_capnp::{reply, request};
use sap_interfaces::peer_capnp::{peer_imports, Mode};
use sap_loader::{self as loader, Give, Launch, Parent, Slots, LAUNCH};
use sap_ring::transfer::{Frames, Sender, TransferError, MAX_LEN, MAX_PAGES};
use sap_ring::{as_bytes, as_bytes_mut, Consumer, Layout as RingLayout, Producer, RingError, PAGE};

/// Slot `i` of this process's CNode.
fn slot(i: usize) -> SlotRef {
    SlotRef::new(process_slot::CNODE, i)
}

/// What the checks work with: the startup record and the bootstrap
/// message, laid out the way the kernel's boot information is, since the
/// checks were written for the root task.
struct Env {
    timer_hz: u64,
    cnode_radix: usize,
    /// Free slots.
    empty: SlotRange,
    asid_pool: usize,
    /// Untyped memory, in consecutive slots, described in order in
    /// `untyped_list`.
    untyped: SlotRange,
    untyped_list: [UntypedDesc; MAX_BOOT_UNTYPED],
}

/// Read the bootstrap message into an `Env`.
fn env(startup: &'static Startup) -> Result<Env, capnp::Error> {
    let segments = [process::bootstrap(startup)];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let imports = reader.get_root::<check_imports::Reader>()?;
    let import = |index: u32| process::import(startup, index).expect("check: no such import");
    let regions = imports.get_untyped()?;
    assert!(regions.len() as usize <= MAX_BOOT_UNTYPED, "check: too many untyped regions");
    let first = if regions.is_empty() { startup.free.start } else { import(regions.get(0).get_import()) };
    let mut untyped_list = [UntypedDesc { paddr: 0, size_bits: 0, is_device: 0, _pad: [0; 6] }; MAX_BOOT_UNTYPED];
    for (i, region) in regions.iter().enumerate() {
        assert_eq!(import(region.get_import()), first + i, "check: untyped imports must be consecutive");
        untyped_list[i] = UntypedDesc { paddr: region.get_paddr() as usize, size_bits: region.get_size_bits(), is_device: 0, _pad: [0; 6] };
    }
    Ok(Env {
        timer_hz: startup.timer_hz,
        cnode_radix: startup.cnode_radix,
        empty: startup.free,
        asid_pool: import(imports.get_asid_pool()),
        untyped: SlotRange { start: first, end: first + regions.len() as usize },
        untyped_list,
    })
}

/// Where the heap grows: a 1 GiB range nothing else here uses.
const HEAP: core::ops::Range<usize> = 0x08_0000_0000..0x08_4000_0000;
/// The checks hand out slots below this by hand; the runtime's slot
/// allocator has the rest.
const CHECK_SLOTS: usize = 1024;

/// The untyped region the runtime gets for the heap: the largest, but never
/// the first, which `check_cnode_operations` revokes. No check uses it.
fn runtime_untyped(info: &Env) -> usize {
    let list = &info.untyped_list[1..info.untyped.len()];
    let (i, _) = list.iter().enumerate().filter(|(_, u)| u.is_device == 0).max_by_key(|(_, u)| u.size_bits).expect("no untyped memory for the heap");
    info.untyped.start + 1 + i
}

/// The first untyped region of at least `2^bits` bytes, other than the
/// runtime's.
fn untyped_at_least(info: &Env, bits: u8) -> usize {
    let runtime = runtime_untyped(info);
    (info.untyped.start..info.untyped.end)
        .zip(&info.untyped_list)
        .find(|&(cptr, u)| u.size_bits >= bits && u.is_device == 0 && cptr != runtime)
        .map(|(cptr, _)| cptr)
        .unwrap_or_else(|| panic!("no untyped region of 2^{bits} bytes"))
}

#[unsafe(no_mangle)]
fn sap_main(a0: usize, _: usize, _: usize) -> ! {
    // SAFETY: the loader started this process with its startup record.
    let startup = unsafe { process::startup(a0) };
    let env = env(startup).expect("check: bootstrap message");
    let info = &env;
    runtime::start(Resources {
        cnode: process_slot::CNODE,
        slots: info.empty.start + CHECK_SLOTS..info.empty.end,
        vspace: process_slot::VSPACE,
        untyped: runtime_untyped(info),
        heap: HEAP,
    });
    check_cnode_operations(info);
    check_retype(info);
    check_mapping(info);
    check_threads(info);
    check_notifications(info);
    check_ipc(info);
    check_cap_transfer(info);
    check_faults(info);
    check_process(info);
    // Before anything else allocates: it checks that the heap starts empty.
    check_heap(info);
    check_ring(info);
    check_echo_service(info);
    measure_transfer(info);
    check_capnp();
    println!("check: all checks passed");
    process::exit(0)
}

fn expect(what: &str, result: Result<(), Error>, wanted: Result<(), usize>) {
    let got = result.map_err(|e| e.0);
    if got != wanted {
        panic!("{what}: got {got:?}, wanted {wanted:?}");
    }
}

/// Copy, move, delete and revoke capabilities, and check the errors for
/// misuse.
fn check_cnode_operations(info: &Env) {
    let (a, b, c) = (slot(info.empty.start), slot(info.empty.start + 1), slot(info.empty.start + 2));
    let tcb = slot(process_slot::TCB);
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
    println!("check: CNode copy, move, delete and revoke behave");
}

/// Retype untyped memory into objects, run it out, and get it back by
/// revoking.
fn check_retype(info: &Env) {
    let empty = info.empty.start + 8;
    // A 64 KiB untyped region carved from the first big enough block, so
    // running out is quick.
    let parent = untyped_at_least(info, 16);
    let region = empty;
    // Depth 0 names the root CNode itself as the destination.
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
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
    expect("copy into the new cnode", inner.copy_from(slot(process_slot::TCB), rights::ALL), Ok(()));

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

    // A copy of untyped memory takes the memory over: the original counts
    // as full until the copy and everything made through it are gone, so
    // the two can't hand out the same memory twice.
    let copy = cnode;
    expect("one frame", untyped::retype(region, object::FRAME_4K, 0, in_root, copy, 1), Ok(()));
    expect("revoke it", slot(region).revoke(), Ok(()));
    expect("copy the empty region", slot(copy).copy_from(slot(region), rights::ALL), Ok(()));
    expect("the original is full", untyped::retype(region, object::FRAME_4K, 0, in_root, copy + 1, 1), Err(error::NOT_ENOUGH_MEMORY));
    expect("the copy retypes", untyped::retype(copy, object::FRAME_4K, 0, in_root, copy + 1, 1), Ok(()));
    expect("delete the copy", slot(copy).delete(), Ok(()));
    expect("full while the copy's frame lives", untyped::retype(region, object::FRAME_4K, 0, in_root, copy + 2, 1), Err(error::NOT_ENOUGH_MEMORY));
    expect("revoke the frame", slot(region).revoke(), Ok(()));
    expect("the original retypes again", untyped::retype(region, object::FRAME_4K, 0, in_root, copy, 1), Ok(()));
    expect("revoke once more", slot(region).revoke(), Ok(()));
    expect("revoke the carve", slot(parent).revoke(), Ok(()));
    println!("check: untyped retype, exhaustion and reuse after revoke behave");
}

/// Map page tables and frames into this address space and a new one, read
/// and write through the mappings, and check the misuse errors.
fn check_mapping(info: &Env) {
    let base = info.empty.start + 32;
    let parent = untyped_at_least(info, 22);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let me = process_slot::VSPACE;

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
    let word = mapped::<u64>(va);
    // SAFETY: the frame was just mapped read-write at va.
    unsafe { word.write_volatile(0x5a9_cafe) };
    expect("unmap frame", vspace::unmap_frame(f1), Ok(()));
    expect("remap read-only", vspace::map_frame(f1, me, va + 0x3000, rights::READ, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: mapped read-only at va + 0x3000.
    let back = unsafe { mapped::<u64>(va + 0x3000).read_volatile() };
    assert_eq!(back, 0x5a9_cafe, "value didn't survive remapping");

    // Identifying capabilities without invoking them: the type, whether
    // the capability is the only one to its object (copies, including the
    // one a thread keeps of its IPC buffer, make it shared), and whether it
    // is mapped.
    let identify = |s: usize| slot(s).identify().map(|i| (i.object_type, i.flags)).map_err(|e| e.0);
    let (exclusive, is_mapped) = (identify::EXCLUSIVE, identify::MAPPED);
    assert_eq!(identify(f1), Ok((object::FRAME_4K, exclusive | is_mapped)), "a mapped frame with one capability");
    expect("copy a frame", slot(f2 + 1).copy_from(slot(f1), rights::ALL), Ok(()));
    assert_eq!(identify(f1), Ok((object::FRAME_4K, is_mapped)), "a copied frame isn't exclusive");
    assert_eq!(identify(f2 + 1), Ok((object::FRAME_4K, 0)), "nor is its copy, which starts unmapped");
    expect("delete the copy", slot(f2 + 1).delete(), Ok(()));
    assert_eq!(identify(f1), Ok((object::FRAME_4K, exclusive | is_mapped)), "exclusive again");
    assert_eq!(identify(process_slot::IPC_BUFFER), Ok((object::FRAME_4K, is_mapped)), "the TCB holds a copy of the IPC buffer");
    assert_eq!(identify(l0), Ok((object::PAGE_TABLE, exclusive | is_mapped)), "a mapped page table");
    assert_eq!(identify(big), Ok((object::FRAME_2M, exclusive)), "an unmapped 2 MiB frame");
    assert_eq!(identify(info.asid_pool).map(|i| i.0), Ok(object::ASID_POOL), "the ASID pool");
    assert_eq!(identify(parent).map(|i| i.0), Ok(object::UNTYPED), "untyped memory is never exclusive");
    assert_eq!(identify(parent).map(|i| i.1), Ok(0), "untyped memory is never exclusive");
    assert_eq!(identify(f2 + 1), Err(error::FAILED_LOOKUP), "an empty slot");

    // A 2 MiB frame goes straight into the level-1 table.
    let big_va = va + 0x20_0000;
    expect("map 2 MiB frame", vspace::map_frame(big, me, big_va, rw, attr::EXECUTE_NEVER), Ok(()));
    // SAFETY: mapped read-write.
    unsafe { mapped::<u64>(big_va + 0x1f_fff8).write_volatile(7) };
    let address = vspace::frame_address(big).expect("frame address");
    let region = info.untyped_list[parent - info.untyped.start].paddr;
    assert!(address == region, "2 MiB frame at {address:#x}, region at {region:#x}");

    // A new address space: an unmapped page table becomes a root with an
    // ASID, and mappings go into it without touching this one.
    let root = base + 6;
    expect("retype root", untyped::retype(parent, object::PAGE_TABLE, 0, in_root, root, 3), Ok(()));
    expect("map into non-root", vspace::map_page_table(base + 7, root, va), Err(error::INVALID_CAPABILITY));
    expect("assign asid", vspace::assign_asid(info.asid_pool, root), Ok(()));
    expect("assign twice", vspace::assign_asid(info.asid_pool, root), Err(error::INVALID_CAPABILITY));
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
    let again = unsafe { mapped::<u64>(va).read_volatile() };
    assert_eq!(again, 0x5a9_cafe, "value didn't survive a new table");

    // Revoking the region deletes every object: frames and tables are
    // unmapped, and the new address space's ASID is freed.
    expect("revoke", slot(parent).revoke(), Ok(()));
    println!("check: page tables, frames, ASIDs and unmapping behave");
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
fn check_threads(info: &Env) {
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
    println!("check: threads, priorities, preemption, suspension and deletion behave");
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
fn check_notifications(info: &Env) {
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

/// Start a thread in this address space at `entry(args)`, with a 4 KiB
/// stack and an IPC buffer in a fresh 1 GiB-aligned region at `va`. Uses
/// six slots from `slots` and memory from `untyped`. Returns the TCB slot.
/// The thread starts at once unless `start` is false.
fn spawn_with(untyped: usize, slots: usize, va: usize, entry: extern "C" fn(usize, usize, usize) -> !, args: [usize; 3], start: bool) -> usize {
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
fn check_ipc(info: &Env) {
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
fn check_cap_transfer(info: &Env) {
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
fn check_faults(info: &Env) {
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

/// The second program, built by this crate's build script.
static PEER: &[u8] = include_bytes!(env!("PEER_ELF"));

/// A peer's bootstrap message and its length, built without the heap
/// (which `check_heap` wants untouched): `mode`, then the indices of its
/// imports in the order `connect_peer` and `check_process` give them.
fn peer_bootstrap(mode: Mode, data_pages: usize) -> ([u8; 256], usize) {
    let mut scratch = [0u64; 32];
    let mut allocator = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch));
    let mut message = Builder::new(&mut allocator);
    let mut imports = message.init_root::<peer_imports::Builder>();
    imports.set_mode(mode);
    imports.set_endpoint(0);
    imports.set_data_pages(data_pages as u32);
    imports.set_notification(1);
    imports.set_doorbell(2);
    imports.set_reply(3);
    let segments = message.get_segments_for_output();
    assert_eq!(segments.len(), 1, "a bootstrap message is one segment");
    // Copied out: the allocator zeroes its buffer when the message goes.
    let mut bytes = [0; 256];
    bytes[..segments[0].len()].copy_from_slice(segments[0]);
    (bytes, segments[0].len())
}

/// Load the peer into a process of its own and talk to it: a call across
/// address spaces, and its attempt to read this process's memory, which
/// faults and is reported here.
fn check_process(info: &Env) {
    let base = info.empty.start + 300;
    let parent_untyped = untyped_at_least(info, 20);
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let (ep, reply, badged) = (base, base + 1, base + 2);
    expect("retype endpoint", untyped::retype(parent_untyped, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("retype reply", untyped::retype(parent_untyped, object::REPLY, 0, in_root, reply, 1), Ok(()));
    expect("badge", slot(badged).mint_from(slot(ep), rights::WRITE | rights::GRANT, 0x9ee), Ok(()));

    // The scratch region is where this process fills pages; the peer will
    // try to read it.
    let scratch = 0x38_0000_0000;
    let parent = Parent {
        cnode: process_slot::CNODE,
        vspace: process_slot::VSPACE,
        asid_pool: info.asid_pool,
        authority: process_slot::TCB,
        untyped: parent_untyped,
        scratch,
        timer_hz: info.timer_hz,
    };
    let mut slots = Slots { next: base + 3, end: base + 200 };
    let (bootstrap, len) = peer_bootstrap(Mode::Call, 0);
    let imports = [(badged, Give::Copy(rights::WRITE | rights::GRANT))];
    let launch = Launch { priority: 255, budget_bits: Some(16), imports: &imports, bootstrap: &bootstrap[..len], ..LAUNCH };
    let peer = loader::load(PEER, &parent, &mut slots, &launch);
    // The process ABI's slots: its own objects, its budget, its import.
    let theirs = |index| peer.slot(index).identify().map(|id| id.object_type).map_err(|e| e.0);
    for (index, wanted) in [
        (process_slot::TCB, object::TCB),
        (process_slot::CNODE, object::CNODE),
        (process_slot::VSPACE, object::PAGE_TABLE),
        (process_slot::UNTYPED, object::UNTYPED),
        (process_slot::FIRST_IMPORT, object::ENDPOINT),
    ] {
        assert_eq!(theirs(index), Ok(wanted), "the peer's slot {index}");
    }
    assert_eq!(theirs(process_slot::FIRST_IMPORT + 1), Err(error::FAILED_LOOKUP), "the peer has one import");
    expect("fault handler", tcb::set_fault_handler(peer.tcb, Some(badged)), Ok(()));
    // Something mapped at the scratch address, so the peer's read would
    // succeed if it could see this address space.
    expect("scratch page", untyped::retype(parent_untyped, object::FRAME_4K, 0, in_root, slots.next, 1), Ok(()));
    let marker = slots.next;
    expect("map scratch", vspace::map_frame(marker, process_slot::VSPACE, scratch, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
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
    println!("check: a second process, calls between address spaces and isolation behave");
}

/// Where a peer maps its channel (init maps each at an address of its own).
const CHANNEL_THERE: usize = 0x20_0000_0000;

/// Where a peer maps frames it receives by page transfer.
const TRANSFER_THERE: usize = CHANNEL_THERE + 0x10_0000;
/// Where init maps the frames it sends by page transfer.
const TRANSFER_HERE: usize = 0x2c_0000_0000;

/// One end of a channel: a ring out and a ring back, and the peer's
/// endpoint for page transfers.
struct Channel {
    out: Producer,
    back: Consumer,
    ep: usize,
}

/// Load a peer in `mode` and connect it by a channel of two rings with
/// `data_pages` data pages each, mapped here at `here`. Everything is made
/// from `parent.untyped`, so revoking it removes the peer and the channel.
fn connect_peer(parent: &Parent, slots: &mut Slots, mode: Mode, data_pages: usize, here: usize) -> Channel {
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let mut take = || {
        slots.next += 1;
        slots.next - 1
    };
    // A notification each, and a badged capability to signal the other.
    let (mine, theirs, my_bell, their_bell) = (take(), take(), take(), take());
    expect("notification", untyped::retype(parent.untyped, object::NOTIFICATION, 0, in_root, mine, 1), Ok(()));
    expect("notification", untyped::retype(parent.untyped, object::NOTIFICATION, 0, in_root, theirs, 1), Ok(()));
    expect("my doorbell", slot(my_bell).mint_from(slot(mine), rights::WRITE, 1), Ok(()));
    expect("their doorbell", slot(their_bell).mint_from(slot(theirs), rights::WRITE, 1), Ok(()));

    // An endpoint the peer receives page transfers on, and a reply object
    // for it to answer them with.
    let (ep, reply) = (take(), take());
    expect("endpoint", untyped::retype(parent.untyped, object::ENDPOINT, 0, in_root, ep, 1), Ok(()));
    expect("reply object", untyped::retype(parent.untyped, object::REPLY, 0, in_root, reply, 1), Ok(()));

    let (bootstrap, len) = peer_bootstrap(mode, data_pages);
    let imports = [(ep, Give::Copy(rights::READ)), (theirs, Give::Copy(rights::READ)), (my_bell, Give::Copy(rights::WRITE)), (reply, Give::Copy(rights::ALL))];
    let launch = Launch { priority: 255, imports: &imports, bootstrap: &bootstrap[..len], ..LAUNCH };
    let peer = loader::load(PEER, parent, slots, &launch);
    let out = RingLayout::new(here, data_pages);
    let back = RingLayout::new(here + out.pages() * PAGE, data_pages);
    let offset = CHANNEL_THERE.wrapping_sub(here);
    let (ro, rw) = (rights::READ, rights::READ | rights::WRITE);
    // Each page is writable only by the side that owns its contents: init
    // produces on `out` and consumes on `back`.
    for (layout, init_produces) in [(out, true), (back, false)] {
        for i in 0..layout.pages() {
            let (frame, copy) = (slots.next, slots.next + 1);
            slots.next += 2;
            expect("ring page", untyped::retype(parent.untyped, object::FRAME_4K, 0, in_root, frame, 1), Ok(()));
            expect("copy for the peer", slot(copy).copy_from(slot(frame), rights::ALL), Ok(()));
            let init_writes = layout.producer_writes(i) == init_produces;
            let va = layout.base + i * PAGE;
            let new_table = || {
                let table = slots::alloc().expect("slot");
                untyped::retype(parent.untyped, object::PAGE_TABLE, 0, in_root, table, 1).map(|()| table)
            };
            let mapped = vspace::map_frame_with_tables(frame, process_slot::VSPACE, va, if init_writes { rw } else { ro }, attr::EXECUTE_NEVER, new_table);
            expect("map here", mapped, Ok(()));
            peer.map_frame(parent, slots, copy, va.wrapping_add(offset), if init_writes { ro } else { rw });
        }
    }
    peer.map_tables(parent, slots, TRANSFER_THERE);
    peer.start();
    // SAFETY: both rings are mapped here with the rights each end needs,
    // and this thread is the only user of these ends.
    unsafe { Channel { out: Producer::new(out, their_bell, mine), back: Consumer::new(back, their_bell, mine), ep } }
}

/// `MAX_PAGES` frames in slots from `first`, for sending by page transfer
/// from `TRANSFER_HERE`; each is mapped once there, so the page tables
/// exist, then unmapped.
fn transfer_sender(parent: &Parent, slots: &mut Slots) -> (Sender, usize) {
    let in_root = SlotRef { root: process_slot::CNODE, index: 0, depth: 0 };
    let first = slots.next;
    slots.next += MAX_PAGES;
    expect("transfer frames", untyped::retype(parent.untyped, object::FRAME_4K, 0, in_root, first, MAX_PAGES), Ok(()));
    for i in 0..MAX_PAGES {
        let new_table = || {
            slots.next += 1;
            untyped::retype(parent.untyped, object::PAGE_TABLE, 0, in_root, slots.next - 1, 1).map(|()| slots.next - 1)
        };
        let va = TRANSFER_HERE + i * PAGE;
        expect("map once", vspace::map_frame_with_tables(first + i, process_slot::VSPACE, va, rights::READ, attr::EXECUTE_NEVER, new_table), Ok(()));
        expect("unmap", vspace::unmap_frame(first + i), Ok(()));
    }
    let frames = Frames { cnode: process_slot::CNODE, first, vspace: process_slot::VSPACE, base: TRANSFER_HERE };
    // SAFETY: fresh, unmapped frames with no other capabilities, and an
    // address range nothing else in init uses, with its tables in place.
    (unsafe { Sender::new(frames) }, first)
}

/// Untyped memory and slots for the channel checks; revoking the memory
/// removes every peer and channel made from it.
fn channel_resources(info: &Env) -> (Parent, Slots) {
    let first = slots::alloc_range(400).expect("slots");
    let parent = Parent {
        cnode: process_slot::CNODE,
        vspace: process_slot::VSPACE,
        asid_pool: info.asid_pool,
        authority: process_slot::TCB,
        untyped: untyped_at_least(info, 20),
        scratch: 0x38_0000_0000,
        timer_hz: info.timer_hz,
    };
    (parent, Slots { next: first, end: first + 400 })
}

/// Two processes over a channel of two rings, with notifications as
/// doorbells: 10,000 word records echoed by a peer, with at most 48 in
/// flight.
fn check_ring(info: &Env) {
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, .. } = connect_peer(&parent, &mut slots, Mode::EchoWords, 1, 0x28_0000_0000);
    const MESSAGES: u64 = 10_000;
    const WINDOW: u64 = 48;
    let (mut sent, mut received, mut checksum) = (0u64, 0u64, 0u64);
    let mut words = [0u64; 2];
    while received < MESSAGES {
        while sent < MESSAGES && sent - received < WINDOW {
            out.send(echo::WORDS, sent, as_bytes(&[sent, sent * 3]));
            sent += 1;
        }
        let record = back.recv(&mut words).expect("ring");
        assert_eq!((record.kind, record.id, record.len), (echo::WORDS, received, 16), "echo record");
        assert_eq!(words, [received + 1, received * 3 + 1], "echo out of order or corrupted");
        checksum = checksum.wrapping_add(words[0]);
        received += 1;
    }
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut words).map(|r| r.kind), Ok(echo::STOP), "the peer should echo the stop record");
    assert_eq!(checksum, MESSAGES * (MESSAGES + 1) / 2, "checksum");
    sys::yield_now(); // let the peer report and stop
    println!(
        "check: {MESSAGES} records round trip over shared-memory rings; check slept {} time(s) waiting for echoes, {} for space",
        back.sleeps, out.sleeps
    );
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
    println!("check: shared-memory rings between processes behave");
}

/// The blob length of request number `i` on the ring: it grows and shrinks
/// with `i`, so records of many sizes cross the ring.
fn small_blob(i: u64) -> usize {
    (i as usize * 37) % 900
}

/// The request `init` sends as number `i`, with a blob of `blob_len` bytes;
/// its text and numbers vary with `i` too.
fn echo_request<'a, 'b>(i: u64, blob_len: usize, scratch: &'a mut SingleSegmentAllocator<'b>) -> Builder<&'a mut SingleSegmentAllocator<'b>> {
    let mut message = Builder::new(scratch);
    let mut request = message.init_root::<request::Builder>();
    request.set_id(i);
    let text = &"sapabilities: capabilities everywhere, Cap'n Proto across the rings. "[..(i as usize * 7) % 70];
    request.set_text(text);
    let mut numbers = request.reborrow().init_numbers((i % 50) as u32);
    for k in 0..i % 50 {
        numbers.set(k as u32, (k * i) as u32);
    }
    request.init_blob(blob_len as u32).iter_mut().enumerate().for_each(|(k, b)| *b = (k as u64 + i) as u8);
    message
}

/// Check `reply` against request number `i`, whose blob was `blob_len`
/// bytes.
fn check_reply(i: u64, blob_len: usize, body: &[u8]) {
    let segments = [body];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let reply = reader.get_root::<reply::Reader>().expect("reply");
    let text = &"sapabilities: capabilities everywhere, Cap'n Proto across the rings. "[..(i as usize * 7) % 70];
    assert_eq!(reply.get_id(), i);
    assert!(reply.get_text().unwrap().to_str().unwrap().chars().eq(text.chars().rev()), "reversed text");
    assert_eq!(reply.get_sum(), (0..i % 50).map(|k| (k * i) as u32 as u64).sum::<u64>(), "sum");
    let blob_len = blob_len as u64;
    assert_eq!(reply.get_blob_length() as u64, blob_len, "blob length");
    assert_eq!(reply.get_blob_sum(), (0..blob_len).map(|k| (k + i) as u8 as u64).sum::<u64>(), "blob sum");
}

/// The echo interface between processes: Cap'n Proto requests of many sizes
/// out, replies back, read from a private copy or (trusting the peer) in
/// place; a malformed request, one too long for the peer, and a peer that
/// breaks the ring's rules.
fn check_echo_service(info: &Env) {
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, ep } = connect_peer(&parent, &mut slots, Mode::ServeEcho, 2, 0x28_0000_0000);
    let mut scratch_words = [0u64; 512];
    let mut scratch = SingleSegmentAllocator::new(as_bytes_mut(&mut scratch_words));
    let mut reply_words = [0u64; 512];
    const REQUESTS: u64 = 300;
    const WINDOW: u64 = 6;
    let (mut sent, mut received, mut bytes) = (0, 0, 0);
    while received < REQUESTS {
        while sent < REQUESTS && sent - received < WINDOW {
            let message = echo_request(sent, small_blob(sent), &mut scratch);
            let segment = message.get_segments_for_output()[0];
            bytes += segment.len();
            out.send(echo::REQUEST, sent, segment);
            sent += 1;
        }
        if received % 2 == 0 {
            let record = back.recv(&mut reply_words).expect("ring");
            assert_eq!((record.kind, record.id), (echo::REPLY, received), "reply record");
            check_reply(received, small_blob(received), &as_bytes(&reply_words)[..record.len]);
        } else {
            // SAFETY: the peer is this test's own program, which never
            // writes a record after publishing it.
            let record = unsafe { back.recv_in_place(|record, body| { check_reply(received, small_blob(received), body); record }) }.expect("ring");
            assert_eq!((record.kind, record.id), (echo::REPLY, received), "reply record");
        }
        received += 1;
    }
    println!("check: {REQUESTS} Cap'n Proto requests ({} KiB) answered across processes, read copied and in place", bytes / 1024);

    // Requests too large for the ring (its largest body is under 4 KiB) go
    // by page transfer: built in a private buffer, written into init's
    // frames, moved to the peer, and read there in place.
    let (mut sender, first) = transfer_sender(&parent, &mut slots);
    let mut big_words = alloc::vec![0u64; MAX_LEN / 8];
    let mut big = SingleSegmentAllocator::new(as_bytes_mut(&mut big_words));

    // Senders that break the rules, by hand: each sends one capability for
    // a 100-byte message and gets it back with an error record.
    let spare = slots.next;
    slots.next += 1;
    let mut lie = |id: u64, cap: ipc::Give, window: usize, why: &str| {
        out.send(echo::TRANSFER, id, &[]);
        ipc::set_receive_window(process_slot::CNODE, window, 1);
        let returned = ipc::call(ep, echo::TRANSFER as usize, &[100, id as usize], &[cap], &mut []).expect("call");
        assert_eq!(returned.info.extra_caps(), 1, "the capability comes back: {why}");
        assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, id)), "{why}");
    };
    // Keeping a copy of a frame would let the sender change the message
    // while the peer reads it.
    expect("keep a copy", slot(spare).copy_from(slot(first), rights::ALL), Ok(()));
    lie(3000, ipc::Give::moved(first), first, "a frame the sender shares");
    expect("drop the copy", slot(spare).delete(), Ok(()));

    // A frame still mapped by the sender (moving a capability keeps its
    // mapping), which the sender could write through.
    expect("map a frame here", vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rights::READ | rights::WRITE, attr::EXECUTE_NEVER), Ok(()));
    lie(3001, ipc::Give::moved(first), first, "a frame the sender maps");
    expect("unmap it", vspace::unmap_frame(first), Ok(()));
    // Not a frame: an endpoint, which the peer would call if it invoked it
    // to map it.
    lie(3002, ipc::Give::copy(ep), spare, "a capability that isn't a frame");
    expect("drop it", slot(spare).delete(), Ok(()));

    // Then requests of up to 60 KiB go through.
    let mut transferred = 0;
    for (k, blob_len) in [5_000, 20_000, 60_000].into_iter().enumerate() {
        let id = 2000 + k as u64;
        let message = echo_request(id, blob_len, &mut big);
        let segment = message.get_segments_for_output()[0];
        transferred += segment.len();
        out.send(echo::TRANSFER, id, &[]);
        sender.call(ep, echo::TRANSFER as usize, id, segment.len(), |frames| frames.copy_from_slice(segment), &mut []).expect("page transfer");
        let record = back.recv(&mut reply_words).expect("ring");
        assert_eq!((record.kind, record.id), (echo::REPLY, id), "a transfer's reply");
        check_reply(id, blob_len, &as_bytes(&reply_words)[..record.len]);
    }
    assert_eq!(sender.call(ep, 0, 0, MAX_LEN + 1, |_| (), &mut []).map(drop), Err(TransferError::TooLong), "longer than one call carries");
    println!("check: 3 requests ({} KiB) by page transfer, read in place; shared, mapped and non-frame transfers refused", transferred / 1024);

    // Not Cap'n Proto: the peer answers with an error and carries on.
    out.send(echo::REQUEST, 1000, &[0xff; 40]);
    assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, 1000)), "a malformed request");
    // Longer than the peer's 2 KiB buffer: skipped, with an error.
    out.send(echo::REQUEST, 1001, &[0; 3000]);
    assert_eq!(back.recv(&mut reply_words).map(|r| (r.kind, r.id)), Ok((echo::ERROR, 1001)), "an oversized request");
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut reply_words).map(|r| r.kind), Ok(echo::STOP), "the stop record");
    sys::yield_now();

    // A peer that answers with garbage, then publishes a record longer than
    // what it published: the garbage fails to parse, the record is refused.
    let Channel { mut out, mut back, .. } = connect_peer(&parent, &mut slots, Mode::Misbehave, 1, 0x28_0001_0000);
    let message = echo_request(1, small_blob(1), &mut scratch);
    out.send(echo::REQUEST, 1, message.get_segments_for_output()[0]);
    drop(message);
    let record = back.recv(&mut reply_words).expect("ring");
    let segments = [&as_bytes(&reply_words)[..record.len]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    assert!(reader.get_root::<reply::Reader>().and_then(|r| r.get_text().map(drop)).is_err(), "a garbage reply should not parse");
    assert_eq!(back.recv(&mut reply_words), Err(RingError::Corrupt), "a record past what was published");
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
    println!("check: Cap'n Proto over rings refuses malformed and oversized requests and a lying peer");
}

/// The peer's checksum (`sum_bytes`): little-endian words, the tail as
/// bytes.
fn checksum(bytes: &[u8]) -> u64 {
    let (words, tail) = bytes.as_chunks::<8>();
    let tail = tail.iter().map(|&b| u64::from(b)).fold(0, u64::wrapping_add);
    words.iter().map(|&w| u64::from_le_bytes(w)).fold(tail, u64::wrapping_add)
}

/// Time round trips that move `size` bytes to a peer and get their
/// checksum back, through a ring (copied in by init, copied out by the
/// peer) and by page transfer (written into frames by init, read in place
/// by the peer), and print both: where copying stops paying (M5). Rings of
/// 32 data pages take bodies up to 64 KiB, like one transfer.
fn measure_transfer(info: &Env) {
    const ROUNDS: u64 = 64;
    let (parent, mut slots) = channel_resources(info);
    let Channel { mut out, mut back, ep } = connect_peer(&parent, &mut slots, Mode::SumBytes, 32, 0x28_0000_0000);
    let (mut sender, first) = transfer_sender(&parent, &mut slots);

    // What one page costs: the system calls each side makes per page.
    let micro = |f: &mut dyn FnMut()| {
        let start = sys::time();
        for _ in 0..ROUNDS {
            f();
        }
        (sys::time() - start) * 10_000_000 / info.timer_hz / ROUNDS
    };
    let rw = rights::READ | rights::WRITE;
    let identify = micro(&mut || {
        let _ = slot(first).identify();
    });
    let map_unmap = micro(&mut || {
        let _ = vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rw, attr::EXECUTE_NEVER);
        let _ = vspace::unmap_frame(first);
    });
    let null = micro(&mut || sys::yield_now());
    let touch = micro(&mut || {
        let _ = vspace::map_frame(first, process_slot::VSPACE, TRANSFER_HERE, rw, attr::EXECUTE_NEVER);
        // SAFETY: just mapped read-write; nothing else uses it.
        unsafe { mapped::<u64>(TRANSFER_HERE).write_volatile(1) };
        let _ = vspace::unmap_frame(first);
    });
    let us = |t: u64| format!("{}.{} us", t / 10, t % 10);
    println!(
        "check: per call: yield {}, identify {}, map+unmap {}, map+write+unmap {}",
        us(null), us(identify), us(map_unmap), us(touch)
    );
    let data: Vec<u8> = (0..MAX_LEN).map(|k| (k * 7 + k / 251) as u8).collect();
    let mut sum = [0u64; 1];
    let mut round_trip = |n: usize, id: u64, transfer: bool| {
        if transfer {
            out.send(echo::TRANSFER, id, &[]);
            sender.call(ep, echo::TRANSFER as usize, id, n, |frames| frames.copy_from_slice(&data[..n]), &mut []).expect("transfer");
        } else {
            out.send(echo::BYTES, id, &data[..n]);
        }
        let record = back.recv(&mut sum).expect("ring");
        assert_eq!((record.kind, record.id, sum[0]), (echo::SUM, id, checksum(&data[..n])), "the peer's checksum");
    };
    // Tenths of a microsecond per round trip.
    let tenths = |ticks: u64| ticks * 10_000_000 / info.timer_hz / ROUNDS;
    println!("check: round trips of n bytes, ring copy vs page transfer (QEMU, {} Hz timer):", info.timer_hz);
    for n in [256, 1024, 4096, 8192, 16384, 32768, 61440] {
        let mut ticks = [0u64; 2];
        for (way, transfer) in [false, true].into_iter().enumerate() {
            round_trip(n, 0, transfer); // warm up
            let start = sys::time();
            for id in 0..ROUNDS {
                round_trip(n, id, transfer);
            }
            ticks[way] = sys::time() - start;
        }
        println!("check:   {n:>6} B: copy {}, transfer {}", us(tenths(ticks[0])), us(tenths(ticks[1])));
    }
    out.send(echo::STOP, 0, &[]);
    assert_eq!(back.recv(&mut sum).map(|r| r.kind), Ok(echo::STOP), "the stop record");
    sys::yield_now();
    expect("revoke", slot(parent.untyped).revoke(), Ok(()));
    slots::free_range(slots.end - 400, 400);
}

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
/// tables, alignment, reuse of freed memory, and two threads allocating at
/// once through the heap's lock.
fn check_heap(info: &Env) {
    assert_eq!(heap::mapped(), 0, "nothing should be mapped before the first allocation");
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
    println!("check: heap grows, aligns, reuses memory and serves two threads ({} KiB mapped); a mutex sleeps and wakes", heap::mapped() / 1024);
}

/// A word-aligned buffer, as Cap'n Proto segments must be.
#[repr(C, align(8))]
struct Words<const N: usize>([u8; N]);

/// Cap'n Proto within one process: a request built in a fixed buffer with no
/// heap, read back through the checked reader, and a corrupted pointer, bad
/// UTF-8 and an over-long traversal rejected rather than trusted.
fn check_capnp() {
    let mut space = Words([0; 1024]);
    let mut message = Builder::new(SingleSegmentAllocator::new(&mut space.0));
    {
        let mut req = message.init_root::<request::Builder>();
        req.set_id(7);
        req.set_text("capabilities");
        let mut numbers = req.reborrow().init_numbers(4);
        for (i, n) in [3u32, 1, 4, 1].into_iter().enumerate() {
            numbers.set(i as u32, n);
        }
        req.set_blob(&[0xff; 5]);
    }
    let segment = message.get_segments_for_output()[0];
    let mut copy = Words([0; 1024]);
    copy.0[..segment.len()].copy_from_slice(segment);
    let words = &copy.0[..segment.len()];

    let segments = [words];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let req = reader.get_root::<request::Reader>().expect("root");
    assert_eq!(req.get_id(), 7);
    assert_eq!(req.get_text().unwrap().to_str(), Ok("capabilities"));
    assert_eq!(req.get_numbers().unwrap().iter().map(u64::from).sum::<u64>(), 9);
    assert_eq!(req.get_blob().unwrap(), &[0xff; 5]);

    // Each corruption is in a private copy, as a receiver's would be.
    let mut bad = Words([0; 1024]);
    let n = segment.len();
    // The root pointer (word 0) pointing far outside the segment.
    bad.0[..n].copy_from_slice(segment);
    bad.0[..4].copy_from_slice(&0x7fff_fff0u32.to_le_bytes());
    let segments = [&bad.0[..n]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    assert!(reader.get_root::<request::Reader>().is_err(), "an out-of-bounds root should be refused");
    // Text that isn't UTF-8: found and overwritten in place.
    bad.0[..n].copy_from_slice(segment);
    let at = bad.0[..n].windows(12).position(|w| w == b"capabilities").expect("the text");
    bad.0[at] = 0xc3;
    bad.0[at + 1] = 0x28;
    let segments = [&bad.0[..n]];
    let reader = Reader::new(SegmentArray::new(&segments), ReaderOptions::new());
    let req = reader.get_root::<request::Reader>().expect("root");
    assert!(req.get_text().unwrap().to_str().is_err(), "invalid UTF-8 should be refused");
    // A traversal limit smaller than the message.
    let segments = [words];
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(4));
    let reader = Reader::new(SegmentArray::new(&segments), options);
    let over = reader.get_root::<request::Reader>().and_then(|r| r.get_numbers().and_then(|_| r.get_blob()).map(drop));
    assert!(over.is_err(), "a traversal past the limit should be refused");
    println!("check: Cap'n Proto messages build without a heap, read back, and refuse bad pointers, UTF-8 and traversal");
}
