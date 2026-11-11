//! Address spaces: mapping frames and page tables.

use sap_rt::abi::{BootInfo, attr, error, identify, init_slot, object, rights};
use sap_rt::address::mapped;
use sap_rt::{println, vspace};

use crate::support::{RW, VSPACE, fails, make, make_many, ok, slot, untyped_at_least};

/// A value written through one mapping and read back through others.
const VALUE: u64 = 0x5a9_cafe;

/// The type and flags of the capability in slot `index`.
fn identified(index: usize) -> Result<(usize, usize), usize> {
    let identity = slot(index).identify().map_err(|e| e.0)?;
    Ok((identity.object_type, identity.flags))
}

/// Map page tables and frames into this address space and a new one, read
/// and write through the mappings, and check the misuse errors.
pub fn mapping(info: &BootInfo) {
    let base = info.empty.start + 32;
    let parent = untyped_at_least(info, 22);
    let big = base;
    let l1 = base + 1;
    let l0 = base + 2;
    let spare = base + 3;
    let f1 = base + 4;
    let f2 = base + 5;
    let copy = base + 6;

    // Three page tables (one per level below the root, and a spare), two
    // 4 KiB frames and a 2 MiB frame. The 2 MiB frame goes first so it is
    // aligned at the start of the region.
    make(parent, object::FRAME_2M, big);
    make_many(parent, object::PAGE_TABLE, l1, 3);
    make_many(parent, object::FRAME_4K, f1, 2);

    // 1 GiB up: nothing of this program's is mapped there, so both tables
    // are new.
    let va: usize = 0x4000_0000;
    let map = |frame, at| vspace::map_frame(frame, VSPACE, at, RW, attr::EXECUTE_NEVER);

    fails("frame without tables", map(f1, va), error::FAILED_LOOKUP);
    ok("map level-1 table", vspace::map_page_table(l1, VSPACE, va));
    ok("map level-0 table", vspace::map_page_table(l0, VSPACE, va));
    let every_level = vspace::map_page_table(spare, VSPACE, va);
    fails("every level has a table", every_level, error::DELETE_FIRST);
    ok("map frame", map(f1, va));

    fails(
        "map frame twice",
        map(f1, va + 0x1000),
        error::INVALID_CAPABILITY,
    );
    fails("map over a mapping", map(f2, va), error::DELETE_FIRST);
    fails("unaligned", map(f2, va + 1), error::ALIGNMENT_ERROR);
    let kernel_va = 0xffff_ffc0_0000_0000;
    fails(
        "kernel address",
        map(f2, kernel_va),
        error::INVALID_ARGUMENT,
    );
    let write_only = vspace::map_frame(f2, VSPACE, va + 0x1000, rights::WRITE, 0);
    fails("write-only", write_only, error::INVALID_ARGUMENT);

    // Write through the mapping, move the frame to another address
    // read-only, and read the value back there.
    // SAFETY: the frame was just mapped read-write at va.
    unsafe { mapped::<u64>(va).write_volatile(VALUE) };
    ok("unmap frame", vspace::unmap_frame(f1));
    let read_only = vspace::map_frame(f1, VSPACE, va + 0x3000, rights::READ, attr::EXECUTE_NEVER);
    ok("remap read-only", read_only);
    // SAFETY: mapped read-only at va + 0x3000.
    let back = unsafe { mapped::<u64>(va + 0x3000).read_volatile() };
    assert_eq!(back, VALUE, "value didn't survive remapping");

    identifying(parent, big, l0, f1, copy);

    // A 2 MiB frame goes straight into the level-1 table.
    let big_va = va + 0x20_0000;
    ok("map 2 MiB frame", map(big, big_va));
    // SAFETY: mapped read-write.
    unsafe { mapped::<u64>(big_va + 0x1f_fff8).write_volatile(7) };
    let address = ok("frame address", vspace::frame_address(big));
    let region = info.untyped_list[parent - info.untyped.start].paddr;
    assert!(
        address == region,
        "2 MiB frame at {address:#x}, region at {region:#x}"
    );

    // A new address space: an unmapped page table becomes a root with an
    // ASID, and mappings go into it without touching this one.
    let root = base + 6;
    make_many(parent, object::PAGE_TABLE, root, 3);
    let non_root = vspace::map_page_table(base + 7, root, va);
    fails("map into non-root", non_root, error::INVALID_CAPABILITY);
    ok(
        "assign asid",
        vspace::assign_asid(init_slot::ASID_POOL, root),
    );
    let twice = vspace::assign_asid(init_slot::ASID_POOL, root);
    fails("assign twice", twice, error::INVALID_CAPABILITY);
    ok(
        "map table in new space",
        vspace::map_page_table(base + 7, root, va),
    );
    ok(
        "map table in new space",
        vspace::map_page_table(base + 8, root, va),
    );
    ok(
        "map frame in new space",
        vspace::map_frame(f2, root, va, RW, 0),
    );

    // Unmapping a page table detaches what is mapped through it: the frame
    // mapped through it must be unmapped and can't be mapped again until a
    // table is back.
    ok("unmap level-0 table", vspace::unmap_page_table(l0));
    ok("unmap frame", vspace::unmap_frame(f1));
    let no_table = vspace::map_frame(f1, VSPACE, va, RW, 0);
    fails(
        "frame needs the table again",
        no_table,
        error::FAILED_LOOKUP,
    );
    ok(
        "map the spare table",
        vspace::map_page_table(spare, VSPACE, va),
    );
    let remapped = vspace::map_frame(f1, VSPACE, va, rights::READ, attr::EXECUTE_NEVER);
    ok("map frame again", remapped);
    // SAFETY: mapped read-only; a fresh table, the same frame.
    let again = unsafe { mapped::<u64>(va).read_volatile() };
    assert_eq!(again, VALUE, "value didn't survive a new table");

    // Revoking the region deletes every object: frames and tables are
    // unmapped, and the new address space's ASID is freed.
    ok("revoke", slot(parent).revoke());
    println!("page tables, frames, ASIDs and unmapping behave");
}

/// Identifying capabilities without invoking them: the type, whether the
/// capability is the only one to its object (copies, including the one a
/// thread keeps of its IPC buffer, make it shared), and whether it is
/// mapped. `f1` and `l0` are mapped, `big` isn't, and `copy` is a free slot.
fn identifying(parent: usize, big: usize, l0: usize, f1: usize, copy: usize) {
    let exclusive = identify::EXCLUSIVE;
    let is_mapped = identify::MAPPED;
    let frame = object::FRAME_4K;

    let only = Ok((frame, exclusive | is_mapped));
    assert_eq!(identified(f1), only, "a mapped frame with one capability");

    ok("copy a frame", slot(copy).copy_from(slot(f1), rights::ALL));
    assert_eq!(
        identified(f1),
        Ok((frame, is_mapped)),
        "a copied frame isn't exclusive"
    );
    assert_eq!(
        identified(copy),
        Ok((frame, 0)),
        "nor is its copy, which starts unmapped"
    );

    ok("delete the copy", slot(copy).delete());
    assert_eq!(identified(f1), only, "exclusive again");

    let buffer = identified(init_slot::IPC_BUFFER);
    assert_eq!(
        buffer,
        Ok((frame, is_mapped)),
        "the TCB holds a copy of the IPC buffer"
    );

    let table = Ok((object::PAGE_TABLE, exclusive | is_mapped));
    assert_eq!(identified(l0), table, "a mapped page table");
    let unmapped = Ok((object::FRAME_2M, exclusive));
    assert_eq!(identified(big), unmapped, "an unmapped 2 MiB frame");

    let pool = identified(init_slot::ASID_POOL).map(|i| i.0);
    assert_eq!(pool, Ok(object::ASID_POOL), "the ASID pool");
    let region = Ok((object::UNTYPED, 0));
    assert_eq!(
        identified(parent),
        region,
        "untyped memory is never exclusive"
    );
    assert_eq!(identified(copy), Err(error::FAILED_LOOKUP), "an empty slot");
}
