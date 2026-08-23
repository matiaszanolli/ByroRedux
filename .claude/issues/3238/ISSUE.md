# 3238: SAFE-D9: Ball/Capsule/Cylinder Rapier conversion has no upper-bound clamp (Cuboid got #2543, siblings didn't)

**Severity**: HIGH · **Dimension**: Safety Dimension 9 (NIFAL Boundary — NaN/Inf / unbounded values reaching a live subsystem) · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-23.md` (SAFE-D9-2026-08-23-01)

## Description

`crates/physics/src/convert.rs` is documented as "the single choke point every producer's output passes through before reaching Rapier" (comment at `:190-192`). Issue #2543 (closed, HIGH, this same dimension) fixed exactly this choke point for `CollisionShape::Cuboid`: an astronomically large but finite half-extent — reachable from an unclamped upstream scale on a corrupt/adversarial NIF — used to reach `SharedShape::cuboid()` unbounded, handing Rapier's broadphase an effectively-infinite AABB overlapping the entire scene. The fix added a `clamp_lane` helper (`MAX_SANE_SHAPE_EXTENT = 1_048_576.0`) that floors non-finite lanes to `1e-3` and clamps finite lanes to `[1e-3, MAX_SANE_SHAPE_EXTENT]`, with a dedicated regression test.

The sibling arms in the same `match` — `Ball` (`:205-207`), `Capsule` (`:245-256`), and `Cylinder` (`:257-268`) — were never given the equivalent ceiling clamp. Each only applies `.max(1e-3)` (a floor, inherited from before #2543), with no upper bound.

## Evidence

```rust
// crates/physics/src/convert.rs — Ball/Capsule/Cylinder arms, no ceiling
CollisionShape::Ball { radius } => {
    out.push((parent_iso, SharedShape::ball((*radius * scale).max(1e-3))));
}
CollisionShape::Capsule { half_height, radius } => {
    out.push((parent_iso, SharedShape::capsule_y(
        (*half_height * scale).max(1e-3),
        (*radius * scale).max(1e-3),
    )));
}
CollisionShape::Cylinder { half_height, radius } => {
    out.push((parent_iso, SharedShape::cylinder(
        (*half_height * scale).max(1e-3),
        (*radius * scale).max(1e-3),
    )));
}
```
vs. `Cuboid`'s `clamp_lane` at `:208-244` and its dedicated test `huge_finite_cuboid_extent_clamps_to_sane_ceiling` (`:521-534`). No `huge_finite_ball_radius…`/`…capsule…`/`…cylinder…` test exists alongside it.

`Ball` is fed directly by `BhkSphereShape` and every sphere in `BhkMultiSphereShape`; `Capsule`/`Cylinder` are fed by `BhkCapsuleShape`/`BhkCylinderShape`. All four producers only guard *finiteness* at the NIF import boundary (`crates/nif/src/import/collision/shape.rs`'s `finite()`/`finite_vec()`) — exactly the posture `Cuboid`'s producer had before #2543.

## Impact

A corrupt-but-finite radius/half-extent (e.g. `1e30`, corrupt-but-legal per IEEE 754) on any `BhkSphereShape`/`BhkMultiSphereShape`/`BhkCapsuleShape`/`BhkCylinderShape` reaches Rapier's broadphase as an unbounded collider — the same "AABB overlapping the entire scene" failure #2543 rated HIGH for `Cuboid`. Once live, every other collider in the scene reports spurious contact pairs against it, corrupting the physics-driven `Transform` updates that feed the per-frame GPU instance buffer for the whole scene, not just the offending entity — an all-scene blast radius from a single malformed collision block.

## Related

#2543 (the sibling `Cuboid` fix this generalizes).

## Suggested Fix

Reuse `Cuboid`'s `clamp_lane` (or a shared `clamp(1e-3, MAX_SANE_SHAPE_EXTENT)` helper) for the `radius` lane in `Ball`, and for both `half_height`/`radius` lanes in `Capsule`/`Cylinder`, mirroring the non-finite-floors-to-`1e-3`/finite-ceilings-to-`MAX_SANE_SHAPE_EXTENT` posture already proven for `Cuboid`; add a `huge_finite_{ball,capsule,cylinder}_*_clamps_to_sane_ceiling` test per shape alongside the existing `Cuboid` regression test.

## Completeness Checks
- [ ] **SIBLING**: Same clamp pattern applied to all three remaining shape arms, not just one
- [ ] **TESTS**: A dedicated regression test per shape, matching `huge_finite_cuboid_extent_clamps_to_sane_ceiling`
