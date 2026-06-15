# #1597 — FO4-D4-LOW-01: No half_to_f32 IEEE 754 edge-case unit test (regression guard gap)

**Severity**: LOW · **Dimension**: NIF BSVER 130 + Half-Float + Collision
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D4-LOW-01)
**Location**: `crates/nif/src/import/mesh/decode.rs:18` (`half_to_f32`)

## Description
`half_to_f32` decode is correct today for every binary16 class, but no unit test exercises the NaN / denormal / Inf edge cases. The half-float path is the core of FO4 vertex decode (positions/normals default to half on BSVER ≥ 130); a future refactor of the bit-twiddling could silently regress denormal/NaN handling with no test to catch it.

## Evidence
grep for half-float tests finds normal-value round-trips only; no NaN/denormal/Inf assertion. All `half_to_f32` consumers route through the one canonical impl (no drift), so a single test pins all of them.

## Impact
Code-correctness today, missing regression guard. No content impact (decode is correct).

## Related
FO4 half-float vertex path (`bs_tri_shape.rs`).

## Suggested Fix
Add a unit test asserting `half_to_f32` against known binary16 → f32 values incl. +0/−0, smallest denormal, largest denormal, ±Inf, a NaN payload, and a normal mid-range value.

## Completeness Checks
- [ ] **SIBLING**: The single canonical `half_to_f32` pins all consumers (verify no second decode impl drifted in)
- [ ] **TESTS**: A regression test asserts +0/−0, smallest/largest denormal, ±Inf, NaN payload, and a normal value
