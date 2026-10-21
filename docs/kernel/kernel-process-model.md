---
status: proposed
summary: A process starts with an initial capability table and one typed bootstrap capability matching its declared imports; arguments are typed values and configuration lives in the object store. No environment, argv strings, ambient user id, fork or exec - a launcher calls spawn on a process factory with code identified by hash.
updated: 2026-10-20
sources: ["[[2026-10-20-685b0203#m12|§m12]]", "[[2026-09-27-future-of-computing#m21|§m21]]", "[[2026-09-27-future-of-computing#m24|§m24]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Process model

The human's starting point: a process may not need an environment or arguments at all, since all of that can go into the object store ([[2026-09-27-future-of-computing#m24|§m24]]). The chat built that into a concrete model. The process manager lives in userspace, as seL4's root task does. The process factory is its RPC interface, not a kernel object (decided 2026-10-11, [[prog-loading]]). What a running process holds, and how it gets further objects from its bootstrap, is in [[cap-process-authority]].

## What a process starts with

([[2026-09-27-future-of-computing#m25|§m25]])
- **An initial capability table:** its own address space and thread, a scheduling context (CPU budget), a memory quota, and **one bootstrap capability**.
- **Nothing ambient:** no environment variables, no argv strings, no global file system, no user id.

The **bootstrap capability** is a typed interface matching the program's declared imports. This is the WebAssembly component model's "world" idea, the one part of WIT the design keeps ([[iface-capnp]]). Arguments arrive as a typed value in the bootstrap message, not as strings. Configuration and data are object-store references.

> [!todo] Not built
> Until Cap'n Proto RPC brings capability tables (M6), a capability in the bootstrap message is the index of a transferred capability, in an integer field (decided 2026-10-20, [[2026-10-20-685b0203#m12|§m12]]).

## Spawning

No fork, no exec. A launcher holding a **process-factory** capability calls `spawn(code, initial_caps)`. The code is identified by its content hash in the store, so what runs can be verified ([[2026-09-27-future-of-computing#m25|§m25]]).

Example: a `resize` program declares its imports ([[2026-09-27-future-of-computing#m29|§m29]]):

```
world resize {
  input:  borrow<image-ref, read>
  output: own<image-ref, write>
  params: { width: u32, height: u32, filter: lanczos | bilinear }
}
```

A shell or agent launches it:

```
spawn(code: h:resize-bin…, bootstrap: {
  input:  <capability to photos/1043>,
  output: <capability to a new ref>,
  params: { width: 800, height: 600, filter: lanczos }
})
```

To record or rerun the invocation, store it as a value: `{ code: h:…, params: {…}, input: <sturdy ref>, output: <sturdy ref> }`. That single form is shell history, a script, and a Nix-style build record ([[store-sturdy-refs]]).

(The examples use WIT-like notation because that is how the chat wrote them; the real schema language is Cap'n Proto, [[iface-capnp]].)

## Starting on demand

The human proposed starting a process when another process first needs its capability ([[2026-10-08-22c3947b#m22|§m22]]), as inetd, systemd socket activation, launchd and Sandstorm's grains do. The ingredient that makes it easy: **the endpoint outlives the server.** An activator (part of the process manager) creates the endpoint and hands out capabilities to it before the server exists; clients that call it block in the kernel's endpoint queue, as they would with a busy server.

Two cases, proposed:
- **Restoring a sturdy ref** whose host isn't running. The store asks the process manager to start the host named in the ref, then forwards the restore. Entirely userspace, and it covers anything persistent ([[store-sturdy-refs]]).
- **A live capability to a server that isn't running.** The activator must learn that a sender is waiting. Proposed: a small kernel feature, a notification bound to an endpoint that is signalled when a sender queues on it with no receiver. The activator then starts the server and passes it the endpoint's receive capability. The alternative needs no kernel change but costs more: the activator receives the first message and forwards it, capabilities included.

The reverse also follows: a server idle for long enough can exit. Its endpoint stays with the activator, so clients' capabilities stay valid and the next call starts it again. That is transparent for stateless servers; stateful ones keep their state in the store and come back through sturdy refs. To know which code serves an interface, the activator reads the code object's declared exports ([[prog-loading#Code objects and formats]]). This section is the mechanism. When to start, which instance to bind, and what to do with failed or hung servers is policy, covered in [[prog-composition]].

## Root authority

At login, the user's shell receives a capability to the user's root ref in the object store. Every other capability the user's programs hold derives from that root or is granted through the powerbox ([[2026-09-27-future-of-computing#m21|§m21]]; [[store-granting-access]]).

## Loading code

How a language links its own libraries (statically, or at load time by its loader) and keeps its binaries compatible with each other and the platform is that language's problem, not the OS's. The OS adds only content-hash identity for code, so a loader can verify what it loads. The human's summary: the OS has avoided the ABI issue by pushing it onto languages ([[2026-09-27-future-of-computing#m28|§m28]], [[2026-09-27-future-of-computing#m29|§m29]]). See [[iface-bindings-and-languages]]. How the process manager loads native code, other formats and the boot image is in [[prog-loading]].

## Prior art

CloudABI (the capability-based POSIX subset WASI grew from): programs started with only the descriptors they were given, and arguments came as a typed tree ("argdata"). Fuchsia's process arguments plus component namespaces are the production version ([[2026-09-27-future-of-computing#m25|§m25]]). See [[cloudabi]], [[fuchsia]].

> [!question] Open
> Process exit and teardown, how a parent learns a child died (a notification on the factory?), and how a crashed component is restarted; restart likely reuses the on-demand path above, since the endpoint survives the crash. Whether the kernel adds the waiting-sender notification.
