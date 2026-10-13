---
status: reference
summary: A C++ toolkit for zero-copy shared-memory IPC with Cap'n Proto on Linux. Shows the ecosystem's demand for exactly sap's IPC model, where Linux's latency floor sits for such libraries, and the typical gap - performance solved, trust boundary not.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m22|§m22]]", "[[2026-09-27-future-of-computing#m23|§m23]]", "[[2026-09-27-future-of-computing#m25|§m25]]"]
---

# Flow-IPC

**What it is:** a modern C++ toolkit for high-speed IPC on Linux: zero-copy transfer via shared memory, Cap'n Proto integration, native handle passing, sessions ([[2026-09-27-future-of-computing#m23|§m23]]).

**Why it matters to sap:** the human's reaction was that "the ecosystem wants this to be a thing so badly": everyone loves these concepts, everyone rolls their own, nobody integrates it into the OS, and nobody targets the desktop ([[2026-09-27-future-of-computing#m22|§m22]]). Its README's list of pain points (naming every socket and segment, coordinating creation and cleanup, handshakes, per-object permissions) is effectively a spec for the missing OS primitive.

**Numbers (its own benchmark):** about 100 µs round trip regardless of payload size with zero-copy, versus about 1 ms for 1 MB over classic sockets and approaching a second at 1 GB. Small messages are ~100 µs either way, so its fixed cost is signaling machinery on Linux primitives, not copying and not a limit of paging ([[2026-09-27-future-of-computing#m23|§m23]], [[2026-09-27-future-of-computing#m25|§m25]]; [[kernel-crossing-costs]]).

**What sap avoids:** its security model is user/group presets with processes sharing an arena, a transport between friends not designed for hostile peers ([[ipc-resource-limits]]).
