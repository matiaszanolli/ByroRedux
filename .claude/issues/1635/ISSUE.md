# Issue #1635 — REG-03: water-caustic sun-direction sign (#1459) has no guard test

_Snapshot as filed (immutable). GitHub is authoritative for current state._

**Source:** `docs/audits/AUDIT_REGRESSION_2026-06-14.md` — REG-03 (PARTIAL hardening gap, LOW)

The fix for **#1459** is **present and correct**; this issue tracks the missing guard test, not a regression.

## Description
The sun-direction sign was corrected in both the water shadow/refract path (`water.frag` directional case) and the caustic splat directional branch (`caustic_splat.comp`). The original issue states "shader math not unit-testable; RenderDoc capture is the regression gate."

## Evidence
- `crates/renderer/shaders/water.frag` — directional sun case (sign-corrected)
- `crates/renderer/shaders/caustic_splat.comp` — directional branch (sign-corrected)

## Impact
A sign flip would suppress caustics for an overhead sun again — invisible to `cargo test` (shader math, no host-side harness).

## Suggested Fix
Intentional manual/RenderDoc gate — acceptable as-is. If desired, leave/confirm an explanatory comment at both sites naming the sun-direction sign convention so a future edit is flagged in review.

## Completeness Checks
- [ ] **SIBLING**: Sun-direction sign convention consistent across `water.frag` and `caustic_splat.comp`
- [ ] **TESTS**: Manual/RenderDoc gate documented (shader math not cargo-observable)
