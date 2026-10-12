//! Boot information from a flattened device tree (RISC-V, and Arm later).
//!
//! The kernel reads only what it needs to run; full device discovery belongs
//! to a userspace device manager, which gets the blob itself.

use fdt::Fdt;

use crate::boot::{BootInfo, Region, Regions};

/// Parse the blob at physical address `dtb`. Panics on an invalid blob or a
/// missing required node: the kernel can't boot without this information.
/// The Devicetree Specification requires `/cpus` and `/memory`, which is why
/// the `fdt` crate's accessors for them panic rather than return `None`.
pub fn parse(dtb: usize, boot_cpu: usize) -> BootInfo {
    let fdt = open(dtb);
    let blob = Region { start: dtb, size: fdt.total_size() };

    let mut memory = Regions::new();
    for r in fdt.memory().regions() {
        if let Some(size) = r.size
            && !memory.push(Region { start: r.starting_address as usize, size })
        {
            // Losing a memory region only wastes RAM.
            crate::warn!("devicetree: too many memory regions, ignoring {:#x}", r.starting_address as usize);
        }
    }

    // The specification has two ways to reserve memory, and firmware may use
    // either: the older memory reservation block in the blob's header (plain
    // address/size pairs), and the `/reserved-memory` node, whose children
    // can also carry flags such as `no-map`. OpenSBI uses the node. Neither
    // is architecture-specific. Children with a `size` but no `reg` ask the
    // OS to allocate a region; nothing here needs those, so they're skipped.
    let mut reserved = Regions::new();
    let mut reserve = |region: Region| {
        // Dropping a reservation could hand firmware memory to the
        // allocator, so this is fatal rather than a warning.
        assert!(reserved.push(region), "devicetree: too many reserved regions");
    };
    for r in fdt.memory_reservations() {
        reserve(Region { start: r.address() as usize, size: r.size() });
    }
    if let Some(node) = fdt.find_node("/reserved-memory") {
        for child in node.children() {
            for r in child.reg().into_iter().flatten() {
                if let Some(size) = r.size {
                    reserve(Region { start: r.starting_address as usize, size });
                }
            }
        }
    }
    reserve(blob);

    let mut cpus = 0;
    let mut timer_hz = 0;
    for cpu in fdt.cpus() {
        cpus += 1;
        let hz = cpu.timebase_frequency() as u64;
        if timer_hz == 0 {
            timer_hz = hz;
        } else if hz != timer_hz {
            // On RISC-V the `time` counter is platform-wide, so every hart
            // should report the same frequency.
            crate::warn!("devicetree: cpus disagree on timebase ({hz} vs {timer_hz} Hz), using {timer_hz}");
        }
    }

    BootInfo { boot_cpu, cpus, timer_hz, memory, reserved, devicetree: Some(blob) }
}

/// Address of the first register range of the first node compatible with
/// any of `compatible`. For architecture code finding platform devices.
pub fn find_device(dtb: usize, compatible: &[&str]) -> Option<usize> {
    open(dtb)
        .find_compatible(compatible)
        .and_then(|node| node.reg()?.next())
        .map(|r| r.starting_address as usize)
}

fn open(dtb: usize) -> Fdt<'static> {
    // SAFETY: firmware passes a valid blob address, and the blob stays
    // reserved for the kernel's lifetime. `from_ptr` checks the header and
    // bounds every later read by its `totalsize`. A bad address would fault
    // in here and panic through the trap handler; there is no unwinding.
    match unsafe { Fdt::from_ptr(dtb as *const u8) } {
        Ok(fdt) => fdt,
        Err(e) => panic!("invalid device tree at {dtb:#x}: {e:?}"),
    }
}
