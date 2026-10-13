---
status: proposed
summary: How code becomes a running process - a userspace process manager (decided) reads a code object by hash and builds the process from kernel objects; native code is static ELF, other formats are handled by loader components; a small per-architecture process ABI; components that don't assume they own an address space, so per-process versus shared address spaces is a deployment choice; and a boot image that breaks the loader/store circularity.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]", "[[2026-10-08-22c3947b#m20|§m20]]"]
---

# Program loading

Loading turns a code object in the store into a running process: an address space with the program's segments mapped, a stack, an IPC buffer, initial registers, an initial capability table, and the bootstrap message ([[kernel-process-model]], [[cap-process-authority]]). The human expected this to be where sap has to touch architecture and binary formats again, and asked whether to have a driver per executable format and whether a flat address space is the way to go ([[2026-10-08-22c3947b#m18|§m18]]). Proposed; nothing is built.

## Who loads

The **process manager**, a userspace component, as in seL4, where the kernel knows nothing about executables and the root task or a CapDL initializer builds every process ([[kernel-sel4-differences]]). It holds untyped memory, creates an address space, capability table and thread, maps frames, writes the bootstrap message and starts the thread.

**Decided (2026-10-11):** the *process factory* is an RPC interface hosted by the process manager, not a kernel object. The kernel only needs threads, address spaces and capability tables, as seL4 shows. The human's reason: it makes multiple executable formats and interpreters easy, since each is just another loader behind the same interface ([[2026-10-08-22c3947b#m20|§m20]]).

The kernel itself loads exactly one program, the root task, as seL4 does; it is built today as a static position-independent ELF embedded in the kernel ([[kernel-root-task]]). The process manager will be that root task.

## Code objects and formats

A code object is a store value:

```capnp
struct Code {
  format @0 :Format;        # elfStatic, wasmComponent, linuxElf, ...
  arch   @1 :Arch;          # riscv64, x86_64, aarch64, none (for Wasm)
  image  @2 :Data;          # or a hash reference to a blob
  world  @3 :SchemaHash;    # the imports struct the program declares
  exports @4 :List(SchemaHash);  # interfaces it serves
}
```

The process manager loads one format itself, and every other format is a **loader component**, so the human's "driver per executable format" is right, with the trusted path kept to one format:

- **Native: static ELF, position-independent.** The process manager reads the `PT_LOAD` segments and maps them; no dynamic linker, no `PT_INTERP`. A language that wants dynamic linking ships its own loader inside the program ([[iface-bindings-and-languages]]). The ELF parser is small and its input is untrusted, so it must be bounds-checked like any message.
- **Wasm:** a Wasm runtime is an ordinary native program whose world includes the module to run. Loading a Wasm component means spawning the runtime with the module as an import ([[prog-representation]]).
- **Linux binaries:** handled by the compatibility layer, a POSIX personality component or a Linux VM ([[system-legacy-compat]]).

Because code is immutable and identified by hash, read-only segments (text, read-only data) can be mapped straight from the store's frames into every process running that code, deduplicated by hash. Writable segments are copied. The `exports` list lets the activator start a program when its interface is first needed ([[kernel-process-model#Starting on demand]]) and lets a registry index implementations by interface ([[system-package-registry]]).

**All code is position-independent**, kernel-loaded or not (decided 2026-10-11, [[2026-10-08-22c3947b#m22|§m22]]). Mappings are never both writable and executable.

## The process ABI

This is the only part of loading that is per-architecture, and it is small: a short document plus a few dozen lines of startup code per architecture, like seL4's `sel4runtime`:
- entry point and initial registers (on RISC-V: `a0` = address of a startup record giving the IPC buffer, the stack and the initial slot layout);
- stack alignment and initial stack contents (none beyond alignment: no argv, no environment);
- the thread-pointer register for thread-local storage (`tp` on RISC-V, `fs` on x86-64) and who sets it up (the runtime, from `PT_TLS`);
- the syscall instruction and register convention, shared with the kernel ([[kernel-objects-and-syscalls]]).

Rust programs start on the bare `riscv64gc-unknown-none-elf` target with a `libsap` runtime crate. A proper target triple and a port of Rust's standard library come later.

## Address spaces

"Flat" can mean two things. **No segmentation** within an address space: yes, on every architecture sap targets. **One address space shared by many or all processes** (a single-address-space OS, such as Opal or Mungi) is open. The human is unsure about per-process address spaces: Sv39's 512 GiB is tight, but avoiding page-table switches could pay for CHERI's costs ([[2026-10-08-22c3947b#m20|§m20]]).

**For a shared address space:**
- On CHERI, protection comes from capabilities, not page tables. Components in one address space call each other at near function-call cost, with no page-table switch and no TLB flush. That is CHERI's main performance win, and it needs a shared address space to happen ([[ipc-paging-vs-cheri]]).
- Pointers mean the same thing in every component, although Cap'n Proto's relative offsets already make messages and stored values position-independent ([[iface-capnp]]).

**For per-process address spaces:**
- Without CHERI, a shared address space still needs a page-table switch per protection domain, so it saves nothing. Address-space identifiers (ASIDs on RISC-V and Arm, PCIDs on x86) already avoid TLB flushes on a switch.
- Virtual addresses don't become a global resource to allocate.
- Sv39's limit only binds when everything shares one space. Sv48 (256 TiB) and Sv57 exist, QEMU `virt` supports both, and newer RISC-V cores implement Sv48.

**Paging doesn't go away either way.** Even CHERI systems use paging for what it does besides protection: mapping scattered physical memory contiguously, sharing frames, guard pages, mapping devices. A shared address space removes the *switching* cost, not the page tables. CheriBSD's co-processes, several processes in one address space separated by CHERI, are the closest prior art.

**Proposed:** don't choose yet; make it a deployment decision by construction. Components never assume they own their address space. Code is position-independent, and the IPC buffer and stack addresses come from the startup record (whose address arrives in a register), never from fixed addresses. Shared buffers are mapped wherever the process manager puts them. The process manager then places components: one per address space on paging-only hardware; many CHERI compartments per address space, up to all of them, on CHERI hardware. Sv39-only hardware simply runs the per-process layout.

## Boot: before the store exists

Code is loaded by hash from the store, but the store is itself a process loaded from code. The circle is broken by a **boot image**: the kernel, the process manager, and a pack of content-addressed code objects (store, device manager, first drivers, shell) in the same format the store uses on disk ([[store-on-disk-format]]). The process manager starts from the boot image, starts the store, which imports the pack, and from then on loads everything from the store. This plays the role of seL4's root task image and Linux's initramfs.

> [!question] Open
> Where the process manager draws address-space boundaries on CHERI hardware, and whether the kernel needs to know about compartments within an address space. Also: whether code objects carry a signature or only a hash (who vouches for a hash: [[vision-attestation-and-verification]]); how the process manager learns that a child exited or faulted ([[kernel-process-model]]); and when to attempt a Rust standard-library port.
