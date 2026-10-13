---
status: exploring
summary: One user's data spread over several machines - values replicate freely because they are immutable, each mutable ref has one home node that orders its updates, devices hold partial replicas, offline edits merge by per-schema rules, and identity is a user key that authorizes device keys. Covers where data physically lives and what using another machine means.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]"]
---

# Distribution: one store, several machines

A user has a phone, a desktop, perhaps a home server or a rented one, and sometimes uses someone else's machine. The human asked where personal data is actually stored, how configuration moves to a new system or works on someone else's, and how this integrates with network access ([[2026-10-08-22c3947b#m18|§m18]]). The proposal here treats the user's store as **one logical store held by several nodes**. It is exploratory and builds on [[store-data-model]] and [[ipc-network-rpc]].

## Values replicate freely

Values are immutable and named by hash, so copying them between nodes needs no coordination: a node asks for the hashes it lacks and checks what it receives, as Git does when fetching. A copy is either right or detectably wrong. Any node may hold any subset of values; a node missing a value fetches it from a node that has it, given authority to reach it ([[store-data-model#Authority follows reachability]]).

A node the user doesn't fully trust, such as a rented server used for backup, can hold values encrypted, with the keys reachable only through capabilities on the user's own devices.

## Each ref has one home node

Refs are where conflicts happen. **Decided (2026-10-11, [[2026-10-08-22c3947b#m20|§m20]]):** every ref has exactly one **home node**, which orders all updates to it with compare-and-set. Other nodes hold cached copies and subscribe to changes. Typically the always-on node (home server or rented node) is home for most refs, and a device can be home for refs that are its own (a phone's camera roll before upload). Authority can be moved deliberately from one node to another, as AT Protocol moves an account between hosts. The home node is simply the machine where the ref's host runs ([[store-object-graph]]).

**Offline writes.** A disconnected device can still update its cached refs; the updates are queued as proposals and applied by compare-and-set when it reconnects. If the ref moved on in the meantime:
- if the schema declares a merge rule with a `$merge` annotation (a CRDT type such as a set or counter, or a merge function, [[iface-capnp#Further annotations (proposed)]]), the home node merges;
- otherwise the result is a conflict value holding both versions and their common parent, like a Git merge conflict, for the user or an agent to resolve.

Last-writer-wins is never the default, because it silently loses data.

## Where data lives

The user decides; the design only requires that every ref has a home node somewhere reachable.

| Node | Typical role |
|---|---|
| Home or rented server | Always on: home for most refs, full replica, runs the web gateway and heavy agents, relays for devices behind NAT |
| Desktop | Full or large partial replica; home node if there is no server |
| Phone | Partial replica of what is used on it; small battery and storage budget; changes arrive through a push relay rather than open subscriptions |
| Someone else's machine | A temporary, attenuated session (below) |

A single machine with no server is the degenerate case: it is home for everything, and other devices sync when they can reach it.

## Identity

- **A user is a key pair.** The user key signs **device keys**, each a node identity ([[ipc-network-rpc#Identity and connections]]). A node proves it belongs to the user by presenting its signed device key. A lost device is handled by revoking its key.
- **Root authority:** the root ref's home is the user's always-on node; a device of the user's gets a capability to it when paired, and its shell gets that at login ([[store-granting-access#Root authority]]).
- Recovering a lost user key is a social and product problem (backup codes, trusted contacts, a hardware key); it is not designed here.

## Moving to a new machine, or using someone else's

**A new machine of one's own** is paired: the user approves it from an existing device, the new node receives a signed device key and a root capability, and it starts replicating. Preferences, application state and documents are values under the root ref, so they arrive with the first sync; nothing is exported or imported. See [[store-use-cases#Preferences that follow the user]].

**Someone else's machine** gets a guest session. The user hands the host an attenuated, expiring capability (for example by scanning a code from their phone): read-only preferences and the few documents they need, not the root ref. The host's programs run with the user's preferences because those are values reached through the capability. At logout the capability is revoked. What can't be promised is that the host forgets: its kernel saw everything the session used. The honest model is that a guest session trusts the host with exactly what was granted, which is why the grant should be small.

## Prior art

- **Git:** content-addressed replication; merge with a common parent.
- **AT Protocol** (Bluesky): each user's data is a signed, content-addressed repository hosted on a personal data server and portable between hosts, with identity separate from host. The closest current system to this page, though public by design.
- **Solid:** personal data pods on web servers, with access controlled by access-control lists rather than capabilities.
- **Upspin** (Google, 2017): user-owned files named by user and path, with keys and directory servers separating identity from storage.

> [!question] Open
> Partial replication policy (what a phone keeps); encryption of values on semi-trusted nodes, and whether it costs deduplication; how authority over a ref moves safely; whether multi-ref updates need transactions across nodes or only on one node; and user-key recovery.
