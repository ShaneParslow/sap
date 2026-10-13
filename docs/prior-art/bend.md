---
status: reference
summary: Bend 2, a language whose compiler requires proofs that declared laws still hold after every change. An example of behavior specification on the language side; not a foundation for sap's interfaces.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m10|§m10]]", "[[2026-09-27-future-of-computing#m11|§m11]]"]
---

# Bend

**What it is:** Bend 2 lets users declare, in a `LAWS.bend` file, rules their app must never break; the compiler then requires a mathematical proof that they still hold whenever the code changes. Values are affine (closures and arrays can't be shared); numeric types are Nat, U32 and F32. The README says the compiler is 99% AI-written and not fully audited and the checker has no proof, but a `--verdict` mode uses a kernel proven in Lean ([[2026-09-27-future-of-computing#m11|§m11]]). The human had followed it as evidence that formal verification is becoming non-optional ([[2026-09-27-future-of-computing#m10|§m10]]).

**Relation to sap:** a candidate implementation language that plugs into sap's interfaces, and possibly a consumer of contracts attached to them (an interface's contracts could compile into laws). Not the interface language: it is one language with one runtime model ([[iface-spec-contents]], [[prog-toolchain-and-verification]]). Its architecture, a small proven kernel checking an unaudited generator, is principle 8 in [[vision-principles]].
