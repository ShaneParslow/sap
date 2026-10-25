---
status: hub
summary: Every unresolved question in the wiki, grouped by whether it blocks the roadmap, each linked to the page that holds the detail.
updated: 2026-10-24
---

# Open questions

Grouped by urgency, then ordered by how much of the design each one affects. Detail lives on the linked page; when a question is settled, remove it here and record the answer in [[decisions]].

## Blocking near-term milestones

1. **Adopt the `$own`/`$borrow` annotation design?** Needed by M6. → [[iface-capnp]], [[cap-ownership]]
2. **Ring details:** ring sizes, and how a channel is created in one operation. M5 settled the record format and carries plain messages rather than RPC framing for now. → [[ipc-channels]], [[ipc-rings]]
3. **Capability transfer:** native move, batched transfer into a per-connection receive range, and whether that range is its own table. → [[ipc-cap-transfer]], [[kernel-objects-and-syscalls]]

## Design questions that don't block yet

4. Big-lock versus finer locking for multicore. → [[kernel-sel4-differences]]
5. How intent, conformance tests and failure/idempotency declarations attach to Cap'n Proto schemas. → [[iface-spec-contents]]
6. What exactly is hashed for an interface's identity, and how version names resolve. → [[iface-versioning]]
7. The rights vocabulary: fixed set or per interface. → [[cap-model]]
8. Revocation beyond the derivation tree, and the user-facing model for revoking shares. → [[cap-revocation]]
9. Restarting crashed components, and a kernel waiting-sender notification for on-demand start. (Exit and child death are answered by the process factory's `WAIT`, [[prog-process-factory]].) → [[kernel-process-model]]
10. **Composition:** the composition schema, automatic binding policy, nesting, readiness signals. A major userspace concern. → [[prog-composition]]
11. Ref topology and naming on top of hashes; a shared in-memory value cache; promoting transient refs to persistent. → [[store-data-model]], [[store-object-graph]]
12. Store requirements from the use cases: multi-ref atomic updates, branches and typed diffs, subscription semantics; use cases not yet walked through. → [[store-use-cases]]
13. Distribution: partial replication, encrypting values on semi-trusted nodes, moving a ref's authority, user-key recovery. → [[store-distribution]]
14. Powerbox interface; structure of personal data and intent for agents. → [[store-granting-access]]
15. Restorable state: how state is divided among owners so a session can be rebuilt from the store, which live state can't be, and the write cost of high-frequency state. → [[store-restorable-state]]
16. On-disk format: atomic ref updates, side-table protection, garbage collection roots. → [[store-on-disk-format]]
17. Network protocol: QUIC or Noise, relays, three-party handoff or sturdy refs, binding node keys to people. → [[ipc-network-rpc]]
18. Default resource limits and how hitting one is reported. → [[ipc-resource-limits]]
19. How a connection's transport is chosen. → [[ipc-transports]]
20. "Maybe ran" failures over the network beyond idempotency flags. → [[iface-failure-semantics]]
21. A case needing real cross-language in-process FFI. → [[iface-bindings-and-languages]]
22. A "finishing a publish" scheduling hint. → [[kernel-scheduling]]
23. Device manager: complete device-tree parse, and how device capabilities reach drivers. → [[hw-device-discovery]]
24. Page transfer's cost per page: invocations over a range of slots, 2 MiB frames, moving a capability and its mapping together, or no flush after map. On the D1 board each page costs about 69 µs against 25 µs to copy it, and the two map–unmap pairs, through their TLB flushes, are half of it. → [[ipc-page-transfer]], [[hw-d1-board]]

## Long-range

25. When to start an x86-64 port (Limine or a custom UEFI loader), and whether Arm comes first. → [[system-portability]]
26. Memory topology: attribute vocabulary, migration, placement versus quotas. → [[hw-memory-topology]]
27. Per-process or shared address spaces, decided per deployment; where to draw boundaries on CHERI hardware. → [[prog-loading#Address spaces]]
28. Rust target details: distributing the target spec to other people's crates, `sap-rt` versus the `std` platform layer. → [[prog-rust-toolchain]], [[prog-rust-std]]
29. Whether sap defines a canonical program IR at all; signatures on code objects. → [[prog-representation]], [[prog-loading]]
30. Attestation: the form of an exclusivity check for fine-grained claims about a capability subsystem; transparency logs in the core or only as applications. → [[vision-attestation-and-verification]]
31. Registry and name directory: who runs them, policy for choosing implementations, declarative configuration. → [[system-package-registry]]
32. Which legacy-compatibility approach comes first. → [[system-legacy-compat]]
33. Presenting remote capabilities in the powerbox. → [[system-remote-compute]]
