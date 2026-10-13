---
status: proposed
summary: How sap learns what hardware exists - the flattened device tree on RISC-V and embedded Arm, ACPI on x86 and Arm/RISC-V servers - what a minimal DTB parser does versus a complete one, and the proposed split between a minimal kernel parser and full discovery in a userspace device manager.
updated: 2026-10-08
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Device discovery

The kernel's minimal parser is implemented with the `fdt` crate ([[kernel-boot-riscv#Boot information]]). This page covers how platforms describe their hardware and proposes how sap should read that description. The human asked how device-tree parsing looks, how the simplest version differs from a complete one, and how other architectures compare ([[2026-10-08-22c3947b-wiki-setup]]).

## How platforms describe hardware

| Platform | Mechanism |
|---|---|
| RISC-V (embedded, QEMU `virt`, most boards) | Flattened device tree (DTB) passed by firmware |
| Arm embedded and most SoCs | Device tree |
| Arm servers (SBSA/SBBR), many Arm laptops | ACPI, via UEFI |
| RISC-V server platforms | ACPI is being added alongside DT |
| x86 | ACPI tables plus PCI enumeration; memory map from UEFI (or the legacy E820 BIOS call) |

**ACPI** is a set of tables reached from a root pointer (RSDP → XSDT): MADT lists CPUs and interrupt controllers, MCFG locates PCIe configuration space, FADT holds fixed power-management hardware. The DSDT and SSDTs contain **AML**, a bytecode that must be interpreted to enumerate some devices and to do power management. The AML interpreter is the expensive part, and it is where x86's reputation for mess comes from: ACPICA, the reference interpreter, is a large C codebase. A minimal x86 kernel can avoid AML entirely by reading only the memory map and MADT, and finding most devices through PCI enumeration; AML becomes unavoidable for power management and laptop hardware.

A device tree is data only: no bytecode, nothing to execute.

## The DTB format

A flattened device tree is one big-endian blob ([Devicetree Specification](https://www.devicetree.org/specifications/)):

- **Header:** magic `0xd00dfeed`, total size, offsets to the other blocks, version (17).
- **Memory reservation block:** (address, size) pairs the OS must not use, ending with (0, 0).
- **Structure block:** a stream of 32-bit tokens: `BEGIN_NODE` + NUL-terminated name (padded to 4 bytes), `PROP` + value length + offset of the property name + value (padded), `END_NODE`, `NOP`, `END`.
- **Strings block:** NUL-terminated property names, referenced by offset.

Nodes form a tree by nesting. A node's `reg` property is a list of (address, size) cells whose widths come from the *parent's* `#address-cells` and `#size-cells`. Nodes refer to each other by `phandle`, a numeric id (for example, a device's `interrupt-parent`).

## Minimal parser

What sap needs before user mode, and roughly 200–300 lines of `no_std` Rust with no allocation:

1. Check the magic and version, and bounds-check every offset and length against the total size. The blob comes from firmware, but checking is cheap and turns corrupt input into a clean error.
2. Walk the structure block once, tracking depth and the current node path, and borrowing names and values straight from the blob.
3. Read only what the kernel needs:
   - `/cpus` → `timebase-frequency`; the hart count from the `cpu@N` children;
   - `/memory` → `reg`: RAM ranges;
   - the memory reservation block and `/reserved-memory`: ranges to leave alone (OpenSBI's own memory is reserved this way);
   - `/chosen` → `stdout-path`, `bootargs`;
   - nodes by `compatible` string: for example `sifive,test0` / `syscon-poweroff` (QEMU's exit device), `riscv,plic0` (interrupt controller), `virtio,mmio`.
4. Decode `reg` using the parent's `#address-cells` and `#size-cells`.

Off-the-shelf alternative: the `fdt` crate (`no_std`, zero-allocation, read-only, no dependencies, MPL-2.0). As checked on 2026-10-08:
- **Versions:** 0.1.5 (February 2023) is the stable release, with about 1.15 million downloads. 0.2.0 is in alpha (alpha3, October 2026) with a changed API; pin 0.1.5 until 0.2 is released.
- **Panics:** `Fdt::new` validates the header and returns an error, but 0.1.5's convenience accessors panic when a required node is missing (`cpus()`, `memory()`, `chosen()`, `root()` call `expect`), and string lookups `expect` NUL-terminated UTF-8. Use `find_node` and handle `None` where the node is optional. A panic at boot on malformed firmware is acceptable; the kernel can't continue without the information anyway.
- **Lifetimes:** everything borrows from the blob, so the DTB's memory must stay reserved and mapped while parsed data is in use. The DTB is not necessarily in the memory reservation block, so the kernel must exclude its range from free memory itself and pass it on to the root task.
- **MPL-2.0** is file-level copyleft: modified crate files stay MPL; sap's own code is unaffected.

## Complete parser

What a general-purpose OS such as Linux adds:

- **Unflattening** into an in-memory tree with parent pointers and a phandle index, so lookups aren't linear scans.
- **phandle resolution** for interrupts, clocks, resets, power domains, GPIOs, PHYs.
- **Address translation** through `ranges` on every bus between a device and the CPU.
- **Interrupt routing:** `interrupt-parent` chains, `interrupts-extended`, and `interrupt-map` (PCI legacy interrupts).
- **`status`:** skipping devices marked `"disabled"`.
- **Driver matching:** `compatible` lists ordered most specific first, matched against every driver.
- **Aliases** (`/aliases`), **overlays** applied at runtime, multiple memory nodes, `no-map` reserved regions, NUMA and cache topology properties.

Linux's implementation (the OF subsystem) is large because it serves every driver on every board.

## Proposed split for sap

A microkernel should parse as little as possible (principle 4 in [[vision-principles]]):

- **Kernel: the minimal parser.** Only what it needs to run: RAM ranges and reservations (to build untyped memory, [[kernel-objects-and-syscalls]]), timebase, hart count, the interrupt controller, the boot console. The rest of the blob is passed through untouched.
- **Userspace device manager: the complete parser.** The root task (or a device-manager component) receives the DTB, plus capabilities to device memory and interrupts, parses everything, and hands each driver component capabilities to exactly its MMIO ranges and IRQs ([[hw-devices-and-io]]). Drivers never parse firmware tables.
- **Hardware description as data.** The parsed tree could be exposed as typed objects so topology is queryable like everything else ([[hw-memory-topology]]). ACPI on a future x86 or server port would then be a second producer of the same objects, with the AML interpreter living in a userspace component, not the kernel.

seL4 does this differently: its build system turns the platform's device tree into kernel configuration at compile time, and the root task receives the DTB at run time ([[sel4]]). Parsing at run time keeps one kernel binary for multiple boards.

> [!question] Open
> The userspace device manager: which crate or parser it uses for the complete parse (phandles, `ranges`, interrupt maps), and how it hands device capabilities to drivers.
