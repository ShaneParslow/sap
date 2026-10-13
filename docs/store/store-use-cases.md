---
status: exploring
summary: Walkthroughs that test the store design before it is built - preferences that follow the user and reload live, granting a website part of one's data, a system agent changing the system under review, the roles of phone, desktop and server, and interoperating with Linux - each ending in the requirements it puts on the store.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]", "[[2026-09-27-future-of-computing#m9|§m9]]"]
---

# Store use cases

The store is generic enough to fit almost anything, which is why it needs to be tested against concrete uses before it is specified (roadmap item D2, [[roadmap]]). The human asked for use cases covering the network, existing web and Linux, a system agent, and desktop, mobile and server ([[2026-10-08-22c3947b#m18|§m18]]). Each walkthrough uses the proposed model ([[store-data-model]], [[store-sturdy-refs]], [[store-distribution]]) and ends with what it requires. The requirements are collected at the end.

## Preferences that follow the user

**Setup.** Keybindings are a typed value, `Keybinds` (a schema identified by hash), at the ref `prefs/keybinds` under the user's root. An editor's world imports `keybinds :Ref(Keybinds)` with read rights ([[cap-process-authority]]).

**Use.** At start the editor reads the value. It also **subscribes** to the ref: when the user changes a binding, the store sends the new value's hash, and the editor re-reads and applies it. That is live reloading with no file watching, no parsing and no restart. Because the value is typed, a malformed config can't be written: the store checks it against the schema on `put`.

**Editing.** A change is "put a new value, compare-and-set the ref", which is atomic and keeps history. Since every config value carries its schema, one generic settings editor can edit any program's preferences, and the program needs no settings UI of its own. A program can attach a validator for rules a schema can't express (no two actions on one key).

**New machine, or someone else's.** Preferences are values under the root ref, so a newly paired machine has them after its first sync. On a friend's machine, a guest session receives a read-only, expiring capability to `prefs/` ([[store-distribution#Moving to a new machine, or using someone else's]]).

**Schema change.** Version 2 of the editor uses `Keybinds` v2. Cap'n Proto's append-only evolution means v1 values read as v2 with defaults ([[iface-versioning]]); an incompatible change needs an adapter, and the old value stays readable by old versions.

## Granting a website part of one's data

**Setup.** A print shop's website wants twenty photos from one album.

**Use.** The site sends the user to their own web gateway with a request ("read: images"). The gateway opens the [[store-granting-access|powerbox]], the user picks the album or individual photos, and the gateway returns a capability URL scoped to that selection, read-only, expiring in a week ([[ipc-network-rpc#Existing web platforms]]). The site fetches over HTTPS from the gateway, which serves from the home node, so the data is "stored" at home and the site has access, not ownership. The user sees the grant in a list of active shares and can revoke it early.

**Sharing a snapshot instead.** If the user picks "send a copy", the site gets the immutable values and no capability ([[store-granting-access#Snapshot versus live sharing]]).

**Caveat.** A site that has read the photos can keep them. The store controls access, not what a reader does afterwards.

## A system agent

**Setup.** An agent maintains the system for the user: installs and updates software, adjusts configuration, reorganizes files. It needs broad authority, which makes it the hardest case for least authority.

**Use.** The agent runs with a scheduling context and memory quota as its budget ([[kernel-scheduling]]) and a capability to the parts of the root it maintains. It does not change live refs directly. It **works on a branch**: a copy of the relevant refs, which is cheap because values are shared by hash. It installs a program by adding a code object and a launcher entry, edits configuration, and runs checks. The user, or a policy the user wrote, reviews the result as a diff between two trees of typed values. Accepting it is **one atomic update of several refs**; rejecting it costs nothing. Every action the agent took is recorded as a value (code hash, parameters, inputs and outputs, [[kernel-process-model]]), so the log is the audit trail and rollback means pointing the refs back.

**What it shows.** Immutable values plus refs make "propose, review, apply, undo" nearly free. The same pattern serves any agent the user doesn't fully trust, and is the store's strongest argument against a mutable file system.

## Phone, desktop and server

- **Phone:** a partial replica holding what is used there; subscribes through a push relay to save battery; home for its own camera roll until upload. Runs the powerbox for grants made from the phone.
- **Desktop:** a full or large replica; runs interactive programs and local agents; may be the home if there is no server.
- **Server (home or rented):** always on; home for most refs; hosts the web gateway and the relay for devices behind NAT; runs heavy or long agents and inference ([[system-remote-compute]]).

The same store interface serves all three; what differs is replication policy and which node is the home ([[store-distribution]]). Server-only workloads (other people's data, multi-tenant services) are each tenant's own store on a shared machine, isolated by capabilities.

## Interoperating with Linux

**Linux programs on sap** run in the compatibility layer and see a POSIX file tree that is a projection of part of the store: directories map to refs holding directory values, files to blobs, a write to a new value and a ref update ([[system-legacy-compat]]). Things a file system can't express, such as typed values, capability fields and history, are invisible to them.

**The store on Linux.** The store, network bridge and gateway are ordinary userspace programs. Built to also run on Linux and macOS, they would give people the personal data store, its sync and its sharing before sap the OS runs on their hardware, with sap the OS as the place where the same model extends down to processes and devices. That is an adoption path, proposed here and not discussed.

## Requirements collected

| Requirement | From | Page |
|---|---|---|
| Schema check on `put` | Preferences | [[store-data-model]] |
| `subscribe` delivers the new value's hash, coalesced | Preferences, phone | [[store-data-model]] |
| Atomic update of several refs | System agent | [[store-data-model]] |
| Cheap branches of a set of refs, and typed diffs | System agent | [[store-data-model]] |
| Per-schema merge rules for offline edits | Phone, desktop | [[store-distribution]] |
| Partial replication policy | Phone | [[store-distribution]] |
| Scoped, expiring, listable, revocable grants | Website, guest session | [[store-granting-access]], [[cap-revocation]] |
| A generic editor driven by schemas | Preferences | [[store-granting-access]] |
| POSIX projection | Linux | [[system-legacy-compat]] |

> [!question] Open
> Use cases not yet walked through: the original D2 set (a structured screenshot share, an agent reorganizing photos on a budget, a library across an incompatible version, an attested game, a legacy build tool), collaborative editing with other people, and large media.
