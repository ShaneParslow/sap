---
status: reference
summary: A clean-slate personal computing stack whose tiny VM (Nock) gets performance from jets, native code registered against function hashes. Shows the canonical-semantics-plus-native-cache pattern and why the cache must be verified; also a lesson in adoption barriers.
updated: 2026-10-08
sources: ["[[2026-09-27-future-of-computing#m3|§m3]]", "[[2026-09-27-future-of-computing#m5|§m5]]", "[[2026-09-27-future-of-computing#m6|§m6]]", "[[2026-09-27-future-of-computing#m7|§m7]]"]
---

# Urbit

**What it is:** a full personal computing stack built from scratch. Its core VM, Nock, is tiny and far too slow to run directly; performance comes from "jets", native implementations registered against the hash of the Nock function they replace ([[2026-09-27-future-of-computing#m5|§m5]]).

**What sap takes:** the pattern of semantic definition as ground truth with native code as a cache of it, which is the layered model in [[prog-representation]].

**What sap avoids:**
- Jets that didn't exactly match their Nock definitions caused Urbit's real bugs. The cache must be proven equivalent or checked by translation validation ([[2026-09-27-future-of-computing#m5|§m5]]; [[prog-toolchain-and-verification]]).
- Building a whole world before anyone can use it ([[2026-09-27-future-of-computing#m3|§m3]]).
- Invented vocabulary: the human found reading the docs "like having a stroke" and called naming the number-one adoption problem. The founder's political writing has been another barrier for many people ([[2026-09-27-future-of-computing#m6|§m6]], [[2026-09-27-future-of-computing#m7|§m7]]).
