---
status: exploring
summary: How programs are wired together and started - splitting the problem into resolution (which instance serves each declared import, automatic and checked at launch), instances (shared or per client) and lifecycle (eager, lazy or manual start, stop and restart), with compositions stored as data, a factory import as the escape hatch for programs that want control, and answers to the edge cases (cycles, failed starts, hung servers, stop races).
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m22|§m22]]", "[[2026-10-08-22c3947b#m24|§m24]]", "[[2026-10-08-22c3947b#m26|§m26]]"]
---

# Composition: wiring and starting programs

Programs declare the interfaces they import ([[cap-process-authority]]). That should make it obvious, and automatic, which other programs must run to support one, and that is one of the main benefits of standard declared interfaces. But starting every import's server eagerly is heavy-handed: an import may never be used, a program may want control over how its server starts, and a server can hang. Leaving all of it to programs loses the benefit. The human called this one of the major userspace concerns, since it shapes IPC, program startup and how programs are structured ([[2026-10-08-22c3947b#m24|§m24]]). Nothing here is decided. The kernel mechanism for starting a server on demand is in [[kernel-process-model#Starting on demand]]; this page is the policy above it.

The human observed that this reinvents shared libraries: more dynamic, architecture-independent, and with each library in its own process ([[2026-10-08-22c3947b#m26|§m26]]). The analogy holds closely. Import resolution is dynamic linking at component granularity, a composition is the link map, lazy start is lazy symbol binding, and a factory import is `dlopen`. The difference is that every "library" is isolated and holds only the authority it was given.

## Three separate questions

Most of the trouble comes from treating "start the server" as one decision. It is three:

1. **Resolution:** which instance serves each import. This can be automatic and declarative, and it happens **when the consumer is launched**, so a missing or ambiguous implementation is an error at launch, not at first call.
2. **Instances:** whether the import is served by a shared instance (one per system or per user), a fresh instance per consumer (a sandboxed helper), or a capability the launcher already holds.
3. **Lifecycle:** when the instance starts (eagerly with the consumer, lazily on first call, or by the consumer itself), when it stops, and whether it restarts.

Resolving at launch and starting lazily gives both properties: the wiring is fixed and checked up front, and nothing runs until it is used.

## Compositions are data

The wiring lives in a **composition**: a value in the graph listing components (code by hash), how each import is routed (to another component's export, to a capability the parent holds, or to a [[store-granting-access|powerbox]] request), and each instance's lifecycle policy. The process manager enacts it, acting as supervisor. Programs never see it. They see only their imports.

Proposed default resolution, for imports the composition doesn't route explicitly:
1. if exactly one installed implementation exports the interface and policy allows automatic binding, bind it;
2. otherwise ask the user or the system agent, or refuse to launch.

Compositions are what the system agent and the package registry edit ([[system-package-registry#The agent as installer]]); a declarative system configuration is a composition plus the choices that produced it.

## Lifecycle options

| Policy | Starts | Use for |
|---|---|---|
| Eager | with the consumer | services needed at once (display, input for a GUI program) |
| Background | with the consumer, but the consumer doesn't wait for it | slow-starting services likely to be used soon |
| Lazy (default) | on first call, through the activator | most services; imports that may never be used |
| Manual | when the consumer chooses | servers the consumer configures or runs several of |

**Slow starts** are the cost of lazy by default ([[2026-10-08-22c3947b#m26|§m26]]): the first call to a service with a long startup waits for all of it. Background start hides that when use is likely. Choosing between them is a job for the system agent, which can learn from history which services are slow and which are always used. A server that is slow to become ready can also report readiness early and finish loading behind its first calls.

**Manual is the escape hatch, and it stays declarative.** A program that wants control over how its server starts (arguments, how many instances, when) doesn't import the interface; it imports a **factory** for it. The factory is itself a declared, resolvable import, so the program still states what it depends on, but it holds the start decision. "Let programs start things" becomes one option within the model rather than an exit from it.

**Optional imports** are marked in the program's world. Resolution may leave them unbound, and the program sees an absent capability rather than failing to launch.

## Edge cases

- **Cycles.** Lazy start breaks startup cycles (A and B can import each other, and neither runs until called). Eager cycles are rejected when the composition is resolved.
- **Many callers at once** queue on the same endpoint in the kernel. The activator starts one instance.
- **A failed start** breaks the capability for the waiting callers, who handle it like any failed call ([[iface-failure-semantics]]). The supervisor retries with backoff, per policy.
- **A hung server,** whether during start or later, is not specific to lazy start. The human pointed out that call deadlines are a poor tool here: short ones break slow but healthy calls, and long ones turn every hang into a long, disruptive wait ([[2026-10-08-22c3947b#m26|§m26]]). Proposed instead:
  - **Detect hangs by progress, not total duration.** A long call can report progress through its result promise (or a streaming result), and the caller's runtime watches for *no progress* rather than *too long*.
  - **Make waits attributable and cancellable.** The graph knows which server a call is waiting on, so an interface can show "waiting for the thumbnailer" and let the user cancel, rather than freezing without explanation. Cancellation is already part of the failure model ([[iface-failure-semantics]]).
  - **Leave liveness to the supervisor.** A fresh server must reach its first receive within a start deadline. After that the supervisor can check liveness through a heartbeat interface, and kills and restarts per policy, as Erlang/OTP supervisors do.

  Deadlines remain available to callers that have a real time budget (an agent's budget, a frame deadline), but are not the default hang detector.
- **Stopping an idle server** must not lose calls. The server stops only while waiting to receive, handing the endpoint back to the activator. Calls arriving meanwhile queue in the kernel and trigger the next start.
- **State across restarts** must be in the store; a restartable server keeps nothing it can't rebuild ([[store-object-graph]]).
- **Authority.** A server's own imports are resolved by the composition, recursively by the same rules, never by who happened to call it first. Otherwise the first caller could shape the server's authority, a confused-deputy risk.

## Prior art

- **Fuchsia's component framework** is the closest: manifests declare what each component uses and exposes, a parent's manifest routes capabilities between children, and components start lazily when a routed capability is first used, with eager start as an opt-in ([[fuchsia]]).
- **Genode** routes every service explicitly in its init configuration and usually starts everything up front; Sculpt lets the user edit the routing at run time ([[genode]]).
- **systemd and launchd** start services on first connection to a socket, with dependency and restart policies, but over untyped sockets and with the services' full ambient authority.
- **Erlang/OTP supervisors** define restart strategies (restart one child, or all of them) and restart-rate limits.

> [!question] Open
> The composition schema; whether automatic binding is allowed by default or only inside compositions the user approved; how compositions nest (a user session inside the system's composition, an application's helpers inside the session); and readiness and liveness signals beyond "reached its first receive".
