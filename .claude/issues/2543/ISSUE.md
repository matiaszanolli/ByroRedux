# SAFE-2026-08-07-01: synthesize_packed_havok_proxy can build an unbounded/infinite collider from unclamped REFR scale; the only guard is a debug_assert! compiled out of release builds

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2543
**Finding ID**: SAFE-2026-08-07-01

**Severity**: HIGH
**Dimension**: 9 (NIFAL Boundary — NaN/Inf / unbounded values reaching a live subsystem)
**Location**: `byroredux/src/cell_loader/spawn.rs:90-193` (`transformed_mesh_aabb`, `synthesize_packed_havok_proxy`, `spawn_packed_havok_proxy`); consumed at `crates/physics/src/convert.rs:117-133` (`flatten_to_parts`, `CollisionShape::Cuboid` arm)
**Status**: NEW

## Description
`716b7ee9` ("improve packed collision compatibility") added a proxy path: when a placement authors FO4+/FO76/Starfield packed collision (`BhkNPCollisionObject`, opaque `BhkSystemBinary`) with no decoded collision shape, on a `Clutter`/`Actor`-layer object, the cell loader synthesizes a conservative `CollisionShape::Cuboid` from the render-mesh AABB:
```rust
// spawn.rs:150
let half_extents = ((max - min) * 0.5 * ref_scale.abs()).max(Vec3::splat(0.5));
```
`ref_scale` is the placement's REFR `XSCL` scale, read as a raw, **unclamped** `f32` off disk (`crates/plugin/src/esm/cell/walkers.rs:680-682`: `scale = r.f32().unwrap_or(1.0)`), threaded through unmodified as `outer_scale` into `ref_scale` here. `synthesize_packed_havok_proxy` checks only `ref_scale.is_finite()` — it does not bound the *magnitude* of `ref_scale`, and does not re-check the *computed* `half_extents` after the multiply. A large-but-finite `ref_scale` produces an unbounded `half_extents`; an `f32` overflow produces a literal `Infinity` that still passes `is_finite()` upstream of the multiply (the check runs on the input, not the product) and is inserted directly into the ECS as `CollisionShape::Cuboid { half_extents }` with no further validation. This breaks the idiom used at every other Cuboid-construction site in the codebase — e.g. `BhkBoxShape` (`crates/nif/src/import/collision/shape.rs:139-150`) wraps its computed half-extents in `finite_vec(half_extents)?` before returning. `synthesize_packed_havok_proxy` is the one new call site in this diff range that skips that pattern.

The only remaining backstop is inside the physics shape flattener:
```rust
// crates/physics/src/convert.rs:117-125
CollisionShape::Cuboid { half_extents } => {
    debug_assert!(
        half_extents.is_finite()
            && half_extents.x >= 0.0 && half_extents.y >= 0.0 && half_extents.z >= 0.0,
        "canonical Cuboid half-extents must be finite non-negative magnitudes, got {half_extents:?}"
    );
    out.push((parent_iso, SharedShape::cuboid(
        half_extents.x.max(1e-3), half_extents.y.max(1e-3), half_extents.z.max(1e-3),
    )));
}
```
`debug_assert!` is compiled out of `cargo build --release` (this project's documented release build). In release, `Infinity.max(1e-3) == Infinity`, so `SharedShape::cuboid(Infinity, Infinity, Infinity)` (or an astronomically large finite equivalent) is handed to Rapier3D unfiltered.

## Evidence
Confirmed directly at `spawn.rs:145-151` (no post-multiply finite/bounds check) and `convert.rs:117-130` (debug-only guard).

## Impact
A malformed or crafted ESM plugin with an extreme `XSCL` on a `Clutter`/`Actor` REFR referencing an FO4+/FO76/Starfield mesh with opaque packed Havok collision and no other decoded collider triggers this path. In a release build the resulting collider has effectively-infinite (or merely astronomically large) half-extents, spawned as a live kinematic body parented into the world. Rapier3D's broad-phase AABB tree then treats this collider as overlapping essentially everything in the scene, corrupting collision queries/contact generation engine-wide for the running session — not just for the one bad placement. Real, reachable, engine-wide physics-integrity regression from a genuinely new feature, gated only by a build-profile-dependent assert.

## Related
Introduced by `716b7ee9`. Falls into the Physics/PHYSAL coverage gap noted in `_audit-common.md`'s "Un-owned subsystems" table — nothing else in the audit rotation checks Rapier-bound shape parameter bounds outside the NIF-import boundary itself.

## Suggested Fix
In `synthesize_packed_havok_proxy`, replace the bare `.max(Vec3::splat(0.5))` with a `finite_vec(half_extents)?`-style check (return `None` on non-finite, matching every other shape-construction site), and clamp the upper bound to a sane ceiling (e.g. a multiple of the cell's expected extent, or whatever ceiling the `Architecture` trimesh fallback already assumes) so a corrupt-but-finite `ref_scale` can't produce a degenerate collider. Promoting the `convert.rs` `debug_assert!` to a real runtime clamp (matching the `Ball`/`Capsule`/`Cylinder` arms' `.max(1e-3)` pattern, plus an upper bound) would additionally close this class of gap for any future unguarded `CollisionShape::Cuboid` producer, not just this one call site.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a REFR with an extreme `XSCL` through `synthesize_packed_havok_proxy` and confirms the result is either `None` or clamped to a sane ceiling
- [ ] **SIBLING**: All other `CollisionShape::Cuboid` producers checked for the same `finite_vec`-style pattern
- [ ] **UNSAFE**: N/A (no unsafe code touched by the fix)
