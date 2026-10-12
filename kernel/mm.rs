//! Physical memory at boot: which frames are free, and a bump allocator the
//! kernel uses for its own page tables before handing the rest on.
//!
//! The kernel doesn't keep a general-purpose allocator. Following seL4, the
//! memory left after boot becomes untyped memory for userspace to retype
//! (M3); this allocator only serves the kernel's own setup.

use crate::boot::{Bytes, Region, Regions};

pub const PAGE_SIZE: usize = 4096;

const fn align_up(x: usize) -> usize {
    (x + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

const fn align_down(x: usize) -> usize {
    x & !(PAGE_SIZE - 1)
}

/// `memory` minus every overlapping `reserved` region, trimmed to whole
/// pages. `reserved` may be unsorted and overlapping.
pub fn free_regions(memory: &Regions, reserved: &Regions) -> Regions {
    let mut free = Regions::new();
    for m in memory.iter() {
        let end = align_down(m.end());
        let mut cursor = align_up(m.start);
        while cursor < end {
            // The first reservation that overlaps [cursor, end).
            let next = reserved
                .iter()
                .filter(|r| r.end() > cursor && r.start < end)
                .min_by_key(|r| r.start);
            let (gap_end, resume) = match next {
                Some(r) => (align_down(r.start.max(cursor)), align_up(r.end())),
                None => (end, end),
            };
            if gap_end > cursor && !free.push(Region { start: cursor, size: gap_end - cursor }) {
                crate::warn!("mm: too many free regions, ignoring {:#x}", cursor);
            }
            cursor = resume.max(cursor);
        }
    }
    free
}

/// Hands out free frames from the front of the free regions.
pub struct BootAllocator {
    free: Regions,
    allocated: usize,
}

impl BootAllocator {
    pub fn new(free: Regions) -> Self {
        Self { free, allocated: 0 }
    }

    /// The physical address of a free 4 KiB frame. Its contents are
    /// undefined. Panics when memory runs out, which at boot is fatal.
    pub fn alloc_frame(&mut self) -> usize {
        let region = self.free.first_mut().expect("mm: out of memory at boot");
        let frame = region.start;
        region.start += PAGE_SIZE;
        region.size -= PAGE_SIZE;
        if region.size == 0 {
            self.free.remove(0);
        }
        self.allocated += 1;
        frame
    }

    /// Frames handed out so far.
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// What is left, for whoever takes over memory next.
    pub fn into_free(self) -> Regions {
        self.free
    }
}

pub fn log_free(free: &Regions) {
    let total: usize = free.iter().map(|r| r.size).sum();
    for r in free.iter() {
        crate::debug!("mm: free {:#012x}..{:#012x} {}", r.start, r.end(), Bytes(r.size));
    }
    crate::info!("mm: {:#} free in {} region(s)", Bytes(total), free.iter().count());
}
