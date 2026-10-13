---
status: proposed
summary: A capability has a live form (kernel/RPC table index, valid while the holder lives) and a persistent form (a sturdy ref, an unguessable token). The store keeps tokens in a private side table and converts between the two, so programs never handle token bytes. Worked example included.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m20|§m20]]", "[[2026-09-27-future-of-computing#m21|§m21]]", "[[2026-09-27-future-of-computing#m28|§m28]]", "[[2026-09-27-future-of-computing#m29|§m29]]"]
---

# Capabilities in the store

The human's intuition was that stored capabilities are a way to share capabilities between processes and end up as one more data type in the store ([[2026-09-27-future-of-computing#m20|§m20]]). That is right, with special handling.

## Two forms of one reference

([[2026-09-27-future-of-computing#m21|§m21]])
- **Live reference:** an entry in a kernel or RPC capability table, seen by the process only as a local index. Valid while the process exists and the capability isn't revoked. The index means nothing anywhere else, so it can never be written to storage directly.
- **Persistent reference, or sturdy ref** (Cap'n Proto's term, inherited from E): a serializable, unguessable token. Presenting it to the service hosting the object restores a live reference. Possessing the token *is* the authority, so token bytes are secrets.

The everyday analogy is a share link: unguessable, typed, carrying specific rights, revocable ([[2026-09-27-future-of-computing#m29|§m29]]).

## Programs never see token bytes

In a stored value, a capability field works like a capability in a Cap'n Proto message: the value holds a typed slot, and the token lives in a **side table only the store can read** ([[2026-09-27-future-of-computing#m21|§m21]]).
- When a process with read access reads a value containing a capability field, the store **restores** it and hands back a live reference.
- When a process writes a live reference into a value, the store **converts** it into a sturdy ref.

So a stored capability is typed by its interface hash like everything else, but it can't be read as bytes, copied out, or leaked. Tokens leave the machine only when the user deliberately shares something outside it.

## Example: editing a photo

Store contents ([[2026-09-27-future-of-computing#m29|§m29]]):

```
values (immutable, by hash):
  h:a91f… = image { schema: h:img-v2, bytes: … }

refs (mutable cells, reached only through capabilities):
  home        → { photos: ref, docs: ref, apps: ref }
  photos/1043 → h:a91f…

store side table (never visible to apps):
  token 7Qx… → { target: photos/1043, iface: image-ref, rights: read, expires: 2026-11-01 }
```

1. The editor asks the powerbox for an image; the user picks photo 1043; the editor gets a live capability to `photos/1043` with read and write rights ([[store-granting-access]]).
2. The editor writes a new value `h:c27b…` and sets the ref to it. `h:a91f…` still exists, so history is free.
3. The editor wants "recent files" to survive restarts, so it writes the live capability into its own app-state value. The store converts it to a sturdy ref and keeps the token in the side table; the editor only sees a capability slot.
4. On next launch the editor reads its app-state and the store restores live capabilities. The editor never handled token bytes.
5. The user shares the photo with a friend. Now the token leaves, as the link. The user can revoke it, or it expires.

Program invocations can be stored the same way, as values holding the code hash, parameters and sturdy refs to inputs and outputs ([[kernel-process-model]]).

## Lifecycle

- **Revocation:** the hosting service deletes the token's mapping. Delegated capabilities go through caretakers so they can be cut off independently ([[cap-revocation]]).
- **Garbage collection:** none across the distributed capability graph; stored capabilities carry expiry or leases instead ([[2026-09-27-future-of-computing#m21|§m21]]).

## Why a userspace store

KeyKOS and EROS took the other approach: the kernel periodically checkpoints the whole system, capabilities included, so nothing is ever serialized. It is elegant but all-or-nothing: it handles external network peers and hardware poorly, and upgrading code is hard because code state is persisted with everything else. A userspace store keeps the kernel small and is just another component with an interface ([[2026-09-27-future-of-computing#m21|§m21]]).

The human asked whether the store is really purely userspace, since it seems to integrate with the kernel ([[2026-10-08-22c3947b#m18|§m18]]). With the store now defined as the persistent part of the object graph ([[store-object-graph#Where the kernel is involved]]), the answer splits: the kernel makes every cross-host reference unforgeable, but the graph lives in userspace hosts. The store in particular uses only ordinary kernel objects. Badged endpoints tell it which client and which ref a call is on. Frames let it hand out read-only mappings of blobs without copying, which is safe because values are immutable. Notifications deliver `subscribe` wake-ups. What makes the store special is its *position*, not kernel support: it holds the token side table, so it is trusted with every stored capability, and the process manager depends on it for code. That dependency is circular at boot, and a boot image breaks it ([[prog-loading#Boot: before the store exists]]). Kernel objects themselves are never stored: after a reboot, a stored capability is restored by asking the service that hosts the object, which recreates whatever kernel objects it needs.

Across machines, a sturdy ref must also name the node that hosts it, by public key ([[ipc-network-rpc#Network sturdy refs]]).

Prior art: [[sandstorm]], built on Cap'n Proto by its author, uses sturdy refs and a powerbox between isolated "grains"; it is the closest existing system to this layer.
