---
status: hub
summary: What sap assumes about hardware - a boring RISC-V core, CHERI as an optional accelerator, memory topology exposed as capabilities, and devices behind queue-based class interfaces.
updated: 2026-10-08
---

# Hardware

The chat concluded that the instruction set is the least consequential choice in the design: sap needs an ordinary RISC core and puts its novelty into protection and data movement ([[2026-09-27-future-of-computing#m7|§m7]]). Pages are ordered from the core outward.

| Page | Status | Summary |
|---|---|---|
| [[hw-isa]] | decided | RISC-V first. Why a plain RISC core rather than a language-specific or VLIW design, and how RISC-V, Arm and x86 compare for a microkernel. |
| [[hw-cheri]] | decided | CHERI support is wanted from the start, as an accelerator never required for correctness. What it is, its status, what it fixes and what it doesn't. |
| [[hw-memory-topology]] | exploring | Memory tiers (HBM, unified, CXL), and memory as a capability with placement attributes. |
| [[hw-device-discovery]] | proposed | How hardware is described (device tree, ACPI); a minimal DTB parser in the kernel, the complete one in a userspace device manager. |
| [[hw-devices-and-io]] | proposed | Submission/completion queues, class-standard devices (virtio), drivers as userspace components, accelerators reached through capabilities. |

The cost of crossing into the kernel is a hardware question too, but it shapes kernel design more than hardware choice, so it lives at [[kernel-crossing-costs]].
