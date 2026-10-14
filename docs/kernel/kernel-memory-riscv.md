---
status: implemented
summary: How the kernel manages memory on RISC-V today - Sv39 with the kernel in the upper half, a boot page table built in assembly, the kernel's own address space with per-section permissions and a direct map of RAM, free memory computed from the device tree, a boot-only bump allocator, user address spaces that share the kernel half, and hardware ASIDs.
updated: 2026-10-13
code: [kernel/arch/riscv/entry.rs, kernel/arch/riscv/linker.ld, kernel/arch/riscv/paging.rs, kernel/mm.rs, kernel/boot.rs]
verified: 0c39997
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Memory on RISC-V

Paging is on from the first instructions of boot, and the kernel never allocates memory after boot: what it doesn't use for itself is meant to become untyped memory for the root task (M3), as in seL4 ([[kernel-objects-and-syscalls]]). Part of roadmap milestone M2 ([[roadmap]]). Boot order is on [[kernel-boot-riscv]].

## Address space layout

Sv39 translates 39-bit virtual addresses through three levels of 512-entry tables, with leaves at any level: 4 KiB, 2 MiB or 1 GiB pages. The lower half belongs to user space and the upper half to the kernel (`kernel/arch/riscv/paging.rs:22-42`):

| Range | Contents | Permissions |
|---|---|---|
| `0` .. `0x40_0000_0000` (`USER_END`) | user address space | per mapping, user-accessible |
| `0xffff_ffc0_0000_0000` (`PHYSMAP_BASE`) + physical address | direct map: all RAM, plus devices the kernel uses | read/write, never execute |
| `0xffff_ffff_8000_0000` (`KERNEL_VBASE`) .. image end | the kernel image | text read/execute, rodata read, data/bss/stack read/write |

The direct-map window is 254 GiB, everything below the image window. Sv39 can't direct-map RAM above that, and boot stops with "needs Sv48" if the device tree reports any. All kernel mappings are global, so they stay in the TLB across address-space switches.

The kernel is linked at `KERNEL_VBASE` (`kernel/arch/riscv/linker.ld:9`), with load addresses starting at `0x80200000` (line 10) so QEMU's loader puts it where OpenSBI jumps. The physical location isn't built in anywhere else: the entry code maps the image from wherever it actually is, which only has to be 2 MiB-aligned. Section boundaries are page-aligned (`__text_end`, `__rodata_end`, `__kernel_end`, lines 23, 30, 52) so each can get its own permissions.

## Boot page table

The entry code (`kernel/arch/riscv/entry.rs:22-120`) builds a temporary table in two pages of `.bss` (`boot_root`, line 123) before any Rust runs, because compiled Rust code holds absolute addresses that are only valid at the linked address. It maps:
- the 1 GiB around the image at its physical address, so the instructions right after `csrw satp` still have a mapping;
- the image at `KERNEL_VBASE`, in 2 MiB pages from its actual physical address;
- the whole direct-map window, 0..254 GiB of physical addresses, in 1 GiB pages. QEMU puts the device tree at the top of RAM, so Rust code needs the direct map before it knows how much RAM there is.

It then jumps to the linked address by loading it from a literal (`.Lhigh_entry`), since a PC-relative `lla` would yield the physical address.

## Kernel address space

`paging::init` (`kernel/arch/riscv/paging.rs:406`) builds the kernel's real address space and switches to it, replacing the boot table:
- the image, section by section, with the permissions above;
- every RAM region in the direct map;
- the devices the architecture code uses, today only QEMU's test device, in the direct map at their physical address.

`AddressSpace::map` (line 215) maps a range with the largest pages that fit the alignment of both addresses and the remaining size; on QEMU the kernel's tables take 5–6 frames. Mapping over an existing mapping panics. Leaves get the accessed and dirty bits set up front, so hardware without automatic updates (Svade) doesn't fault on first use. `activate` (line 256) writes `satp` (Sv39, ASID 0) and flushes the TLB; it is used once, to switch from the boot page table.

`phys_to_virt` and `virt_to_phys` (lines 102, 107) translate between physical addresses and the direct map or image. The console uses `virt_to_phys` to hand SBI physical buffer addresses.

Tested: writing to kernel text and reading the old identity-mapped address both fault after the switch.

## Free memory and the boot allocator

`mm::free_regions` (`kernel/mm.rs:34`) subtracts every reserved region from every memory region, trimmed to whole pages; reservations may be unsorted and overlapping. On QEMU with 128 MiB this leaves three regions, about 127 MiB: below the kernel image, between the image and the device tree, and above the device tree.

`BootAllocator` (`kernel/mm.rs:59`) hands out frames from the front of the free regions: single frames for page tables (`alloc_frame`, line 71) and contiguous runs for the root task's image and stack (`alloc_contiguous`, line 85). Nothing is ever freed. It exists only for the kernel's own setup; what remains is passed to `kmain` and is meant to become untyped memory in M3.

## User address spaces

`AddressSpace::new_user` (`kernel/arch/riscv/paging.rs:197`) creates a root table whose upper half is copied from the kernel's root, so its entries point at the kernel's lower-level tables: the kernel stays mapped (without the user bit) while user code runs, and a trap needs no `satp` switch. `map_user` (line 204) maps below `USER_END` with architecture-neutral permissions (`mm::Perms`) plus the user bit.

A `TODO` notes that a kernel mapping added later at the root level would not reach existing user spaces; the kernel adds none after boot. Address spaces created later from retyped page tables get the same upper half when they are given an ASID (`make_root`, line 375). How userspace builds and maps them is on [[kernel-capabilities#Address spaces]].

## ASIDs and the TLB

Each address space has an address-space identifier (ASID): the root task's is 1, later ones come from the ASID pool. At boot `probe_asid_bits` (`kernel/arch/riscv/paging.rs:268`) finds how many ASID bits `satp` implements, by writing all ones and reading back what sticks; QEMU implements 16. When an address space's ASID fits, `activate_root` (line 288) puts it in `satp` and flushes nothing, because the TLB tags entries with their ASID and the kernel's mappings are global. A space whose ASID doesn't fit runs as ASID 0 with a full flush on every switch. Unmapping a page flushes just that page in that ASID (`flush_page`); unmapping a table, or freeing an ASID for reuse, flushes the ASID (`flush_asid`).

> [!todo] Not built
> - Skipping `no-map` reservations (OpenSBI's memory is mapped in the direct map today, though PMP blocks access).
> - Remote TLB shootdown, once more harts run.
> - Sv48 for machines with more than 254 GiB of RAM.
