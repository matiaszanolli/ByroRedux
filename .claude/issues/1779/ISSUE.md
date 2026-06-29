**Severity**: LOW · **Dimension**: Collision Import (Havok → CollisionShape) · **Status**: NEW
**Location**: `crates/nif/src/import/collision.rs` — `resolve_packed_mesh` (~892), `resolve_tri_strips_data_refs` (~833), `resolve_compressed_mesh` (~983); consumer `crates/physics/src/convert.rs:156-176`
**Source**: `docs/audits/AUDIT_FO3_2026-06-28.md` (D5-01)

## Description

The #1409 (NIFAL-S4) hardening added a `finite()` / `finite_vec()` guard to every *primitive* shape resolver — sphere, box, capsule, cylinder, convex hull, multi-sphere — so a NaN/±Inf scalar from a corrupt or truncated Havok decode drops the authored primitive to the synthesized-trimesh fallback instead of poisoning the parry3d/Rapier collider. The three *mesh* resolvers were never brought into that sweep: `resolve_packed_mesh`, `resolve_tri_strips_data_refs`, and `resolve_compressed_mesh` build their `CollisionShape::TriMesh { vertices }` from raw `read_f32_le` / dequantized values with **no finite check**, and only bail on `vertices.is_empty()`. The downstream consumer `collision_shape_to_parts` (`convert.rs:156`) likewise guards only `vertices.is_empty() || indices.is_empty()` before calling `SharedShape::trimesh_with_flags`.

## Evidence

- Finite-guard grep over `collision.rs` shows guards on all primitive arms (`finite(...)` / `finite_vec(...)` at the body CInfo + ragdoll joint paths, #1534/#1409) but **none inside the three TriMesh builders** — verts are pushed via `all_verts.push(havok_to_engine(...))` / a dequantize loop with no `is_finite`. The empty-only bails are at `collision.rs:877` / `:897` / `:1063`.
- `parry3d::shape::trimesh::with_flags` asserts only `!indices.is_empty()` and then builds a Qbvh from per-triangle AABBs; the default `ContactConfig` flags include `MERGE_DUPLICATE_VERTICES`, whose spatial-hash bucketing on NaN coordinates is undefined. A NaN vertex therefore yields NaN AABB bounds → corrupted broadphase, not a clean drop.
- The "finite sweep" referenced in the NIFAL audit (`walk/mod.rs`) covers particle-emitter scalars only, not collision mesh vertices — verified, it does not defend this path.

## Impact

FO3 leans heavily on `bhkPackedNiTriStripsShape` (MOPP→PackedNiTriStrips is the dominant static-architecture collider) and `bhkNiTriStripsShape`, so the missing-guard surface is exactly FO3's most-used collision path. Real-world reachability is low: vanilla FO3 BSA content is finite, so this only fires on corrupt/truncated/adversarial NIFs — which is precisely the threat model #1409 was written for. When it does fire, the failure (a poisoned broadphase / Qbvh with NaN bounds) is *worse* than the clean fallback the primitives get; it can manifest as a whole cell's physics misbehaving rather than one missing collider. LOW because it does not drop *translatable* vanilla content — it is an unfinished corner of the #1409 hardening on FO3's hottest shape kind.

## Suggested Fix

- Mirror the primitive guard in the three mesh builders: after building the vertex list, `if all_verts.iter().any(|v| !v.is_finite()) { return None; }` so the synthesized-trimesh fallback fires instead — identical posture to `BhkConvexVerticesShape`.
- Belt-and-suspenders: add the same `is_finite` filter at `convert.rs:156` before `trimesh_with_flags`, since that is the single choke point all TriMesh sources pass through (including the synth fallback).
- This is part of the NIFAL canonical-translation tier (collision); see `/audit-nifal`.

## Completeness Checks
- [ ] **SIBLING**: All three TriMesh resolvers (`resolve_packed_mesh`, `resolve_tri_strips_data_refs`, `resolve_compressed_mesh`) get the guard, plus the `convert.rs:156` choke point covering the synth fallback.
- [ ] **CANONICAL-BOUNDARY**: The guard stays at the NIFAL parser→`CollisionShape` boundary (collision.rs) and the physics convert choke point — no NaN-handling pushed into Rapier/render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test feeds a non-finite vertex through each resolver and asserts `None` (clean fallback), not a built `TriMesh`.
