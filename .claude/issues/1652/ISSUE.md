# D5-1: bhk motion_type → MotionType mapping is wrong vs canonical Havok enum

**Issue**: #1652
**Source audit**: docs/audits/AUDIT_FO3_2026-06-18.md (HEAD `2aac5351`)
**Severity**: HIGH · **Labels**: high, nif-parser, legacy-compat, bug
**Dimension**: 5 — Collision (Havok → CollisionShape)
**Location**: `crates/nif/src/import/collision.rs:155-159`

## Description
`bhkRigidBody.motion_type` is mapped with `1..=3 => Dynamic`, `4 => Keyframed`,
`_ => Static`. This contradicts the canonical Havok `hkMotionType` enum
(nif.xml:1109-1121): BOX_INERTIA (4) should be Dynamic but becomes Keyframed;
BOX_STABILIZED (5), KEYFRAMED (6), THIN_BOX (8) all fall into `_` and become Static.

## Evidence
- Mapping confirmed at `collision.rs:155-159`.
- Consumed: `byroredux/src/scene/nif_loader.rs:421-431` → `crates/physics/src/sync.rs:215-222`
  (`motion_type_to_rapier`). `push_kinematic` gated `!= Keyframed` (sync.rs:361);
  `pull_dynamic` gated `!= Dynamic` (sync.rs:411).
- BOX_INERTIA (4) crate → KinematicPositionBased → frozen with infinite mass.
- KEYFRAMED (6) door → Static → Fixed → never tracks transform.

## Impact
Physics-correctness regression across all Havok-content games (FNV/FO3/Oblivion/Skyrim).
Full bhk dynamic-body population. Not CRITICAL: no crash/corruption; FIXED=7 still correct.

## Related
Distinct from #1540, #1539. Not #1598 (MOVS).

## Suggested Fix
`1..=5 | 8 => Dynamic`, `6 => Keyframed`, `7 => Static`, `9 => CharacterKinematic`,
`0 | _ => Static`. Regression test for 4, 5, 6, 8. `MotionType` variants confirmed at
`crates/core/src/ecs/components/collision.rs:67`.
