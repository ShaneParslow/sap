---
status: implemented
summary: sap on real hardware - the MangoPi MQ-Pro (Allwinner D1, one T-Head C906 at 1 GHz, 1 GiB), booted over USB in FEL mode with tools/board/d1, the unmodified kernel inside OpenSBI's fw_payload; what the board needs (a device tree with memory and CLINT nodes, caches turned on, a watchdog for recovery) and what it is like.
updated: 2026-10-24
sources: ["[[2026-10-20-685b0203-ipc-cost-and-cheri]]", "[[2026-10-22-e547b584-debrief]]"]
code: [tools/board/d1/build.sh, tools/board/d1/run.sh, tools/board/d1/mq-pro.dts, tools/board/d1/sun20i-d1-mangopi-mq-pro.dts, tools/board/d1/caches.S, tools/board/d1/probe.S, toolchain/build.sh]
---

# The D1 board

sap's one real RISC-V machine is the human's MangoPi MQ-Pro, an Allwinner D1 board ([[2026-10-16-2c1ac53c#m22|§m22]]). Its job is measurement: QEMU is fine for correctness but distorts costs, since a system call there takes microseconds while copies run at close to host speed ([[hw-isa#Real hardware]]). The kernel boots on it unchanged and every check passes; what differs from QEMU is all firmware-side, in `tools/board/d1/`.

## The board

Checked on the board on 2026-10-20 and 2026-10-24:
- **Core:** one T-Head C906 at 1 GHz, RV64GC with an MMU (Sv39) and 16 ASID bits. Its vector unit predates the ratified vector extension (version 0.7.1), which standard toolchains don't target.
- **One hart,** so it measures single-core costs only, not the multi-CPU polling the rings are designed for ([[ipc-rings#What runs show]]).
- **Memory attributes:** the C906's own page-table bits (63–59) apply only when `mxstatus.MAEE` is set, which Allwinner's firmware and U-Boot do and Linux then handles as a vendor erratum. In this boot path MAEE is clear, so standard Sv39 entries work and attributes come from the chip's fixed address map (the "sysmap"; see [openc906's `sysmap.h`](https://github.com/XUANTIE-RV/openc906/blob/main/C906_RTL_FACTORY/gen_rtl/mmu/rtl/sysmap.h)).
- **Caches:** the data cache is off after the boot ROM, and nothing in OpenSBI turns it on. Uncached, a system call takes about 10 µs and copying runs at a few MB/s; cached, 0.7 µs.
- **No power-off:** a run ends with the core stopped in OpenSBI, whose system reset does nothing here.

> [!warning] Two readings of the vendor CSRs
> On 2026-10-20, `mxstatus` and `mhcr` read 0 under FEL. On 2026-10-24, after a watchdog reset into FEL, `run.sh probe` read `mxstatus` 0xc0408000 (machine mode, `MM` and `THEADISAEE` set, `MAEE` clear), `mhcr` 0x109 (instruction cache on, data cache off), `mcor` 0x3 and `mhint` 0x4000. Why they differ is unknown; perhaps a cold power-on leaves them at 0 and a watchdog reset doesn't. Either way MAEE is clear and the data cache is off, which is what the boot path relies on.

## Building

`tools/board/d1/build.sh` builds everything into `target/board/d1/`:
1. the plain RISC-V kernel in release mode, as a flat binary;
2. the device tree (`dtc`, `tools/board/d1/build.sh:33`) from `mq-pro.dts`, which includes Linux's tree for the board and adds the two nodes the kernel needs (`tools/board/d1/mq-pro.dts:9`, `tools/board/d1/mq-pro.dts:18`): a `memory` node (0x40000000, 1 GiB), which Linux gets from U-Boot instead, and the CLINT (`thead,c900-clint` at 0x14000000), without which OpenSBI's timer never fires and the kernel's timer check hangs;
3. the two M-mode stubs, `caches.S` and `probe.S`, linked at 0x41f00000 (`tools/board/d1/build.sh:35`);
4. OpenSBI's `fw_payload` for the generic platform, from `toolchain/opensbi` built for plain RV64GC, with the kernel as its payload at 0x40200000 and the device tree built in (`tools/board/d1/build.sh:48`). OpenSBI's make doesn't notice a changed payload, so the firmware images are rebuilt every time (`tools/board/d1/build.sh:43`).

Linux's tree is vendored with its includes already expanded (`sun20i-d1-mangopi-mq-pro.dts`, from Linux v7.1-rc7, dual-licensed GPL-2.0+ or MIT), so no Linux tree is needed. The compiler is the in-tree LLVM if it is built, else clang and lld from the `PATH`; nothing depends on the machine the scripts were written on.

## Running

`tools/board/d1/run.sh` boots it over USB, with the board in the boot ROM's **FEL mode** (no SD card, powered from its USB-OTG port), using [xfel](https://github.com/xboot/xfel), a submodule in `toolchain/` built by `toolchain/build.sh xfel` ([[hw-cheri#Platform support]] for the rest of the toolchain). In order, it:
1. waits for the FEL device (`wait_for_fel`, `tools/board/d1/run.sh:40`), which comes back a few seconds after the previous run;
2. opens the serial console on the USB-serial adapter (`SERIAL`, default `/dev/ttyUSB0`, 115200 8N1) and copies it to `target/board/d1/console.log` (`tools/board/d1/run.sh:76`). It refuses if another process has the port open (`tools/board/d1/run.sh:66`), since two readers split the output between them;
3. initialises DRAM, writes `fw_payload` to 0x40000000 and, unless `CACHES=0`, the cache stub to 0x41f00000;
4. arms the SoC watchdog (`tools/board/d1/run.sh:88`; base 0x020500a0, registers as in Linux's `sunxi_wdt.c`): whole-system reset after 16 s;
5. jumps to the stub, which sets `mcor`, `mhcr` and `mhint` to U-Boot's values (0x70013 to invalidate, 0x11ff, 0x16e30c) and enters OpenSBI; or straight to OpenSBI;
6. prints the console until the kernel halts or panics, or 16 s pass (`tools/board/d1/run.sh:99`), and exits with success only if the kernel halted with success (`tools/board/d1/run.sh:104`).

The **watchdog** is what makes runs repeatable without anyone at the board: however a run ends, the board resets into FEL, so runs can follow each other unattended (tested twice in a row on 2026-10-20). It also bounds a run to 16 s.

`run.sh probe` instead runs `probe.S`, which stores the C906's vendor CSRs (`mxstatus`, `mhcr`, `mcor`, `mhint`) to memory and returns to the boot ROM, and dumps them (`tools/board/d1/run.sh:49`).

The console is OpenSBI's, through the SBI debug console over the 16550-style UART, so the kernel needs no UART driver.

> [!todo] Not built
> The serial capture in `run.sh` (steps 2 and 6) was written on 2026-10-24 and hasn't run yet: the serial port wasn't readable by the user, who isn't in its group (`uucp` on Arch); `run.sh` says so rather than failing silently. The FEL steps, the watchdog and the stubs are those of the 2026-10-20 runs, and the build reproduces those runs' stubs and device tree byte for byte.

## What it measured

On the D1 with caches on (release build, 2026-10-20): a yield takes 0.7 µs, an identify 0.9–1.0 µs and a map plus an unmap 16.8–17.1 µs; a 60 KiB message costs 391 µs by ring copy and 1,081 µs by page transfer, and copying wins at every size. The TLB flushes after mapping are about half of each transferred page's cost. The tables and the experiments behind them are in [[ipc-page-transfer#What it costs]].
