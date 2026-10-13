//! The first user program: the root task. It will become the process
//! manager (M7); for now it checks the kernel features as they arrive.

#![no_std]
#![no_main]

use libsap::abi::{error, init_slot, object, rights, BootInfo};
use libsap::cnode::SlotRef;
use libsap::sys::Error;
use libsap::{debug, println, untyped};

/// Slot `i` of the root CNode.
fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

#[unsafe(no_mangle)]
fn sap_main(info: &'static BootInfo) -> ! {
    report(info);
    check_cnode_operations(info);
    check_retype(info);
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
