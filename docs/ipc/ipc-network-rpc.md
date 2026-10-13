---
status: exploring
summary: Capabilities between machines - Cap'n Proto RPC over an authenticated, encrypted connection between nodes identified by public keys, carried by a bridge component so local paths never pay for it; network sturdy refs; the missing three-party handoff and why sturdy refs work around it; OCapN as prior art; and a web gateway for existing sites.
updated: 2026-10-11
sources: ["[[2026-10-08-22c3947b#m18|§m18]]", "[[2026-10-08-22c3947b#m20|§m20]]"]
---

# Network RPC

The network is one of sap's three transports ([[ipc-transports]]), and the human wants an object store reachable over the internet, a native sap protocol between machines alongside components that make existing web platforms work, and to know whether Cap'n Proto RPC is the right protocol ([[2026-10-08-22c3947b#m18|§m18]]). This page is the proposed shape, except for the four properties under [[#Decided properties]].

## The bridge component

Applications never speak the network protocol. A **network bridge** component holds the machine's network identity and its connections to other machines. A capability to a remote object is, locally, an ordinary capability to a proxy object exported by the bridge; calls on it travel over a local connection to the bridge, which forwards them. This settles the human's "as long as it doesn't get in the way" requirement: local programs link no network code, and local calls never pass through it. Plan 9's `exportfs` and `import` had the same shape ([[system-remote-compute]]).

## Identity and connections

- **A node is identified by a public key**, not by a DNS name or IP address. Addresses and names are only hints for finding the node. Two nodes authenticate each other by key when connecting, as WireGuard and Tor onion services do, so no certificate authority is involved.
- **The connection** is encrypted and authenticated, over QUIC or a Noise handshake over TCP. Nodes behind carrier-grade NAT, as most homes are, reach each other through a relay that forwards ciphertext only ([[system-remote-compute]] covers why residential networking is hard).
- **On the connection, Cap'n Proto RPC** runs unchanged: capability passing, promise pipelining, the same wire format as local messages ([[iface-capnp]]). Because the layout is constant across transports, a value read from a remote store is byte-identical to a local one.

## Network sturdy refs

A sturdy ref that crosses machines must name where it can be restored: **(hosting node's public key, address hints, secret token)**. A text form for sharing would look like `sap://<node-key>@<hint>/<token>`. Presenting it to the named node, over a connection authenticated to that key, restores a live capability ([[store-sturdy-refs]]). Because the key is part of the reference, a relay or a spoofed address can't impersonate the host.

## Cap'n Proto's gaps

Cap'n Proto defines four RPC levels. As of this writing the C++ and Rust implementations do level 1 (two-party connections with capability passing and pipelining). Level 2 (persistent capabilities) is left to applications, which is what sap's store does anyway. Level 3, **three-party handoff**, is specified but not implemented.

Three-party handoff matters when node A holds a capability to an object on node C and passes it to node B. Without it, B's calls go to A, which forwards them to C. A must stay online and sits in the path. That breaks a common personal-data case: a phone grants a website access to data hosted on the user's home server, and the website should then talk to the home server directly, not through the phone.

**Workaround:** across machines, pass sturdy refs rather than live capabilities whenever the recipient will use the object for long. B restores the sturdy ref directly from C, using C's key from the reference. That gives the effect of a handoff for long-lived grants, at the cost of an extra connection and of the grant being persistent, so it must carry an expiry ([[cap-revocation]]). Short-lived proxying through A remains fine.

## OCapN

OCapN (Object Capability Network), led by the Spritely Institute with Agoric and others, is a set of specifications for exactly this layer: CapTP, the protocol Cap'n Proto RPC descends from; pluggable "netlayers" (TCP with TLS, Tor onion services, libp2p); sturdy refs as URIs; and certificate-based three-party handoff ([Spritely's introduction](https://www.spritely.institute/news/introducing-ocapn-interoperable-capabilities-over-the-network.html)). It uses its own encoding (Syrup), not Cap'n Proto's.

Proposed: keep Cap'n Proto on the wire, so the layout stays constant across all three transports, and copy OCapN's model for node identity, netlayers, sturdy ref URIs and handoff certificates. Adopting OCapN itself would put a second encoding at the network boundary. Revisit when its specifications stabilize.

## Existing web platforms

Websites and existing services don't speak capability RPC, so a **web gateway** component, a client of the bridge and the store, translates:
- **Capability URLs:** an HTTPS URL containing a sturdy ref's token, scoped to a subtree or a single object, read-only or read-write, with an expiry. This is the share-link form ([[store-sturdy-refs]]).
- **A consent flow for sites:** a site that wants data redirects to the user's gateway, which opens the [[store-granting-access|powerbox]]; the user's selection becomes a capability URL handed back to the site, much like an OAuth grant but scoped to objects instead of account-wide scopes.
- **Protocol adapters** that expose parts of the store as existing protocols: WebDAV for files, CalDAV and CardDAV for calendars and contacts, and possibly ActivityPub or AT Protocol for publishing.

A site that reads data can still keep a copy. Capabilities control access, not what a reader does afterwards; the user-facing promise is "this site can read these items until you revoke it", never "this data stays home".

## Decided properties

The human adopted four properties on 2026-10-11, because they are hard to retrofit and shape the formats of everything stored or sent ([[2026-10-08-22c3947b#m20|§m20]]):

1. **Sturdy refs name their hosting node by public key.** Such a name is *self-certifying*: whoever holds it can check they reached the right node without a registry, as in Mazières's Self-certifying File System (1999) and Tor onion addresses. The human compared it to Urbit's global cryptographic identities. The difference is that sap needs no global registry and no scarcity: any node can make a key. The cost is Zooko's triangle: names can't be secure, decentralized and human-meaningful at once. Keys are the first two, and people get human-meaningful names as local **petnames** ("Mum's laptop") mapped to keys in their own store.
2. **Hashes are tagged with their algorithm**, and more generally every encoded identifier says what it is, so formats can change without breaking stored references ([[iface-versioning]]).
3. **Values that are stored or sent contain no machine-local identifiers:** no slot numbers, local paths or process ids. This can't be checked in general, since a local id is just an integer, so it is enforced in layers. Capability fields, the normal way to refer to anything, are converted automatically and never contain local ids ([[store-sturdy-refs]]). A proposed `$local` schema annotation marks fields that are meaningful only on one machine; the persistence service and the bridge refuse values whose `$local` fields are set. Code review and lints catch the rest.
4. **Each mutable ref has one home node**, which orders its updates. This is what keeps replication free of conflicting duplicates ([[store-distribution]]).

These properties put information into schemas, so they extend Cap'n Proto through annotations in the same way as `$own` and `$borrow`. The human proposed doing the same for merge rules ([[iface-capnp#Further annotations (proposed)]]).

> [!question] Open
> QUIC versus Noise over TCP; relay infrastructure for home nodes; whether to implement three-party handoff in capnproto-rust eventually or rely on sturdy refs; how a node's key is bound to a person and recovered if lost; and how petnames are shared.
