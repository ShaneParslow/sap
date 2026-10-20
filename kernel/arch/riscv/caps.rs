//! RISC-V capabilities: page tables (one type for every level, as in seL4 on
//! RISC-V) and frames of the three Sv39 page sizes.

use sap_abi::{object, rights};

/// Where a page table sits in an address space.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PtMapping {
    Unmapped = 0,
    /// The root of an address space with this address-space identifier.
    Root { asid: u16 },
    /// A table at `level` (0 = leaf tables, 1 = middle) covering virtual
    /// addresses from `va`, in address space `asid`.
    Mapped { asid: u16, level: u8, va: usize },
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchCap {
    /// A page table at physical address `pa`.
    PageTable { pa: usize, mapping: PtMapping } = 1,
    /// A frame of `1 << size_bits` bytes (12, 21 or 30) at physical `pa`.
    /// Each frame capability can map the frame once, at `va` in address
    /// space `asid`; `asid` 0 (the kernel's) means unmapped.
    Frame { size_bits: u8, rights: u8, asid: u16, pa: usize, va: usize },
    /// The kernel's pool of address-space identifiers.
    ///
    /// TODO: seL4 retypes ASID pools from untyped memory through an ASID
    /// control capability; sap has one fixed pool for now.
    AsidPool,
}

impl ArchCap {
    pub fn extent(&self) -> Option<(usize, u8)> {
        match *self {
            ArchCap::PageTable { pa, .. } => Some((pa, 12)),
            ArchCap::Frame { pa, size_bits, .. } => Some((pa, size_bits)),
            ArchCap::AsidPool => None,
        }
    }

    pub fn same_region_as(&self, b: &ArchCap) -> bool {
        match (*self, *b) {
            (ArchCap::PageTable { pa, .. }, ArchCap::PageTable { pa: p2, .. }) => pa == p2,
            (ArchCap::AsidPool, ArchCap::AsidPool) => true,
            (ArchCap::Frame { pa, size_bits, .. }, ArchCap::Frame { pa: p2, size_bits: b2, .. }) => {
                pa <= p2 && p2 + (1usize << b2) <= pa + (1usize << size_bits)
            }
            _ => false,
        }
    }

    pub fn same_object_as(&self, b: &ArchCap) -> bool {
        match (*self, *b) {
            (ArchCap::Frame { pa, size_bits, .. }, ArchCap::Frame { pa: p2, size_bits: b2, .. }) => {
                pa == p2 && size_bits == b2
            }
            _ => self.same_region_as(b),
        }
    }

    /// The object type (`sap_abi::object`) this capability names.
    pub fn object_type(&self) -> usize {
        match *self {
            ArchCap::PageTable { .. } => object::PAGE_TABLE,
            ArchCap::Frame { size_bits: 12, .. } => object::FRAME_4K,
            ArchCap::Frame { size_bits: 21, .. } => object::FRAME_2M,
            ArchCap::Frame { .. } => object::FRAME_1G,
            ArchCap::AsidPool => object::ASID_POOL,
        }
    }

    /// Whether this capability maps its frame or page table somewhere.
    pub fn is_mapped(&self) -> bool {
        match *self {
            ArchCap::PageTable { mapping, .. } => mapping != PtMapping::Unmapped,
            ArchCap::Frame { asid, .. } => asid != 0,
            ArchCap::AsidPool => false,
        }
    }

    pub fn masked(&self, mask: usize) -> ArchCap {
        match *self {
            ArchCap::Frame { size_bits, rights, asid, pa, va } => {
                ArchCap::Frame { size_bits, rights: rights & (mask as u8), asid, pa, va }
            }
            other => other,
        }
    }
}

/// log2 of the size of an architecture object type, or None if the type
/// isn't one.
pub fn object_bits(object_type: usize) -> Option<usize> {
    match object_type {
        object::PAGE_TABLE | object::FRAME_4K => Some(12),
        object::FRAME_2M => Some(21),
        object::FRAME_1G => Some(30),
        _ => None,
    }
}

pub fn is_frame_type(object_type: usize) -> bool {
    matches!(object_type, object::FRAME_4K | object::FRAME_2M | object::FRAME_1G)
}

/// The capability for a new architecture object at physical `pa`.
pub fn create_object(object_type: usize, pa: usize) -> ArchCap {
    match object_type {
        object::PAGE_TABLE => ArchCap::PageTable { pa, mapping: PtMapping::Unmapped },
        frame => unmapped_frame(pa, object_bits(frame).expect("not a frame type") as u8, FRAME_RIGHTS_ALL),
    }
}

/// A frame capability that maps nothing.
pub fn unmapped_frame(pa: usize, size_bits: u8, rights: u8) -> ArchCap {
    ArchCap::Frame { size_bits, rights, asid: 0, pa, va: 0 }
}

/// Rights a fresh frame capability carries.
pub const FRAME_RIGHTS_ALL: u8 = (rights::READ | rights::WRITE) as u8;
