//! Address spaces as capabilities: which root page table each address-space
//! identifier (ASID) names, activating a thread's address space, and what
//! deleting a frame or page-table capability does to the page tables.
//!
//! Frame and page-table capabilities record their mapping as (ASID, virtual
//! address), as in seL4, and the kernel finds the root table through this
//! table. Deleting an address space frees its ASID, so a capability still
//! recording a mapping in it can't reach page tables that were reused.

use super::caps::{ArchCap, PtMapping};
use super::paging::{self, Entry};
use crate::global::Global;

/// ASIDs the kernel can hand out. ASID 0 is the kernel's own.
///
/// TODO: seL4 keeps only a small top-level table here and retypes pools of
/// ASIDs from untyped memory, so the count isn't fixed; do the same, and use
/// the hardware ASID field in `satp` to avoid flushing the TLB on every
/// switch (M3, mapping invocations).
pub const MAX_ASIDS: usize = 4096;

/// Root table physical address per ASID; 0 = free.
static ASIDS: Global<[usize; MAX_ASIDS]> = Global::new([0; MAX_ASIDS]);

fn asids() -> &'static mut [usize; MAX_ASIDS] {
    // SAFETY: never held across calls (see global.rs).
    unsafe { ASIDS.get() }
}

/// Record that `asid` names the address space rooted at `root`.
pub fn assign_asid(asid: u16, root: usize) {
    assert!(asid != 0 && asids()[asid as usize] == 0, "asid {asid} already in use");
    asids()[asid as usize] = root;
}

/// The root table of the address space `asid` names, if any.
pub fn asid_root(asid: u16) -> Option<usize> {
    match asids().get(asid as usize) {
        Some(&root) if root != 0 => Some(root),
        _ => None,
    }
}

/// Switch to the address space a thread's address-space capability names.
/// Returns false if the capability isn't a live address-space root.
pub fn activate(cap: crate::cap::Cap) -> bool {
    let crate::cap::Cap::Arch(ArchCap::PageTable { pa, mapping: PtMapping::Root { asid } }) = cap else {
        return false;
    };
    if asid_root(asid) != Some(pa) {
        return false;
    }
    paging::activate_root(pa);
    true
}

/// What deleting an architecture capability does. A frame capability that
/// maps its frame unmaps it, whether or not it was the last capability to
/// the frame (each capability is its own mapping). Deleting the last
/// capability to a page table unmaps it, or frees the ASID of a root.
pub fn finalise(cap: ArchCap, last: bool) {
    match cap {
        ArchCap::Frame { pa, size_bits, asid, va, .. } if asid != 0 => {
            let level = (size_bits as usize - 12) / 9;
            unmap(asid, va, level, pa);
        }
        ArchCap::PageTable { pa, mapping: PtMapping::Mapped { asid, va, level } } if last => {
            // A table at `level` is pointed to by an entry one level up.
            unmap(asid, va, level as usize + 1, pa);
        }
        ArchCap::PageTable { pa, mapping: PtMapping::Root { asid } } if last => {
            if asid_root(asid) == Some(pa) {
                asids()[asid as usize] = 0;
                // The space may be active; its mappings must not survive.
                riscv::asm::sfence_vma_all();
            }
        }
        _ => {}
    }
}

/// Clear the entry at `level` for `va` in address space `asid`, if it still
/// points at `pa`.
fn unmap(asid: u16, va: usize, level: usize, pa: usize) {
    let Some(root) = asid_root(asid) else { return };
    if let Some(entry) = paging::entry_at(root, va, level)
        && entry.is_valid()
        && entry.pa() == pa
    {
        *entry = Entry::EMPTY;
        // TODO: flush only `va` in `asid` once ASIDs are used in hardware.
        riscv::asm::sfence_vma_all();
    }
}
