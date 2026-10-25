# Runs before OpenSBI: turn on the C906's caches and branch prediction,
# which the boot ROM leaves off, with the values U-Boot's SPL uses, then
# enter OpenSBI's fw_payload at 0x40000000. mxstatus is left alone, so MAEE
# stays off and memory attributes come from the D1's fixed sysmap. run.sh
# loads it at 0x41f00000, above the firmware and below the kernel's memory.
    .globl _start
_start:
    li t0, 0x70013      # mcor: invalidate I/D caches, BHT and BTB
    csrw 0x7c2, t0
    li t0, 0x11ff       # mhcr: IE DE WA WB RS BPE, write burst, BTB
    csrw 0x7c1, t0
    li t0, 0x16e30c     # mhint: prefetch
    csrw 0x7c5, t0
    fence.i
    csrr a0, mhartid    # OpenSBI's entry arguments: the hart; a1 and a2
    li a1, 0            # are unused, since fw_payload carries its own
    li a2, 0            # device tree
    li t0, 0x40000000
    jr t0
