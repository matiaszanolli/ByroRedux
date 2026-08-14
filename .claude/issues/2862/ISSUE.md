# PHYS-D1-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2862

---

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW (same class as #1409 / #1779, both CLOSED — those covered radii/extents and vertices, not the compound child TRS)
**Location**: `crates/nif/src/import/collision/shape.rs:252-259`, `crates/nif/src/import/collision/mod.rs:513-548`, `crates/physics/src/convert.rs:117-123` + `:57-59`

## Trigger Conditions
A `bhkTransformShape` whose 16 raw `f32` matrix words contain a NaN or +/-Inf — corrupt, truncated-then-misaligned, or adversarial content. Vanilla content will not fire it; a mod or a mid-stream desync will.

## Description
Every other arm of `resolve_shape_inner` funnels its output through `finite()` / `finite_vec()` or an explicit `is_finite()` sweep — `Ball` (`:89`), `Cuboid` (`:149`), `Capsule` (`:155-159`), `Cylinder` (`:168-171`), `ConvexHull` (`:188`), `MultiSphere` (`:118`). The `BhkTransformShape` arm alone calls `decompose_havok_matrix` and emits its `(translation, rotation)` straight into a `Compound` child with **no guard**:

```rust
// crates/nif/src/import/collision/shape.rs:253
if let Some(s) = block.as_any().downcast_ref::<BhkTransformShape>() {
    let child = resolve_shape(scene, s.shape_ref, visited)?;
    let (translation, rotation) = decompose_havok_matrix(&s.transform, scale); // unguarded
    return Some(CollisionShape::Compound { children: vec![(translation, rotation, Box::new(child))] });
}
```

`BhkTransformShape::parse` reads all 16 words with no validation (`crates/nif/src/blocks/collision/shape_compound.rs:87-92`), and `decompose_havok_matrix` only `.normalize()`s — which propagates NaN rather than rejecting it. Downstream, `flatten_to_parts`'s `Compound` arm passes `(*t, *r)` to `iso_from_trs` with no check — the same function hardened for vertices (#1779) and Cuboid extents (#2543) has no equivalent guard on the child TRS. `quat_to_na` uses `UnitQuaternion::new_normalize`, which yields a NaN quaternion for NaN input.

## Impact
A NaN collider isometry gives Rapier's broad-phase a NaN AABB — the same corruption mode #1779 was filed for, but reached through the transform rather than the vertex buffer. It silently poisons proximity/ray queries for the entire island, and unlike the vertex case there is **no tiny-ball fallback to absorb it**. Debug builds will trip glam's internal assertions in `Quat::normalize`; release builds will not.

## Suggested Fix
Return `None` from the `BhkTransformShape` arm when `!translation.is_finite() || !rotation.is_finite()` (dropping to the trimesh fallback, matching the `ConvexVertices` precedent), and add the release-profile backstop in `flatten_to_parts`'s `Compound` arm so the choke point holds for every producer, not just this one.

## Related
- #1409 (CLOSED, radii/half-extents), #1779 (CLOSED, TriMesh vertices), #2543 (CLOSED, Cuboid extent clamp), #1534 (CLOSED, ragdoll pose finite guards)
