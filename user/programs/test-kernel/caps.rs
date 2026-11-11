//! Capability spaces: CNode operations and retyping untyped memory.

use sap_rt::abi::{BootInfo, error, object, rights};
use sap_rt::cnode::SlotRef;
use sap_rt::println;

use crate::support::{TCB, fails, ok, slot, try_retype, untyped_at_least};

/// Copy, move, delete and revoke capabilities, and check the errors for
/// misuse.
pub fn cnode_operations(info: &BootInfo) {
    let a = slot(info.empty.start);
    let b = slot(info.empty.start + 1);
    let c = slot(info.empty.start + 2);
    let tcb = slot(TCB);
    let untyped = slot(info.untyped.start);

    ok("copy tcb", a.copy_from(tcb, rights::ALL));
    let occupied = a.copy_from(tcb, rights::ALL);
    fails("copy into occupied slot", occupied, error::DELETE_FIRST);
    ok("copy the copy", b.copy_from(a, rights::ALL));
    ok("move", c.move_from(b));
    let emptied = b.copy_from(b, rights::ALL);
    fails("copy from emptied slot", emptied, error::FAILED_LOOKUP);
    ok("delete", c.delete());
    ok("delete", a.delete());
    ok("delete empty slot", a.delete());

    // A copy of untyped memory is its child: revoking the original deletes
    // it.
    ok("copy untyped", a.copy_from(untyped, rights::ALL));
    ok("copy untyped copy", b.copy_from(a, rights::ALL));
    ok("revoke untyped", untyped.revoke());
    let copy = c.copy_from(a, rights::ALL);
    fails("revoked copy is gone", copy, error::FAILED_LOOKUP);
    let grandchild = c.copy_from(b, rights::ALL);
    fails(
        "revoked grandchild is gone",
        grandchild,
        error::FAILED_LOOKUP,
    );

    // Addresses that don't resolve.
    let out_of_range = a.copy_from(slot(1 << info.cnode_radix), rights::ALL);
    fails("slot out of range", out_of_range, error::FAILED_LOOKUP);
    let wrong_depth = a.copy_from(SlotRef { depth: 8, ..tcb }, rights::ALL);
    fails("wrong depth", wrong_depth, error::FAILED_LOOKUP);

    println!("CNode copy, move, delete and revoke behave");
}

/// Retype untyped memory into objects, run it out, and get it back by
/// revoking.
pub fn retype(info: &BootInfo) {
    let parent = untyped_at_least(info, 16);

    // A 64 KiB untyped region carved from the first big enough block, so
    // running out is quick.
    let region = info.empty.start + 8;
    ok(
        "carve 64 KiB",
        try_retype(parent, object::UNTYPED, 16, region, 1),
    );

    // A CNode with 16 slots, a TCB, a page table and four frames.
    let cnode = region + 1;
    ok(
        "retype cnode",
        try_retype(region, object::CNODE, 4, cnode, 1),
    );
    ok(
        "retype tcb",
        try_retype(region, object::TCB, 0, cnode + 1, 1),
    );
    let table = try_retype(region, object::PAGE_TABLE, 0, cnode + 2, 1);
    ok("retype page table", table);
    let frames = try_retype(region, object::FRAME_4K, 0, cnode + 3, 4);
    ok("retype frames", frames);

    let occupied = try_retype(region, object::FRAME_4K, 0, cnode, 1);
    fails("retype into occupied slot", occupied, error::DELETE_FIRST);
    let unknown = try_retype(region, 99, 0, cnode + 7, 1);
    fails("retype unknown type", unknown, error::INVALID_ARGUMENT);
    let with_children = slot(cnode + 7).copy_from(slot(region), rights::ALL);
    fails(
        "copy untyped with children",
        with_children,
        error::REVOKE_FIRST,
    );

    // Capabilities can go into the new CNode, addressed through it: its
    // capability is at slot `cnode` of the root, and its own slots take 4
    // bits.
    let inner = SlotRef {
        root: cnode,
        index: 3,
        depth: 4,
    };
    ok(
        "copy into the new cnode",
        inner.copy_from(slot(TCB), rights::ALL),
    );

    // Fill the rest of the 64 KiB with frames. The CNode (1 KiB) and TCB
    // (1 KiB) share the first page; the page table and four frames take
    // five more; ten pages remain.
    let filled = try_retype(region, object::FRAME_4K, 0, cnode + 7, 10);
    ok("fill with frames", filled);
    let exhausted = try_retype(region, object::FRAME_4K, 0, cnode + 17, 1);
    fails("out of memory", exhausted, error::NOT_ENOUGH_MEMORY);

    // Revoking the region deletes every object made from it, including the
    // copy inside the new CNode, and the memory can be used again.
    ok("revoke region", slot(region).revoke());
    let gone = slot(cnode + 17).copy_from(slot(cnode), rights::ALL);
    fails("objects are gone", gone, error::FAILED_LOOKUP);
    let reused = try_retype(region, object::FRAME_4K, 0, cnode, 16);
    ok("reuse after revoke", reused);
    ok("revoke again", slot(region).revoke());

    untyped_copies(region, cnode);

    ok("revoke the carve", slot(parent).revoke());
    println!("untyped retype, exhaustion and reuse after revoke behave");
}

/// A copy of untyped memory takes the memory over: the original counts as
/// full until the copy and everything made through it are gone, so the two
/// can't hand out the same memory twice. `region` must be empty, and the
/// slots from `copy` on free.
fn untyped_copies(region: usize, copy: usize) {
    let frame = try_retype(region, object::FRAME_4K, 0, copy, 1);
    ok("one frame", frame);
    ok("revoke it", slot(region).revoke());
    ok(
        "copy the empty region",
        slot(copy).copy_from(slot(region), rights::ALL),
    );

    let full = try_retype(region, object::FRAME_4K, 0, copy + 1, 1);
    fails("the original is full", full, error::NOT_ENOUGH_MEMORY);
    let through_copy = try_retype(copy, object::FRAME_4K, 0, copy + 1, 1);
    ok("the copy retypes", through_copy);
    ok("delete the copy", slot(copy).delete());

    let still_full = try_retype(region, object::FRAME_4K, 0, copy + 2, 1);
    fails(
        "full while the copy's frame lives",
        still_full,
        error::NOT_ENOUGH_MEMORY,
    );
    ok("revoke the frame", slot(region).revoke());

    let again = try_retype(region, object::FRAME_4K, 0, copy, 1);
    ok("the original retypes again", again);
    ok("revoke once more", slot(region).revoke());
}
