//! Boot information from the flattened device tree passed by firmware.
//!
//! The kernel reads only what it needs to run; full device discovery belongs
//! to a userspace device manager, which gets the blob itself (see `dtb`).

use fdt::Fdt;

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

const MAX_REGIONS: usize = 16;

/// A fixed-capacity list of regions; there is no allocator yet.
pub struct Regions {
    items: [Region; MAX_REGIONS],
    len: usize,
}

impl Regions {
    const fn new() -> Self {
        Self { items: [Region { start: 0, size: 0 }; MAX_REGIONS], len: 0 }
    }

    fn push(&mut self, region: Region) {
        if self.len == MAX_REGIONS {
            crate::warn!("devicetree: more than {MAX_REGIONS} regions, ignoring {:#x}", region.start);
            return;
        }
        self.items[self.len] = region;
        self.len += 1;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Region> {
        self.items[..self.len].iter()
    }
}

pub struct BootInfo {
    /// The blob itself. Kept intact and reserved so it can be handed on.
    pub dtb: Region,
    pub timebase_hz: u64,
    pub harts: usize,
    /// RAM, from `/memory`.
    pub memory: Regions,
    /// Memory the kernel must not allocate: the reservation block,
    /// `/reserved-memory` children, and the blob.
    pub reserved: Regions,
}

/// Parse the blob at physical address `dtb`. Panics on an invalid blob or a
/// missing required node: the kernel can't boot without this information.
pub fn parse(dtb: usize) -> BootInfo {
    // SAFETY: firmware passes a valid blob address; `from_ptr` checks the
    // header and bounds everything by its `totalsize`.
    let fdt: Fdt<'static> = match unsafe { Fdt::from_ptr(dtb as *const u8) } {
        Ok(fdt) => fdt,
        Err(e) => panic!("invalid device tree at {dtb:#x}: {e:?}"),
    };

    let blob = Region { start: dtb, size: fdt.total_size() };

    let mut memory = Regions::new();
    for r in fdt.memory().regions() {
        if let Some(size) = r.size {
            memory.push(Region { start: r.starting_address as usize, size });
        }
    }

    let mut reserved = Regions::new();
    for r in fdt.memory_reservations() {
        reserved.push(Region { start: r.address() as usize, size: r.size() });
    }
    if let Some(node) = fdt.find_node("/reserved-memory") {
        for child in node.children() {
            for r in child.reg().into_iter().flatten() {
                if let Some(size) = r.size {
                    reserved.push(Region { start: r.starting_address as usize, size });
                }
            }
        }
    }
    reserved.push(blob);

    let mut harts = 0;
    let mut timebase_hz = 0;
    for cpu in fdt.cpus() {
        harts += 1;
        timebase_hz = cpu.timebase_frequency() as u64;
    }

    BootInfo { dtb: blob, timebase_hz, harts, memory, reserved }
}

impl BootInfo {
    pub fn log(&self) {
        crate::info!(
            "devicetree: {} bytes at {:#x}, {} hart(s), timebase {} Hz",
            self.dtb.size, self.dtb.start, self.harts, self.timebase_hz
        );
        for r in self.memory.iter() {
            crate::info!("devicetree: memory   {:#012x}..{:#012x} ({} MiB)", r.start, r.end(), r.size >> 20);
        }
        for r in self.reserved.iter() {
            crate::info!("devicetree: reserved {:#012x}..{:#012x} ({} KiB)", r.start, r.end(), r.size >> 10);
        }
    }
}
