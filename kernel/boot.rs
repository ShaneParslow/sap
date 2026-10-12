//! What the kernel learns about the machine at boot, independent of how it
//! learned it. The architecture code fills a `BootInfo` (from a device tree
//! on RISC-V and Arm; from UEFI and ACPI on a future x86 port) and hands it
//! to `kmain`. Nothing here may name an architecture or a platform device.

use core::fmt;

#[derive(Clone, Copy)]
pub struct Region {
    pub start: usize,
    pub size: usize,
}

impl Region {
    pub fn end(&self) -> usize {
        self.start + self.size
    }
}

/// Region lists are fixed-capacity because they are built before any memory
/// allocator exists: the allocator is set up *from* these lists, so they
/// can't come from it. Device-tree platforms have a handful of entries
/// (QEMU `virt`: one memory region, two or three reservations).
///
/// TODO: UEFI memory maps on x86 can have over a hundred entries. Coalesce
/// adjacent usable ranges while filling, and raise the capacity, when that
/// port starts.
const MAX_REGIONS: usize = 16;

pub struct Regions {
    items: [Region; MAX_REGIONS],
    len: usize,
}

impl Regions {
    pub const fn new() -> Self {
        Self { items: [Region { start: 0, size: 0 }; MAX_REGIONS], len: 0 }
    }

    /// Returns false if the list is full.
    #[must_use]
    pub fn push(&mut self, region: Region) -> bool {
        if self.len == MAX_REGIONS {
            return false;
        }
        self.items[self.len] = region;
        self.len += 1;
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &Region> {
        self.items[..self.len].iter()
    }

    pub fn first_mut(&mut self) -> Option<&mut Region> {
        self.items[..self.len].first_mut()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Region> {
        self.items[..self.len].get_mut(index)
    }

    pub fn remove(&mut self, index: usize) {
        self.items.copy_within(index + 1..self.len, index);
        self.len -= 1;
    }
}

pub struct BootInfo {
    /// The CPU that booted the kernel (the hart id on RISC-V).
    pub boot_cpu: usize,
    pub cpus: usize,
    /// Frequency of the architecture's timer counter.
    pub timer_hz: u64,
    /// RAM.
    pub memory: Regions,
    /// Memory the kernel must never allocate: firmware, the firmware tables
    /// below, and the kernel image.
    pub reserved: Regions,
    /// The device tree blob, kept intact so it can be handed to the
    /// userspace device manager.
    pub devicetree: Option<Region>,
}

impl BootInfo {
    pub fn log(&self) {
        crate::info!("boot: cpu {} of {}, timer {} Hz", self.boot_cpu, self.cpus, self.timer_hz);
        if let Some(dt) = self.devicetree {
            crate::info!("boot: device tree {:#012x}..{:#012x} {:#}", dt.start, dt.end(), Bytes(dt.size));
        }
        for r in self.memory.iter() {
            crate::info!("boot: memory   {:#012x}..{:#012x} {:#}", r.start, r.end(), Bytes(r.size));
        }
        for r in self.reserved.iter() {
            crate::info!("boot: reserved {:#012x}..{:#012x} {:#}", r.start, r.end(), Bytes(r.size));
        }
    }
}

/// A byte count for humans: the smallest binary unit that keeps the number
/// under five digits (`9287 B`, `128 MiB`, `1234 MiB`), rounded to nearest
/// and marked `~` when inexact. The alternate form (`{:#}`) adds the exact
/// count.
pub struct Bytes(pub usize);

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
        let mut unit = 0;
        while self.0 >> (10 * unit) >= 10_000 && unit < UNITS.len() - 1 {
            unit += 1;
        }
        let shift = 10 * unit;
        let exact = self.0 & ((1usize << shift) - 1) == 0;
        let value = if unit == 0 { self.0 } else { (self.0 + (1 << (shift - 1))) >> shift };
        write!(f, "{}{value} {}", if exact { "" } else { "~" }, UNITS[unit])?;
        if f.alternate() && unit > 0 {
            write!(f, " ({} bytes)", self.0)?;
        }
        Ok(())
    }
}
