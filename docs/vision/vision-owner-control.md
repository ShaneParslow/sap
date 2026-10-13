---
status: decided
summary: The goal behind sap - computing that answers to its owner - and why abundant AI-written software makes standardized interfaces, capabilities and user-held data the foundations that matter.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m0|§m0]]", "[[2026-09-27-future-of-computing#m1|§m1]]", "[[2026-09-27-future-of-computing#m2|§m2]]", "[[2026-09-27-future-of-computing#m3|§m3]]"]
---

# Owner-controlled computing

sap's goal is a system whose owner holds the authority: over what code runs, over where data lives and who can reach it, and over what agents acting on the owner's behalf are allowed to do. The premise is a future where AI makes writing software almost free. This page records why that premise leads to the design in the rest of the wiki. The goal itself is the human's and is settled; the analysis around it is a mix of the human's view and the assistant's, noted where they differed.

## What changes when software is cheap

When code can be regenerated on demand, code stops being where control sits. Control moves to what can't be regenerated: physical capital (fabs, datacenters, last-mile networks), cryptographic roots of trust (who signs what a device will boot), frontier model weights and compute, network effects and identity, and legal permission (anti-circumvention law, terms of service) ([[2026-09-27-future-of-computing#m1|§m1]]).

The human's view, which the chat adopted, is that this produces extreme user customization but not a world of throwaway software. Writing something will always cost more than reusing something that already exists and has been proven, so reuse still wins: verified libraries, standard formats and firm interfaces get downloaded and tweaked, and generation fills in presentation and glue. **Customization at the edges demands standardization at the core** ([[2026-09-27-future-of-computing#m2|§m2]]). The assistant's argument for why this holds is verification cost: a library that has run correctly on a billion machines carries evidence that fresh code can't have, so proven code is capital. Biology and the internet show the same shape, a small conserved core (the ribosome, IP) under freely varying periphery ([[2026-09-27-future-of-computing#m3|§m3]]).

## Two futures

The chat identified two stable end states ([[2026-09-27-future-of-computing#m1|§m1]]):
- **Enclosure.** Devices become attested appliances, services only talk to attested clients, and a platform's agent gates everything. This is the default drift, because the pieces already ship (remote attestation APIs, locked bootloaders, anti-automation terms).
- **Owner control.** The owner holds the root keys, runs capable models locally or on hardware they can leave, keeps data in a store they control that services reach through open protocols, and their agent is treated as them.

The human sees more inherent advantage for the open side than the assistant initially did (anyone can build any software; general-purpose computing persists the way Linux has; interoperability pressure pushes toward openness) and frames the question as the ratio between the two futures rather than which one wins ([[2026-09-27-future-of-computing#m2|§m2]]). Both agreed that the biggest lever on the ratio is whether the open path costs users nothing, as Proton did for Linux gaming ([[2026-09-27-future-of-computing#m3|§m3]]).

## What sap contributes

Most of the levers are outside an OS: legal rights for agents acting for users, owner-enrollable boot keys, open-weight models. sap targets the technical ones the chat identified as foundations that can be built now:
- **Least authority by construction.** An agent or program holds references only to what its task needs. This bounds the damage from buggy or misaligned code without solving alignment, and it is also the permissions cleanup that POSIX needs ([[2026-09-27-future-of-computing#m3|§m3]]). See [[cap-model]].
- **A user-held data store.** Personal data, preferences and intent in a store the owner controls, shared with programs and agents by grant. The human named this a prerequisite for an interoperable agent ecosystem ([[2026-09-27-future-of-computing#m2|§m2]]). See [[store]].
- **Standard, typed interfaces as the conserved core.** Self-describing data and formally specified interfaces, so that customization doesn't fragment the system. See [[interfaces]].

The human's original framing for the store was "everything is key → value associations, self-describing", pulled down from cloud designs (S3, REST, GraphQL) to the native level, in the spirit of Lisp machines: the language agents build programs from is also the data ([[2026-09-27-future-of-computing#m2|§m2]]). How much of that last idea survives is in [[prog-representation]].

> [!note] Out of scope for sap itself
> Policy positions from the chat (attestation neutrality, legal right of agent access, agent fiduciary duty, proof of personhood) are context for why the project exists, not design. Proof of personhood was left explicitly unresolved by both sides.
