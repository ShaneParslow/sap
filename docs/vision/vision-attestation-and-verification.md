---
status: exploring
summary: What cryptography can and cannot prove about a running program, scoped versus whole-device attestation, and cheaper alternatives to consensus. Background; not yet part of sap's design.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m4|§m4]]", "[[2026-09-27-future-of-computing#m5|§m5]]"]
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

## Collective verification without consensus

The human asked whether blockchain-style replication could give collective verification. Smart contracts verify by replication (every node re-runs everything), the most expensive option. Cheaper tools cover most needs ([[2026-09-27-future-of-computing#m5|§m5]]):

| Need | Tool |
|---|---|
| Everyone sees the same history; nobody rewrites it quietly | Transparency logs: append-only Merkle logs with independent monitors (Certificate Transparency, Sigstore Rekor) |
| A computation was done correctly | zk proofs |
| Globally unique claims among parties who distrust each other (names, double-spend) | Consensus |

Content-addressed objects ([[store-data-model]]) plus signed transparency logs give a git-shaped history anyone can audit; consensus is needed only for the few namespaces that must be globally unique.

> [!question] Open
> Whether sap needs any of this in its core (an attested-enclave component type, transparency logs for code provenance) or only as applications. One of the use-case walkthroughs in [[roadmap]] is a game that needs an attested enclave.
