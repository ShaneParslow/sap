//! Physical memory at boot: which frames are free, and a bump allocator for
//! the kernel's own page tables and the root task's image and stack.
//!
//! The kernel doesn't keep a general-purpose allocator. Following seL4, the
//! memory left after boot becomes untyped memory for the root task to
//! retype (M3); this allocator only serves the kernel's own setup.

use bitflags::bitflags;

use crate::boot::{Bytes, Region, Regions};

bitflags! {
    /// Access rights for a mapping, independent of the page-table format.
    #[derive(Clone, Copy)]
    pub struct Perms: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
    }
}

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

    /// The physical address of `count` contiguous free frames. Panics when
    /// no free region is big enough.
    pub fn alloc_contiguous(&mut self, count: usize) -> usize {
        let bytes = count * PAGE_SIZE;
        let index = self
            .free
            .iter()
            .position(|r| r.size >= bytes)
            .unwrap_or_else(|| panic!("mm: no {} contiguous at boot", Bytes(bytes)));
        let region = self.free.get_mut(index).unwrap();
        let start = region.start;
        region.start += bytes;
        region.size -= bytes;
        if region.size == 0 {
            self.free.remove(index);
        }
        self.allocated += count;
        start
    }

    /// Frames handed out so far.
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// What is left.
    pub fn free(&self) -> &Regions {
        &self.free
    }
}

pub fn log_free(free: &Regions) {
    let total: usize = free.iter().map(|r| r.size).sum();
    for r in free.iter() {
        crate::debug!("mm: free {:#012x}..{:#012x} {}", r.start, r.end(), Bytes(r.size));
    }
    crate::info!("mm: {:#} free in {} region(s)", Bytes(total), free.iter().count());
}
