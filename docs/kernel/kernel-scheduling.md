---
status: proposed
summary: CPU time is a capability (scheduling contexts) so agents get hard budgets; synchronous calls donate the caller's time; waiting is adaptive (poll, then sleep); preemption awareness is a latency optimization, not a correctness need.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m24|§m24]]", "[[2026-09-27-future-of-computing#m25|§m25]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Scheduling

## CPU time as a capability

A **scheduling context** is a kernel object representing a budget of CPU time; a thread runs only while bound to one with budget left. This is seL4's MCS model. It matters directly for agents: hand an agent a budget of CPU time, memory (a quota of untyped memory) and, later, accelerator time, and it cannot exceed it ([[2026-09-27-future-of-computing#m7|§m7]]). Budgets are capabilities like any other authority ([[cap-model]]), and the same mechanism bounds denial-of-service from misbehaving peers ([[ipc-resource-limits]]).

## Timeslice donation

On a synchronous `Call`, the caller donates its scheduling context so the callee runs immediately on the caller's budget. This removes queueing delay and charges the callee's work to the caller, which keeps budgets meaningful when work crosses components. Servers can be *passive*: they have no budget of their own and run only on callers' donated time. Linux can't do this; Google's FUTEX_SWAP and UMCG proposals targeted it and were not merged ([[2026-09-27-future-of-computing#m25|§m25]]).

## Waiting

Adaptive: poll while a channel is busy, sleep on a notification when idle, the way io_uring's SQPOLL times out ([[2026-09-27-future-of-computing#m17|§m17]]). Hot channels can be served by a polling core ([[kernel-crossing-costs]]).

## Preemption awareness

Preemption in the middle of writing a message is not a correctness problem: publishing a message is a single release-store, and single-producer/single-consumer rings have no locks, so there is no priority inversion; a preempted writer only delays its own message ([[2026-09-27-future-of-computing#m25|§m25]]).

The human's point is that awareness could still reduce latency, by letting a writer finish its message before losing the CPU ([[2026-09-27-future-of-computing#m28|§m28]]). Mechanisms that exist: Linux restartable sequences (rseq) tell a thread it was preempted or migrated (mainly useful for per-CPU data); proposals for a short time-slice extension requested through rseq have circulated on Linux mailing lists, status unknown. In sap, donation on synchronous calls and polling cores cover most of the same ground ([[2026-09-27-future-of-computing#m29|§m29]]). Message size limits should come from ring size and latency targets, not timeslice length.

> [!question] Open
> Whether sap's kernel offers a "finishing a publish, extend my slice briefly" hint. Low priority; measure first.
