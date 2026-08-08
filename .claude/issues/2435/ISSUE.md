# COORD-2: Door/XTEL transition rotation bypasses the --rotation-mode dispatcher while its doc comment claims it uses it

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2435
**Finding ID**: COORD-2 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — Coordinate-system correctness
**Location**: `byroredux/src/cell_loader/transition.rs:160-166`
**Status**: NEW

## Description
`rotation_zup_to_yup_quat` is documented as "wrapper over `euler_zup_to_quat_yup_refr` — same convention REFR placements use", but the body calls the plain canonical `euler_zup_to_quat_yup`, not the A/B dispatcher. It is the one caller converting a REFR-sourced Euler triple (XTEL teleport-destination rotation) that skips the dispatcher; all other REFR-family sites (`references/mod.rs:392`, `refr.rs:498`, `placement_lod.rs:171`) do use it.

## Evidence
Body vs. doc-link mismatch confirmed by direct read: the function calls `super::euler_zup_to_quat_yup(rot[0], rot[1], rot[2])`, not `euler_zup_to_quat_yup_refr`.

## Impact
Zero at the shipping default (mode 1 ≡ canonical). Under `--rotation-mode 0/2/3` the player lands at a door with an orientation from a different convention than the surrounding geometry — exactly the scenario the diagnostic flag exists to triage.

## Suggested Fix
Call `euler_zup_to_quat_yup_refr` to match the doc and the rest of the REFR family, or rewrite the comment to state the deliberate pin.

## Completeness Checks
- [ ] **TESTS**: Existing door-transition tests still pass; add a rotation-mode-sweep assertion if none exists
