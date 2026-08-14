# PHYS-D1-01

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2860

---

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: HIGH · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:585-587` (+ `crates/physics/src/convert.rs:57-59`); producers `byroredux/src/cell_loader/spawn.rs:1064-1090`, `byroredux/src/scene/nif_loader.rs:481-504`

> Merges PHYS-D3-03 (Dimension 3), which is the registration-side face of the same defect. PHYS-D4-01 (ragdoll joint pivots) is a *distinct* sibling this fix would not cover.

## Trigger Conditions
Any cell containing a REFR with `XSCL != 1.0` (or a collision-bearing `NiNode` with node scale != 1.0) whose NIF carries decodable classic `bhk` collision — Oblivion / FO3 / FNV / Skyrim, where scaled rocks, rubble and clutter are routine. FO4+/Starfield are unaffected (they take the synth-trimesh path, which bakes scale).

## Description
`spawn_collision_shapes` composes `final_scale = ref_scale * coll.scale` into both `Transform` and `GlobalTransform`, then `register_newcomers` builds the Rapier body from **translation and rotation only**:

```rust
// crates/physics/src/sync.rs:585-586 — final_scale never read again
let mut body_builder = RigidBodyBuilder::new(body_type)
    .position(iso_from_trs(n.global.translation, n.global.rotation))
```

and hands `collision_shape_to_parts` the *unscaled* `CollisionShape`. Nothing in `crates/physics` reads `GlobalTransform::scale` (`grep -rn '\.scale' crates/physics/src/sync.rs` returns nothing). Rapier exposes `SharedShape::scaled` and every primitive here is uniformly scalable, so this is a **dropped** value, not an unrepresentable one.

The checklist's three acceptable outcomes — reject / convert to TriMesh / explicitly document — are all unmet. The bhk path is the only one of three collider producers that does not pre-bake scale:
- `synthesize_static_trimesh` multiplies every vertex by `world_scale` (`byroredux/src/cell_loader/spawn.rs:340-343`)
- `spawn_packed_havok_proxy` passes `ref_scale` through (`byroredux/src/cell_loader/spawn.rs:263`)
- `spawn_collision_shapes` does neither.

Note: the engine's `Transform`/`GlobalTransform` scale is a scalar `f32`, so the *non-uniform* case the usual guidance warns about cannot arise. The uniform case is dropped instead.

## Impact
Colliders are the wrong size relative to the geometry they represent on every scaled placement — a 2x rock has a half-size collider (player clips into visible stone), a 0.5x one has an oversized invisible wall.

**Worse for multi-part collision**: `compose_trs` *does* scale each part's position, so a multi-node assembly on a scaled REFR gets its parts spread apart while each keeps its original size — literal gaps open between adjacent colliders that a KCC or dynamic body passes through.

Blast radius is every classic-chain game and every scaled placement. Invisible to `cargo test`: no test exercises a non-unit scale through the collider boundary.

## Suggested Fix
Bake the uniform scale at the single `collision_shape_to_parts` boundary (multiply primitive dims / vertex sets, and scale composed child translations during the compound flatten), or wrap each emitted part in `SharedShape::scaled`. Pass `GlobalTransform::scale` into `collision_shape_to_parts` explicitly so the drop cannot recur silently. Add a regression test that a `ref_scale = 2.0` cuboid emits doubled half-extents, and state the convention in `docs/engine/physics.md` beside the existing note at `:383-384` so all three producers document one rule.

## Related
- `docs/engine/physics.md:379-390` (the packed-Havok fallback bullets, which *do* bake scale)
- #2543 (CLOSED — clamped `ref_scale` on the synth proxy path)
- PHYS-D4-01 (ragdoll pivots — sibling, separate fix)
