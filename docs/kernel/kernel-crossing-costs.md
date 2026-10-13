---
status: reference
summary: What mode switches and address-space switches cost on current hardware, rough IPC latency floors, and the three techniques that keep crossings off the hot path (batching, dedicated cores, same-address-space isolation).
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Kernel crossing costs

The human asked whether mode switches are still bad enough to argue against a microkernel. The answer: they still cost real time, and the design goal is to keep crossings off the hot path rather than to make them cheap.

## Where the cost comes from

([[2026-09-27-future-of-computing#m9|§m9]])
- **Direct:** the trap itself, plus Spectre/Meltdown mitigations (page-table isolation, branch-predictor flushing), which made x86 kernel entry noticeably more expensive.
- **Indirect, and usually larger:** a cold pipeline, polluted caches, and a TLB flush if the address space changes.
- **Microkernel-specific:** a call to a service often means two address-space switches, out and back.

Hardware features that help: ASIDs/PCIDs (switch address spaces without flushing the TLB), x86 FRED for cheaper entry/exit. See [[hw-isa]] for how RISC-V, Arm and x86 compare.

## Latency floors

Rough floors on ordinary paging hardware ([[2026-09-27-future-of-computing#m25|§m25]]):

| Mechanism | Rough latency |
|---|---|
| Cross-core polling (one cache-line transfer) | ~100–200 ns |
| seL4-style synchronous IPC, including address-space switch | hundreds of cycles |
| Linux futex wakeup plus context switch | low single-digit µs |
| Flow-IPC round trip (its own benchmark) | ~100 µs, flat across payload sizes with zero-copy |

The Flow-IPC number is its own signaling machinery on Linux, not a limit of paging; paging alone reaches sub-microsecond IPC ([[2026-09-27-future-of-computing#m23|§m23]]; see [[flow-ipc]]). CHERI's gain is different: cross-compartment calls at roughly function-call cost and cheap fine-grained grants ([[hw-cheri]]).

## Keeping crossings off the hot path

Three techniques, which combine ([[2026-09-27-future-of-computing#m9|§m9]]):

1. **Batching through rings.** Many operations share one crossing; with polling (io_uring SQPOLL, NVMe polling) the steady state needs none. sap's data path is rings for this reason ([[ipc-channels]]).
2. **Dedicated cores.** System services run on their own cores and talk over shared-memory rings (Barrelfish's multikernel explored this). With dozens of cores, two or three for services is cheap and the trap disappears.
3. **Isolation within one address space.** Protection keys (Intel MPK, Arm POE), CHERI compartments, or software fault isolation remove the TLB cost ([[ipc-paging-vs-cheri]]).

What remains is crossings for control operations (create, grant, revoke), which are rare.
