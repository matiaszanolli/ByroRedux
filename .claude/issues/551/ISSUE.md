# NIF-06: bhkBlendController missing from dispatch (1,427 FNV+FO3 blocks)

**Severity**: HIGH | **Dimension**: Coverage Gaps | **Game**: FO3, FNV | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-06

## Summary
`bhkBlendController` drives blend weights between multiple Havok animations (nif.xml: `NiTimeController` subclass with a single `Keys: NiFloatInterpolator` ref). Affects ragdoll blending and animation layering on FO3/FNV NPCs.

## Evidence
FNV 845, FO3 582.

## Location
`crates/nif/src/blocks/mod.rs` — no dispatch arm.

## Suggested fix
Thin wrapper around `NiSingleInterpController::parse` (same shape) + dispatch arm. ~15 LOC.

## Completeness Checks
- [ ] **TESTS**: Synthetic fixture + dispatch test
- [ ] **REAL-DATA**: FNV + FO3 unknown sweeps drop `bhkBlendController` to 0

Fix with: /fix-issue <number>
