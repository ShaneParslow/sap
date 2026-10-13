//! The first user program: the root task. It will become the process
//! manager (M7); for now it checks the kernel features as they arrive.

#![no_std]
#![no_main]

use libsap::abi::{error, init_slot, rights, BootInfo};
use libsap::cnode::SlotRef;
use libsap::sys::Error;
use libsap::{debug, println};

/// Slot `i` of the root CNode.
fn slot(i: usize) -> SlotRef {
    SlotRef::new(init_slot::CNODE, i)
}

#[unsafe(no_mangle)]
fn sap_main(info: &'static BootInfo) -> ! {
    report(info);
    check_cnode_operations(info);
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
