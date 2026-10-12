OUTPUT_ARCH(riscv)
ENTRY(_start)

/* The kernel runs at KERNEL_VBASE (paging.rs must agree). QEMU loads the
   image at its load addresses (LMA), starting at KERNEL_PBASE, where OpenSBI
   (fw_dynamic) jumps in S-mode with a0 = hart id, a1 = DTB address. The entry
   code maps the image from wherever it actually is, so KERNEL_PBASE only
   tells the loader where to put it; it must be 2 MiB-aligned. */
KERNEL_VBASE = 0xFFFFFFFF80000000;
KERNEL_PBASE = 0x80200000;
BOOT_STACK_SIZE = 64K;

SECTIONS
{
    . = KERNEL_VBASE;

    /* Executable: mapped read + execute. */
    .text : AT(ADDR(.text) - KERNEL_VBASE + KERNEL_PBASE) {
        __kernel_start = .;
        KEEP(*(.text.entry))
        *(.text .text.*)
        . = ALIGN(4K);
        __text_end = .;
    }

    /* Read-only: mapped read. */
    .rodata : AT(ADDR(.rodata) - KERNEL_VBASE + KERNEL_PBASE) {
        *(.rodata .rodata.* .srodata .srodata.*)
        . = ALIGN(4K);
        __rodata_end = .;
    }

    /* Everything from here to __kernel_end is mapped read + write. */
    .data : AT(ADDR(.data) - KERNEL_VBASE + KERNEL_PBASE) {
        *(.data .data.* .sdata .sdata.*)
    }

    .bss (NOLOAD) : ALIGN(4K) {
        __bss_start = .;
        *(.sbss .sbss.* .bss .bss.*)
        *(COMMON)
        . = ALIGN(16);
        __bss_end = .;
    }

    .stack (NOLOAD) : ALIGN(16) {
        . += BOOT_STACK_SIZE;
        __boot_stack_top = .;
    }

    . = ALIGN(4K);
    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame .eh_frame_hdr)
    }
}
