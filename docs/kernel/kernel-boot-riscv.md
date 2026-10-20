---
status: implemented
summary: How the kernel boots on QEMU virt riscv64 today - OpenSBI enters _start in S-mode, the entry code turns on a boot page table and jumps to the kernel's upper-half address, riscv_start turns the device tree into architecture-neutral boot information and sets up traps, paging and the timer, and kmain runs boot checks and starts the root task. Also the console, log and shutdown.
updated: 2026-10-19
code: [rust-toolchain.toml, Cargo.toml, .cargo/config.toml, kernel/Cargo.toml, kernel/build.rs, kernel/arch/riscv/linker.ld, kernel/arch/riscv/entry.rs, kernel/arch/riscv/mod.rs, kernel/arch/riscv/sbi.rs, kernel/arch/riscv/test_device.rs, kernel/arch/mod.rs, kernel/boot.rs, kernel/devicetree.rs, kernel/console.rs, kernel/log.rs, kernel/main.rs]
verified: 810d575
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Boot on RISC-V

What the kernel does from firmware hand-off until the root task runs, as implemented for roadmap milestones M1 and M2 ([[roadmap]]). The kernel is Rust (`no_std`, edition 2024, target `riscv64gc-unknown-none-elf`). Paging is on [[kernel-memory-riscv]], traps and the timer on [[kernel-traps-riscv]], the root task on [[kernel-root-task]].

## Building and running

The repository is one Cargo workspace (`Cargo.toml`): the kernel, the shared `abi` crate, `libsap` and the user programs, with one lockfile and one set of profiles (`panic = "abort"`). From the top or from `kernel/`: `cargo build`, or `cargo run` to boot in QEMU; the kernel is the default member. `.cargo/config.toml` sets the target, `riscv64gc-unknown-none-elf`, and the runner, `qemu-system-riscv64 -machine virt -nographic -bios default -kernel`, so QEMU's bundled OpenSBI is the firmware. `kernel/build.rs` passes the linker script to the linker and builds `user/init`, the root task, so one `cargo run` builds and boots everything ([[kernel-root-task]]). The compiler is the nightly pinned in `rust-toolchain.toml`, which rustup installs on first use ([[prog-rust-toolchain]]).

Dependencies, all `no_std`: `fdt` (pinned to 0.1.5) for the device tree, `riscv` for CSR access and trap causes, `sbi-rt` for SBI calls, `bitflags`, and `elf` for loading the root task.

A successful run prints the OpenSBI banner, the kernel log, the root task's greeting, and exits with status 0. A panic or a fault in the root task exits with status 1.

## Boot sequence

1. **Firmware.** OpenSBI runs in M-mode, then jumps to the image's physical address (`0x80200000` on QEMU) in S-mode with `a0` = hart id and `a1` = the device tree's physical address. Only one hart is started; the others stay parked in the firmware. With `-smp 4` the boot hart varies from run to run.
2. **Entry** (`_start`, `kernel/arch/riscv/entry.rs:22`). Runs with the MMU off at the physical address, so it uses only PC-relative addressing. It records the image's physical address (line 25), zeroes `.bss`, builds a boot page table, enables Sv39 (line 96), and jumps to `high_entry` (line 113) at the kernel's linked address `0xffffffff80000000`, where it sets the stack and calls `riscv_start`. The boot page table is described in [[kernel-memory-riscv#Boot page table]].
3. **`riscv_start`** (`kernel/arch/riscv/mod.rs:28`), the RISC-V half of boot:
   - records the image's physical address and installs the trap vector;
   - finds QEMU's test device, *before* anything else can panic, so even an early panic exits QEMU with a failure status;
   - parses the device tree into a `BootInfo` and adds the kernel image to the reserved regions;
   - builds the kernel's own page tables from free memory and switches to them ([[kernel-memory-riscv]]);
   - gives the timer its frequency;
   - calls `kmain` with the boot information and the boot allocator.
4. **`kmain`** (`kernel/main.rs:31`), the architecture-neutral kernel: logs the boot information and free memory, starts the timer at 100 Hz, runs `boot_checks` (line 48), loads the root task ([[kernel-root-task]]), and starts the scheduler ([[kernel-threads]]). The boot checks exercise each implemented piece so `cargo run` shows it working (currently a breakpoint trap and ten timer ticks, [[kernel-traps-riscv]]).

## Boot information

`BootInfo` (`kernel/boot.rs:69`) is the contract between the architecture code and the rest of the kernel: nothing in it names an architecture or a platform device. A device tree fills it on RISC-V; a future x86 port would fill it from UEFI and ACPI ([[system-portability]]).

| Field | From the device tree (`devicetree::parse`, `kernel/devicetree.rs:15`) |
|---|---|
| `boot_cpu` | the hart id OpenSBI passed |
| `cpus` | number of `cpu` nodes under `/cpus` |
| `timer_hz` | `timebase-frequency`; a warning if cpus disagree |
| `memory` | `reg` of `/memory` |
| `reserved` | the memory reservation block, `reg` of each `/reserved-memory` child, the blob itself; then the kernel image, added by `riscv_start` |
| `devicetree` | the blob's address and `totalsize`, kept intact for the userspace device manager |

The two reservation mechanisms are both part of the Devicetree Specification, not architecture-specific: the older reservation block in the blob's header and the newer `/reserved-memory` node, which OpenSBI uses ([[hw-device-discovery]]).

Region lists are fixed-capacity (`MAX_REGIONS = 16`, `kernel/boot.rs:28`) because they exist before any allocator, which is built from them. Overflowing the memory list warns and loses RAM; overflowing the reserved list is fatal, because dropping a reservation could hand firmware memory out. A `TODO` notes that x86 UEFI memory maps will need coalescing and a larger capacity.

Platform devices that only the architecture code uses are found with `devicetree::find_device` (`kernel/devicetree.rs:74`), which keeps them out of `BootInfo`. Today that is only QEMU's test device. The device tree is read through the direct map, so `devicetree.rs` works the same before and after the switch to the final page tables.

On QEMU `virt` with defaults: one hart, 10 MHz timebase, 128 MiB of RAM at `0x80000000`, OpenSBI's two reservations at `0x80000000` (256 KiB) and `0x80040000` (128 KiB), a 9,287-byte blob at `0x87e00000`, and the kernel image at `0x80200000` (276 KiB in a debug build, including the embedded root task). Tested with `-smp 4 -m 512M`, `-m 8G`, `-m 15G` (the blob is then at 17 GiB) and `-m 64G`.

Sizes are logged with `Bytes` (`kernel/boot.rs:104`): the smallest binary unit that keeps the number under five digits, rounded to nearest and marked `~` when inexact (`~15 GiB`), with the exact byte count in the alternate form.

## Console and log

`println!` and `print!` (`kernel/console.rs`) format into `arch::console_write`. There is no lock: there is one hart, and interrupt handlers don't print. The console is a debugging aid; a `TODO` says to make kernel printing debug-only once a userspace log service exists, as seL4 does.

The kernel log (`kernel/log.rs`) adds levels: `error!`, `warn!`, `info!`, `debug!`, `trace!` print `[level] message`. `MAX_LEVEL` (`kernel/log.rs:13`) is `Debug` in debug builds and `Info` in release; suppressed levels compile away.

`console_write` (`kernel/arch/riscv/mod.rs:110`) probes the SBI Debug Console extension (DBCN) on first use. With DBCN, a whole buffer is written per call (`kernel/arch/riscv/sbi.rs:16`); the firmware reads it by physical address, so the buffer's address is translated first. Anything the kernel prints is in the image or the direct map, which are physically contiguous. Without DBCN, it falls back to the legacy one-byte `putchar` (`sbi.rs:30`). SBI calls go through the `sbi-rt` crate; `sbi.rs` adapts it to what the kernel needs.

## Shutdown and panics

`arch::shutdown(failure)` (`kernel/arch/riscv/mod.rs:123`) first tries QEMU's test device (`kernel/arch/riscv/test_device.rs:24`): it writes `0x5555` (pass) or `0x3333 | code << 16` (fail, code 1), and QEMU exits with status 0 or the code. OpenSBI's own shutdown can't report failure under QEMU, which is why the test device exists; it only serves automated test runs. Without it, shutdown uses the SBI System Reset extension (`sbi.rs:35`) with reason "none" or "system failure", and halts if that returns.

The panic handler (`kernel/main.rs:64`) logs at error level and shuts down with failure. Panics abort; there is no unwinding. A `TODO` there is a frame-pointer backtrace for debug builds.

> [!todo] Not built
> Starting the other harts (SMP); honoring `no-map` reservations ([[kernel-memory-riscv]]); a backtrace on panic.
