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
pub(super) struct Entry(u64);

impl Entry {
    pub(super) const EMPTY: Entry = Entry(0);

    fn new(pa: usize, flags: Flags) -> Self {
        Entry(((pa >> 12) as u64) << 10 | flags.bits())
    }

    pub(super) fn is_valid(self) -> bool {
        self.0 & Flags::VALID.bits() != 0
    }

    /// A leaf maps memory; a non-leaf points at the next-level table.
    fn is_leaf(self) -> bool {
        self.0 & (Flags::READ | Flags::WRITE | Flags::EXECUTE).bits() != 0
    }

    pub(super) fn pa(self) -> usize {
        ((self.0 >> 10) as usize & ((1 << 44) - 1)) << 12
    }

    fn flags(self) -> Flags {
        Flags::from_bits_truncate(self.0)
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

/// The entry at `level` for `va` in the tree rooted at `root`, if the tables
/// above it exist (and aren't leaves).
pub(super) fn entry_at(root: usize, va: usize, level: usize) -> Option<&'static mut Entry> {
    let mut t = root;
    for l in (level + 1..LEVELS).rev() {
        let entry = table(t).0[index(va, l)];
        if !entry.is_valid() || entry.is_leaf() {
            return None;
        }
        t = entry.pa();
    }
    Some(&mut table(t).0[index(va, level)])
}

/// Something mapped in the user half of an address space.
pub enum UserMapping {
    /// A page table at physical `pa` at `level` (0 or 1), covering virtual
    /// addresses from `va`.
    Table { pa: usize, va: usize, level: usize },
    /// A page of `1 << size_bits` bytes at physical `pa`, mapped at `va`.
    Page { pa: usize, va: usize, size_bits: u8, writable: bool },
}

/// Call `f` for every table and page in the user half of the tree rooted at
/// `root`, tables before the pages they hold.
pub fn walk_user(root: usize, mut f: impl FnMut(UserMapping)) {
    fn walk(t: usize, level: usize, base: usize, f: &mut impl FnMut(UserMapping)) {
        let entries = if level == LEVELS - 1 { KERNEL_HALF } else { ENTRIES };
        for i in 0..entries {
            let entry = table(t).0[i];
            if !entry.is_valid() {
                continue;
            }
            let va = base + i * level_size(level);
            if entry.is_leaf() {
                let size_bits = (12 + 9 * level) as u8;
                let writable = entry.flags().contains(Flags::WRITE);
                f(UserMapping::Page { pa: entry.pa(), va, size_bits, writable });
            } else {
                f(UserMapping::Table { pa: entry.pa(), va, level: level - 1 });
                walk(entry.pa(), level - 1, va, f);
            }
        }
    }
    walk(root, LEVELS - 1, 0, &mut f);
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
        make_root(space.root);
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

/// How many ASID bits the hardware implements (0 to 16), found at boot.
static ASID_BITS: AtomicUsize = AtomicUsize::new(0);

/// Find how many ASID bits `satp` implements: write all ones and read back
/// what sticks (the privileged specification's method).
fn probe_asid_bits() -> usize {
    let saved = satp::read();
    // SAFETY: same root and mode, only the ASID changes; the kernel's
    // mappings are global, so they stay valid under any ASID.
    unsafe { satp::set(satp::Mode::Sv39, 0xffff, saved.ppn()) };
    let bits = satp::read().asid().count_ones() as usize;
    // SAFETY: restores the previous value.
    unsafe { satp::set(satp::Mode::Sv39, saved.asid(), saved.ppn()) };
    bits
}

/// Whether address-space identifier `asid` can be used in hardware.
fn hardware_asid(asid: u16) -> bool {
    (asid as usize) < 1 << ASID_BITS.load(Ordering::Relaxed)
}

/// Switch to the address space rooted at physical address `root` with
/// identifier `asid`, unless it is already active. With hardware ASIDs the
/// TLB keeps every space's entries apart, so switching flushes nothing;
/// without them, every switch flushes the TLB.
pub fn activate_root(root: usize, asid: u16) {
    let current = satp::read();
    let hw = if hardware_asid(asid) { asid as usize } else { 0 };
    if current.ppn() != root >> 12 || current.asid() != hw {
        // SAFETY: user address spaces share the kernel's upper half, so the
        // kernel stays mapped across the switch.
        unsafe { satp::set(satp::Mode::Sv39, hw, root >> 12) };
        if hw == 0 {
            riscv::asm::sfence_vma_all();
        }
    }
}

/// Flush the TLB entry for `va` in address space `asid`.
pub(super) fn flush_page(asid: u16, va: usize) {
    if hardware_asid(asid) {
        riscv::asm::sfence_vma(asid as usize, va);
    } else {
        riscv::asm::sfence_vma_all();
    }
}

/// Flush every TLB entry of address space `asid` (before it is reused).
pub(super) fn flush_asid(asid: u16) {
    if hardware_asid(asid) {
        // SAFETY: sfence.vma only invalidates TLB entries; rs1 = zero
        // means every address.
        unsafe { core::arch::asm!("sfence.vma zero, {}", in(reg) asid as usize, options(nostack)) };
    } else {
        riscv::asm::sfence_vma_all();
    }
}

/// Why a mapping couldn't be made.
pub(super) enum MapError {
    /// No page table at this level covers the address yet.
    MissingTable,
    /// Something is already mapped there.
    Occupied,
}

fn user_flags(perms: Perms) -> Flags {
    let mut flags = Flags::USER | Flags::VALID | Flags::ACCESSED | Flags::DIRTY;
    flags.set(Flags::READ, perms.contains(Perms::READ));
    flags.set(Flags::WRITE, perms.contains(Perms::WRITE));
    flags.set(Flags::EXECUTE, perms.contains(Perms::EXECUTE));
    flags
}

/// Install the page table at physical `pt` at the first level below the
/// root where `va` has no table yet. Returns that level and the virtual
/// address the new table covers from.
pub(super) fn install_table(root: usize, va: usize, pt: usize) -> Result<(usize, usize), MapError> {
    let mut t = root;
    for level in (1..LEVELS).rev() {
        let entry = &mut table(t).0[index(va, level)];
        if !entry.is_valid() {
            *entry = Entry::new(pt, Flags::VALID);
            let span = level_size(level);
            return Ok((level - 1, va & !(span - 1)));
        }
        if entry.is_leaf() {
            return Err(MapError::Occupied);
        }
        t = entry.pa();
    }
    Err(MapError::Occupied)
}

/// Map a user page of the size `level` implies at `va` to `pa`. The tables
/// above it must exist.
pub(super) fn map_user_page(root: usize, va: usize, pa: usize, level: usize, perms: Perms) -> Result<(), MapError> {
    let entry = entry_at(root, va, level).ok_or(MapError::MissingTable)?;
    if entry.is_valid() {
        return Err(MapError::Occupied);
    }
    *entry = Entry::new(pa, user_flags(perms));
    Ok(())
}

/// Clear a page table's entries (when it is unmapped, as seL4 does).
pub(super) fn clear_table(pa: usize) {
    table(pa).0.fill(Entry::EMPTY);
}

/// Make a fresh page table into an address-space root: empty user half,
/// and the kernel's upper half.
pub(super) fn make_root(pa: usize) {
    let kernel = table(KERNEL_ROOT.load(Ordering::Relaxed));
    let t = table(pa);
    t.0[..KERNEL_HALF].fill(Entry::EMPTY);
    t.0[KERNEL_HALF..].copy_from_slice(&kernel.0[KERNEL_HALF..]);
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
    ASID_BITS.store(probe_asid_bits(), Ordering::Relaxed);
    crate::info!(
        "paging: Sv39 on, image {:#x} -> {:#x}, {} page-table frame(s), {} ASID bits",
        KERNEL_VBASE,
        image.start,
        alloc.allocated(),
        ASID_BITS.load(Ordering::Relaxed)
    );
    space
}
