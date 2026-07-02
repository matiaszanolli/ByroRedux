# FNV-D7-04: Ragdoll writeback uses live gt.scale in the inverse while the seed captured scale at activation — drift if a bone is rescaled mid-sim

**Source audit**: `docs/audits/AUDIT_FNV_2026-07-02.md` (finding FNV-D7-04)
**GitHub issue**: https://github.com/matiaszanolli/ByroRedux/issues/1852
**Labels**: low, legacy-compat, bug

**Severity**: LOW
**Dimension**: PHYSAL Ragdoll
**Location**: `byroredux/src/ragdoll.rs:190` (seed) vs `byroredux/src/ragdoll.rs:327` (writeback inverse)
**Status**: NEW

## Description

The activation seed composes the body world pose with the scale read *at activation*: `translation = gt.translation + gt.rotation * (b.local_translation * gt.scale)`. The per-frame writeback inverts it using the bone's *current* `gt.scale`: `gt.translation = t - bone_rotation * (tb.local_translation * gt.scale)`. If a ragdoll bone's `GlobalTransform.scale` changes between activation and a later frame, the offset term de-composes with a different scale than it was composed with, displacing the bone by `local_translation * Δscale`.

## Evidence

Seed at `ragdoll.rs:190` reads `gt.scale`; writeback at `ragdoll.rs:327` reads `gt.scale` again from a fresh `get_mut` — not guaranteed equal across frames. The #1616 round-trip test only validates the same-scale case (`scale: 1.0`, no step), so it can't surface this.

## Impact

Latent — vanilla ragdoll bones ship uniform, constant scale, so the two reads agree and the term is exact. Only a gameplay system that animates ragdoll-bone scale during simulation (shrink/enlarge FX) on a body authored with a non-zero `bhkRigidBodyT` local offset would surface a small positional wobble. Correctness hardening, not an active bug.

## Suggested Fix

Snapshot the seed-time scale into the body spec at activation and use that stored value in the writeback inverse, so compose/decompose always use the identical scale regardless of live `GlobalTransform` mutation. Alternatively, document the "ragdoll bones must not be rescaled while active" invariant next to the writeback math.

## Completeness Checks
- [ ] **SIBLING**: Check other compose/decompose pairs in the physics layer (e.g. skin-palette bone identity path) for the same live-vs-snapshotted-value assumption
- [ ] **TESTS**: A regression test steps a ragdoll bone's `GlobalTransform.scale` between activation and a writeback tick and asserts the bone lands at the correct position
