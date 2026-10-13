//! Capability spaces: slots, the derivation tree, and resolving capability
//! addresses. All three follow seL4.
//!
//! **Slots.** A CNode is an array of `Slot`s. Each slot holds a capability
//! and its place in the derivation tree.
//!
//! **Derivation tree.** Every slot in the system is on one doubly linked
//! list (seL4's "mapping database"), ordered so that a capability's
//! descendants follow it directly. A capability derived from another is
//! inserted right after its source; whether it is a *child* or a sibling is
//! computed (`is_parent_of`), not stored. Revoking a capability deletes the
//! run of children that follows it. No memory is allocated: the links live
//! in the slots.
//!
//! **Addresses.** A capability address (a CPtr) is resolved through a tree
//! of CNodes like a page-table walk: each CNode consumes `guard_bits` bits
//! that must equal its guard, then `radix` bits that index a slot. If that
//! slot holds another CNode and bits remain, the walk continues there.

use core::ptr;

use crate::cap::Cap;

/// log2 of the size of a slot in memory.
pub const SLOT_BITS: usize = 6;

#[repr(C)]
pub struct Slot {
    pub cap: Cap,
    prev: *mut Slot,
    next: *mut Slot,
    /// Whether later capabilities in the run after this one can be its
    /// children (seL4's `mdbRevocable`): true for originals (boot
    /// capabilities and newly retyped objects) and for copies of untyped
    /// memory.
    revocable: bool,
}

const _: () = assert!(core::mem::size_of::<Slot>() <= 1 << SLOT_BITS);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// The walk reached a slot that isn't a CNode, or the root isn't one.
    InvalidRoot,
    /// A CNode's guard didn't match the address bits.
    GuardMismatch,
    /// The bits left didn't fit the CNode at this level.
    DepthMismatch,
    /// The resolved slot holds no capability (for lookups that need one).
    MissingCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteError {
    /// TODO: deleting a CNode deletes its contents recursively. seL4 avoids
    /// unbounded recursion with "zombie" capabilities; until sap has them,
    /// nesting deeper than `MAX_DELETE_DEPTH` CNodes is refused.
    TooDeep,
}

const MAX_DELETE_DEPTH: usize = 32;

fn mask(bits: u32) -> usize {
    if bits >= usize::BITS { usize::MAX } else { (1 << bits) - 1 }
}

/// The slot at index `i` of the CNode at kernel address `cnode`.
fn slot_at(cnode: usize, i: usize) -> *mut Slot {
    (cnode + (i << SLOT_BITS)) as *mut Slot
}

/// Resolve `depth` bits of `cptr` starting at CNode capability `root`.
/// Returns the slot reached and the number of bits left unresolved, which
/// is nonzero when the walk stopped at a capability that isn't a CNode.
pub fn resolve(root: Cap, cptr: usize, depth: usize) -> Result<(*mut Slot, usize), LookupError> {
    let mut node = root;
    let mut left = depth;
    loop {
        let Cap::CNode { ptr, radix, guard_bits, guard } = node else {
            return Err(LookupError::InvalidRoot);
        };
        let (radix, guard_bits) = (radix as usize, guard_bits as usize);
        let level = radix + guard_bits;
        if level == 0 || level > left {
            return Err(LookupError::DepthMismatch);
        }
        if guard_bits > 0 {
            let found = (cptr >> (left - guard_bits)) & mask(guard_bits as u32);
            if found != guard & mask(guard_bits as u32) {
                return Err(LookupError::GuardMismatch);
            }
        }
        let index = (cptr >> (left - level)) & mask(radix as u32);
        let slot = slot_at(ptr, index);
        left -= level;
        if left == 0 {
            return Ok((slot, 0));
        }
        // SAFETY: slot is inside a live CNode.
        let next = unsafe { (*slot).cap };
        if !matches!(next, Cap::CNode { .. }) {
            return Ok((slot, left));
        }
        node = next;
    }
}

/// The slot exactly `depth` bits of `cptr` name, starting at `root`: the
/// form CNode invocations use to name a slot that may be empty.
pub fn lookup_target(root: Cap, cptr: usize, depth: usize) -> Result<*mut Slot, LookupError> {
    match resolve(root, cptr, depth)? {
        (slot, 0) => Ok(slot),
        _ => Err(LookupError::DepthMismatch),
    }
}

/// The capability a full-width address names in the space rooted at `root`.
pub fn lookup_cap(root: Cap, cptr: usize) -> Result<(*mut Slot, Cap), LookupError> {
    let (slot, _) = resolve(root, cptr, usize::BITS as usize)?;
    // SAFETY: resolve returns slots inside live CNodes.
    let cap = unsafe { (*slot).cap };
    if cap.is_null() { Err(LookupError::MissingCapability) } else { Ok((slot, cap)) }
}

/// Put an original capability (a boot capability or a newly created
/// object) into empty slot `dest`. If `parent` is given, the new
/// capability becomes its child, placed right after it; otherwise it starts
/// a tree of its own.
///
/// # Safety
/// `dest` (and `parent`) must be slots in live CNodes or TCBs; `dest` empty.
pub unsafe fn insert_original(cap: Cap, parent: Option<*mut Slot>, dest: *mut Slot) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        debug_assert!((*dest).cap.is_null());
        (*dest).cap = cap;
        (*dest).revocable = true;
        (*dest).prev = ptr::null_mut();
        (*dest).next = ptr::null_mut();
        if let Some(parent) = parent {
            link_after(parent, dest);
        }
    }
}

/// Put `cap`, derived from the capability in `src`, into empty slot `dest`,
/// right after `src` in the derivation tree.
///
/// # Safety
/// Both must be slots in live CNodes or TCBs; `dest` empty, `src` not.
pub unsafe fn insert_derived(cap: Cap, src: *mut Slot, dest: *mut Slot) {
    // SAFETY: per the caller's guarantee.
    unsafe {
        debug_assert!((*dest).cap.is_null());
        (*dest).cap = cap;
        (*dest).revocable = cap.is_revocable_copy(&(*src).cap);
        link_after(src, dest);
    }
}

/// Move the capability in `src` to empty slot `dest`, keeping its place in
/// the derivation tree. `src` ends empty.
///
/// # Safety
/// Both must be slots in live CNodes or TCBs; `dest` empty.
pub unsafe fn move_cap(src: *mut Slot, dest: *mut Slot) {
    // SAFETY: per the caller's guarantee; neighbours on the list are live.
    unsafe {
        if src == dest {
            return;
        }
        let s = &mut *src;
        let d = &mut *dest;
        d.cap = s.cap;
        d.revocable = s.revocable;
        d.prev = s.prev;
        d.next = s.next;
        if !d.prev.is_null() {
            (*d.prev).next = dest;
        }
        if !d.next.is_null() {
            (*d.next).prev = dest;
        }
        s.cap = Cap::Null;
        s.prev = ptr::null_mut();
        s.next = ptr::null_mut();
        s.revocable = false;
    }
}

unsafe fn link_after(prev: *mut Slot, slot: *mut Slot) {
    // SAFETY: both live (caller's guarantee); prev's successor is live.
    unsafe {
        let next = (*prev).next;
        (*slot).prev = prev;
        (*slot).next = next;
        (*prev).next = slot;
        if !next.is_null() {
            (*next).prev = slot;
        }
    }
}

/// Remove `slot` from the derivation tree and empty it.
unsafe fn empty(slot: *mut Slot) {
    // SAFETY: slot is live (caller's guarantee); its neighbours are live.
    unsafe {
        let s = &mut *slot;
        if !s.prev.is_null() {
            (*s.prev).next = s.next;
        }
        if !s.next.is_null() {
            (*s.next).prev = s.prev;
        }
        s.cap = Cap::Null;
        s.prev = ptr::null_mut();
        s.next = ptr::null_mut();
        s.revocable = false;
    }
}

/// Whether the capability in `b` is a child of the one in `a` (seL4's
/// `isMDBParentOf`).
fn is_parent_of(a: &Slot, b: &Slot) -> bool {
    a.revocable && a.cap.same_region_as(&b.cap)
}

/// Whether `slot` holds the last capability to its object: no neighbour on
/// the list names the same object. (Capabilities to one object are always
/// adjacent, since copies are inserted next to their source.)
unsafe fn is_final(slot: *mut Slot) -> bool {
    // SAFETY: slot and its neighbours are live (caller's guarantee).
    unsafe {
        let s = &*slot;
        let same = |other: *mut Slot| !other.is_null() && (*other).cap.same_object_as(&s.cap);
        !same(s.prev) && !same(s.next)
    }
}

/// Whether the capability in `slot` has children.
///
/// # Safety
/// `slot` must be a live slot.
pub unsafe fn has_children(slot: *mut Slot) -> bool {
    // SAFETY: per the caller's guarantee; the next slot is live.
    unsafe {
        let next = (*slot).next;
        !next.is_null() && is_parent_of(&*slot, &*next)
    }
}

/// Delete the capability in `slot`. If it was the last capability to its
/// object, the object is finalised: a thread stops, a CNode's contents are
/// deleted, a mapped frame is unmapped. The object's memory stays where it
/// is until the untyped memory it came from is revoked.
///
/// # Safety
/// `slot` must be a live slot.
pub unsafe fn delete(slot: *mut Slot) -> Result<(), DeleteError> {
    // SAFETY: per the caller's guarantee.
    unsafe { delete_at_depth(slot, 0) }
}

unsafe fn delete_at_depth(slot: *mut Slot, depth: usize) -> Result<(), DeleteError> {
    // SAFETY: slot is live (caller's guarantee).
    unsafe {
        let cap = (*slot).cap;
        if cap.is_null() {
            return Ok(());
        }
        let last = is_final(slot);
        // Empty the slot before finalising, so a CNode that (directly or
        // not) contains this slot can't reach it again while its contents
        // are deleted.
        empty(slot);
        finalise(cap, last, depth)
    }
}

unsafe fn finalise(cap: Cap, last: bool, depth: usize) -> Result<(), DeleteError> {
    match cap {
        Cap::CNode { ptr, radix, .. } if last => {
            if depth >= MAX_DELETE_DEPTH {
                return Err(DeleteError::TooDeep);
            }
            for i in 0..1usize << radix {
                // SAFETY: the CNode is live until its memory is reused, which
                // can't happen while this kernel call runs.
                unsafe { delete_at_depth(slot_at(ptr, i), depth + 1)? };
            }
            Ok(())
        }
        Cap::Thread { ptr } if last => {
            // SAFETY: the TCB is live (see above).
            unsafe { crate::thread::finalise(ptr as *mut crate::thread::Tcb, depth) }
        }
        Cap::Arch(a) => {
            crate::arch::finalise_cap(a, last);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Delete every capability derived from the one in `slot`; the capability
/// itself stays.
///
/// # Safety
/// `slot` must be a live slot.
pub unsafe fn revoke(slot: *mut Slot) -> Result<(), DeleteError> {
    // SAFETY: per the caller's guarantee; children are live slots.
    unsafe {
        while has_children(slot) {
            delete((*slot).next)?;
        }
    }
    Ok(())
}

impl Slot {
    /// Delete the capabilities in the slots of the TCB-embedded array or
    /// CNode `slots` (used when finalising a thread).
    ///
    /// # Safety
    /// The slots must be live.
    pub(crate) unsafe fn delete_all(slots: &mut [Slot], depth: usize) -> Result<(), DeleteError> {
        for slot in slots {
            // SAFETY: per the caller's guarantee.
            unsafe { delete_at_depth(slot, depth + 1)? };
        }
        Ok(())
    }
}
