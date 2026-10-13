---
status: exploring
summary: Memory is becoming tiered and shared (HBM, unified CPU/GPU memory, CXL); sap's tentative answer is to expose topology as capabilities with placement attributes rather than hide it.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m6|§m6]]", "[[2026-09-27-future-of-computing#m7|§m7]]"]
---

# Memory topology

The human raised memory hierarchy as a neglected question that probably shapes the OS, programs and language more than the ISA does, and was skeptical that hiding complex topologies behind an abstraction is right ([[2026-09-27-future-of-computing#m6|§m6]]). The chat produced a direction, not a design.

## The landscape (as of late 2026)

The trend is more tiers and more sharing ([[2026-09-27-future-of-computing#m7|§m7]]):
- **HBM** on accelerators.
- **Unified CPU/GPU memory:** Apple silicon, AMD MI300A, Strix Halo.
- **CXL memory expansion:** shipping, works with current server CPUs, with latency roughly that of remote-socket NUMA memory. CXL 3.x pooling and sharing across hosts exists but is much less deployed.
- **Persistent memory:** effectively gone. Intel discontinued Optane in 2022 and byte-addressable persistent memory mostly went with it. CXL could revive it as a device type; today it is not a real tier.

## Tentative direction

Memory placement is a case where both sides hold part of the information (principle 7 in [[vision-principles]]): the program knows what a region *means* (hot working set, streaming buffer, cold archive, shared with the GPU); the hardware and OS know how it's actually being accessed. Linux is moving the same way with tiered-memory page migration for CXL. Hence ([[2026-09-27-future-of-computing#m7|§m7]]):

- **Don't hide topology, and don't force every program to manage it.**
- **Memory is a capability to a region with attributes:** tier, coherence domain, which devices can reach it.
- **Topology is a queryable object** like everything else.
- **Programs that care request placement**; programs that don't get automatic tiering.

This matters most for accelerators: with coherent shared memory (CXL, unified memory), handing a GPU a buffer means passing a capability, not copying data ([[hw-devices-and-io]]).

> [!question] Open
> Everything here: the attribute vocabulary, who migrates pages, how placement requests interact with memory quotas ([[kernel-scheduling]]), and whether any of it belongs in the kernel or only in a memory-manager component. Nothing in the first milestones depends on it; QEMU `virt` has flat memory.
