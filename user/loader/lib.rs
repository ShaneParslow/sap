//! Loading a program into a new process: its own address space, capability
//! space and thread, built from untyped memory the caller provides. This is
//! the start of the process manager (M7); today the root task uses it to
//! start a second program.
//!
//! The image is a static position-independent ELF, like the root task's.
//! Each page is filled through a scratch mapping in the caller's address
//! space, with relocations applied, then mapped into the new address space
//! with its segment's permissions. The new thread follows the start
//! convention: `sp` at the top of its stack, `tp` at its IPC buffer, three
//! arguments in `a0`..`a2`.
//!
//! TODO: no bootstrap message yet; arguments are three words and the
//! parent copies capabilities into fixed slots (M7, typed bootstrap).

#![no_std]
#![feature(strict_provenance_lints)]
// Pointers come from `sap_sys::address::mapped`, never from integer casts.
#![deny(implicit_provenance_casts, integer_to_ptr_transmutes)]

mod arch;

use elf::abi::{ET_DYN, PF_W, PF_X, PT_LOAD};
use elf::endian::LittleEndian;
use elf::ElfBytes;
use sap_abi::{attr, error, object, reg, rights};

use sap_sys::cnode::{SlotRef, WORD};
use sap_sys::{tcb, untyped, vspace};

const PAGE: usize = 4096;
/// Where images are placed (as the kernel places the root task).
const LOAD_BASE: usize = 0x1_0000;
const USER_END: usize = 1 << 38;
const STACK_PAGES: usize = 4;
const STACK_TOP: usize = USER_END - PAGE;
const IPC_BUFFER: usize = STACK_TOP - STACK_PAGES * PAGE - 2 * PAGE;
/// log2 of the new process's slot count; slot 0 stays empty.
pub const CNODE_RADIX: usize = 6;

/// What the loader takes from its caller.
pub struct Parent {
    /// The caller's root CNode, address space, ASID pool and an authority
    /// thread for priorities.
    pub cnode: usize,
    pub vspace: usize,
    pub asid_pool: usize,
    pub authority: usize,
    /// Untyped memory every object of the new process is made from.
    pub untyped: usize,
    /// An unused, 1 GiB-aligned region of the caller's address space for
    /// filling pages.
    pub scratch: usize,
}

/// Free slots of the caller's root CNode, handed out in order.
pub struct Slots {
    pub next: usize,
    pub end: usize,
}

impl Slots {
    fn take(&mut self) -> usize {
        assert!(self.next < self.end, "loader: out of slots");
        self.next += 1;
        self.next - 1
    }
}

/// A loaded process, configured but not started. Its objects' capabilities
/// are in the caller's slots.
pub struct Process {
    pub tcb: usize,
    pub cnode: usize,
    /// The CNode capability the thread's capability space is rooted at,
    /// with a guard so that full-width addresses name its slots. Give the
    /// process a copy of this one to let it name its own slots.
    pub guarded_cnode: usize,
    pub vspace: usize,
}

impl Process {
    /// Copy the caller's capability `cap` into slot `index` of the
    /// process's CNode, with `rights`.
    pub fn give(&self, parent: &Parent, index: usize, cap: usize, rights: usize) {
        let dest = SlotRef { root: self.cnode, index, depth: CNODE_RADIX };
        dest.copy_from(SlotRef::new(parent.cnode, cap), rights).expect("loader: give a capability");
    }

    /// Add page tables to the process's address space until `va` has every
    /// level, so a frame can later be mapped there without untyped memory.
    pub fn map_tables(&self, parent: &Parent, slots: &mut Slots, va: usize) {
        ensure_tables(parent, slots, self.vspace, va);
    }

    pub fn start(&self) {
        tcb::resume(self.tcb).expect("loader: start");
    }

    /// Map the caller's frame capability `frame` at `va` in the process's
    /// address space (adding page tables as needed). The capability is then
    /// that mapping; copy it first to map the frame elsewhere too.
    pub fn map_frame(&self, parent: &Parent, slots: &mut Slots, frame: usize, va: usize, granted: usize) {
        ensure_tables(parent, slots, self.vspace, va);
        vspace::map_frame(frame, self.vspace, va, granted, attr::EXECUTE_NEVER).expect("loader: map a frame");
    }
}

fn retype(parent: &Parent, slots: &mut Slots, object_type: usize, size_bits: usize) -> usize {
    let slot = slots.take();
    let root = SlotRef { root: parent.cnode, index: 0, depth: 0 };
    untyped::retype(parent.untyped, object_type, size_bits, root, slot, 1).expect("loader: retype");
    slot
}

/// Map page tables in `space` until `va` has every level below the root.
fn ensure_tables(parent: &Parent, slots: &mut Slots, space: usize, va: usize) {
    loop {
        let table = retype(parent, slots, object::PAGE_TABLE, 0);
        match vspace::map_page_table(table, space, va) {
            Ok(()) => continue,
            Err(e) if e.0 == error::DELETE_FIRST => {
                // Every level was there; this table isn't needed.
                let _ = SlotRef::new(parent.cnode, table).delete();
                return;
            }
            Err(e) => panic!("loader: page table: error {}", e.0),
        }
    }
}

/// A fresh frame filled by `fill` through the scratch mapping.
fn filled_frame(parent: &Parent, slots: &mut Slots, fill: impl FnOnce(&mut [u8; PAGE])) -> usize {
    let frame = retype(parent, slots, object::FRAME_4K, 0);
    let rw = rights::READ | rights::WRITE;
    vspace::map_frame(frame, parent.vspace, parent.scratch, rw, attr::EXECUTE_NEVER).expect("loader: scratch map");
    // SAFETY: the frame was just mapped read-write at the scratch address,
    // and nothing else uses that address.
    fill(unsafe { &mut *sap_sys::address::mapped::<[u8; PAGE]>(parent.scratch) });
    vspace::unmap_frame(frame).expect("loader: scratch unmap");
    frame
}

/// Load `image` into a new process at `priority`, with `args` in its first
/// three argument registers.
pub fn load(image: &[u8], parent: &Parent, slots: &mut Slots, priority: usize, args: [usize; 3]) -> Process {
    let elf = ElfBytes::<LittleEndian>::minimal_parse(image).expect("loader: not an ELF file");
    assert!(
        elf.ehdr.e_machine == arch::MACHINE && elf.ehdr.e_type == ET_DYN,
        "loader: not a position-independent executable for this machine"
    );
    let segments = elf.segments().expect("loader: no program headers");
    let (sections, strings) = elf.section_headers_with_strtab().expect("loader: bad section headers");
    let (sections, strings) = (sections.expect("loader: no sections"), strings.expect("loader: no strings"));
    let relocations = || {
        sections
            .iter()
            .filter(|h| strings.get(h.sh_name as usize).is_ok_and(|n| n == ".rela.dyn"))
            .flat_map(|h| elf.section_data_as_relas(&h).expect("loader: bad relocations"))
    };

    let space = retype(parent, slots, object::PAGE_TABLE, 0);
    vspace::assign_asid(parent.asid_pool, space).expect("loader: assign an ASID");
    let cnode = retype(parent, slots, object::CNODE, CNODE_RADIX);
    let thread = retype(parent, slots, object::TCB, 0);
    ensure_tables(parent, slots, parent.vspace, parent.scratch);

    for s in segments.iter().filter(|s| s.p_type == PT_LOAD) {
        let (vaddr, filesz, memsz, offset) = (s.p_vaddr as usize, s.p_filesz as usize, s.p_memsz as usize, s.p_offset as usize);
        let mut granted = rights::READ;
        if s.p_flags & PF_W != 0 {
            granted |= rights::WRITE;
        }
        let attributes = if s.p_flags & PF_X != 0 { 0 } else { attr::EXECUTE_NEVER };
        assert!(s.p_flags & PF_W == 0 || s.p_flags & PF_X == 0, "loader: writable and executable segment");
        let start = vaddr & !(PAGE - 1);
        let end = (vaddr + memsz).next_multiple_of(PAGE);
        for page in (start..end).step_by(PAGE) {
            let frame = filled_frame(parent, slots, |bytes| {
                bytes.fill(0);
                // The part of the file image that lands in this page.
                let from = vaddr.max(page);
                let to = (vaddr + filesz).min(page + PAGE);
                if from < to {
                    let src = offset + (from - vaddr);
                    bytes[from - page..to - page].copy_from_slice(&image[src..src + (to - from)]);
                }
                // Relative relocations in this page: load base + addend.
                for rela in relocations() {
                    assert!(rela.r_type == arch::RELATIVE, "loader: unsupported relocation {}", rela.r_type);
                    let at = rela.r_offset as usize;
                    if (page..page + PAGE).contains(&at) {
                        let value = (LOAD_BASE as i64 + rela.r_addend) as u64;
                        bytes[at - page..at - page + 8].copy_from_slice(&value.to_le_bytes());
                    }
                }
            });
            ensure_tables(parent, slots, space, LOAD_BASE + page);
            vspace::map_frame(frame, space, LOAD_BASE + page, granted, attributes).expect("loader: map image");
        }
    }

    let rw = rights::READ | rights::WRITE;
    for i in 0..STACK_PAGES {
        let frame = retype(parent, slots, object::FRAME_4K, 0);
        let va = STACK_TOP - (i + 1) * PAGE;
        ensure_tables(parent, slots, space, va);
        vspace::map_frame(frame, space, va, rw, attr::EXECUTE_NEVER).expect("loader: map stack");
    }
    let buffer = retype(parent, slots, object::FRAME_4K, 0);
    ensure_tables(parent, slots, space, IPC_BUFFER);
    vspace::map_frame(buffer, space, IPC_BUFFER, rw, attr::EXECUTE_NEVER).expect("loader: map IPC buffer");

    // The thread's capability space is the new CNode, with a guard so that
    // full-width addresses name its slots directly.
    let guarded = slots.take();
    let guard = WORD - CNODE_RADIX; // guard size in bits, guard value 0
    SlotRef::new(parent.cnode, guarded)
        .mint_from(SlotRef::new(parent.cnode, cnode), rights::ALL, guard)
        .expect("loader: guarded CNode");
    tcb::configure(thread, guarded, space, Some((buffer, IPC_BUFFER))).expect("loader: configure");
    tcb::set_priority(thread, parent.authority, priority).expect("loader: priority");
    let mut regs = [0; reg::COUNT];
    regs[reg::PC] = LOAD_BASE + elf.ehdr.e_entry as usize;
    regs[reg::SP] = STACK_TOP;
    regs[reg::TP] = IPC_BUFFER;
    regs[reg::A0..reg::A0 + 3].copy_from_slice(&args);
    tcb::write_registers(thread, false, &regs).expect("loader: registers");
    Process { tcb: thread, cnode, guarded_cnode: guarded, vspace: space }
}
