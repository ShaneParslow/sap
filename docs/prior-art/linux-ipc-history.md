---
status: reference
summary: Linux's attempts at better IPC - kdbus and bus1 (rejected), dbus-broker and varlink (adopted in userspace), memfd sealing, SCM_RIGHTS, and the unmerged scheduler-handoff proposals - and the lesson that the kernel should provide mechanism, not bus policy.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m9|§m9]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Linux IPC history

| Item | What happened | Lesson for sap |
|---|---|---|
| kdbus (~2013–2016) | Put D-Bus semantics in the kernel. Rejected: baked one bus's policy into the kernel, performance claims didn't hold up, sender-credential capture raised security concerns ([[2026-09-27-future-of-computing#m25|§m25]]). | Mechanism in the kernel, policy in userspace ([[vision-principles]]). |
| dbus-broker | Userspace reimplementation by kdbus's authors; delivered most of the gains. | Most IPC needs no kernel support ([[kernel]]). |
| bus1 | Simpler capability-based kernel IPC; never built momentum. | |
| varlink | Simple typed IPC protocol with interface definitions; systemd has been moving interfaces to it ([[2026-09-27-future-of-computing#m23|§m23]]). | Evidence of what the desktop will actually adopt. |
| memfd sealing (2014) | Seal a memfd against writes before sending, so an untrusted receiver can rely on immutability; added for kdbus and Wayland. | The page-transfer pattern ([[ipc-validation]]). |
| SCM_RIGHTS | Passing file descriptors between processes over Unix sockets. | fds are copyable and unrevocable, so not true capabilities ([[cap-revocation]]). |
| FUTEX_SWAP, UMCG | Google proposals for direct scheduler handoff on synchronous IPC; not merged. | sap needs timeslice donation in its kernel ([[kernel-scheduling]]). |
| rseq | Restartable sequences: userspace learns of preemption or migration. | Preemption awareness ([[kernel-scheduling]]). |

Also relevant: Landlock and seccomp for removing ambient authority, which the abandoned Linux-prototype plan relied on ([[2026-09-27-future-of-computing#m9|§m9]]).
