//! Sv39 paging: three levels of 512-entry tables translating 39-bit virtual
//! addresses, with leaves at any level (4 KiB, 2 MiB or 1 GiB pages).
//!
//! Kernel address space layout (the upper half of Sv39):
//!
//! | Range                                     | Contents                      |
//! |-------------------------------------------|-------------------------------|
//! | `PHYSMAP_BASE` + physical address         | RAM and mapped devices        |
//! | `KERNEL_VBASE` .. image end               | the kernel image              |
//!
//! The lower half is left for user address spaces (M2, user mode).

use core::sync::atomic::{AtomicUsize, Ordering};

use bitflags::bitflags;
use riscv::register::satp;

use crate::boot::{BootInfo, Region};
use crate::mm::{BootAllocator, Perms, PAGE_SIZE};

/// Physical address `p` is mapped at `PHYSMAP_BASE + p` (entry.rs agrees).
pub const PHYSMAP_BASE: usize = 0xFFFF_FFC0_0000_0000;
/// Where the kernel image is linked (linker.ld agrees).
pub const KERNEL_VBASE: usize = 0xFFFF_FFFF_8000_0000;
/// Size of the direct-map window: from `PHYSMAP_BASE` up to the image
/// window. Sv39 can't direct-map RAM above 254 GiB; a machine with more
/// would need Sv48. The boot page table maps the whole window (entry.rs).
const PHYSMAP_SIZE: usize = 254 << 30;

const ENTRIES: usize = 512;
const LEVELS: usize = 3;

/// Physical address of the image, passed by the entry code.
static KERNEL_PBASE: AtomicUsize = AtomicUsize::new(0);
/// Root table of the kernel's address space, whose upper half every user
/// address space shares.
static KERNEL_ROOT: AtomicUsize = AtomicUsize::new(0);

/// Root entries from here up map the kernel (the upper half of Sv39).
const KERNEL_HALF: usize = ENTRIES / 2;
/// User virtual addresses are below this (the lower half of Sv39).
pub const USER_END: usize = 1 << 38;

bitflags! {
    /// Page table entry flags (Sv39).
    #[derive(Clone, Copy)]
    pub struct Flags: u64 {
        const VALID = 1 << 0;
        const READ = 1 << 1;
        const WRITE = 1 << 2;
        const EXECUTE = 1 << 3;
        const USER = 1 << 4;
        /// Present in every address space; the kernel's mappings.
        const GLOBAL = 1 << 5;
        /// Set up front so hardware without automatic accessed/dirty
        /// updates doesn't fault on first use (Svade).
        const ACCESSED = 1 << 6;
        const DIRTY = 1 << 7;
    }
}

const KERNEL_RW: Flags = Flags::READ.union(Flags::WRITE).union(Flags::GLOBAL);
const KERNEL_RX: Flags = Flags::READ.union(Flags::EXECUTE).union(Flags::GLOBAL);
const KERNEL_RO: Flags = Flags::READ.union(Flags::GLOBAL);

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Entry(u64);

impl Entry {
    fn new(pa: usize, flags: Flags) -> Self {
        Entry(((pa >> 12) as u64) << 10 | flags.bits())
    }

    fn is_valid(self) -> bool {
        self.0 & Flags::VALID.bits() != 0
    }

    /// A leaf maps memory; a non-leaf points at the next-level table.
    fn is_leaf(self) -> bool {
        self.0 & (Flags::READ | Flags::WRITE | Flags::EXECUTE).bits() != 0
    }

    fn pa(self) -> usize {
        ((self.0 >> 10) as usize & ((1 << 44) - 1)) << 12
    }
}

#[repr(C, align(4096))]
struct Table([Entry; ENTRIES]);

pub fn set_kernel_pbase(pa: usize) {
    KERNEL_PBASE.store(pa, Ordering::Relaxed);
}

pub fn phys_to_virt(pa: usize) -> usize {
    PHYSMAP_BASE + pa
}

/// Physical address behind a kernel virtual address (image or direct map).
pub fn virt_to_phys(va: usize) -> usize {
    if va >= KERNEL_VBASE {
        va - KERNEL_VBASE + KERNEL_PBASE.load(Ordering::Relaxed)
    } else if va >= PHYSMAP_BASE {
        va - PHYSMAP_BASE
    } else {
        panic!("virt_to_phys: {va:#x} is not a kernel address")
    }
}

/// The table at physical address `pa`, through the direct map.
fn table(pa: usize) -> &'static mut Table {
    // SAFETY: `pa` is a page-table frame this module allocated or the root;
    // the direct map covers it, and the kernel is single-threaded with
    // interrupts off while page tables are edited.
    unsafe { &mut *(phys_to_virt(pa) as *mut Table) }
}

/// Bytes mapped by one entry at `level` (0 = 4 KiB, 1 = 2 MiB, 2 = 1 GiB).
const fn level_size(level: usize) -> usize {
    PAGE_SIZE << (9 * level)
}

const fn index(va: usize, level: usize) -> usize {
    (va >> (12 + 9 * level)) & (ENTRIES - 1)
}

/// A page-table tree, by the physical address of its root.
pub struct AddressSpace {
    root: usize,
}

impl AddressSpace {
    pub fn new(alloc: &mut BootAllocator) -> Self {
        Self { root: alloc_table(alloc) }
    }

    /// An empty user address space. Its upper half points at the kernel's
    /// own lower-level tables, so the kernel stays mapped (but not
    /// user-accessible) while it runs.
    ///
    /// TODO: kernel mappings added later at the root level would not reach
    /// existing user spaces. Today the kernel never adds any after boot.
    pub fn new_user(alloc: &mut BootAllocator) -> Self {
        let space = Self::new(alloc);
        let kernel = table(KERNEL_ROOT.load(Ordering::Relaxed));
        table(space.root).0[KERNEL_HALF..].copy_from_slice(&kernel.0[KERNEL_HALF..]);
        space
    }

    /// Map user memory: `size` bytes at `va` (below `USER_END`) to `pa`.
    pub fn map_user(&mut self, va: usize, pa: usize, size: usize, perms: Perms, alloc: &mut BootAllocator) {
        assert!(va + size <= USER_END, "map_user: {va:#x} is not a user address");
        let mut flags = Flags::USER;
        flags.set(Flags::READ, perms.contains(Perms::READ));
        flags.set(Flags::WRITE, perms.contains(Perms::WRITE));
        flags.set(Flags::EXECUTE, perms.contains(Perms::EXECUTE));
        self.map(va, pa, size, flags, alloc);
    }

    /// Map `size` bytes at `va` to `pa` with the largest pages that fit.
    /// All three must be page-aligned. Panics if anything is already mapped.
    pub fn map(&mut self, va: usize, pa: usize, size: usize, flags: Flags, alloc: &mut BootAllocator) {
        assert!((va | pa | size) % PAGE_SIZE == 0, "map: unaligned {va:#x} -> {pa:#x} ({size:#x})");
        let mut offset = 0;
        while offset < size {
            let level = (0..LEVELS)
                .rev()
                .find(|&l| {
                    let s = level_size(l);
                    (va + offset) % s == 0 && (pa + offset) % s == 0 && size - offset >= s
                })
                .unwrap_or(0);
            self.map_page(va + offset, pa + offset, level, flags, alloc);
            offset += level_size(level);
        }
    }

    fn map_page(&mut self, va: usize, pa: usize, level: usize, flags: Flags, alloc: &mut BootAllocator) {
        let mut t = self.root;
        for l in (level + 1..LEVELS).rev() {
            let entry = &mut table(t).0[index(va, l)];
            if !entry.is_valid() {
                *entry = Entry::new(alloc_table(alloc), Flags::VALID);
            }
            assert!(!entry.is_leaf(), "map: {va:#x} is inside a larger page");
            t = entry.pa();
        }
        let entry = &mut table(t).0[index(va, level)];
        assert!(!entry.is_valid(), "map: {va:#x} is already mapped");
        *entry = Entry::new(pa, flags | Flags::VALID | Flags::ACCESSED | Flags::DIRTY);
    }

    /// Physical address of the root table.
    pub fn root(&self) -> usize {
        self.root
    }

    /// Switch this hart to this address space.
    ///
    /// # Safety
    /// Everything the kernel is about to touch, including its own code,
    /// stack and the direct map it uses for page tables, must be mapped.
    pub unsafe fn activate(&self) {
        // SAFETY: per the caller's guarantee. ASID 0 for the kernel.
        unsafe { satp::set(satp::Mode::Sv39, 0, self.root >> 12) };
        riscv::asm::sfence_vma_all();
    }
}

/// Switch to the address space rooted at physical address `root`, unless it
/// is already active.
///
/// TODO: all user address spaces share ASID 0, so every switch flushes the
/// whole TLB; give each its own ASID (M3, address spaces).
pub fn activate_root(root: usize) {
    if satp::read().ppn() != root >> 12 {
        // SAFETY: user address spaces share the kernel's upper half, so the
        // kernel stays mapped across the switch.
        unsafe { satp::set(satp::Mode::Sv39, 0, root >> 12) };
        riscv::asm::sfence_vma_all();
    }
}

fn alloc_table(alloc: &mut BootAllocator) -> usize {
    let pa = alloc.alloc_frame();
    table(pa).0.fill(Entry(0));
    pa
}

unsafe extern "C" {
    static __kernel_start: u8;
    static __text_end: u8;
    static __rodata_end: u8;
    static __kernel_end: u8;
}

/// The physical extent of the kernel image, for the reserved list.
pub fn kernel_image() -> Region {
    let start = &raw const __kernel_start as usize;
    let end = &raw const __kernel_end as usize;
    Region { start: virt_to_phys(start), size: end - start }
}

/// Build the kernel's address space and switch to it, replacing the boot
/// page table. Maps the image with per-section permissions, all RAM in the
/// direct map (read/write, never executable), and `devices` (MMIO ranges)
/// in the direct map at their physical addresses.
pub fn init(boot: &BootInfo, devices: &[Region], alloc: &mut BootAllocator) -> AddressSpace {
    let mut space = AddressSpace::new(alloc);

    let text_end = &raw const __text_end as usize;
    let rodata_end = &raw const __rodata_end as usize;
    let image = kernel_image();
    let sections = [
        (KERNEL_VBASE, text_end, KERNEL_RX),
        (text_end, rodata_end, KERNEL_RO),
        (rodata_end, KERNEL_VBASE + image.size, KERNEL_RW),
    ];
    for (start, end, flags) in sections {
        space.map(start, virt_to_phys(start), end - start, flags, alloc);
    }

    // TODO: skip reservations marked `no-map` (OpenSBI's own memory, which
    // PMP protects anyway) once the device-tree parser records the flag.
    for r in boot.memory.iter() {
        assert!(r.end() <= PHYSMAP_SIZE, "paging: RAM above {:#x} needs Sv48", PHYSMAP_SIZE);
        space.map(phys_to_virt(r.start), r.start, r.size, KERNEL_RW, alloc);
    }
    for d in devices {
        let start = d.start & !(PAGE_SIZE - 1);
        let size = (d.end() - start).next_multiple_of(PAGE_SIZE);
        space.map(phys_to_virt(start), start, size, KERNEL_RW, alloc);
    }

    // SAFETY: the image, the direct map of RAM (which holds every page
    // table and the device tree) and the devices in use are all mapped.
    unsafe { space.activate() };
    KERNEL_ROOT.store(space.root, Ordering::Relaxed);
    crate::info!(
        "paging: Sv39 on, image {:#x} -> {:#x}, {} page-table frame(s)",
        KERNEL_VBASE,
        image.start,
        alloc.allocated()
    );
    space
}
