//! Loading the root task: the one program the kernel starts by itself, as
//! seL4 does. Every other program is loaded by the root task, which will
//! become the process manager (M7), so this is the only ELF loader in the
//! kernel and it handles only what `user/init` produces: a static,
//! position-independent RISC-V executable whose only relocations are
//! relative ones.
//!
//! The kernel also gives the root task all the authority there is, as
//! capabilities in a CNode it builds here: its own thread, CNode and address
//! space, the frames and page tables of its image, and every byte of free
//! memory as untyped memory. Its boot-info page says where each is.

use elf::abi::{EM_RISCV, ET_DYN, PF_R, PF_W, PF_X, PT_LOAD, R_RISCV_RELATIVE};
use elf::endian::LittleEndian;
use elf::ElfBytes;
use sap_abi::{init_slot, BootInfo, SlotRange, UntypedDesc, MAX_BOOT_UNTYPED};

use crate::arch::{self, AddressSpace, ArchCap, PtMapping, UserContext, UserMapping, FRAME_RIGHTS_ALL, USER_END};
use crate::boot::Bytes;
use crate::cap::Cap;
use crate::cspace::{self, Slot, SLOT_BITS};
use crate::mm::{BootAllocator, Perms, PAGE_SIZE};
use crate::thread::{self, Tcb, MAX_PRIORITY};

static IMAGE: &[u8] = include_bytes!(env!("INIT_ELF"));

/// Where the image is placed. Leaves the low pages unmapped so null pointer
/// accesses fault.
const LOAD_BASE: usize = 0x1_0000;
const STACK_SIZE: usize = 16 * 1024;
/// The stack ends one page below the top of user space, leaving a guard.
const STACK_TOP: usize = USER_END - PAGE_SIZE;
/// The IPC buffer and boot-info pages sit below the stack, each after a
/// guard page.
const IPC_BUFFER_VA: usize = STACK_TOP - STACK_SIZE - 2 * PAGE_SIZE;
const BOOT_INFO_VA: usize = IPC_BUFFER_VA - 2 * PAGE_SIZE;
/// The root task's address-space identifier.
const ASID: u16 = 1;
/// log2 of the number of slots in the root CNode, as seL4's default.
const CNODE_RADIX: usize = 12;
/// The largest untyped region handed out, as seL4 on 64-bit RISC-V.
const MAX_UNTYPED_BITS: u32 = 38;

/// Load the root task, give it every capability, and make it runnable. The
/// rest of free memory becomes its untyped memory, so the boot allocator is
/// used up here. The caller then starts the scheduler.
pub fn start(mut alloc: BootAllocator) {
    let alloc = &mut alloc;
    let elf = ElfBytes::<LittleEndian>::minimal_parse(IMAGE).expect("root task: not an ELF file");
    assert!(
        elf.ehdr.e_machine == EM_RISCV && elf.ehdr.e_type == ET_DYN,
        "root task: not a position-independent RISC-V executable"
    );
    let segments = elf.segments().expect("root task: no program headers");
    let loads = || segments.iter().filter(|s| s.p_type == PT_LOAD);

    // Copy every segment into one contiguous run of frames, at the same
    // offsets it has in the address space; the gaps and .bss stay zero.
    let span = loads().map(|s| (s.p_vaddr + s.p_memsz) as usize).max().expect("root task: nothing to load");
    let pages = span.div_ceil(PAGE_SIZE);
    let image_pa = alloc.alloc_contiguous(pages);
    // SAFETY: the frames were just allocated and the direct map covers them.
    let image = unsafe { core::slice::from_raw_parts_mut(arch::phys_to_virt(image_pa) as *mut u8, pages * PAGE_SIZE) };
    image.fill(0);
    for s in loads() {
        let (offset, vaddr, filesz) = (s.p_offset as usize, s.p_vaddr as usize, s.p_filesz as usize);
        image[vaddr..vaddr + filesz].copy_from_slice(&IMAGE[offset..offset + filesz]);
    }

    // Apply relative relocations: each one stores load base + addend at an
    // offset in the image (pointers in statics, the GOT).
    let (sections, strings) = elf.section_headers_with_strtab().expect("root task: bad section headers");
    let (sections, strings) = (sections.expect("root task: no sections"), strings.expect("root task: no strings"));
    let mut relocations = 0;
    for header in sections.iter().filter(|h| strings.get(h.sh_name as usize).is_ok_and(|name| name == ".rela.dyn")) {
        for rela in elf.section_data_as_relas(&header).expect("root task: bad relocations") {
            assert!(rela.r_type == R_RISCV_RELATIVE, "root task: unsupported relocation {}", rela.r_type);
            let at = rela.r_offset as usize;
            let value = (LOAD_BASE as i64 + rela.r_addend) as u64;
            image[at..at + 8].copy_from_slice(&value.to_le_bytes());
            relocations += 1;
        }
    }

    let mut space = AddressSpace::new_user(alloc);
    for s in loads() {
        let start = s.p_vaddr as usize & !(PAGE_SIZE - 1);
        let end = (s.p_vaddr + s.p_memsz) as usize;
        let size = (end - start).next_multiple_of(PAGE_SIZE);
        let mut perms = Perms::empty();
        perms.set(Perms::READ, s.p_flags & PF_R != 0);
        perms.set(Perms::WRITE, s.p_flags & PF_W != 0);
        perms.set(Perms::EXECUTE, s.p_flags & PF_X != 0);
        assert!(
            !perms.contains(Perms::WRITE | Perms::EXECUTE),
            "root task: segment at {start:#x} is writable and executable"
        );
        space.map_user(LOAD_BASE + start, image_pa + start, size, perms, alloc);
    }

    let stack_pa = alloc.alloc_contiguous(STACK_SIZE / PAGE_SIZE);
    space.map_user(STACK_TOP - STACK_SIZE, stack_pa, STACK_SIZE, Perms::READ | Perms::WRITE, alloc);

    let ipc_buffer_pa = zeroed_frame(alloc);
    space.map_user(IPC_BUFFER_VA, ipc_buffer_pa, PAGE_SIZE, Perms::READ | Perms::WRITE, alloc);
    let boot_info_pa = zeroed_frame(alloc);
    space.map_user(BOOT_INFO_VA, boot_info_pa, PAGE_SIZE, Perms::READ, alloc);
    arch::assign_asid(ASID, space.root());

    let cnode_pa = alloc.alloc_contiguous(1 << (CNODE_RADIX + SLOT_BITS - 12));
    let cnode = arch::phys_to_virt(cnode_pa);
    // SAFETY: just allocated; zeroed slots are empty.
    unsafe { core::ptr::write_bytes(cnode as *mut u8, 0, 1 << (CNODE_RADIX + SLOT_BITS)) };
    // SAFETY: a fresh frame, reached through the direct map.
    let tcb = unsafe { Tcb::init(arch::phys_to_virt(zeroed_frame(alloc))) };

    let mut caps = RootCaps { cnode, next: init_slot::FIRST_FREE };
    let cnode_cap = Cap::CNode {
        ptr: cnode,
        radix: CNODE_RADIX as u8,
        guard_bits: (usize::BITS as usize - CNODE_RADIX) as u8,
        guard: 0,
    };
    let vspace_cap = Cap::Arch(ArchCap::PageTable { pa: space.root(), mapping: PtMapping::Root { asid: ASID } });
    let frame = |pa, rights: u8, va| Cap::Arch(ArchCap::Frame { size_bits: 12, rights, asid: ASID, pa, va });
    caps.put(init_slot::TCB, Cap::Thread { ptr: tcb as usize });
    caps.put(init_slot::CNODE, cnode_cap);
    caps.put(init_slot::VSPACE, vspace_cap);
    caps.put(init_slot::IPC_BUFFER, frame(ipc_buffer_pa, FRAME_RIGHTS_ALL, IPC_BUFFER_VA));
    caps.put(init_slot::BOOT_INFO, frame(boot_info_pa, sap_abi::rights::READ as u8, BOOT_INFO_VA));
    caps.put(init_slot::ASID_POOL, Cap::Arch(ArchCap::AsidPool));

    // Capabilities to everything else mapped in the root task's address
    // space: its frames, then its page tables.
    let image_frames = caps.range(|caps| {
        arch::walk_user(space.root(), |m| {
            if let UserMapping::Page { pa, va, size_bits, writable } = m
                && va != IPC_BUFFER_VA
                && va != BOOT_INFO_VA
            {
                let rights = if writable { FRAME_RIGHTS_ALL } else { sap_abi::rights::READ as u8 };
                caps.push(Cap::Arch(ArchCap::Frame { size_bits, rights, asid: ASID, pa, va }));
            }
        })
    });
    let image_paging = caps.range(|caps| {
        arch::walk_user(space.root(), |m| {
            if let UserMapping::Table { pa, va, level } = m {
                let mapping = PtMapping::Mapped { asid: ASID, va, level: level as u8 };
                caps.push(Cap::Arch(ArchCap::PageTable { pa, mapping }));
            }
        })
    });

    // The thread's own slots hold copies of its CNode, address space and
    // IPC buffer capabilities, as in seL4.
    // SAFETY: tcb is live and its slots are empty; the sources were filled
    // above.
    unsafe {
        let t = &mut *tcb;
        cspace::insert_derived(cnode_cap, caps.slot(init_slot::CNODE), &mut t.slots[thread::CSPACE_SLOT]);
        cspace::insert_derived(vspace_cap, caps.slot(init_slot::VSPACE), &mut t.slots[thread::VSPACE_SLOT]);
        let buffer = Cap::Arch(arch::unmapped_frame(ipc_buffer_pa, 12, FRAME_RIGHTS_ALL));
        cspace::insert_derived(buffer, caps.slot(init_slot::IPC_BUFFER), &mut t.slots[thread::IPC_BUFFER_SLOT]);
        t.ipc_buffer = IPC_BUFFER_VA;
    }

    // Everything still free becomes untyped memory, in naturally aligned
    // power-of-two blocks.
    let used_frames = alloc.allocated();
    // SAFETY: boot_info_pa is a frame allocated above.
    let info = unsafe { &mut *(arch::phys_to_virt(boot_info_pa) as *mut BootInfo) };
    let mut untyped_bytes = 0;
    let untyped = caps.range(|caps| {
        let mut count = 0;
        for r in alloc.free().iter() {
            let (mut start, end) = (r.start, r.end());
            while start < end {
                let bits = start.trailing_zeros().min((end - start).ilog2()).min(MAX_UNTYPED_BITS);
                if count == MAX_BOOT_UNTYPED {
                    crate::warn!("root task: too many untyped regions, dropping {:#x}..{end:#x}", start);
                    break;
                }
                caps.push(Cap::Untyped { base: start, size_bits: bits as u8, free_index: 0, device: false });
                info.untyped_list[count] = UntypedDesc { paddr: start, size_bits: bits as u8, is_device: 0, _pad: [0; 6] };
                count += 1;
                untyped_bytes += 1 << bits;
                start += 1 << bits;
            }
        }
    });

    info.node_id = 0;
    info.num_nodes = 1;
    info.ipc_buffer = IPC_BUFFER_VA;
    info.cnode_radix = CNODE_RADIX;
    info.image_frames = image_frames;
    info.image_paging = image_paging;
    info.untyped = untyped;
    info.empty = SlotRange { start: caps.next, end: 1 << CNODE_RADIX };

    let entry = LOAD_BASE + elf.ehdr.e_entry as usize;
    crate::info!(
        "root task: {} image, {relocations} relocation(s), entry {entry:#x}",
        Bytes(IMAGE.len())
    );
    crate::info!(
        "root task: {} frame(s), {} page table(s), {} untyped region(s) ({:#}), {} boot frame(s), {} slot(s) free",
        image_frames.len(),
        image_paging.len(),
        untyped.len(),
        Bytes(untyped_bytes),
        used_frames,
        info.empty.len()
    );

    // SAFETY: tcb was initialized above.
    unsafe {
        (*tcb).context = UserContext::new_user(entry, STACK_TOP, BOOT_INFO_VA);
        (*tcb).priority = MAX_PRIORITY;
        (*tcb).mcp = MAX_PRIORITY;
        crate::sched::make_runnable(tcb);
    }
}

fn zeroed_frame(alloc: &mut BootAllocator) -> usize {
    let pa = alloc.alloc_frame();
    // SAFETY: just allocated, reached through the direct map.
    unsafe { core::ptr::write_bytes(arch::phys_to_virt(pa) as *mut u8, 0, PAGE_SIZE) };
    pa
}

/// Fills the root CNode: fixed slots by index, the rest in order.
struct RootCaps {
    cnode: usize,
    next: usize,
}

impl RootCaps {
    fn slot(&self, index: usize) -> *mut Slot {
        assert!(index < 1 << CNODE_RADIX, "root task: root CNode full");
        (self.cnode + (index << SLOT_BITS)) as *mut Slot
    }

    fn put(&mut self, index: usize, cap: Cap) {
        // SAFETY: the slot is in the root CNode and empty.
        unsafe { cspace::insert_original(cap, None, self.slot(index)) };
    }

    fn push(&mut self, cap: Cap) {
        self.put(self.next, cap);
        self.next += 1;
    }

    /// Run `f`, which pushes capabilities, and return the slots it filled.
    fn range(&mut self, f: impl FnOnce(&mut Self)) -> SlotRange {
        let start = self.next;
        f(self);
        SlotRange { start, end: self.next }
    }
}
