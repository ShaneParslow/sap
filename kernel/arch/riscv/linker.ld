OUTPUT_ARCH(riscv)
ENTRY(_start)

/* OpenSBI (fw_dynamic) jumps here in S-mode with a0 = hart id, a1 = DTB address. */
KERNEL_BASE = 0x80200000;
BOOT_STACK_SIZE = 64K;

SECTIONS
{
    . = KERNEL_BASE;

    .text : {
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(16) {
        *(.rodata .rodata.* .srodata .srodata.*)
    }

    .data : ALIGN(16) {
        *(.data .data.* .sdata .sdata.*)
    }

    .bss (NOLOAD) : ALIGN(16) {
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

    /DISCARD/ : {
        *(.eh_frame .eh_frame_hdr)
    }
}
