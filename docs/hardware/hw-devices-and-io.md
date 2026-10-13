---
status: proposed
summary: All high-performance I/O converges on shared-memory submission/completion queues; sap uses class-standard devices (virtio), drivers as userspace components holding device, IRQ and DMA capabilities behind the IOMMU, and accelerators reached through capabilities.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m29|§m29]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# Devices and I/O

## Queues are the universal I/O model

Nearly all high-performance I/O has converged on **submission and completion queues in shared memory**: NVMe, GPU command rings, RDMA, virtio, io_uring. The kernel sets up the rings and permissions and then stays out of the data path ([[2026-09-27-future-of-computing#m7|§m7]]). sap's own IPC uses the same shape ([[ipc-channels]]), so devices and components look alike.

## Class-standard devices

The human's wish for "more standard peripherals" is really about device *interfaces*. Devices built to a standard class interface need one driver per class rather than per device; virtio, NVMe and xHCI already work this way. If hardware keeps converging on class interfaces, per-device drivers mostly disappear ([[2026-09-27-future-of-computing#m7|§m7]]). On QEMU `virt`, every peripheral sap needs early (block, network, console) is virtio ([[2026-09-27-future-of-computing#m29|§m29]]).

## Drivers as components

A driver is an ordinary userspace component holding:
- a capability to the device's MMIO range,
- an interrupt capability,
- a DMA capability, with the IOMMU enforcing that the device can only reach memory the driver was granted ([[2026-09-27-future-of-computing#m7|§m7]]).

It exports a typed interface like any other component, which is why file systems and I/O "fall out" of the RPC design ([[2026-09-27-future-of-computing#m18|§m18]]). A crashed driver is a partial failure that callers handle like any other ([[iface-failure-semantics]]).

High-throughput drivers (NVMe, network cards) feed the hardware's own queues directly; the RPC interface sits on top for setup and control, not in the per-packet path ([[2026-09-27-future-of-computing#m19|§m19]]). The seL4 Device Driver Framework (sDDF) already structures drivers this way: components talking over single-producer/single-consumer rings with notifications as doorbells ([[2026-09-27-future-of-computing#m31|§m31]]; see [[sel4]]).

## The kernel interface for drivers

The human asked how userspace drivers interface with the kernel ([[2026-10-08-22c3947b#m18|§m18]]). Proposed, following seL4 ([[kernel-sel4-differences#Differences that shape sap's interfaces]]):
- **Registers:** at boot the kernel hands the device manager *device untyped* capabilities covering MMIO ranges (RAM-backed untyped is kept separate, since device memory must never back kernel objects). The device manager carves out frames for each device and passes them to its driver, which maps them uncached.
- **Interrupts:** the device manager holds the interrupt-control capability and creates one IRQ handler capability per interrupt line, bound to a notification. The driver waits on the notification and acknowledges through the handler, which unmasks the line at the interrupt controller (the PLIC on RISC-V `virt`).
- **DMA:** the driver allocates frames from its own memory quota and asks for their physical (or IOMMU) addresses to program the device. With an IOMMU, an I/O address space capability limits the device to those frames. **Without one, a driver of a DMA-capable device can reach all of memory and must be trusted.** RISC-V's IOMMU specification was ratified in 2023 and QEMU has a model of it, but sap's early drivers on `virt` will run without one.

Which capabilities a driver gets is decided by the device manager from the device tree ([[hw-device-discovery]]). The driver declares them as typed imports ([[cap-process-authority]]).

## Accelerators

GPUs and other accelerators are reached through capabilities to their command queues and to the memory they may use. With coherent memory, handing over a buffer is passing a capability ([[hw-memory-topology]]). VLIW-style static scheduling is at home inside accelerators, where latencies are predictable ([[hw-isa]]). Program code for accelerators is a lowering target of the program representation ([[prog-representation]]).

> [!question] Open
> GPU, Wi-Fi and desktop hardware have no native drivers anywhere in the seL4-style ecosystem. The chat's answer is a contained Linux VM driving that hardware behind a typed interface ([[system-legacy-compat]]).
