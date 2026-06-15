# D7-03: Dynamic trimesh ragdoll body has degenerate inertia (advisory)

**Issue**: #1540 · **Severity**: LOW (advisory) · **Labels**: low, legacy-compat, bug
**Source**: AUDIT_FNV_2026-06-14 (D7-03) · **Status when filed**: NEW, CONFIRMED

## Location
- `crates/physics/src/convert.rs:156-176`
- `crates/physics/src/ragdoll.rs:110-124`

## Description
If a ragdoll body's shape resolves to `CollisionShape::TriMesh` (possible when a bone hosts a `bhkPackedNiTriStripsShape`/compressed mesh rather than a primitive), `build_ragdoll` attaches a trimesh collider to a dynamic body. Rapier's mass/inertia for a possibly-open trimesh is unreliable; `.mass(part_mass)` sets total mass but not a sane inertia tensor, so the link can spin pathologically.

## Evidence
`convert.rs:156` `CollisionShape::TriMesh { vertices, indices }` branch present; `ragdoll.rs:110-124` build path attaches resolved shape to a dynamic body with a `.mass(part_mass)` override.

## Impact
Potential jitter/spin on the rare trimesh-shaped ragdoll bone (mods, creatures). Vanilla FNV bones almost always author capsules/boxes — likely doesn't fire on stock content, hence advisory.

## Suggested Fix
For ragdoll bodies, prefer a convex-hull or bounding-capsule substitution when the resolved shape is a `TriMesh`, or set a principal-inertia tensor from the collider AABB.

## Related
#1534 (PHYSAL ragdoll extraction NaN finite guards) — adjacent (bad mass/inertia reaching Rapier) but distinct: #1534 is NaN from NIF decode, this is a degenerate-but-finite tensor from a valid trimesh.
