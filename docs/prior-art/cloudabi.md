---
status: reference
summary: A capability-based POSIX subset (the precursor of WASI) where programs started with only the descriptors they were given and received typed arguments (argdata). Precedent for sap's process startup.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# CloudABI

**What it is:** a capability-based subset of POSIX, which WASI started from. Programs began with only the descriptors they were given, with no global file system access, and arguments came as a typed tree ("argdata") instead of strings ([[2026-09-27-future-of-computing#m25|§m25]]).

**What sap takes:** the startup model in [[kernel-process-model]]: an initial capability set, no ambient authority, typed arguments in the bootstrap message.
