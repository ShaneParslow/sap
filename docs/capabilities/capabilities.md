---
status: hub
summary: The capability model that every layer of sap uses - what a capability is, how ownership moves, and how authority is taken back.
updated: 2026-10-08
---

# Capabilities

A capability is the only form of authority in sap. The same idea appears at four layers (hardware pointers, kernel objects, RPC references, stored references), so it gets its own area ahead of the layers that implement it. Read [[cap-model]] first; the other two pages follow the lifecycle of a capability after it is created.

| Page | Status | Summary |
|---|---|---|
| [[cap-model]] | proposed | What a capability is, the four layers it appears at, how they connect, and the rules that hold everywhere. |
| [[cap-ownership]] | proposed | `own` and `borrow`: how capabilities are transferred and lent, how protocol rules become types, and who enforces them. |
| [[cap-revocation]] | proposed | Taking authority back: derivation trees, caretakers, cancelling borrows, expiry, and what revocation costs on paging and CHERI. |
| [[cap-process-authority]] | proposed | The view from inside one process: kernel, RPC and stored capabilities in one table, runtime capabilities versus declared imports, and how every object arrives in a message. |

Related elsewhere: how capabilities appear in interfaces ([[iface-spec-contents]]), how they travel in messages ([[ipc-channels]]), how they persist ([[store-sturdy-refs]]), and CHERI's hardware version ([[hw-cheri]]).
