---
status: reference
summary: A capability-based component framework that can run on seL4, with a usable desktop (Sculpt OS) where the user explicitly routes every capability. Closest in spirit to owner control.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m31|§m31]]"]
---

# Genode

**What it is:** a framework of capability-isolated components that runs on several microkernels including seL4. Sculpt OS is a usable desktop built on it ([[2026-09-27-future-of-computing#m19|§m19]]).

**What sap takes:** the user explicitly routing capabilities between components, which is the closest existing expression of user sovereignty ([[vision-owner-control]]); running POSIX and Linux as contained components rather than the foundation, a pattern it shares with Qubes ([[system-legacy-compat]]) ([[2026-09-27-future-of-computing#m3|§m3]], [[2026-09-27-future-of-computing#m31|§m31]]).

Not yet studied in detail for sap; worth reading for how a real desktop on capabilities handles the powerbox problem ([[store-granting-access]]).
