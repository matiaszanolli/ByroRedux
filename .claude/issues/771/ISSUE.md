# Issue #771: LC-D3-NEW-01 — `SkinnedMesh::compute_palette_into` drops `global_skin_transform`

**Severity**: MEDIUM · **Domain**: nif-parser, animation, legacy-compat · **Type**: bug
**Source audit**: docs/audits/AUDIT_LEGACY_COMPAT_2026-04-30.md
**Related**: M41.0 Phase 1b.x (commits 8ec6a69, 4177e06, 41aed79); #767 (NiSkinData field-order on disk — fixed)

## Summary

Gamebryo's per-bone palette formula is `boneToWorld × inverse(global) × inverse(boneBind)`. Redux currently composes only the per-bone term — `world * bind_inv` at `crates/core/src/ecs/components/skinned_mesh.rs:137`. The `global_skin_transform` field is captured at import (`scene.rs:1735`) and stored on `SkinnedMesh` (line 55) but never multiplied in.

## Self-acknowledged in code

`scene.rs:1729-1734` documents that a prior right-multiply attempt looked visually worse and was reverted. Likely root cause: OSG row-vec ↔ glam column-major translation order error.

## Investigation step (NOT a one-line fix)

1. Pick a real skinned NIF where global_skin_transform is non-identity (Doc Mitchell head/body).
2. Write a numeric invariant test against the OpenMW formula.
3. Establish ground truth before re-attempting the multiplication.

## Pairs with #772

D5-NEW-03 (NPC AnimationPlayer not attached) likely dissolves once this is closed — bind-pose mismatch may be the same root cause.
