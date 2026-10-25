//! Capability spaces: CNode operations and retyping untyped memory.

use sap_process::process_slot;
use sap_rt::abi::{error, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::{println, untyped};
use crate::{Env, expect, slot, untyped_at_least};

/// Copy, move, delete and revoke capabilities, and check the errors for
/// misuse.
pub fn check_cnode_operations(info: &Env) {
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
pub fn check_retype(info: &Env) {
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
