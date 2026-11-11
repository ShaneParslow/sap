//! Address spaces as capabilities: which root page table each address-space
//! identifier (ASID) names, the page-table, frame and ASID-pool
//! invocations, activating a thread's address space, and what deleting a
//! frame or page-table capability does to the page tables. All as in seL4 on
//! RISC-V.
//!
//! Frame and page-table capabilities record their mapping as (ASID, virtual
//! address), and the kernel finds the root table through the ASID table.
//! Deleting an address space frees its ASID, so a capability still
//! recording a mapping in it can't reach page tables that were reused.

use sap_abi::{attr, error, label, rights};

use super::caps::{ArchCap, PtMapping};
use super::paging::{self, Entry, MapError, USER_END};
use crate::cap::Cap;
use crate::cspace::Slot;
use crate::global::Global;
use crate::invocation::Message;
use crate::mm::Perms;

/// ASIDs the kernel can hand out. ASID 0 is the kernel's own.
///
/// TODO: seL4 keeps only a small top-level table here and retypes pools of
/// ASIDs from untyped memory, so the count isn't fixed; do the same.
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

/// The root table and ASID of a live address-space capability.
fn vspace_of(cap: Cap) -> Option<(usize, u16)> {
    match cap {
        Cap::Arch(ArchCap::PageTable { pa, mapping: PtMapping::Root { asid } }) if asid_root(asid) == Some(pa) => {
            Some((pa, asid))
        }
        _ => None,
    }
}

/// Whether `cap` is a live address-space root.
pub fn is_root(cap: Cap) -> bool {
    vspace_of(cap).is_some()
}

/// Switch to the address space a thread's address-space capability names.
/// Returns false if the capability isn't a live address-space root.
pub fn activate(cap: Cap) -> bool {
    match vspace_of(cap) {
        Some((root, asid)) => {
            paging::activate_root(root, asid);
            true
        }
        None => false,
    }
}

/// Handle an invocation of an architecture capability in `slot`.
pub fn invoke(slot: *mut Slot, cap: ArchCap, m: &mut Message) -> Result<usize, usize> {
    match (cap, m.label()) {
        (ArchCap::PageTable { pa, mapping: PtMapping::Unmapped }, label::PAGE_TABLE_MAP) => {
            let va = m.word(0)?;
            let (root, asid) = vspace_of(m.extra_cap(0)?.1).ok_or(error::INVALID_CAPABILITY)?;
            if va >= USER_END {
                return Err(error::INVALID_ARGUMENT);
            }
            let (level, base) = paging::install_table(root, va, pa).map_err(|_| error::DELETE_FIRST)?;
            set_cap(slot, ArchCap::PageTable { pa, mapping: PtMapping::Mapped { asid, level: level as u8, va: base } });
            Ok(0)
        }
        (ArchCap::PageTable { .. }, label::PAGE_TABLE_MAP) => Err(error::INVALID_CAPABILITY),
        (ArchCap::PageTable { pa, mapping: PtMapping::Mapped { asid, level, va } }, label::PAGE_TABLE_UNMAP) => {
            unmap(asid, va, level as usize + 1, pa);
            paging::clear_table(pa);
            set_cap(slot, ArchCap::PageTable { pa, mapping: PtMapping::Unmapped });
            Ok(0)
        }
        (ArchCap::PageTable { .. }, label::PAGE_TABLE_UNMAP) => Ok(0),
        (ArchCap::Frame { size_bits, rights: cap_rights, asid, pa, .. }, label::FRAME_MAP) => {
            if asid != 0 {
                // Each frame capability maps once; copy it to map again.
                return Err(error::INVALID_CAPABILITY);
            }
            let (va, wanted, attributes) = (m.word(0)?, m.word(1)?, m.word(2)?);
            let (root, space) = vspace_of(m.extra_cap(0)?.1).ok_or(error::INVALID_CAPABILITY)?;
            let size = 1usize << size_bits;
            if va % size != 0 {
                return Err(error::ALIGNMENT_ERROR);
            }
            if va.checked_add(size).is_none_or(|end| end > USER_END) {
                return Err(error::INVALID_ARGUMENT);
            }
            let granted = wanted & cap_rights as usize;
            if granted & rights::READ == 0 {
                // RISC-V has no write-only pages, and an inaccessible page
                // isn't a mapping.
                return Err(error::INVALID_ARGUMENT);
            }
            let mut perms = Perms::READ;
            perms.set(Perms::WRITE, granted & rights::WRITE != 0);
            perms.set(Perms::EXECUTE, attributes & attr::EXECUTE_NEVER == 0);
            perms.set(Perms::CAPABILITIES, attributes & attr::CAPABILITIES != 0);
            let level = (size_bits as usize - 12) / 9;
            paging::map_user_page(root, va, pa, level, perms).map_err(|e| match e {
                MapError::MissingTable => error::FAILED_LOOKUP,
                MapError::Occupied => error::DELETE_FIRST,
            })?;
            paging::flush_page(space, va);
            set_cap(slot, ArchCap::Frame { size_bits, rights: cap_rights, asid: space, pa, va });
            Ok(0)
        }
        (ArchCap::Frame { size_bits, rights, asid, pa, va }, label::FRAME_UNMAP) => {
            if asid != 0 {
                unmap(asid, va, (size_bits as usize - 12) / 9, pa);
            }
            set_cap(slot, ArchCap::Frame { size_bits, rights, asid: 0, pa, va: 0 });
            Ok(0)
        }
        (ArchCap::Frame { pa, .. }, label::FRAME_GET_ADDRESS) => {
            m.set_result(0, pa)?;
            Ok(1)
        }
        (ArchCap::AsidPool, label::ASID_POOL_ASSIGN) => {
            let (target, cap) = m.extra_cap(0)?;
            let Cap::Arch(ArchCap::PageTable { pa, mapping: PtMapping::Unmapped }) = cap else {
                return Err(error::INVALID_CAPABILITY);
            };
            let asid = (1..MAX_ASIDS).find(|&a| asids()[a] == 0).ok_or(error::NOT_ENOUGH_MEMORY)? as u16;
            paging::make_root(pa);
            paging::flush_asid(asid);
            assign_asid(asid, pa);
            set_cap(target, ArchCap::PageTable { pa, mapping: PtMapping::Root { asid } });
            Ok(0)
        }
        _ => Err(error::ILLEGAL_OPERATION),
    }
}

fn set_cap(slot: *mut Slot, cap: ArchCap) {
    // SAFETY: slot is the live slot the capability was found in; only the
    // capability's own state changes, not its place in the tree.
    unsafe { (*slot).cap = Cap::Arch(cap) };
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
                paging::flush_asid(asid);
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
        // A table entry covers more than one page; flush the whole space.
        if level == 0 {
            paging::flush_page(asid, va);
        } else {
            paging::flush_asid(asid);
        }
    }
}

#[cfg(feature = "audit")]
pub mod audit {
    //! The audit's checks of address spaces (`crate::audit`).

    use sap_abi::object;

    use super::super::paging::{self, UserMapping};
    use super::{ArchCap, MAX_ASIDS, asid_root, asids};
    use crate::audit::ledger::{Identity, Ledger};
    use crate::cap::Cap;

    /// Whether `cap` is a frame capability whose recorded mapping is in
    /// place: the entry where it says it maps its frame points at the frame.
    pub fn mapping_in_place(cap: &Cap) -> bool {
        let Cap::Arch(ArchCap::Frame {
            size_bits,
            asid,
            pa,
            va,
            ..
        }) = *cap
        else {
            return false;
        };
        let Some(root) = (asid != 0).then(|| asid_root(asid)).flatten() else {
            return false;
        };

        let level = (size_bits as usize - 12) / 9;
        let entry = paging::entry_at(root, va, level);
        entry.is_some_and(|e| e.is_valid() && e.pa() == pa)
    }

    /// Check every address space the ASID table names: its root and every
    /// page table in it a live page table, and every page a live frame of
    /// the page's size. Counts each table's and frame's references.
    pub fn address_spaces(ledger: &mut Ledger) {
        for asid in 1..MAX_ASIDS {
            let root = asids()[asid];
            if root == 0 {
                continue;
            }

            let identity = Identity {
                pa: root,
                bits: 12,
                object_type: object::PAGE_TABLE,
            };
            reference(ledger, identity, asid, 0);

            paging::walk_user(root, |mapping| match mapping {
                UserMapping::Table { pa, va, .. } => {
                    let identity = Identity {
                        pa,
                        bits: 12,
                        object_type: object::PAGE_TABLE,
                    };
                    reference(ledger, identity, asid, va);
                }

                UserMapping::Page {
                    pa, va, size_bits, ..
                } => {
                    let object_type = match size_bits {
                        12 => object::FRAME_4K,
                        21 => object::FRAME_2M,
                        _ => object::FRAME_1G,
                    };
                    let identity = Identity {
                        pa,
                        bits: size_bits,
                        object_type,
                    };
                    reference(ledger, identity, asid, va);
                }
            });
        }
    }

    fn reference(ledger: &mut Ledger, identity: Identity, asid: usize, va: usize) {
        let Some(index) = ledger.live(identity) else {
            panic!("audit: ASID {asid} maps {identity:?} at {va:#x}, which isn't live");
        };
        ledger.entries()[index].refs += 1;
    }
}
