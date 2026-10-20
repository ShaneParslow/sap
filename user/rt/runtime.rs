//! Starting the runtime: what a program lets it use on its behalf.

use core::ops::Range;

use sap_sys::abi::object;
use sap_sys::cnode::SlotRef;
use sap_sys::untyped;

#[cfg(feature = "heap")]
use crate::heap;
use crate::slots;

/// What the runtime may use on the program's behalf.
pub struct Resources {
    /// The root CNode (single-level and guarded, so slot numbers are
    /// capability addresses).
    pub cnode: usize,
    /// Free slots of it for the slot allocator: nothing else may use them.
    pub slots: Range<usize>,
    /// Untyped memory for the runtime's own objects and the heap. Revoking
    /// it destroys them.
    pub untyped: usize,
    /// The program's address space, where the heap is mapped.
    #[cfg(feature = "heap")]
    pub vspace: usize,
    /// An unused, page-aligned address range for the heap to grow into.
    #[cfg(feature = "heap")]
    pub heap: Range<usize>,
}

/// Start the slot allocator and, with the `heap` feature, the heap. Call
/// once, before using either.
///
/// TODO: a program started by another receives its resources in a typed
/// bootstrap message, and the entry code starts the runtime before
/// `sap_main` (M7); today only the root task calls this.
pub fn start(resources: Resources) {
    let (cnode, free, untyped) = (resources.cnode, resources.slots, resources.untyped);
    // A notification for each lock, from the first two free slots.
    let locks = free.start;
    let root = SlotRef { root: cnode, index: 0, depth: 0 };
    untyped::retype(untyped, object::NOTIFICATION, 0, root, locks, 2).expect("runtime: notifications");
    slots::init(free.start + 2..free.end, locks);
    #[cfg(feature = "heap")]
    heap::init(cnode, resources.vspace, untyped, resources.heap, locks + 1);
}
