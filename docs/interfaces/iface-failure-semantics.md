---
status: proposed
summary: Following the 1994 Sun note on distributed computing, every call can fail, interfaces declare failure modes and idempotency, nothing but values and capabilities crosses a boundary, and objects run on single-threaded event loops. This is what makes one interface over local and remote transports sound.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m18|§m18]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m20|§m20]]"]
---

# Failure semantics

sap uses one interface for direct calls, shared-memory channels and the network ([[iface-bindings-and-languages]]). That is only sound if interfaces are honest about failure. The human adopted network transparency as first-class "as long as it doesn't get in the way" ([[2026-09-27-future-of-computing#m20|§m20]]); the rules below are what make it safe.

## The Sun note

"A Note on Distributed Computing" (Waldo, Wyant, Wollrath, Kendall; Sun Labs, 1994) argued against making remote objects look exactly like local ones, the goal of CORBA and DCOM. It names four differences ([[2026-09-27-future-of-computing#m19|§m19]]):

1. **Latency.** Remote calls are orders of magnitude slower. The only difference engineering can mostly hide.
2. **Memory access.** A pointer into another address space means nothing to the receiver.
3. **Partial failure.** The central one. Locally, a program works or crashes as a whole; remotely, a call can fail midway and nobody can tell whether it ran.
4. **Concurrency.** Remote objects have truly concurrent callers, which the interface must address.

Its conclusion: make these differences visible in the interface, design for distribution, and treat local calls as the special case. Java RMI applied this by requiring every remote method to declare `RemoteException`.

## How sap handles each

([[2026-09-27-future-of-computing#m19|§m19]])

| Difference | sap's answer |
|---|---|
| Latency | Asynchronous calls by default; promise pipelining batches dependent calls into one round trip ([[iface-capnp]]). |
| Memory access | Only values and capabilities cross a boundary; messages are position-independent, so no pointer is ever meaningful on the other side. |
| Partial failure | Every call returns a result that can be an error. A capability becomes *broken* when its peer disconnects or dies. Each operation declares whether it is **idempotent** (safe to retry) or **at-most-once**. |
| Concurrency | E's model: each object runs on a single-threaded event loop, and no mutable state is shared across a boundary. |

The idempotency declaration is an addition the spec format needs ([[iface-spec-contents]]).

## Local failure is real failure

A microkernel with isolated components has partial failure locally: a crashed driver is exactly the paper's failure case on one machine. So the uniform model isn't pretending remote calls are local, which is the mistake the paper warned about; it treats local calls with the honesty about failure that remote calls require, which is what the paper recommended. Code written to tolerate failure on the network works on all three transports ([[2026-09-27-future-of-computing#m17|§m17]], [[2026-09-27-future-of-computing#m19|§m19]]).

The human found this the most attractive part of the design: file systems and I/O drivers fall out simply as RPC servers, it fits the object store, and it maps onto networks and the web ([[2026-09-27-future-of-computing#m18|§m18]]). The remote side of it is in [[system-remote-compute]].

> [!question] Open
> What a caller sees when a call fails "maybe ran" over the network, beyond idempotency flags (for example, request ids for deduplication), and how restarts of crashed local components are reported. Not discussed.
