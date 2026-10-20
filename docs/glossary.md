---
status: reference
summary: Terms used in the wiki with one-line definitions and the page that covers each - also a list of search terms for further reading.
updated: 2026-10-11
---

# Glossary

One name per concept; pages use these terms. Grouped by area in reading order, alphabetical within each group. Terms marked *(search)* are mainly useful as search terms for outside reading.

## Capabilities

- **Attenuation:** deriving a weaker capability (fewer rights, narrower interface or range) from one you hold. [[cap-model]]
- **Borrow:** lending a capability for the duration of one call; revoked on return or cancellation. [[cap-ownership]]
- **Capability:** an unforgeable, transferable, attenuable reference to an object, typed by an interface and carrying rights. [[cap-model]]
- **Caretaker / membrane:** an indirection the grantor can disable, used to revoke delegated access; a membrane extends it over an object graph. [[cap-revocation]]
- **Confused deputy:** a component tricked into using its authority on a caller's behalf; prevented by passing capabilities instead of names. [[cap-model]]
- **Derivation tree:** each capability records its parent; revoking a parent revokes its descendants (seL4 CDT). [[cap-ownership]]
- **Exclusive capability:** the only capability to its object; `CNODE_IDENTIFY` reports it by looking at the capability's neighbours in the derivation list. A frame's holder that maps an exclusive frame holds its only mapping. [[ipc-page-transfer]]
- **Identify (`CNODE_IDENTIFY`):** asking the kernel, through one's own CNode, what a capability is (type, exclusive, mapped) without invoking it. [[kernel-capabilities#Invocation]]
- **Own:** moving a capability; the sender loses it. [[cap-ownership]]
- **Typestate:** encoding protocol states as handle types so illegal call orders can't be written. [[cap-ownership]]
- **Declared imports (world):** the Cap'n Proto struct of capabilities and values a program needs, delivered as its bootstrap message; distinct from the runtime capabilities every process gets. [[cap-process-authority]]

## Hardware

- **ASID / PCID:** address-space tags that let the TLB hold several address spaces, avoiding flushes on switch. [[kernel-crossing-costs]]
- **CHERI:** hardware capabilities: pointers with bounds, permissions and a validity tag. [[hw-cheri]]
- **CHERI-QEMU:** QEMU fork emulating CHERI-RISC-V and Morello. [[hw-cheri]]
- **Cornucopia / revocation sweep** *(search)*: CHERI temporal safety by quarantining freed memory until a sweep invalidates capabilities to it. [[hw-cheri]]
- **CXL:** interconnect for memory expansion and sharing across devices and hosts. [[hw-memory-topology]]
- **IOMMU:** restricts which memory a device can reach by DMA. [[hw-devices-and-io]]
- **Morello:** Arm's CHERI evaluation board. [[hw-cheri]]
- **Sealed capability:** a CHERI capability that can't be dereferenced or modified until unsealed with a matching key; used as an opaque handle. [[hw-cheri]]
- **SBI / OpenSBI:** RISC-V Supervisor Binary Interface and its reference firmware; gives a kernel a console and timer. [[hw-isa]]
- **Sv39:** RISC-V's three-level 39-bit virtual-address paging scheme. [[roadmap]]
- **virtio:** standard class interfaces for virtual (and some physical) devices. [[hw-devices-and-io]]

## Kernel

- **IPC buffer:** a per-thread page at a known address for message data beyond registers. [[kernel-objects-and-syscalls]]
- **MCS / scheduling context** *(search: seL4 MCS)*: CPU time as a kernel object and capability. [[kernel-scheduling]]
- **Notification:** asynchronous doorbell object; a word of signal bits. [[kernel-objects-and-syscalls]]
- **Passive server:** a server with no CPU budget of its own that runs on callers' donated time. [[kernel-scheduling]]
- **Process factory:** capability used to create a process from code and initial capabilities (`spawn`). [[kernel-process-model]]
- **Root task:** the first user program on seL4, given all initial capabilities. [[kernel-choice]]
- **rseq** *(search)*: Linux restartable sequences; userspace learns of preemption. [[kernel-scheduling]]
- **Timeslice donation:** a synchronous caller lends its scheduling context to the callee. [[kernel-scheduling]]
- **TLB shootdown:** interrupting other cores to flush stale mappings after an unmap. [[cap-revocation]]
- **Untyped memory / retype:** raw memory from which kernel objects are created, giving kernel memory accounting. [[kernel-objects-and-syscalls]]
- **vDSO:** kernel code and data Linux maps into every process. [[kernel-objects-and-syscalls]]
- **Device untyped:** untyped memory covering device registers, retyped only into frames for drivers. [[hw-devices-and-io]]
- **Big kernel lock:** one lock held for the whole of each syscall on multicore; seL4's approach. [[kernel-sel4-differences]]
- **Receive range:** slots a receiver dedicates to a connection, into which the kernel transfers capabilities that ring messages refer to. [[ipc-cap-transfer]]

## Interfaces

- **Bootstrap capability:** the single typed capability a new process starts with, matching its declared imports. [[kernel-process-model]]
- **Canonical ABI:** WIT's lowering of types to Wasm memory; copy-based. [[wasm-component-model]]
- **Conformance suite:** language-neutral tests every implementation of an interface version must pass. [[iface-spec-contents]]
- **Intent:** non-normative natural-language rationale attached to spec items. [[iface-spec-contents]]
- **Orphan (Cap'n Proto):** unreachable data left in a message arena after a field is replaced. [[iface-capnp]]
- **Promise pipelining:** calling methods on a call's result before it returns; one round trip for a chain. [[iface-capnp]]
- **Traversal limit:** Cap'n Proto's cap on total data a reader follows. [[ipc-resource-limits]]
- **World (WIT):** declaration of a component's imports and exports. [[wasm-component-model]]

## IPC

- **Double fetch / TOCTOU:** a hostile writer changes shared data between the receiver's check and use. [[ipc-validation]]
- **memfd sealing / F_SEAL_WRITE** *(search)*: making a Linux shared buffer immutable before sending it. [[ipc-validation]]
- **Page transfer:** sending a message too large for a ring as whole frames whose capabilities move to the receiver, which checks that nobody else can write them, reads the message in place and gives the frames back. [[ipc-page-transfer]]
- **Single-fetch rule:** read each structural field once into a local and use only the local. [[ipc-validation]]
- **SPSC ring:** single-producer/single-consumer ring buffer; one per channel direction. [[ipc-channels]]
- **Software fault isolation (SFI):** isolation enforced by compiler-inserted checks. [[ipc-paging-vs-cheri]]
- **Trust domain:** a set of components that trust each other and skip validation between them. [[ipc-transports]]
- **Network bridge:** the component that holds a machine's network identity and forwards calls on remote capabilities; local programs link no network code. [[ipc-network-rpc]]
- **OCapN:** Object Capability Network; specifications for CapTP, netlayers, sturdy-ref URIs and three-party handoff. [[ipc-network-rpc]]
- **Three-party handoff:** passing a capability to an object on a third machine so the recipient connects to it directly instead of through the introducer. [[ipc-network-rpc]]

## Store

- **Powerbox:** trusted dialog through which a user grants a program access; selecting and granting are one act. [[store-granting-access]]
- **Ref:** a mutable cell in the store pointing at a value, reached through a capability. [[store-data-model]]
- **Graph:** every object in the system (values, refs, live objects), transient, persistent or remote. [[store-object-graph]]
- **Store:** the userspace component hosting the persistent part of the graph, over a block device driver. [[store-object-graph]]
- **Petname:** a person's own local name for a key or object, stored in their store. [[ipc-network-rpc]]
- **Self-certifying name:** a name containing (a hash of) a public key, so reaching the right holder can be checked without a registry. [[ipc-network-rpc]]
- **Side table:** the store's private map from sturdy-ref tokens to targets and rights. [[store-sturdy-refs]]
- **Sturdy ref:** a persistent capability: an unguessable token that restores a live capability. [[store-sturdy-refs]]
- **Value:** an immutable, content-addressed object carrying its schema's hash. [[store-data-model]]
- **Home node:** the one node that orders updates to a given ref; others cache and subscribe. Unrelated to Unix home directories. [[store-distribution]]
- **Device key:** a node's identity key, signed by the user's key. [[store-distribution]]

## Programs and system

- **Jet:** Urbit's native implementation registered against a function's hash. [[urbit]]
- **MLIR:** compiler framework of IR dialects and lowerings. [[prog-representation]]
- **Translation validation:** checking each compiler output against its input rather than verifying the compiler. [[prog-toolchain-and-verification]]
- **Partial failure:** a remote or isolated call fails midway with unknown effect. [[iface-failure-semantics]]
- **Activator:** the part of the process manager that holds a server's endpoint and starts the server when a client first needs it. [[kernel-process-model]]
- **Introducer:** a source that suggests a name-to-key mapping (a directory, a contact, a QR code); the user accepts it into their petnames. [[system-package-registry]]
- **Package registry:** a community source of interfaces, implementations and claims about them, fetched into the store by hash. [[system-package-registry]]
- **Composition:** a value describing which components run, how each import is routed, and each instance's lifecycle policy; enacted by the process manager. [[prog-composition]]
- **Boot image:** the kernel, process manager and a pack of initial code objects, used before the store is running. [[prog-loading]]
- **Code object:** a store value holding a program's format, architecture, image and declared world. [[prog-loading]]
- **Process ABI:** per-architecture startup convention: entry registers, startup record, stack, thread pointer. [[prog-loading]]
- **Single-address-space OS:** a design where all processes share one virtual address space; sap proposes against it. [[prog-loading]]

## Search terms not covered elsewhere

CloudABI argdata; Fuchsia processargs and component namespaces; kdbus, bus1, dbus-broker, varlink; FUTEX_SWAP, UMCG; SCM_RIGHTS; Barrelfish multikernel; Xen grant tables; Alive2; MLGO; CompCert; KeyKOS / EROS orthogonal persistence; E language CapTP; Spritely Goblins.
