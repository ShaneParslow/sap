//! Retyping untyped memory into kernel objects, as seL4 does. This is the
//! only way kernel objects come into existence after boot, so every object's
//! memory is accounted to whoever held the untyped capability.
//!
//! An untyped capability hands out memory with a watermark (`free_index`):
//! objects are placed at the next suitably aligned address, and memory below
//! the watermark is never reused while anything retyped from the region
//! still exists. Once all of it has been revoked (no children remain), the
//! next retype resets the watermark to zero. Objects are zeroed when
//! created.

use sap_abi::{error, object, rights};

use crate::arch;
use crate::cap::Cap;
use crate::cspace::{self, Slot, SLOT_BITS};
use crate::endpoint::{ENDPOINT_BITS, REPLY_BITS};
use crate::notification::NOTIFICATION_BITS;
use crate::thread::{Tcb, TCB_BITS};

/// The largest CNode, in bits of slots (seL4 allows up to the word size; 26
/// bits is a 4 GiB table).
const MAX_CNODE_RADIX: usize = 26;

/// What the caller asked for.
pub struct Request {
    pub object_type: usize,
    pub size_bits: usize,
    /// The CNode capability whose slots receive the new capabilities.
    pub dest: Cap,
    pub offset: usize,
    pub count: usize,
}

/// log2 of the size of one object of `object_type` with `size_bits`.
fn object_bits(object_type: usize, size_bits: usize) -> Result<usize, usize> {
    match object_type {
        object::UNTYPED => {
            if !(object::MIN_UNTYPED_BITS..=object::MAX_UNTYPED_BITS).contains(&size_bits) {
                return Err(error::RANGE_ERROR);
            }
            Ok(size_bits)
        }
        object::TCB => Ok(TCB_BITS),
        object::CNODE => {
            if !(1..=MAX_CNODE_RADIX).contains(&size_bits) {
                return Err(error::RANGE_ERROR);
            }
            Ok(size_bits + SLOT_BITS)
        }
        object::NOTIFICATION => Ok(NOTIFICATION_BITS),
        object::ENDPOINT => Ok(ENDPOINT_BITS),
        object::REPLY => Ok(REPLY_BITS),
        other => arch::object_bits(other).ok_or(error::INVALID_ARGUMENT),
    }
}

/// Retype part of the untyped memory capability in `slot` per `request`.
/// Returns an error code without changing anything if the request can't be
/// met.
///
/// # Safety
/// `slot` must be a live slot holding an untyped capability.
pub unsafe fn retype(slot: *mut Slot, request: &Request) -> Result<(), usize> {
    // SAFETY: per the caller's guarantee.
    let Cap::Untyped { size_bits: region_bits, device, base, free_index } = (unsafe { (*slot).cap }) else {
        return Err(error::INVALID_CAPABILITY);
    };
    let bits = object_bits(request.object_type, request.size_bits)?;
    if device && !matches!(request.object_type, object::UNTYPED) && !arch::is_frame_type(request.object_type) {
        // Device memory may hold registers; only untyped and frames can
        // describe it.
        return Err(error::INVALID_ARGUMENT);
    }
    if request.count == 0 || request.count > object::MAX_RETYPE_COUNT {
        return Err(error::RANGE_ERROR);
    }

    // Destination slots: a range of empty slots in a CNode.
    let Cap::CNode { ptr: dest, radix, .. } = request.dest else {
        return Err(error::INVALID_CAPABILITY);
    };
    if request.offset.checked_add(request.count).is_none_or(|end| end > 1 << radix) {
        return Err(error::RANGE_ERROR);
    }
    let dest_slot = |i: usize| (dest + ((request.offset + i) << SLOT_BITS)) as *mut Slot;
    // SAFETY: the slots are inside the destination CNode.
    if (0..request.count).any(|i| unsafe { !(*dest_slot(i)).cap.is_null() }) {
        return Err(error::DELETE_FIRST);
    }

    // Placement: after the watermark, or from the start if everything
    // retyped from this region has been revoked.
    // SAFETY: slot is live.
    let reset = unsafe { !cspace::has_children(slot) };
    let start = if reset { 0 } else { free_index };
    let aligned = start.next_multiple_of(1 << bits);
    let end = aligned + (request.count << bits);
    if bits > region_bits as usize || end > 1 << region_bits {
        return Err(error::NOT_ENOUGH_MEMORY);
    }

    // Commit: advance the watermark, then create and record each object.
    // SAFETY: slot is live and holds this untyped capability.
    unsafe {
        (*slot).cap = Cap::Untyped { size_bits: region_bits, device, base, free_index: end };
    }
    for i in 0..request.count {
        let pa = base + aligned + (i << bits);
        if !device {
            // SAFETY: this memory is inside the region and above the old
            // watermark (or the region has no children), so nothing uses it.
            unsafe { core::ptr::write_bytes(arch::phys_to_virt(pa) as *mut u8, 0, 1 << bits) };
        }
        let cap = create(request.object_type, request.size_bits, pa, device);
        // SAFETY: the parent and destination slots are live; the
        // destination is empty (checked above).
        unsafe { cspace::insert_original(cap, Some(slot), dest_slot(i)) };
    }
    Ok(())
}

/// The capability for a new, zeroed object at physical address `pa`.
fn create(object_type: usize, size_bits: usize, pa: usize, device: bool) -> Cap {
    let ptr = arch::phys_to_virt(pa);
    match object_type {
        object::UNTYPED => Cap::Untyped { size_bits: size_bits as u8, device, base: pa, free_index: 0 },
        // SAFETY: the TCB's memory was just zeroed and belongs to it.
        object::TCB => Cap::Thread { ptr: unsafe { Tcb::init(ptr) } as usize },
        object::CNODE => Cap::CNode { radix: size_bits as u8, guard_bits: 0, ptr, guard: 0 },
        object::NOTIFICATION => Cap::Notification { rights: rights::ALL as u8, ptr, badge: 0 },
        object::ENDPOINT => Cap::Endpoint { rights: rights::ALL as u8, ptr, badge: 0 },
        object::REPLY => Cap::Reply { ptr },
        other => Cap::Arch(arch::create_object(other, pa)),
    }
}
