---
status: exploring
summary: Legacy software and hardware support come from containment - POSIX as a sandboxed component, and a Linux VM driving hardware sap has no driver for - exposed through typed interfaces and capabilities rather than adopted as the foundation.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# Legacy compatibility

The known answer from prior systems is to run POSIX as a sandboxed component, as Genode and Fuchsia do, rather than make it the foundation ([[2026-09-27-future-of-computing#m3|§m3]]). sap follows that; the details are open.

## Approaches

- **Linux in a VM.** Run Linux on a virtual machine monitor to drive hardware sap has no native driver for (GPUs, Wi-Fi) and to run legacy applications, exposed to the rest of the system through typed interfaces. Qubes and Genode use the same pattern: native components for what matters, a contained Linux for everything else. On seL4 this is its VMM; on a custom kernel it needs hypervisor support (the RISC-V H extension) ([[2026-09-27-future-of-computing#m31|§m31]]).
- **A POSIX personality component.** A component that implements enough POSIX for ported programs, backed by capabilities: a program's "file system" is a view onto store refs it was granted.
- **A file-system bridge.** The Linux-prototype plan used FUSE so POSIX programs see files while the store sees capability grants ([[2026-09-27-future-of-computing#m9|§m9]]). The same idea applies inside a Linux VM: the guest mounts a view of granted store objects.

## Constraints from the rest of the design

- No ambient authority leaks in: a legacy component gets exactly the capabilities it was granted, like any other ([[cap-model]]).
- Linux file descriptors are copyable and unrevocable, so capabilities crossing into a Linux guest must be mediated by a broker on the sap side ([[cap-revocation]]).
- A legacy POSIX build tool running through the compatibility layer is one of the use-case walkthroughs proposed for testing the store design ([[roadmap]]).

> [!question] Open
> Which approach comes first, and when. The POSIX file tree as a projection of the store is sketched in [[store-use-cases#Interoperating with Linux]]; on RISC-V a Linux driver VM needs the hypervisor extension in sap's kernel, which seL4 lacks there ([[kernel-sel4-differences]]). Not needed until sap runs real applications.
