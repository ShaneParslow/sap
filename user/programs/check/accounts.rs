//! Memory accounts, from the memory manager (`interfaces/memory.capnp`):
//! reservations and limits, chunks and what is made from them, revoking
//! and ending an account.

use sap_process::process_slot;
use sap_rt::abi::{error, object};
use sap_rt::account::{self, Error, Status, Usage};
use sap_rt::cnode::SlotRef;
use sap_rt::{println, untyped};
use crate::{Env, slot};

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;

fn refused(status: Status) -> Error {
    Error::Refused(status)
}

/// Whether slot `s` is empty.
fn empty(s: usize) -> bool {
    slot(s).identify().map_err(|e| e.0) == Err(error::FAILED_LOOKUP)
}

pub fn check_accounts(info: &Env) {
    let base = info.empty.start;
    let cnode = process_slot::CNODE;
    let in_root = SlotRef { root: cnode, index: 0, depth: 0 };
    let before = account::usage(info.account).expect("check's own account");

    // A child with 512 KiB reserved and a 2 MiB limit.
    let (holder, control) = (base, base + 1);
    assert_eq!(account::child(info.account, 512 * KIB, 2 * MIB, cnode, holder), Ok(()));
    assert_eq!(account::usage(holder), Ok(Usage { used: 0, reservation: 512 * KIB, limit: 2 * MIB }));
    assert_eq!(account::usage(info.account).map(|u| u.used), Ok(before.used), "a child's reservation isn't usage");

    // Chunks up to the limit, beyond the reservation from the pool.
    let a = account::allocate(holder, 20, cnode, base + 2).expect("a chunk within the limit");
    assert_eq!(slot(base + 2).identify().map(|i| i.object_type), Ok(object::UNTYPED), "a chunk is untyped memory");
    assert_eq!(untyped::retype(base + 2, object::FRAME_4K, 0, in_root, base + 3, 1), Ok(()));
    let b = account::allocate(holder, 20, cnode, base + 4).expect("a chunk beyond the reservation");
    assert_ne!(a.paddr, b.paddr);
    assert_eq!(account::allocate(holder, 12, cnode, base + 5).map(drop), Err(refused(Status::OverLimit)), "past the limit");
    assert_eq!(account::usage(holder).map(|u| u.used), Ok(2 * MIB));
    assert_eq!(account::usage(info.account).map(|u| u.used), Ok(before.used + 2 * MIB), "a child's usage counts for its parent");
    assert_eq!(account::allocate(holder, 11, cnode, base + 5).map(drop), Err(refused(Status::BadRequest)), "too small a chunk");

    // Releasing a chunk destroys what was made from it.
    assert_eq!(account::release(holder, a.id), Ok(()));
    assert!(empty(base + 2) && empty(base + 3), "a released chunk and its frame are gone");
    assert_eq!(account::release(holder, a.id), Err(refused(Status::NoSuchChunk)));
    assert_eq!(account::release(info.account, b.id), Err(refused(Status::NoSuchChunk)), "a parent can't release a child's chunk");

    // A limit below usage refuses allocation until enough is released.
    assert_eq!(account::set_limit(control, 512 * KIB), Ok(()));
    assert_eq!(account::allocate(holder, 12, cnode, base + 5).map(drop), Err(refused(Status::OverLimit)), "over a lowered limit");
    assert_eq!(account::release(holder, b.id), Ok(()));
    let c = account::allocate(holder, 12, cnode, base + 5).expect("under the limit again");
    assert_eq!(account::set_limit(control, 256 * KIB), Err(refused(Status::BadRequest)), "below the reservation");
    assert_eq!(account::set_limit(control, 1 << 40), Err(refused(Status::OverLimit)), "above the parent's limit");

    // A grandchild's reservation comes from the child's.
    assert_eq!(account::child(holder, MIB, MIB, cnode, base + 6), Err(refused(Status::OverLimit)), "more than the parent has reserved");
    assert_eq!(account::child(holder, 256 * KIB, 128 * KIB, cnode, base + 6), Err(refused(Status::BadRequest)), "a limit below the reservation");
    assert_eq!(account::child(holder, 256 * KIB, 512 * KIB, cnode, base + 6), Ok(()));
    account::allocate(base + 6, 12, cnode, base + 8).expect("a grandchild's chunk");

    // Revoking empties the account and ends its children.
    assert_eq!(account::revoke(control), Ok(()));
    assert!(empty(base + 5) && empty(base + 8), "revoked chunks are gone");
    assert!(empty(base + 6), "a revoked child's capability is gone");
    assert_eq!(account::usage(base + 6), Err(Error::Call(sap_rt::sys::Error(error::FAILED_LOOKUP))));
    assert_eq!(account::usage(holder).map(|u| u.used), Ok(0));
    let d = account::allocate(holder, 12, cnode, base + 5).expect("a revoked account still works");
    let _ = (c, d);

    // Destroying ends it, and gives its reservation back.
    assert_eq!(account::destroy(control), Ok(()));
    assert!(empty(base + 5), "a destroyed account's chunks are gone");
    assert!(empty(holder) && empty(control), "a destroyed account's capabilities are gone");
    assert_eq!(account::usage(info.account), Ok(before));
    for s in [holder, control, base + 6, base + 7] {
        let _ = slot(s).delete();
    }
    println!("memory accounts: reservations, limits, chunks, release, revoke and destroy behave");
}
