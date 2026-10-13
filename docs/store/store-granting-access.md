---
status: proposed
summary: How programs and agents get access - the user's shell holds the root ref, everything else comes through a trusted powerbox where selecting an item and granting it are one act; sharing a snapshot differs from sharing live access only by whether the value holds a capability; agents get budgets as well as scopes.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m21|§m21]]"]
---

# Granting access

This page covers how authority over the store reaches programs, agents and other people, in the order a session encounters it.

## Root authority

At login, the user's shell receives a capability to the user's root ref. Every other capability derives from that root or is granted through the powerbox ([[2026-09-27-future-of-computing#m21|§m21]]). There is no ambient access to "the user's files" ([[cap-model]]).

## The powerbox

An app or agent can't reach the user's data directly. It asks the **powerbox**, a trusted system dialog, and the user picks what to grant; the requester receives a capability to exactly that selection. Selecting the item and granting authority become the same act, so consent is built into the operation instead of being a separate permission prompt ([[2026-09-27-future-of-computing#m21|§m21]]). This is where agents get scoped access instead of the run of a home directory ([[2026-09-27-future-of-computing#m3|§m3]]).

## Snapshot versus live sharing

The human's screenshot example resolves cleanly ([[2026-09-27-future-of-computing#m21|§m21]]):
- Sharing a **snapshot** shares an immutable value containing no capabilities.
- Sharing **live access** shares a value containing a capability to a mutable ref, read-only or read-write.

That is the only difference, and the system can show it to the user. Since sharing an object shares everything reachable from it ([[store-data-model]]), the UI must say exactly what that is before sending.

## Budgets for agents

Agents receive budgets as well as scopes: CPU time, memory and (later) accelerator time as capabilities they cannot exceed ([[kernel-scheduling]]) ([[2026-09-27-future-of-computing#m7|§m7]]). Capability-based least authority bounds the damage a buggy or misaligned agent can do without solving alignment ([[vision-owner-control]]).

## Remote compute

A remote inference server or cloud agent gets exactly the capabilities the user hands it into the store, and nothing else: computation moves out, authority stays home ([[system-remote-compute]]).

> [!question] Open
> The powerbox's interface (what a request can specify, how grants are shown and revoked later) and how the user's personal data, preferences and intent are structured for agents to request ([[2026-09-27-future-of-computing#m2|§m2]]). Not designed. The web consent flow and a schema-driven settings editor are sketched in [[store-use-cases]] and [[ipc-network-rpc#Existing web platforms]].
