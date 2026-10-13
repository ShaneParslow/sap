---
status: hub
summary: The graph is sap's universal data model - immutable content-addressed values, mutable refs and live objects, transient, persistent or remote - and the store is its persistent part; both live in userspace hosts.
updated: 2026-10-08
---

# Graph and store

The object store replaces the file system as sap's data model: personal data, configuration, program code and invocation records all live in it. The **graph** is every object, transient, persistent and remote; the **store** is the userspace component hosting its persistent part, and the network bridge hosts proxies for remote parts ([[store-object-graph]], decided 2026-10-11). This area keeps the folder name `store` and covers both. The kernel makes references between hosts unforgeable but doesn't see the graph. That sap is object-store based is decided; the design here is proposed. The human called the relation between live capabilities and stored ones "the final thread to spec out" to confirm the approach ([[2026-09-27-future-of-computing#m20|§m20]]).

Pages go from what the store is (the object graph), to the data model, to how capabilities live in it, to how access is granted to programs and agents, to how it spans machines, to how it is stored on disk; the use cases last test all of them.

| Page | Status | Summary |
|---|---|---|
| [[store-object-graph]] | decided | The graph is every object in the system; memory, the store (disk) and the network are backends; each object has one host; where the kernel is involved. |
| [[store-data-model]] | proposed | Immutable values by hash plus mutable refs; schema-by-hash; get/put/invoke/subscribe; authority follows reachability. |
| [[store-sturdy-refs]] | proposed | Live versus persistent capabilities, the side table that keeps tokens away from programs, and a worked example. |
| [[store-granting-access]] | proposed | The powerbox, root authority at login, snapshots versus live shares, and budgets for agents. |
| [[store-distribution]] | exploring | One store over several machines: free replication of values, one home node per ref, offline merges, device keys, guest sessions. |
| [[store-on-disk-format]] | exploring | An append-only log of content-addressed blobs plus an index; Git's formats as prior art. |
| [[store-use-cases]] | exploring | Walkthroughs (roaming preferences with live reload, granting a website data, a system agent, device roles, Linux) and the requirements they produce. |

The store is also the human's user-held personal data store from [[vision-owner-control]], and where program code lives by hash ([[kernel-process-model]], [[prog-representation]]).
