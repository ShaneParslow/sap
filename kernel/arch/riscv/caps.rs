//! RISC-V capabilities: page tables (one type for every level, as in seL4 on
//! RISC-V) and frames of the three Sv39 page sizes.

use sap_abi::rights;

/// Where a page table sits in an address space.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PtMapping {
    // TODO: constructed once page tables can be retyped (M3, untyped).
    #[allow(dead_code)]
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
}

impl ArchCap {
    pub fn extent(&self) -> Option<(usize, u8)> {
        match *self {
            ArchCap::PageTable { pa, .. } => Some((pa, 12)),
            ArchCap::Frame { pa, size_bits, .. } => Some((pa, size_bits)),
        }
    }

    pub fn same_region_as(&self, b: &ArchCap) -> bool {
        match (*self, *b) {
            (ArchCap::PageTable { pa, .. }, ArchCap::PageTable { pa: p2, .. }) => pa == p2,
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

    pub fn masked(&self, mask: usize) -> ArchCap {
        match *self {
            ArchCap::Frame { size_bits, rights, asid, pa, va } => {
                ArchCap::Frame { size_bits, rights: rights & (mask as u8), asid, pa, va }
            }
            other => other,
        }
    }
}

/// A frame capability that maps nothing.
pub fn unmapped_frame(pa: usize, size_bits: u8, rights: u8) -> ArchCap {
    ArchCap::Frame { size_bits, rights, asid: 0, pa, va: 0 }
}

/// Rights a fresh frame capability carries.
pub const FRAME_RIGHTS_ALL: u8 = (rights::READ | rights::WRITE) as u8;
