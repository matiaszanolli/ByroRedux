# FO4-DIM6-09: CTDA variant — FO4 32-byte stride not verified across parsers

**Severity:** LOW | esm, legacy-compat
**Source:** `docs/audits/AUDIT_FO4_2026-04-23.md` Dim 6

## Problem
FO4 CTDA = 32 bytes (vs Skyrim 28, FO3/FNV 24). Existing CTDA parsers were
designed around the Skyrim 28-byte stride; FO4 32-byte path not gated.
Audit isn't certain there's an actual bug — just an unverified stride
across the consumers in `actor.rs` + `misc.rs` (PACK, QUST, PERK).

## Risk
Dormant — no runtime consumer of conditions yet (perk/quest system
deferred). But future condition evaluator on FO4 records would read
garbage for the `reference` FormID field.

## Fix scope
1. Audit existing CTDA reads — what stride do they use?
2. Gate on `GameKind` if stride differs.
3. Sibling: Skyrim SE 28-byte vs Starfield (if it changed again).
4. Corpus regression on FO4 QUST + PACK + PERK.
