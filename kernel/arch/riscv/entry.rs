//! First instructions after OpenSBI hands over. Runs with the MMU off at the
//! image's physical address, so it uses only PC-relative addressing (`lla`).
//!
//! It zeroes .bss, builds a boot page table, turns on Sv39 and jumps to the
//! kernel's linked (virtual) address, then calls `riscv_start`. The boot
//! table maps:
//! - the 1 GiB around the image at its physical address (identity), so the
//!   instructions right after enabling paging still have a mapping;
//! - the image at KERNEL_VBASE in 2 MiB pages, from its actual physical
//!   address, so the kernel can be loaded at any 2 MiB-aligned address;
//! - physical memory 0..254 GiB at PHYSMAP_BASE in 1 GiB pages (the whole
//!   direct-map window), so Rust code can read the device tree, which QEMU
//!   puts at the top of RAM, and build the real page tables.
//!
//! `paging::init` then replaces this table with one that has per-section
//! permissions and maps only RAM and known devices.

core::arch::global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    # a0 = hart id, a1 = DTB physical address: preserved for riscv_start.
    # t2 = physical address of the image (and of _start, its first byte).
    lla     t2, _start

    # The image must be 2 MiB-aligned to be mapped with 2 MiB pages.
    slli    t0, t2, 64 - 21
    bnez    t0, .Lhang

    # Zero .bss, which also zeroes the boot page tables in it.
    lla     t0, __bss_start
    lla     t1, __bss_end
1:  bgeu    t0, t1, 2f
    sd      zero, 0(t0)
    addi    t0, t0, 8
    j       1b
2:
    lla     s0, boot_root           # level-2 (root) table
    lla     s1, boot_image          # level-1 table for the image window

    # Identity: root[pa >> 30] = 1 GiB leaf at the gigapage containing the
    # image. A leaf PTE is (pa >> 12) << 10 | flags; for a gigapage that is
    # (pa >> 30) << 28. Flags 0xcf = V R W X A D.
    srli    t0, t2, 30
    slli    t1, t0, 28
    ori     t1, t1, 0xcf
    andi    t0, t0, 511
    slli    t0, t0, 3
    add     t0, s0, t0
    sd      t1, 0(t0)

    # Physical memory map: root[256 + i] = gigapage i, for i in 0..254.
    # 0xe7 = V R W G A D (no execute). Index 256 is PHYSMAP_BASE; the window
    # ends where the image window (index 510) begins.
    li      t0, 0
    li      t3, 254
    li      t4, 256 * 8             # too big for an addi immediate
    add     t4, s0, t4
3:  slli    t1, t0, 28
    ori     t1, t1, 0xe7
    sd      t1, 0(t4)
    addi    t4, t4, 8
    addi    t0, t0, 1
    bltu    t0, t3, 3b

    # Image window: root[510] points at boot_image (a non-leaf PTE has only
    # V set). Index 510 is KERNEL_VBASE = 0xffffffff80000000.
    srli    t0, s1, 12
    slli    t0, t0, 10
    ori     t0, t0, 0x01
    li      t1, 510 * 8
    add     t1, s0, t1
    sd      t0, 0(t1)

    # boot_image[j] = 2 MiB leaf at t2 + j * 2 MiB, for every 2 MiB of the
    # image. 0xef = V R W X G A D; paging::init tightens permissions.
    lla     t3, __kernel_end
    sub     t3, t3, t2              # image size
    mv      t4, s1
    mv      t0, t2
    li      t5, 0x200000
4:  srli    t1, t0, 12
    slli    t1, t1, 10
    ori     t1, t1, 0xef
    sd      t1, 0(t4)
    addi    t4, t4, 8
    add     t0, t0, t5
    sub     t3, t3, t5
    bgtz    t3, 4b

    # satp = mode Sv39 (8) << 60 | root PPN; then flush stale translations.
    srli    t0, s0, 12
    li      t1, 8 << 60
    or      t0, t0, t1
    csrw    satp, t0
    sfence.vma

    # Jump to the linked address. `lla` would give the current (physical)
    # address, so load the absolute address from a literal instead.
    lla     t0, .Lhigh_entry
    ld      t0, 0(t0)
    jr      t0

.Lhang:
    wfi
    j       .Lhang

    .balign 8
.Lhigh_entry:
    .dword  high_entry

high_entry:
    # Now at KERNEL_VBASE: lla yields virtual addresses.
    lla     sp, __boot_stack_top
    mv      a2, t2                  # image physical address
    call    riscv_start
5:  wfi
    j       5b

    .section .bss.boot_page_tables, "aw", @nobits
    .balign 4096
boot_root:
    .space  4096
boot_image:
    .space  4096
"#
);
