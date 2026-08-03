# FNV-D7-01: activate_ragdoll double-composes the bone transform — every ragdoll seeded in the wrong place

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 7 (PHYSAL Ragdoll — FNV Reference Slice), finding FNV-D7-01.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2336
Labels: high, legacy-compat, bug

**Severity**: HIGH
**Dimension**: Dimension 7 — PHYSAL Ragdoll (FNV reference slice; PHYSAL-wide, not FNV-specific)
**Location**: `byroredux/src/ragdoll.rs:239-240` (`activate_ragdoll`), `crates/nif/src/import/collision/ragdoll.rs:64-88` (`extract_ragdoll`), `crates/nif/src/import/types.rs:971-974` (`ImportedRagdollBody` doc comment)

## Description

`activate_ragdoll` seeds each Rapier body's world pose as:

```rust
// byroredux/src/ragdoll.rs:239-240
let translation = gt.translation + gt.rotation * (b.local_translation * gt.scale);
let rotation = gt.rotation * b.local_rotation;
```

i.e. `world = bone_GlobalTransform ∘ body_local`, on the documented assumption
that `ImportedRagdollBody.translation`/`.rotation` is bone-relative:

```rust
// crates/nif/src/import/types.rs:971-974
/// Rigid-body origin offset relative to the host bone (Y-up, scaled).
pub translation: Vec3,
/// Rigid-body orientation relative to the host bone (Y-up).
pub rotation: Quat,
```

But the import path (`extract_ragdoll`, `crates/nif/src/import/collision/ragdoll.rs:64-88`)
stores the *raw* Havok rigid-body CInfo transform, converted Havok→engine
space and havok-scaled, with no derivation against the host bone's rest
transform. On real content this raw transform is numerically identical
(within ~1 unit / 360° double-cover) to the host bone's own rest-world
transform — confirmed across all 18 bodies in the FNV `_male\skeleton.nif`
ragdoll, and confirmed on Oblivion (18/18) and Skyrim SE (18/18) skeletons
too. It is not bone-relative as documented.

## Evidence

Probing the real FNV/Oblivion/Skyrim `skeleton.nif` ragdoll data: for every
body, the imported `translation`/`rotation` already equals the host bone's
rest-world transform. Composing it again with `gt` (the bone's live
`GlobalTransform`) therefore double-applies the bone transform.

## Impact

Every ragdoll the engine builds today is seeded in the wrong place — on FNV
this puts the multibody root (`Bip01 NonAccum`) roughly 68 units from the real
pelvis, rotated into the pelvis's own frame. The #1616 round-trip test cannot
catch this: it round-trips the same wrong offset, so a body that hasn't moved
still passes. PHYSAL-wide (Oblivion/FO3/FNV/Skyrim all share this import →
activation path), not FNV-specific.

## Suggested Fix

Treat the imported body transform as skeleton-root-space instead of
bone-relative — either resolve `bone_rest_world⁻¹ ∘ havok_body_transform` at
the translate boundary (`extract_ragdoll`), keeping the existing seed/writeback
math in `activate_ragdoll` unchanged, or seed from the actor root instead of
per-bone. Add a real-data assertion that a seeded body's world position lands
within a few units of its host bone's rest-world position.

## Validation

CONFIRMED against current code (verified independently twice — direct grep/read
plus a background corroboration pass). No open-issue duplicate found.
