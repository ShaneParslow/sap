//! Address spaces: mapping frames and page tables.

use sap_process::process_slot;
use sap_rt::abi::{attr, error, identify, object, rights};
use sap_rt::address::mapped;
use sap_rt::cnode::SlotRef;
use sap_rt::{println, untyped, vspace};
use crate::{Env, expect, slot, untyped_at_least};

/// Map page tables and frames into this address space and a new one, read
/// and write through the mappings, and check the misuse errors.
pub fn check_mapping(info: &Env) {
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
    println!("page tables, frames, ASIDs and unmapping behave");
}
