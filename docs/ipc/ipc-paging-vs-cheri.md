---
status: proposed
summary: The same IPC design implemented with address-space isolation (runs everywhere now) or CHERI compartments (byte-granular, call-cost crossings, harder revocation), plus software fault isolation as a middle option. Design for paging; real systems mix all three.
updated: 2026-10-20
sources: ["[[2026-09-27-future-of-computing#m16|§m16]]", "[[2026-09-27-future-of-computing#m17|§m17]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Paging versus CHERI

The human asked what the whole design looks like under each isolation mechanism ([[2026-09-27-future-of-computing#m16|§m16]]). Because the spec, wire layout and bindings never change, **the hardware choice only affects the transport implementation and the kernel**; applications are identical. So sap designs for paging, which runs everywhere, and treats CHERI and software isolation as faster transports where available ([[2026-09-27-future-of-computing#m17|§m17]]).

## Paging variant

- **Isolation unit:** one address space per trust domain.
- **Channels:** one shared region per connection, mapped into both ends, holding the rings. Small messages are copied in; large buffers get their own pages, granted by mapping and transferred by unmapping from the sender ([[ipc-channels]]).
- **Cross-domain calls:** an address-space switch (cheap with ASIDs), or messages handed between cores over rings ([[kernel-crossing-costs]]).
- **Validation:** generated structural checks and single-fetch reads at every boundary ([[ipc-validation]]).
- **Revocation:** unmap the pages; the real cost is the TLB shootdown. The main pain point of this variant ([[cap-revocation]]).
- **Status:** buildable now on any hardware.

## CHERI variant

- **Isolation unit:** a compartment inside a single address space.
- **Channels:** one flat pool; each message gets its own bounded capability, and the receiver can get a read-only one derived from it. Granting access is passing a pointer. Use a pool per channel, not one global scratchpad, so each component reaches only its own slots and reuse needs no sweep ([[2026-09-27-future-of-computing#m13|§m13]], [[2026-09-27-future-of-computing#m25|§m25]]).
- **Cross-domain calls:** a switch through sealed entry capabilities, about the cost of a function call.
- **Validation:** partly in hardware. A malicious offset past a message's bounds traps instead of leaking; variant tags and UTF-8 still need software checks.
- **Revocation:** freed memory is quarantined until a sweep invalidates outstanding capabilities; avoided on the IPC path by per-channel pools ([[hw-cheri]]).
- **Status:** RISC-V standardization underway; emulated by CHERI-QEMU; no consumer CPU announced.

## Comparison

([[2026-09-27-future-of-computing#m17|§m17]])

| | Paging | CHERI |
|---|---|---|
| Isolation unit | address space | compartment |
| Sharing granularity | 4 KB pages | bytes |
| Cost to grant | page-table operation | pass a pointer |
| Cost to revoke | unmap + TLB shootdown | quarantine + sweep |
| Bad offsets | caught by software checks | trapped by hardware |
| Cross-domain call | address-space switch | about a function call |
| Available | now | emulated now; hardware years out for consumers |

Paging can't give one flat pool with per-object, byte-granular permissions: granularity is 4 KB, every grant is a page-table operation, and every mapping takes TLB entries. The cost is TLB and page-table churn, not RAM, so caches don't make it go away. That is exactly what CHERI adds ([[2026-09-27-future-of-computing#m15|§m15]]).

## Moving ownership

Reading a message in place is sound only if the sender can no longer write it ([[ipc-validation#Double fetch]]). On paging, the kernel provides that by moving frame capabilities and checking the mappings ([[ipc-page-transfer]]). CHERI has no equivalent instruction. A capability is a value: storing it where another compartment can load it leaves the sender's copies intact, and taking those copies away later means revocation, a sweep of memory ([[cap-revocation]]). Two mechanisms come close.

- **Linear capabilities** can't be duplicated: moving one clears its source, so the hardware guarantees a single holder. They appear as an experimental, incomplete feature in the CHERI ISA version 7 report (2018), and the StkTokens calling convention is built on them ([Skorstengaard, Devriese and Birkedal, POPL 2019](https://www.arxiv.org/pdf/1811.02787); [UCAM-CL-TR-927](https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-927.html)). No hardware implements them, and they are not part of CHERI-RISC-V standardization as of October 2026. Their unsolved parts include a holder that never gives the capability back, how they interact with ordinary capabilities, and copying memory that contains one ([Capstone, 2023](https://arxiv.org/pdf/2302.13863)).
- **Lending non-global capabilities.** In CHERIoT, a capability without the Global permission can only be stored through a capability with store-local permission, which only stacks have. The switcher zeroes the stack used across a compartment call, so a callee given a local capability can't keep it once the call returns, and loading through a capability without LoadGlobal strips Global from what is loaded ([CHERIoT concepts](https://cheriot.org/book/top-concepts-top.html)). That restricts the receiver of a capability, not the sender. To stop the sender writing, the roles are reversed: a trusted owner of the buffer lends the sender a local, writable capability only for the length of a fill call, and afterwards gives the receiver a read-only one. The sender had no way to keep the write capability. This is proposed, not designed: it needs compartments in one address space, a trusted buffer owner (a library compartment or the switcher), and stack zeroing at every crossing.

## Middle option: software fault isolation

Wasm or a verified compiler gives in-address-space compartments on ordinary hardware, at some runtime cost; it behaves like CHERI with the checks done in software ([[2026-09-27-future-of-computing#m17|§m17]]).

## End state

Real systems mix these: CHERI or software isolation for fine-grained compartments within a process, paging between coarse domains (users, VMs, untrusted services), and the network between machines. Between address spaces, page transfer remains the way to hand large messages from untrusted peers even on CHERI hardware, since CHERI doesn't revoke the sender's own writable capability ([[2026-09-27-future-of-computing#m25|§m25]]); within one address space, lending can stand in for it ([[#Moving ownership]]).
