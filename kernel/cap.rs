//! Capabilities: what a slot in a capability table holds. A capability names
//! a kernel object and carries what its holder may do with it.
//!
//! Representation and rules follow seL4. Each variant records the object's
//! address and the per-capability state that seL4 keeps in its capability
//! bits (an untyped region's watermark, a frame's mapping). Architecture
//! objects (page tables, frames) are in `Cap::Arch`.
//!
//! An all-zero `Cap` is `Cap::Null`, so zeroed memory is a table of empty
//! slots.

use crate::arch::{self, ArchCap};
use crate::cspace::SLOT_BITS;
use crate::thread::TCB_BITS;

// `repr(u8)` gives a defined layout with the tag first and tag 0 = Null;
// small fields come first in each variant so they pack after the tag.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cap {
    Null = 0,
    /// Untyped memory at physical `base`, `1 << size_bits` bytes, of which
    /// the first `free_index` bytes have been retyped into objects.
    Untyped { size_bits: u8, device: bool, base: usize, free_index: usize },
    /// A table of `1 << radix` slots at kernel address `ptr`. Resolving an
    /// address through it first matches `guard_bits` bits against `guard`.
    CNode { radix: u8, guard_bits: u8, ptr: usize, guard: usize },
    /// A thread control block at kernel address `ptr`.
    Thread { ptr: usize },
    Arch(ArchCap),
}

impl Cap {
    pub fn is_null(&self) -> bool {
        matches!(self, Cap::Null)
    }

    /// The physical memory the object occupies, if it has a fixed extent:
    /// `(base, size_bits)`.
    pub fn extent(&self) -> Option<(usize, u8)> {
        match *self {
            Cap::Untyped { base, size_bits, .. } => Some((base, size_bits)),
            Cap::CNode { ptr, radix, .. } => Some((arch::kernel_to_phys(ptr), radix + SLOT_BITS as u8)),
            Cap::Thread { ptr } => Some((arch::kernel_to_phys(ptr), TCB_BITS as u8)),
            Cap::Arch(a) => a.extent(),
            Cap::Null => None,
        }
    }

    /// Whether `b` names the same object as `self` or, for untyped memory,
    /// an object inside it. Decides parenthood in the derivation tree.
    pub fn same_region_as(&self, b: &Cap) -> bool {
        match (*self, *b) {
            (Cap::Untyped { base, size_bits, .. }, _) => match b.extent() {
                Some((b_base, b_bits)) => {
                    let top = base + (1usize << size_bits) - 1;
                    let b_top = b_base + (1usize << b_bits) - 1;
                    base <= b_base && b_top <= top
                }
                None => false,
            },
            (Cap::CNode { ptr, radix, .. }, Cap::CNode { ptr: p2, radix: r2, .. }) => ptr == p2 && radix == r2,
            (Cap::Thread { ptr }, Cap::Thread { ptr: p2 }) => ptr == p2,
            (Cap::Arch(a), Cap::Arch(b)) => a.same_region_as(&b),
            _ => false,
        }
    }

    /// Whether `b` names exactly the same object as `self`. Two untyped
    /// capabilities never do: each is its own allocator.
    pub fn same_object_as(&self, b: &Cap) -> bool {
        match (*self, *b) {
            (Cap::Untyped { .. }, _) => false,
            (Cap::Arch(a), Cap::Arch(b)) => a.same_object_as(&b),
            _ => self.same_region_as(b),
        }
    }

    /// Whether a copy derived from `src` becomes a parent of later copies
    /// (seL4's `isCapRevocable`): copies of untyped memory do, so revoking
    /// one copy deletes what was retyped through it.
    pub fn is_revocable_copy(&self, _src: &Cap) -> bool {
        matches!(self, Cap::Untyped { .. })
    }

    /// The capability with its rights reduced to `rights` (abi `rights`
    /// bits). Only frames carry rights so far.
    pub fn masked(&self, rights: usize) -> Cap {
        match *self {
            Cap::Arch(a) => Cap::Arch(a.masked(rights)),
            other => other,
        }
    }
}
