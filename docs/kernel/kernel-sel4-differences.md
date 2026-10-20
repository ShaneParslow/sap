---
status: exploring
summary: A source-level look at seL4 against what sap needs - capability transfer (copy, one capability per message), device trees, drivers, multicore, scheduling and virtualization - how far it generalizes to a desktop, and whether to sketch sap's userspace on it.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]", "[[2026-10-08-22c3947b#m20|§m20]]"]
---

# seL4 compared with sap's kernel

sap writes its own kernel modeled on seL4 ([[kernel-choice]]). This page checks the model against seL4's source, to record where sap's kernel should copy seL4 exactly, where it should differ, and what seL4 would and would not give a desktop system. Facts are from seL4 at `6df0b6e` (2026-10-04), Microkit at `39e7561` and rust-sel4 at `86a0dbf`, unless a link says otherwise. The human asked for this closer look, and whether to sketch sap's userspace on seL4 first ([[2026-10-08-22c3947b#m18|§m18]]).

## Differences that shape sap's interfaces

**Capability transfer copies, and delivers one capability per message.** An IPC message may name up to three extra capabilities (`seL4_MsgMaxExtraCaps`, from `seL4_MsgExtraCapBits = 2`), but the receiver names a single receive slot in its IPC buffer, and `transferCaps` in `src/kernel/thread.c` clears that slot after inserting the first capability. The other two can only arrive "unwrapped": if a capability names the endpoint the message is travelling on, the receiver gets just its badge. In practice, **one real capability per message**. The transferred capability is a derived copy (`deriveCap` + `cteInsert`); the sender keeps its own. The wiki previously said "a few" capabilities; one is the accurate number.

Consequences on seL4: a Cap'n Proto message carrying several capabilities needs several IPCs, or one transfer of a CNode holding them all. A move is a copy followed by the sender deleting its own slot, which a hostile sender can skip; where exclusivity matters, the object's host revokes and re-mints ([[cap-ownership]]).

For sap's kernel: transfer a batch of capabilities per operation into a receiver-chosen range of slots, with a per-capability move-or-copy flag. Details in [[ipc-cap-transfer]].

**Kernel object methods are defined in an interface file.** seL4 describes every invocation on kernel objects (retype, map, configure a thread) in XML under `libsel4/include/interfaces/`, and generates the user-side stubs from it. sap can do the same with a Cap'n Proto schema, which lets a program declare kernel objects in its imports like any other interface ([[cap-process-authority]]).

**Identifying a capability.** seL4 offers no way to ask what a capability is outside debug builds (`seL4_DebugCapIdentify`), so a server must trust the protocol about the type of what it was sent, and invoking a capability of the wrong type does that type's operation (on an endpoint, an IPC call that may block). Nor can a program ask whether it holds the only capability to an object. sap adds both as one CNode invocation, `CNODE_IDENTIFY` ([[kernel-capabilities#Invocation]]); page transfer depends on the second ([[ipc-page-transfer]]).

**The device tree is not an obstacle.** seL4's build turns the platform's device tree into kernel configuration at compile time, so there is one kernel binary per platform, and at run time it passes the DTB to the root task as an extra boot-info chunk (`SEL4_BOOTINFO_HEADER_FDT`). On x86 it passes the ACPI root pointer, framebuffer and TSC frequency the same way. sap differs only in parsing at run time ([[hw-device-discovery]]).

**Drivers.** Device memory arrives at the root task as *device untyped* capabilities, retyped into frames and mapped by the driver. Interrupts are reached through an IRQ-control capability that creates per-interrupt handler capabilities bound to notifications. DMA protection exists on x86 (VT-d, through I/O-space capabilities) and on some Arm SMMUs. None of this is verified: the verified configurations exclude IOMMUs and SMMUs ([verified configurations](https://docs.sel4.systems/projects/sel4/verified-configurations.html), checked 2026-10-11). RISC-V has no IOMMU support in seL4. sap's driver interface follows the same shape ([[hw-devices-and-io]]).

## Desktop generality

The kernel is small enough that nothing in it is desktop-hostile; the gaps are in what it leaves out, and most of those are userspace work sap must do on any kernel.

| Desktop need | seL4 | Gap |
|---|---|---|
| Dynamic processes | Root task builds them from untyped memory. Microkit is static: at most 63 protection domains, 62 channels each. | None in the kernel; a process manager is userspace work either way (M7). Microkit is unusable for a desktop. |
| Many cores | Multicore uses one big kernel lock (`src/smp/lock.c`), held for the whole of each short syscall. Threads are pinned; there is no kernel load balancer. Multicore is not verified. | Probably fine to a few tens of cores for a microkernel; balancing threads across cores must be a userspace policy. |
| Interactive scheduling | Fixed priorities, round robin within a priority. MCS adds budgets and periods but is unverified (its own build option says so). | Fairness and interactivity become a userspace policy adjusting scheduling contexts. No one has published a desktop policy. |
| Power management | No idle-state selection, frequency scaling or suspend in the kernel. | A real kernel gap for laptops. |
| Legacy drivers (GPU, Wi-Fi) | Virtualization on x86 (VT-x, outside the verified build) and Arm (hypervisor mode); none on RISC-V. | On RISC-V, no way to run a Linux driver VM ([[system-legacy-compat]]). |
| CHERI | No support in mainline. | sap wants it ([[hw-cheri]]). |
| Proofs | Functional correctness is proved for AArch32, AArch64, RISC-V 64 (without the fast path) and x86-64 (without the fast path); MCS proofs are in progress; multicore, IOMMUs and boot are not covered. | A desktop configuration (multicore, MCS, IOMMU, VMs) is mostly outside the proofs anyway. |

The existence proof is Genode's Sculpt OS, a desktop OS for PCs. Since release 25.10 it ships an experimental image on seL4 13.0 next to its main kernels (NOVA and Genode's own `hw`). Genode's developers describe seL4 as hard to tailor to their needs and the seL4 variant as unofficial ([genodians.org, 2025-11-02](https://genodians.org/alex-ab/2025-11-02-sculpt-multi-kernel)). So a desktop on seL4 works, but its main users run their own kernels.

## Sketching sap's userspace on seL4

The case for it: sap's userspace sketch needs paging, user mode, capability tables, endpoints and notifications (M2–M4), and seL4 has all of them now. The process manager, IPC library and store are userspace and would carry over.

The case against:
- **The limits leak into the design.** One capability per message and copy-only transfer push the IPC library toward workarounds (CNode bundles, host-mediated moves) that would be removed later. Designing around them would hide exactly the questions sap's kernel is meant to answer.
- **The saving is smaller than it looks.** What remains of M2–M4 is the well-trodden part of a kernel, and sap copies seL4's design for it. The userspace sketch still needs a process manager, slot allocator and loader on either kernel, and rust-sel4 provides bindings and a runtime, not those.
- **Setup cost:** seL4's CMake and Python build, platform configuration, and rust-sel4's target specifications.
- **No RISC-V virtualization or CHERI**, both of which sap wants on its first platform.

**Decided (2026-10-11): no sketch on seL4** ([[2026-10-08-22c3947b#m20|§m20]]). sap stays on its own kernel and uses seL4 as a code reference at each milestone (CSpace layout, untyped retyping, endpoint and notification state machines, the fast path), keeping a per-milestone record of where it deliberately differs. A thin `sap-sys` crate over seL4 was the considered fallback.

> [!question] Open
> Whether sap's kernel uses a big lock for multicore as seL4 does (simple, adequate for short syscalls) or finer locking.
