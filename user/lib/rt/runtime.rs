//! Starting the runtime: what a program lets it use on its behalf.

use core::ops::Range;

use sap_sys::abi::object;
use sap_sys::cnode::SlotRef;
use sap_sys::untyped;

#[cfg(feature = "heap")]
use crate::heap;
use crate::slots;

/// Where the runtime's memory comes from.
#[derive(Clone, Copy)]
pub enum Memory {
    /// One untyped region for the runtime's own objects and the heap,
    /// which stops growing when it is used up. Revoking it destroys them.
    Untyped(usize),
    /// A memory account (`interfaces/memory.capnp`): the runtime takes
    /// chunks of it as the heap grows.
    Account(usize),
}

/// The log2 of the first chunk the runtime takes from an account, for its
/// locks and the heap's first growth.
const FIRST_CHUNK_BITS: u8 = 16;

/// What the runtime may use on the program's behalf.
pub struct Resources {
    /// The root CNode (single-level and guarded, so slot numbers are
    /// capability addresses).
    pub cnode: usize,
    /// Free slots of it for the slot allocator: nothing else may use them.
    pub slots: Range<usize>,
    /// Where the runtime's own objects and the heap come from.
    pub memory: Memory,
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
/// TODO: start the runtime in the entry code, from the startup record,
/// before `sap_main` (M7). Today each program calls this itself: the root
/// task from its boot information, `check` from its startup record.
pub fn start(resources: Resources) {
    let (cnode, mut free) = (resources.cnode, resources.slots);
    // The untyped memory the locks come from: the region, or a first
    // chunk of the account, in the first free slot.
    let (untyped, account) = match resources.memory {
        Memory::Untyped(untyped) => (untyped, None),
        Memory::Account(account) => {
            let chunk = free.next().expect("runtime: no free slots");
            crate::account::allocate(account, FIRST_CHUNK_BITS, cnode, chunk).expect("runtime: a first chunk of the account");
            (chunk, Some((account, 1usize << FIRST_CHUNK_BITS)))
        }
    };
    // A notification for each lock, from the next two free slots.
    let locks = free.start;
    let root = SlotRef { root: cnode, index: 0, depth: 0 };
    untyped::retype(untyped, object::NOTIFICATION, 0, root, locks, 2).expect("runtime: notifications");
    slots::init(free.start + 2..free.end, locks);
    #[cfg(feature = "heap")]
    heap::init(cnode, resources.vspace, untyped, account, resources.heap, locks + 1);
    #[cfg(not(feature = "heap"))]
    let _ = account;
}
