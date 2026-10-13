//! Loading the root task: the one program the kernel starts by itself, as
//! seL4 does. Every other program is loaded by the root task, which will
//! become the process manager (M7), so this is the only ELF loader in the
//! kernel and it handles only what `user/init` produces: a static,
//! position-independent RISC-V executable whose only relocations are
//! relative ones.

use elf::abi::{EM_RISCV, ET_DYN, PF_R, PF_W, PF_X, PT_LOAD, R_RISCV_RELATIVE};
use elf::endian::LittleEndian;
use elf::ElfBytes;

use crate::arch::{self, AddressSpace, UserContext, USER_END};
use crate::boot::Bytes;
use crate::mm::{BootAllocator, Perms, PAGE_SIZE};
use crate::thread::{Tcb, MAX_PRIORITY};

static IMAGE: &[u8] = include_bytes!(env!("INIT_ELF"));

/// Where the image is placed. Leaves the low pages unmapped so null pointer
/// accesses fault.
const LOAD_BASE: usize = 0x1_0000;
const STACK_SIZE: usize = 16 * 1024;
/// The stack ends one page below the top of user space, leaving a guard.
const STACK_TOP: usize = USER_END - PAGE_SIZE;

/// Load the root task and make it runnable. The caller then starts the
/// scheduler.
pub fn start(alloc: &mut BootAllocator) {
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

    let entry = LOAD_BASE + elf.ehdr.e_entry as usize;
    crate::info!(
        "root task: {} image, {relocations} relocation(s), entry {entry:#x}",
        Bytes(IMAGE.len())
    );

    // The TCB gets a frame of its own; a TCB is smaller than a page.
    // SAFETY: the frame was just allocated and is reached through the direct
    // map.
    let tcb = unsafe { Tcb::init(arch::phys_to_virt(alloc.alloc_frame()), space.root()) };
    // SAFETY: tcb was just initialized.
    unsafe {
        (*tcb).context = UserContext::new_user(entry, STACK_TOP, 0);
        (*tcb).priority = MAX_PRIORITY;
        crate::sched::make_runnable(tcb);
    }
}
