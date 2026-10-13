---
status: exploring
summary: What it would take to run sap on Arm and x86 - which parts are architecture-specific, why a microkernel quarantines most of x86's mess, where the real cost is (drivers and laptop power management), and a proposed order for ports.
updated: 2026-10-08
sources: ["[[2026-10-08-22c3947b-wiki-setup]]"]
---

# Portability across architectures

The human wants x86 eventually for real use, since Arm adoption on the desktop is slow, but sees x86 as complicated ([[2026-10-08-22c3947b-wiki-setup]]). This page breaks the cost down by layer. Short version: **an x86 port is tractable**, because the microkernel split confines nearly all of x86's complexity either to a bounded architecture layer or to userspace components. The real cost of real-world use is drivers, and that is true on every architecture.

## What is architecture-specific

| Layer | Portable? | Notes |
|---|---|---|
| Components, interfaces, IPC conventions, object store | Yes | Rust and Cap'n Proto; a port only changes the target triple. |
| Device discovery | Mostly | The kernel reads a few facts; full discovery lives in a userspace device manager ([[hw-device-discovery]]). |
| Kernel architecture layer | No | Boot entry, trap entry/exit, context switch, page tables, timer, interrupt controller, inter-processor interrupts, TLB and cache maintenance, syscall entry. |
| Isolation accelerators | No | CHERI exists only on RISC-V and Arm Morello. Already optional by design ([[ipc-paging-vs-cheri]]). |
| Drivers | Per device | The dominant cost on any platform. |

## x86's complexity, layer by layer

**Boot.** The legacy path (real mode → protected mode → long mode) can be skipped entirely: UEFI, or the Limine boot protocol, enters the kernel already in 64-bit long mode with a memory map, the ACPI root pointer, a framebuffer, and optionally the other CPUs started. Limine makes x86 boot roughly as simple as OpenSBI makes RISC-V boot.

**CPU structures.** GDT, IDT and TSS setup, 4- or 5-level page tables, PCID, the local APIC (or x2APIC) and IOAPIC, and calibrating the TSC as a timer. More pieces than RISC-V, but small, well documented, and written once. Starting the other CPUs with INIT-SIPI-SIPI needs a real-mode trampoline, which Limine can do instead.

**Speculation mitigations.** Kernel entry needs page-table isolation and branch-predictor barriers on affected CPUs; this is x86's main recurring cost for a microkernel ([[kernel-crossing-costs]]). FRED, on new CPUs, simplifies entry and exit.

**ACPI.** The kernel needs only static tables: the memory map (from UEFI), the MADT (CPUs and interrupt controllers) and maybe a timer table. No AML. The AML interpreter, needed for power management and some device enumeration, goes into a userspace component, as Genode does with ACPICA. This is where the microkernel split pays off most: the messiest part of the platform never enters the kernel.

**Memory ordering.** x86 is TSO; RISC-V and Arm are weakly ordered. Code correct on the weak models is correct on TSO, but not the other way round. Developing on RISC-V first means ordering bugs show up early rather than on the second port.

Rough size: the x86 architecture layer would be perhaps two to three times the RISC-V one, and bounded. seL4 supports x86-64, Arm and RISC-V from one design, which is direct evidence that the kernel interface ports.

## The real cost: drivers

What decides whether sap is usable on a given machine:
- **Easy, standard on PCs:** NVMe, AHCI, xHCI (USB, large but one driver), HD Audio, the UEFI framebuffer, PS/2 and USB HID input, virtio in VMs.
- **Hard:** GPUs (Intel is the most documented; AMD is open but huge; NVIDIA is mostly firmware-driven), Wi-Fi, laptop power management (suspend and resume, battery, the embedded controller, all through ACPI/AML), and the long tail of Ethernet and Bluetooth chips.

The design's answer for the hard set is a contained Linux VM with the devices passed through behind the IOMMU, exposed to sap through typed interfaces ([[system-legacy-compat]]). On x86 that needs VMX or SVM and VT-d support in the kernel: substantial but bounded work, done before by NOVA, Genode and Qubes.

PCs are arguably the friendliest hardware for a new OS because their buses are standardized (PCIe, xHCI, NVMe, HDA, UEFI). Arm laptops are not easier: the current ones boot with ACPI, and their SoC peripherals are bespoke, with Linux support itself still arriving. Desktops and virtual machines come first; laptops come last, because of power management.

## Proposed order

1. **Keep the architecture interface explicit now.** Today it is `console_write`, `shutdown`, traps, timer and interrupt enable. When paging and context switching arrive, define them as a narrow arch interface (page-table operations, context switch, IRQ controller, IPI) rather than letting RISC-V details leak into arch-neutral code.
2. **Second port: x86-64**, after IPC works (around M5), so the abstraction is tested before too much code depends on it. Target QEMU `q35` with OVMF (UEFI) and Limine. Arm `virt` would be cheaper (device tree, PSCI, similar to RISC-V), but x86 is the one that matters for use.
3. **Real hardware:** an x86 desktop or mini PC with an Intel GPU, NVMe and wired Ethernet; laptops later.

Prior art that runs on real x86 hardware: Genode/Sculpt (on NOVA or seL4), Redox (a Rust microkernel), seL4 itself, and Fuchsia on Intel NUCs ([[genode]], [[sel4]], [[fuchsia]]).

> [!question] Open
> When to start the x86 port, Limine versus a custom UEFI loader, and whether an Arm port is worth doing before or after x86.
