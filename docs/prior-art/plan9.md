---
status: reference
summary: Bell Labs' OS where every resource is a server speaking 9P, each process has its own namespace, and cpu servers run your shell remotely with your namespace imported. sap is roughly Plan 9 with typed interfaces and capabilities.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m19|§m19]]", "[[2026-09-27-future-of-computing#m20|§m20]]", "[[2026-09-27-future-of-computing#m21|§m21]]"]
---

# Plan 9

**What it is:** every resource (drivers, the window system, the network stack, other machines) is a server speaking one protocol, 9P. Each process mounts what it needs into its own namespace, and it all works the same over the network ([[2026-09-27-future-of-computing#m19|§m19]]).

**What sap takes:**
- Per-process namespaces, taken to their conclusion: in sap the namespace *is* the permission system ([[2026-09-27-future-of-computing#m3|§m3]]; [[cap-model]]).
- Uniform local and remote access to resources ([[ipc-transports]]).
- The `cpu` command's direction: computation moves to a remote server, the user's namespace is imported, authority stays home ([[2026-09-27-future-of-computing#m21|§m21]]; [[system-remote-compute]]).

**What sap avoids:** 9P is untyped byte streams on files, so meaning hides in ad hoc text formats. This is the limit the human identified early on with "everything is a file", and why sap's interfaces are typed ([[2026-09-27-future-of-computing#m19|§m19]]).
