# Store the C906's vendor M-mode CSRs at 0x41f01000 and return to the boot
# ROM's FEL loop, for `run.sh probe` to dump: mxstatus, mhcr, mcor, mhint,
# 8 bytes each.
    .globl _start
_start:
    li t0, 0x41f01000
    csrr t1, 0x7c0      # mxstatus
    sd t1, 0(t0)
    csrr t1, 0x7c1      # mhcr
    sd t1, 8(t0)
    csrr t1, 0x7c2      # mcor
    sd t1, 16(t0)
    csrr t1, 0x7c5      # mhint
    sd t1, 24(t0)
    ret
