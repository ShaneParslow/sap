---
status: hub
summary: Catalog of every wiki page in reading order, with status and one-line summary.
updated: 2026-10-13
---

# Index

Every page, in reading order: root pages, then functional areas (concepts first, then layers bottom-up), then sessions and sources. Summaries are copied from each page's frontmatter; update both together.

## Start here

| Page | Status | Summary |
|---|---|---|
| [[overview]] | hub | What sap is, how its layers fit together, what is decided versus open, and where to start reading. |
| [[roadmap]] | hub | Ordered work items - the kernel decision, the bring-up milestones on RISC-V, and design exercises that can run in parallel. |
| [[open-questions]] | hub | Every unresolved question in the wiki, grouped by whether it blocks the roadmap, each linked to the page that holds the detail. |
| [[decisions]] | log | Dated log of decisions and reversals, each with its reason and source. Topic pages state the current conclusion; this page records how it was reached. |
| [[glossary]] | reference | Terms used in the wiki with one-line definitions and the page that covers each - also a list of search terms for further reading. |
| [[log]] | log | Append-only chronological log of wiki operations - ingests, sessions, lint passes. Grep with `grep "^## \[" log.md`. |

## Vision

| Page | Status | Summary |
|---|---|---|
| [[vision]] | hub | Why sap exists - owner-controlled computing in a world of abundant AI-written software - and the design principles that follow. |
| [[vision-owner-control]] | decided | The goal behind sap - computing that answers to its owner - and why abundant AI-written software makes standardized interfaces, capabilities and user-held data the foundations that matter. |
| [[vision-principles]] | decided | The design principles every area applies - standard core, least authority, mechanism in the kernel, honest failure, hardware as accelerator, specs over languages - each with how firmly it is held. |
| [[vision-attestation-and-verification]] | exploring | What cryptography can and cannot prove about a running program, scoped versus whole-device attestation, and cheaper alternatives to consensus. Background; not yet part of sap's design. |

## Capabilities

| Page | Status | Summary |
|---|---|---|
| [[capabilities]] | hub | The capability model that every layer of sap uses - what a capability is, how ownership moves, and how authority is taken back. |
| [[cap-model]] | proposed | A capability is an unforgeable, transferable, attenuable reference to an object, typed by an interface and carrying rights; the same model appears as CHERI pointers, kernel handles, RPC references and stored sturdy refs. |
| [[cap-ownership]] | proposed | How capabilities move and are lent - own and borrow semantics, protocol rules expressed as handle types, derivation trees, and why enforcement is dynamic in the kernel or host and static only as a per-language bonus. |
| [[cap-revocation]] | proposed | How authority is taken back - derivation-tree revocation, caretakers for delegated access, cancelling borrows, leases instead of distributed GC - and what revoking memory costs on paging versus CHERI. |
| [[cap-process-authority]] | proposed | What authority a process holds and how it gets more - kernel capabilities, RPC capabilities and stored capabilities seen from inside one process; the runtime's implicit capabilities versus the program's declared imports; and the single rule that every object arrives in a message, starting with the bootstrap. |

## Hardware

| Page | Status | Summary |
|---|---|---|
| [[hardware]] | hub | What sap assumes about hardware - a boring RISC-V core, CHERI as an optional accelerator, memory topology exposed as capabilities, and devices behind queue-based class interfaces. |
| [[hw-isa]] | decided | sap targets RISC-V first (QEMU virt, with CHERI-QEMU available). The case for a plain RISC core over language-specific or VLIW designs, how RISC-V, Arm and x86 compare for a microkernel, and how to use optional extensions without per-target builds. |
| [[hw-cheri]] | decided | CHERI support is wanted from the start as an accelerator, never a correctness requirement. What CHERI is, its standardization status, what it fixes for sap (bounds, unforgeability, sealed handles, cheap compartment calls) and what it doesn't. |
| [[hw-memory-topology]] | exploring | Memory is becoming tiered and shared (HBM, unified CPU/GPU memory, CXL); sap's tentative answer is to expose topology as capabilities with placement attributes rather than hide it. |
| [[hw-device-discovery]] | proposed | How sap learns what hardware exists - the flattened device tree on RISC-V and embedded Arm, ACPI on x86 and Arm/RISC-V servers - what a minimal DTB parser does versus a complete one, and the proposed split between a minimal kernel parser and full discovery in a userspace device manager. |
| [[hw-devices-and-io]] | proposed | All high-performance I/O converges on shared-memory submission/completion queues; sap uses class-standard devices (virtio), drivers as userspace components holding device, IRQ and DMA capabilities behind the IOMMU, and accelerators reached through capabilities. |

## Kernel

| Page | Status | Summary |
|---|---|---|
| [[kernel]] | hub | The kernel provides only unforgeable authority, memory mapping, wakeups and scheduling handoff; everything else is a component. sap writes its own kernel, modeled on seL4. |
| [[kernel-choice]] | decided | sap writes its own kernel, following seL4's design where useful. Records the seL4 alternative the chat recommended, what it would have given, and the gaps the own kernel must close. |
| [[kernel-sel4-differences]] | exploring | A source-level look at seL4 against what sap needs - capability transfer (copy, one capability per message), device trees, drivers, multicore, scheduling and virtualization - how far it generalizes to a desktop, and whether to sketch sap's userspace on it. |
| [[kernel-objects-and-syscalls]] | proposed | The kernel's object types and a seL4-sized syscall set (Call, Send, Recv, ReplyRecv, Signal, Wait, Yield); everything else is a Call on a capability to a kernel object. Register-based arguments plus a per-thread IPC buffer page. |
| [[kernel-crossing-costs]] | reference | What mode switches and address-space switches cost on current hardware, rough IPC latency floors, and the three techniques that keep crossings off the hot path (batching, dedicated cores, same-address-space isolation). |
| [[kernel-scheduling]] | proposed | CPU time is a capability (scheduling contexts) so agents get hard budgets; synchronous calls donate the caller's time; waiting is adaptive (poll, then sleep); preemption awareness is a latency optimization, not a correctness need. |
| [[kernel-process-model]] | proposed | A process starts with an initial capability table and one typed bootstrap capability matching its declared imports; arguments are typed values and configuration lives in the object store. No environment, argv strings, ambient user id, fork or exec - a launcher calls spawn on a process factory with code identified by hash. |
| [[kernel-boot-riscv]] | implemented | How the kernel boots on QEMU virt riscv64 today - OpenSBI enters _start in S-mode, the entry code turns on a boot page table and jumps to the kernel's upper-half address, riscv_start turns the device tree into architecture-neutral boot information and sets up traps, paging and the timer, and kmain runs boot checks and starts the root task. Also the console, log and shutdown. |
| [[kernel-memory-riscv]] | implemented | How the kernel manages memory on RISC-V today - Sv39 with the kernel in the upper half, a boot page table built in assembly, the kernel's own address space with per-section permissions and a direct map of RAM, free memory computed from the device tree, a boot-only bump allocator, and user address spaces that share the kernel half. |
| [[kernel-traps-riscv]] | implemented | How the kernel takes traps on RISC-V today - one direct-mode vector that saves user registers straight into the current thread's TCB (found through sscratch) and kernel-mode traps on the stack, separate Rust handlers for each, a return path that resumes any thread, and periodic timer interrupts through the SBI TIME extension. |
| [[kernel-threads]] | implemented | Threads and scheduling as implemented today - the thread control block (saved user registers, state, priority, address space), seL4-style fixed-priority round-robin scheduling with time slices counted in timer ticks, the idle loop, and how kernel globals are kept without locks on one hart. |
| [[kernel-root-task]] | implemented | The one program the kernel starts by itself, as in seL4 - user/init, built as a static position-independent ELF and embedded in the kernel; how the kernel loads it (contiguous frames, relative relocations, W^X segments, a stack below the top of user space) and enters user mode; and the two temporary debug syscalls it uses until capability invocations exist. |

## Interfaces

| Page | Status | Summary |
|---|---|---|
| [[interfaces]] | hub | The interface layer is the universal part of sap - language-neutral specs written as Cap'n Proto schemas with ownership annotations, versioned by hash, with generated bindings and honest failure semantics. |
| [[iface-spec-contents]] | decided | An interface spec is the language-neutral contract between components - types (with capabilities as owned or borrowed), operations, protocol via ownership, failure modes, intent, hash identity and a conformance suite. Behavioral contracts and proofs are optional attachments consumed by languages that support them. |
| [[iface-capnp]] | decided | Cap'n Proto is sap's schema language, wire format and RPC - zero-copy layout, append-only evolution, safe reads of untrusted data, capability RPC with promise pipelining - extended with $own/$borrow annotations rather than forked. Replaced WIT. |
| [[iface-versioning]] | proposed | Interfaces are identified by the hash of their formal content; version numbers are names for hashes; compatibility between two hashes is checked mechanically (append-only fields plus conformance suites); incompatible versions coexist and are bridged by adapters. |
| [[iface-bindings-and-languages]] | decided | Application code calls a generated interface and never knows the transport (direct call, shared-memory channel or network). Bindings are generated per language and their quality decides adoption. In-process calling conventions and library linking are each language's problem; the OS only identifies code by hash. |
| [[iface-failure-semantics]] | proposed | Following the 1994 Sun note on distributed computing, every call can fail, interfaces declare failure modes and idempotency, nothing but values and capabilities crosses a boundary, and objects run on single-threaded event loops. This is what makes one interface over local and remote transports sound. |

## IPC

| Page | Status | Summary |
|---|---|---|
| [[ipc]] | hub | How messages move - Cap'n Proto messages in shared-memory rings between trust domains, direct calls inside one, the network between machines - with validation at trust boundaries and bounded resources. |
| [[ipc-transports]] | decided | The spec, wire layout and bindings are constant; the transport is picked per connection - direct call within a trust domain, shared-memory channel between domains on one machine, Cap'n Proto RPC over the network between machines. |
| [[ipc-channels]] | proposed | A channel is two single-producer/single-consumer rings in shared memory, one per direction. Covers slot layout, atomic publish, backpressure when full, message size limits, streaming with credits, and the copy-small/transfer-large rule. |
| [[ipc-cap-transfer]] | proposed | How the kernel's IPC primitives (endpoints for Send/Recv/Call, notifications for Signal/Wait) relate to shared-memory rings, and how capabilities travel with ring messages - they can't go through shared memory, so the kernel moves them in batches into a per-connection receive range that ring messages refer to. |
| [[ipc-validation]] | proposed | The kernel guarantees a buffer exists, not that its contents match the spec, so receivers validate structure at trust boundaries. Covers what needs checking, double fetch and the single-fetch rule, Rust soundness, and making validation cheap with copy, page transfer and memfd-style sealing. |
| [[ipc-shared-buffers]] | proposed | Raw shared memory outside messages (graphics, audio, GPU buffers) is a capability to a memory object with a declared layout schema and explicit synchronization; the OS guarantees memory safety and revocation, not that the bytes make sense. |
| [[ipc-resource-limits]] | proposed | Nothing in a message is taken on faith, and every resource a peer can consume is bounded - ring size, capability-table quotas, pipelined-call caps, borrow timeouts, traversal limits, and CPU/memory budgets. |
| [[ipc-paging-vs-cheri]] | proposed | The same IPC design implemented with address-space isolation (runs everywhere now) or CHERI compartments (byte-granular, call-cost crossings, harder revocation), plus software fault isolation as a middle option. Design for paging; real systems mix all three. |
| [[ipc-network-rpc]] | exploring | Capabilities between machines - Cap'n Proto RPC over an authenticated, encrypted connection between nodes identified by public keys, carried by a bridge component so local paths never pay for it; network sturdy refs; the missing three-party handoff and why sturdy refs work around it; OCapN as prior art; and a web gateway for existing sites. |

## Object store

| Page | Status | Summary |
|---|---|---|
| [[store]] | hub | The graph is sap's universal data model - immutable content-addressed values, mutable refs and live objects, transient, persistent or remote - and the store is its persistent part; both live in userspace hosts. |
| [[store-object-graph]] | decided | The graph is every object in the system - transient, persistent and remote alike - with the store (disk) and the network as backends hosting parts of it. Each object has one host; the kernel makes references between hosts unforgeable but never sees the graph itself. |
| [[store-data-model]] | proposed | The store holds immutable content-addressed values forming a DAG, plus a small set of mutable refs reached only through capabilities. Values reference their schema by hash; operations are get, put, invoke and subscribe; knowing a hash grants nothing, holding a capability grants everything reachable from it. |
| [[store-sturdy-refs]] | proposed | A capability has a live form (kernel/RPC table index, valid while the holder lives) and a persistent form (a sturdy ref, an unguessable token). The store keeps tokens in a private side table and converts between the two, so programs never handle token bytes. Worked example included. |
| [[store-granting-access]] | proposed | How programs and agents get access - the user's shell holds the root ref, everything else comes through a trusted powerbox where selecting an item and granting it are one act; sharing a snapshot differs from sharing live access only by whether the value holds a capability; agents get budgets as well as scopes. |
| [[store-distribution]] | exploring | One user's data spread over several machines - values replicate freely because they are immutable, each mutable ref has one home node that orders its updates, devices hold partial replicas, offline edits merge by per-schema rules, and identity is a user key that authorizes device keys. Covers where data physically lives and what using another machine means. |
| [[store-on-disk-format]] | exploring | The store starts in memory; for persistence the proposal is an append-only log of content-addressed blobs plus an index, which makes crash safety nearly free. Git's object and packfile formats are the prior art; compaction and GC come later. |
| [[store-use-cases]] | exploring | Walkthroughs that test the store design before it is built - preferences that follow the user and reload live, granting a website part of one's data, a system agent changing the system under review, the roles of phone, desktop and server, and interoperating with Linux - each ending in the requirements it puts on the store. |

## Programs

| Page | Status | Summary |
|---|---|---|
| [[programs]] | hub | How programs are loaded, represented and built - loading is proposed concretely; representation is deliberately decoupled from the rest of the OS and still exploratory. |
| [[prog-loading]] | proposed | How code becomes a running process - a userspace process manager (decided) reads a code object by hash and builds the process from kernel objects; native code is static ELF, other formats are handled by loader components; a small per-architecture process ABI; components that don't assume they own an address space, so per-process versus shared address spaces is a deployment choice; and a boot image that breaks the loader/store circularity. |
| [[prog-composition]] | exploring | How programs are wired together and started - splitting the problem into resolution (which instance serves each declared import, automatic and checked at launch), instances (shared or per client) and lifecycle (eager, lazy or manual start, stop and restart), with compositions stored as data, a factory import as the escape hatch for programs that want control, and answers to the edge cases (cycles, failed starts, hung servers, stop races). |
| [[prog-representation]] | exploring | The chat's model for programs - a canonical typed IR stored by hash, lowered on the device to native code cached by (IR hash, target hash), with Wasm only as a lowering for crossing trust boundaries - and why Wasm core and raw MLIR each fall short as the canonical form. |
| [[prog-toolchain-and-verification]] | exploring | AI should write at the level of intent and specs and let verified deterministic tools lower to machine code; AI improves the toolchain (heuristics, superoptimization) with results stored as checked objects. Proofs are optional and live on the language side; Bend is an example language, not a foundation. |

## System

| Page | Status | Summary |
|---|---|---|
| [[system]] | hub | Whole-system concerns that sit across the layers - running legacy software and drivers, porting to other architectures, and using remote machines without giving up authority. |
| [[system-legacy-compat]] | exploring | Legacy software and hardware support come from containment - POSIX as a sandboxed component, and a Linux VM driving hardware sap has no driver for - exposed through typed interfaces and capabilities rather than adopted as the foundation. |
| [[system-portability]] | exploring | What it would take to run sap on Arm and x86 - which parts are architecture-specific, why a microkernel quarantines most of x86's mess, where the real cost is (drivers and laptop power management), and a proposed order for ports. |
| [[system-package-registry]] | exploring | A community registry of interfaces and the programs that implement them, served over HTTPS and native RPC and fetched into the store by hash; declared imports make every package's authority visible before install, so an agent can choose implementations for the user's goals; declarative system configuration; and a name directory linking friendly names to keys. |
| [[system-remote-compute]] | exploring | The network is a first-class transport as long as it doesn't slow local paths. Plan 9's cpu command is the model - run on a powerful remote machine while importing the user's namespace - so remote AI compute gets exactly the capabilities the user hands it. |

## Prior art

| Page | Status | Summary |
|---|---|---|
| [[prior-art]] | hub | External systems sap borrows from or measures itself against, one page each - what it is, what sap takes, what sap avoids. |
| [[sel4]] | reference | The formally verified capability microkernel whose design sap's kernel follows, and the main reference for sap's own kernel (building on it was considered and declined) - plus its ecosystem (Microkit, rust-sel4, sDDF, LionsOS). |
| [[fuchsia]] | reference | Google's capability-based OS; the closest existing architecture to sap (handles, FIDL, per-component namespaces, drivers as components), now deprioritized. Use as a reference design. |
| [[genode]] | reference | A capability-based component framework that can run on seL4, with a usable desktop (Sculpt OS) where the user explicitly routes every capability. Closest in spirit to owner control. |
| [[plan9]] | reference | Bell Labs' OS where every resource is a server speaking 9P, each process has its own namespace, and cpu servers run your shell remotely with your namespace imported. sap is roughly Plan 9 with typed interfaces and capabilities. |
| [[sandstorm]] | reference | Kenton Varda's self-hostable server built on Cap'n Proto, where each document is an isolated grain sharing access through sturdy refs and a powerbox. The closest existing system to sap's store layer. |
| [[cloudabi]] | reference | A capability-based POSIX subset (the precursor of WASI) where programs started with only the descriptors they were given and received typed arguments (argdata). Precedent for sap's process startup. |
| [[urbit]] | reference | A clean-slate personal computing stack whose tiny VM (Nock) gets performance from jets, native code registered against function hashes. Shows the canonical-semantics-plus-native-cache pattern and why the cache must be verified; also a lesson in adoption barriers. |
| [[wasm-component-model]] | reference | WebAssembly's component model and its interface language WIT - typed interfaces, own/borrow resource handles, worlds - which sap considered as its spec language before choosing Cap'n Proto, keeping the ownership and world ideas. |
| [[linux-ipc-history]] | reference | Linux's attempts at better IPC - kdbus and bus1 (rejected), dbus-broker and varlink (adopted in userspace), memfd sealing, SCM_RIGHTS, and the unmerged scheduler-handoff proposals - and the lesson that the kernel should provide mechanism, not bus policy. |
| [[flow-ipc]] | reference | A C++ toolkit for zero-copy shared-memory IPC with Cap'n Proto on Linux. Shows the ecosystem's demand for exactly sap's IPC model, where Linux's latency floor sits for such libraries, and the typical gap - performance solved, trust boundary not. |
| [[bend]] | reference | Bend 2, a language whose compiler requires proofs that declared laws still hold after every change. An example of behavior specification on the language side; not a foundation for sap's interfaces. |

## Sessions

| Page | Status | Summary |
|---|---|---|
| [[2026-09-27-0ce860ab-future-of-computing]] | complete | The founding design chat (claude.ai, 2026-09-27 to 2026-10-08) - from the future of computing under abundant AI down to capabilities, Cap'n Proto IPC, the object store and how to start building. |
| [[2026-10-08-22c3947b-wiki-setup]] | in-progress | Set up the wiki and seeded it from the 2026-09-27 design chat; brought the kernel up to traps, timer and device tree; then (2026-10-11) a design pass on process authority, IPC capability transfer, program loading, network RPC, store distribution and use cases, and a source-level look at seL4. |

## Sources

- [[2026-09-27-future-of-computing]]: claude.ai design chat, 2026-09-27 to 2026-10-08 (raw transcript; original JSON alongside).
- [[llm-wiki]]: the LLM-wiki pattern this wiki follows.

## Wiki maintenance

| Page | Status | Summary |
|---|---|---|
| [[schema]] | meta | How the wiki is laid out, what each page type is, the frontmatter, the status vocabulary, and how to cite chats and code. |
| [[workflows]] | meta | Procedures for working sessions, running summaries, ingesting chats, recovering cut-off sessions, keeping code-backed pages current, answering questions, and linting. |
| [[style]] | meta | Writing rules for the wiki, aimed at the usual failures of LLM-written docs - depth that tracks the conversation instead of the subject, uneven siblings, and arbitrary ordering. |
