---
status: exploring
summary: AI should write at the level of intent and specs and let verified deterministic tools lower to machine code; AI improves the toolchain (heuristics, superoptimization) with results stored as checked objects. Proofs are optional and live on the language side; Bend is an example language, not a foundation.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m4|§m4]]", "[[2026-09-27-future-of-computing#m5|§m5]]", "[[2026-09-27-future-of-computing#m7|§m7]]", "[[2026-09-27-future-of-computing#m10|§m10]]", "[[2026-09-27-future-of-computing#m11|§m11]]"]
---

# Toolchain and verification

## Should AI write assembly?

The human suggested that since LLMs are built for translation, AIs might write assembly directly with nothing in between ([[2026-09-27-future-of-computing#m4|§m4]]). The chat's answer: the middle layers were never mainly for humans. An IR lets deterministic, cheap, checkable algorithms do optimization; lets one program target many microarchitectures (the ISA isn't the real machine); and lets proofs be carried from source to binary, as CompCert does. An LLM emitting assembly is compilation by an expensive process nobody can verify. Translations that must be exact want a proven translator, not a probabilistic one ([[2026-09-27-future-of-computing#m5|§m5]]).

The split instead:
- **AI writes** at the level where intent and specifications live.
- **A verified, deterministic toolchain** lowers that to machine code.
- **AI tunes the toolchain:** heuristics, superoptimizing hot loops, discovering rewrite rules. LLVM already ships ML-guided inlining and register allocation (MLGO).

## AI-written optimizations as checked objects

An optimization is stored as a tuple (IR pattern, target, native implementation, equivalence proof). The compiler uses it when the pattern matches and the proof checks: Urbit's jets with the missing verification added. LLVM already checks peephole optimizations this way with Alive2 ([[2026-09-27-future-of-computing#m7|§m7]]). This is principle 8 in [[vision-principles]].

## Where proofs live

Proofs and behavioral contracts are optional and belong to languages that support them; the OS-level interface spec doesn't require them ([[iface-spec-contents]]). The human's view is that formal verification won't stay optional for long in an AI-written world, but that the OS can't demand it of every language ([[2026-09-27-future-of-computing#m10|§m10]]).

## Bend

Bend 2 is a language whose users declare laws their app must never break (in a `LAWS.bend` file); the compiler then requires a proof that they still hold whenever code changes. It is a good example of behavior specification living in the language. It is not a foundation for sap's interface layer: it is one language with a specific runtime model (affine values, limited numeric types), and its trust story is young (the README says the compiler is 99% AI-written and not fully audited, with a Lean-proven kernel available for a `--verdict` mode) ([[2026-09-27-future-of-computing#m11|§m11]]). Treat it as a candidate implementation language that plugs into sap's interfaces, and possibly a consumer of contracts attached to them. See [[bend]].

> [!question] Open
> None of this affects the first milestones, which are hand-written Rust (or C) kernel and components. It matters once sap has a code-identity and loading story ([[kernel-process-model]]).
