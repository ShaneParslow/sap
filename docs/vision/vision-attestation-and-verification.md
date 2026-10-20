---
status: exploring
summary: What cryptography can and cannot prove about a running program, scoped versus whole-device attestation, the human's proposal of many competing verifiers with fine-grained capability-graph claims, and cheaper alternatives to consensus. Background; not yet part of sap's design.
updated: 2026-10-19
sources: ["[[2026-10-16-2c1ac53c#m4|§m4]]", "[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m4|§m4]]", "[[2026-09-27-future-of-computing#m5|§m5]]"]
---

# Attestation and verification

Some programs legitimately need to prove what they are: tournament game clients, business systems, anything others must trust. The owner-control goal ([[vision-owner-control]]) needs that to be possible without locking the whole device. This page collects what the chat established; sap has no design for it yet.

## What can be proven without hardware

Cryptography can prove that **an output came from program P on some input**: verifiable computation. A SNARK or STARK lets anyone check a short proof that P(x) = y without re-running P; zkVMs such as RISC Zero and SP1 prove ordinary RISC-V programs. The cost is the prover, still orders of magnitude slower than running P (as of late 2026, shrinking) ([[2026-09-27-future-of-computing#m5|§m5]]).

It cannot prove that **the machine in front of you is running only P, with genuine inputs**. The adversary owns the hardware, so other code can read P's memory or feed it fabricated input, and the proof still verifies because P did process those inputs correctly. Obfuscation doesn't close this either. Attesting the environment needs something physical the owner can't forge. Cryptography can shrink what must be attested: a server-authoritative game can prove every state transition followed the rules, leaving only the input channel to trust ([[2026-09-27-future-of-computing#m5|§m5]]).

## Scoped versus whole-device attestation

A container can't attest itself, because the host can fake anything it reports; every form of attestation needs a hardware root. The useful variable is **how much of the system the attestation covers** ([[2026-09-27-future-of-computing#m3|§m3]]):
- **Scoped:** an enclave or confidential VM (SGX, TDX, SEV-SNP, Arm CCA) proves its own contents and nothing else. Legitimate uses (competitions, business clients) need only this.
- **Whole-device:** the entire system must be vendor-signed. This is the form to resist for consumer services.

The human's preferred gradient for programs that need attestation: don't attest at all, then an attested container, then an enclave, then dedicated hardware ([[2026-09-27-future-of-computing#m2|§m2]]). With the correction above, every step after the first is a scoped hardware attestation of increasing size.

## Many verifiers, fine-grained claims

The human proposed accepting hardware-rooted attestation rather than avoiding it, and removing what makes it harmful today: a single signing authority for the operating system ([[2026-10-16-2c1ac53c#m4|§m4]]). The hardware maker is the one unavoidable root, as with Intel and AMD now. Above it, any organization can state which system configurations it accepts, and many do: a game studio, a bank, an employer, a tournament. Requirements then compete, and the pressure is toward asking only for what is needed. The prompt was Epic Games bringing anti-cheat to Linux, which requires a signed kernel, signed modules and secure boot. That request is reasonable in itself; it is distasteful because one vendor's keys decide which systems qualify.

Whether this ends well depends on two things the human named: how easily a system can be changed to meet a given requirement, and **how fine-grained a claim can be**. Capabilities are what make fine-grained claims possible. Today's attestations cover the whole boot chain, because on a monolithic system any kernel module can read any process's memory. On sap, a verified kernel and a small loader are enough to state *"process G runs code with hash H, and only these capabilities reach it: input from driver D, a display from server S, a network connection"*, and to say nothing about the rest of the machine. Anti-cheat needs no more than that.

What the architecture would need (inferred, not yet discussed):
- **Measured boot** of the kernel, the root task and the process manager into a hardware root (a TPM, DICE on small RISC-V parts, or a security monitor such as Keystone), so an attestation service can sign with a key no other component holds.
- **Code identity:** programs are already identified by hash ([[kernel-process-model]]).
- **A provable capability graph.** The attester must know every capability that reaches the subsystem. The kernel already enforces *authority confinement*: capabilities move only through transfers that carry the grant right ([[kernel-ipc#Capability transfer]]). What is missing is a way to show that nobody else holds a copy, such as an invocation reporting whether a capability is the only one to its object (the kernel tracks this already, to finalise objects), or capabilities that can't be copied.
- **Everyone who holds the subsystem's memory is part of the claim.** Whoever retyped the subsystem's frames keeps capabilities to them and could map them ([[prog-libsap#The loader]]). So the process manager belongs in every claim, or the loader hands over and deletes its copies. Device DMA belongs in it too, through IOMMU configuration ([[hw-devices-and-io]]).
- **Privacy:** anonymous or per-verifier keys and selective disclosure, so an attestation doesn't identify the device or reveal the rest of its configuration.

Two limits are known. Such a claim covers storage channels, not timing: seL4's information-flow proof needed a fixed partition schedule and disabled device interrupts to remove even the channels it could model, and it left cache timing out of scope ([[sel4#Information-flow proof]]). And competition among verifiers is not guaranteed: Android's Play Integrity, where a few large verifiers turn away modified systems, is the case that went badly. The owner-control answer ([[vision-owner-control]]) is that attestation is something the owner grants per request, through the same powerbox as any other authority ([[store-granting-access]]), and that a request for more than its stated purpose is visible as such.

## Collective verification without consensus

The human asked whether blockchain-style replication could give collective verification. Smart contracts verify by replication (every node re-runs everything), the most expensive option. Cheaper tools cover most needs ([[2026-09-27-future-of-computing#m5|§m5]]):

| Need | Tool |
|---|---|
| Everyone sees the same history; nobody rewrites it quietly | Transparency logs: append-only Merkle logs with independent monitors (Certificate Transparency, Sigstore Rekor) |
| A computation was done correctly | zk proofs |
| Globally unique claims among parties who distrust each other (names, double-spend) | Consensus |

Content-addressed objects ([[store-data-model]]) plus signed transparency logs give a git-shaped history anyone can audit; consensus is needed only for the few namespaces that must be globally unique.

> [!question] Open
> Whether fine-grained attestation needs a kernel primitive (an exclusivity check or uncopyable capabilities) or can rest on the process manager's records; how a running system is reconfigured to meet a verifier's requirement. Whether sap needs any of this in its core (an attested-enclave component type, transparency logs for code provenance) or only as applications. One of the use-case walkthroughs in [[roadmap]] is a game that needs an attested enclave.
