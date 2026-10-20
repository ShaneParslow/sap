//! RISC-V: `_start` calls `sap_start` with the thread's start arguments,
//! already in `a0`..`a2` (the thread start convention).

core::arch::global_asm!(
    r#"
    .section .text.entry
    .globl _start
_start:
    call    sap_start
1:  j       1b
"#
);
