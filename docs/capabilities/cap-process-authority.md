---
status: proposed
summary: What authority a process holds and how it gets more - kernel capabilities, RPC capabilities and stored capabilities seen from inside one process; the runtime's implicit capabilities versus the program's declared imports; and the single rule that every object arrives in a message, starting with the bootstrap.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]"]
---

# What a process holds

[[cap-model]] describes capabilities layer by layer. This page takes the view from inside one process, which is where the layers get tangled: memory regions, interface specs and stored data all look like "things the process has", but they are different kinds of thing. The human raised the tangle and proposed that a program declares imports such as kernel processes, threads and endpoints and then calls them ([[2026-10-08-22c3947b#m18|§m18]]). The model below is proposed and builds on [[kernel-process-model]].

## Three kinds of capability, one table

A process has exactly one source of authority: its **kernel capability table**. Everything else is reached by invoking entries in it.

| Kind | What it names | How it is invoked | Example |
|---|---|---|---|
| Kernel capability | A kernel object: frame, address space, thread, untyped memory, endpoint, notification, IRQ handler, scheduling context | A syscall on the table slot | Map a frame; signal a notification |
| RPC capability | An object hosted by another process | A Cap'n Proto call over a connection, which is itself built from kernel capabilities | `log.write(…)`, `dir.open(…)` |
| Stored capability | The same kind of object, persistently (a capability field in a persistent value, [[store-object-graph]]) | Never directly: reading a value through a store capability turns it into an RPC capability ([[store-sturdy-refs]]) | A "recent files" entry |

An RPC capability is an entry in a per-connection import table kept by the process's runtime, and the connection is a handful of kernel capabilities (an endpoint, or a shared frame and notifications, [[ipc-cap-transfer]]). The kernel never sees the RPC layer, and the RPC layer never sees token bytes of stored capabilities.

Three things often mistaken for capabilities are not:
- **An interface spec** is the *type* of a capability, a schema identified by hash and stored as an ordinary value ([[iface-versioning]]). Knowing a schema grants nothing.
- **A memory region** is either a mapping in the process's own address space (runtime-managed, invisible to the program), a shared buffer (a frame capability plus a layout schema, [[ipc-shared-buffers]]), or a read-only view of a stored blob that the store mapped for it. Only the frame is a capability.
- **Data in the store** is reached through a capability to a ref or value; the data itself is not held.

## Runtime capabilities and declared imports

Two sets of capabilities arrive at startup, in fixed slots of the initial table:

**Runtime capabilities**, which every process gets and the program never names: its own capability table, address space and main thread; a quota of untyped memory (its memory budget, from which the runtime allocates frames and new threads); its IPC buffer; and the connection to its launcher, over which the bootstrap message arrives and exit is reported. These belong to the language runtime (`libsap` for Rust). The program doesn't import them, any more than a Linux program imports its stack.

**Declared imports**, the program's *world*: a Cap'n Proto struct whose fields are the capabilities and values it needs, delivered as the bootstrap message ([[kernel-process-model]]). Most programs import only RPC interfaces:

```capnp
struct ResizeImports {           # generated from the program's world
  input  @0 :ImageRef $borrow;
  output @1 :ImageRef $own;
  params @2 :ResizeParams;
}
```

Programs that need kernel objects import them the same way, by type. A driver's world names `mmio :Sap.Kernel.Frame`, `irq :Sap.Kernel.IrqHandler`; a process manager's names `untyped :Sap.Kernel.Untyped`. The kernel's object methods are described in a Cap'n Proto schema (seL4 keeps the equivalent in XML, [[kernel-sel4-differences]]), and the generated binding issues syscalls instead of messages. So the human's proposal holds with one refinement: *kernel interfaces are importable like any other, but ordinary programs don't import them*. A program that imports a thread or an untyped quota is asking for more authority, which a launcher or the [[store-granting-access|powerbox]] can show and refuse.

Who satisfies the imports depends on who launches: the device manager for drivers ([[hw-devices-and-io]]), the user's shell or an agent for applications, the process manager at boot for system components.

## How a process gets objects

**Every object a process ever holds arrived in a message.** There is no lookup by name, no global namespace and no ambient root. Three paths, all instances of that rule:

1. **The bootstrap message** at startup, carrying the declared imports.
2. **Return values and arguments** of calls on capabilities it already holds. `dir.open("notes")` returns a new capability. Its authority comes from `dir`, not from the string.
3. **Traversal of the store.** If the world includes a capability to a ref, say the program's own config or state directory, reading values through it yields further capabilities wherever those values have capability fields. The program walks outward from the bootstrap, and what it can reach is fixed by what it was given.

So yes: the bootstrap is the root of everything the process can reach, and the process traverses outward from it. A typical application's world is small: a few service capabilities (log, display, input), one capability to its own state ref, and whatever the user handed it through the powerbox at launch or later.

## Lifetime

When a process exits or dies, the kernel deletes its capability table, which drops every kernel capability and therefore every connection; hosts of RPC objects see the connection break and drop that client's imports ([[cap-revocation]]). Stored capabilities survive because they belong to the store, not to the process. That is the reason to keep anything meant to outlive a run (open documents, preferences) as a stored capability in the program's state ref.

> [!question] Open
> The exact runtime slot layout; whether a process holds its own thread and address space capabilities by default (convenient for runtimes that create threads, but more authority than a simple program needs); and how a launcher shows a program's requested authority to the user.
