---
status: reference
summary: Google's capability-based OS; the closest existing architecture to sap (handles, FIDL, per-component namespaces, drivers as components), now deprioritized. Use as a reference design.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m11|§m11]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m21|§m21]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Fuchsia

**What it is:** a production capability OS on the Zircon kernel. Kernel handles act as capabilities; FIDL is a typed interface language whose messages carry handles, with real versioning annotations (`@available(added=…, removed=…)`) tied to API levels; there is no global file system, only per-component namespaces routed by a component manager; drivers are components; bulk data moves through shared memory objects (VMOs) ([[2026-09-27-future-of-computing#m3|§m3]], [[2026-09-27-future-of-computing#m9|§m9]], [[2026-09-27-future-of-computing#m19|§m19]]).

**Relation to sap:** roughly sap's paging variant. Differences: Zircon channels copy message bytes through the kernel instead of using shared rings; Fuchsia still uses files rather than an object store; no content-addressed identity; no intent or conformance tests in its specs. Its FIDL wire format as the boundary between components, without shared libraries across it, is the same choice sap makes ([[2026-09-27-future-of-computing#m11|§m11]], [[2026-09-27-future-of-computing#m19|§m19]]). Its process arguments plus component namespaces are the production version of sap's bootstrap capability ([[kernel-process-model]]).

**Status (as of April 2026):** still released (F30 was the latest noted), but deprioritized: plans for Assistant speakers and full Chrome were dropped, and Google's new desktop effort is Android-based. The human distrusts it as a Google project; the useful part is the openly documented design decisions ([[2026-09-27-future-of-computing#m21|§m21]]).
