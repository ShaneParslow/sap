//! First instructions after OpenSBI hands over: set up a stack, zero .bss,
//! call `riscv_start`.

core::arch::global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    # a0 = hart id, a1 = DTB physical address; both are passed on to riscv_start.
    la      sp, __boot_stack_top

    la      t0, __bss_start
    la      t1, __bss_end
1:  bgeu    t0, t1, 2f
    sd      zero, 0(t0)
    addi    t0, t0, 8
    j       1b

2:  call    riscv_start
3:  wfi
    j       3b
"#
);
