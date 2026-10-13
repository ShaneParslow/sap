---
status: exploring
summary: The network is a first-class transport as long as it doesn't slow local paths. Plan 9's cpu command is the model - run on a powerful remote machine while importing the user's namespace - so remote AI compute gets exactly the capabilities the user hands it.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m1|§m1]]", "[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m20|§m20]]", "[[2026-09-27-future-of-computing#m21|§m21]]"]
---

# Remote compute

The human sees the network as first class "as long as it doesn't get in the way too much", and noticed that Plan 9's 1980s–90s picture of terminals using more powerful CPU servers rings true again with AI: heavy inference may live in a garage appliance, a rented server or the cloud ([[2026-09-27-future-of-computing#m2|§m2]], [[2026-09-27-future-of-computing#m20|§m20]]).

## The Plan 9 model

Plan 9's `cpu` command ran your shell on a remote machine but **imported your terminal's namespace** into it: programs running remotely saw your local files and devices as if local. Computation moved to the server; authority stayed with you. Today's cloud does the opposite: you upload your data into the provider's namespace ([[2026-09-27-future-of-computing#m21|§m21]]).

In sap the Plan 9 direction is the natural one. A remote inference server, or an agent running in the cloud, gets exactly the capabilities the user hands it into their object store, and nothing else. That is how remote AI compute can be used without giving up control of data ([[vision-owner-control]]).

## Mechanism

- Cap'n Proto RPC already carries capabilities and pipelined calls over a network connection ([[iface-capnp]], [[ipc-transports]]).
- Interfaces declare failure modes and idempotency, so code written for local components works over the network ([[iface-failure-semantics]]).
- Sturdy refs are how a remote party holds access across reconnects ([[store-sturdy-refs]]).
- The protocol itself (node identity by public key, a bridge component, network sturdy refs, the missing three-party handoff) is proposed in [[ipc-network-rpc]]; how the user's data is spread over their own machines is in [[store-distribution]].

## Self-hosting context

From the opening of the chat: self-hosting becomes easy to operate when an agent does the administration; what stays hard is residential networking (CGNAT, asymmetric links, terms banning servers), power, and enough inference compute. Secondhand datacenter GPUs may help, though most are 700 W+ SXM modules; the PCIe variants are what reach homes ([[2026-09-27-future-of-computing#m1|§m1]], [[2026-09-27-future-of-computing#m3|§m3]]).

> [!question] Open
> How remote capabilities are presented in the powerbox. Authentication (device keys signed by a user key) and the local fast path (a bridge component) have proposals in [[store-distribution#Identity]] and [[ipc-network-rpc]].
