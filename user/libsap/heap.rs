//! The heap: the global allocator behind `alloc` (`Box`, `Vec`, `String`).
//!
//! A first-fit free list (the `linked_list_allocator` crate) over a range of
//! the program's address space. The heap starts empty and grows upward on
//! demand: frames are retyped from untyped memory the program gave the
//! runtime, into slots from the slot allocator, and mapped at the top, with
//! page tables added as the range needs them. Before the runtime is
//! initialised, or once the range or the memory runs out, allocation fails
//! (and `alloc` panics).
//!
//! TODO: return memory: the heap never shrinks, and freed frames stay
//! mapped. Needed once long-running services allocate in bursts.

use core::alloc::{GlobalAlloc, Layout};
use core::ops::Range;
use core::ptr::{self, NonNull};

use linked_list_allocator::Heap;
use sap_sys::abi::object::{self, MAX_RETYPE_COUNT};
use sap_sys::abi::{attr, error, rights};

use sap_sys::cnode::SlotRef;
use sap_sys::sys::Error;
use sap_sys::{untyped, vspace};

use crate::slots;
use crate::sync::Mutex;

const PAGE: usize = 4096;
/// The least the heap grows by, to keep growth system calls rare.
const GROW_MIN: usize = 64 * 1024;

struct Grower {
    heap: Heap,
    /// Mapped from `start` up to `top`; `top..end` is reserved and not mapped yet.
    start: usize,
    top: usize,
    end: usize,
    cnode: usize,
    vspace: usize,
    untyped: usize,
}

static HEAP: Mutex<Option<Grower>> = Mutex::new(None);

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct Allocator;

// SAFETY: allocations come from the free list, under the lock; the list
// only covers memory the heap mapped and nothing else uses.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = HEAP.lock();
        let Some(heap) = heap.as_mut() else { return ptr::null_mut() };
        if let Ok(block) = heap.heap.allocate_first_fit(layout) {
            return block.as_ptr();
        }
        // Enough for the block wherever alignment puts it, plus the list's
        // bookkeeping.
        let wanted = (layout.size() + layout.align() + 2 * size_of::<usize>()).max(GROW_MIN);
        match heap.grow(wanted) {
            Ok(()) => heap.heap.allocate_first_fit(layout).map_or(ptr::null_mut(), NonNull::as_ptr),
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        let mut heap = HEAP.lock();
        let heap = heap.as_mut().expect("heap: freeing without a heap");
        // SAFETY: `alloc` returned `block` for `layout` (the caller's
        // guarantee), so it is non-null and from this list.
        unsafe { heap.heap.deallocate(NonNull::new_unchecked(block), layout) };
    }
}

/// Start an empty heap that may grow over `region` (page-aligned, unused),
/// with frames and page tables from `untyped`, their capabilities in slots
/// of `cnode`, mapped in `vspace`. `notification` is the heap lock's.
pub(crate) fn init(cnode: usize, vspace: usize, untyped: usize, region: Range<usize>, notification: usize) {
    assert!(region.start % PAGE == 0 && region.end % PAGE == 0, "heap: region not page-aligned");
    let mut heap = HEAP.lock();
    assert!(heap.is_none(), "heap: already initialised");
    *heap = Some(Grower { heap: Heap::empty(), start: region.start, top: region.start, end: region.end, cnode, vspace, untyped });
    drop(heap);
    HEAP.set_notification(notification);
}

/// How much of the address range is mapped, in bytes.
pub fn mapped() -> usize {
    HEAP.lock().as_ref().map_or(0, |h| h.top - h.start)
}

impl Grower {
    /// Map at least `bytes` more at the top and give them to the list.
    fn grow(&mut self, bytes: usize) -> Result<(), Error> {
        let bytes = bytes.next_multiple_of(PAGE);
        if bytes > self.end - self.top {
            return Err(Error(error::NOT_ENOUGH_MEMORY));
        }
        let pages = bytes / PAGE;
        let first = slots::alloc_range(pages).ok_or(Error(error::NOT_ENOUGH_MEMORY))?;
        let root = SlotRef { root: self.cnode, index: 0, depth: 0 };
        for chunk in (0..pages).step_by(MAX_RETYPE_COUNT) {
            let count = (pages - chunk).min(MAX_RETYPE_COUNT);
            untyped::retype(self.untyped, object::FRAME_4K, 0, root, first + chunk, count)?;
        }
        let (untyped, cnode) = (self.untyped, self.cnode);
        let mut new_table = || {
            let table = slots::alloc().ok_or(Error(error::NOT_ENOUGH_MEMORY))?;
            untyped::retype(untyped, object::PAGE_TABLE, 0, SlotRef { root: cnode, index: 0, depth: 0 }, table, 1)?;
            Ok(table)
        };
        let rw = rights::READ | rights::WRITE;
        for page in 0..pages {
            let va = self.top + page * PAGE;
            vspace::map_frame_with_tables(first + page, self.vspace, va, rw, attr::EXECUTE_NEVER, &mut new_table)?;
        }
        // SAFETY: the new pages are mapped read-write, unused, and directly
        // above what the list already has (or start it).
        unsafe {
            if self.heap.size() == 0 {
                self.heap.init(sap_sys::address::mapped(self.top), bytes);
            } else {
                self.heap.extend(bytes);
            }
        }
        self.top += bytes;
        Ok(())
    }
}
