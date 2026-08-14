# PHYS-D1-05

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2878

---

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/convert.rs:84-85` (doc) vs `:191-217` (code)

## Trigger Conditions
A `CollisionShape::TriMesh` reaching the physics crate with an index >= `vertices.len()`. Not reachable from today's producers (both are guarded), so this is a defense-in-depth + doc-accuracy gap, not a live crash.

## Description
The function's doc comment states `TriMesh` *"falls back to a tiny ball on empty mesh **or if trimesh construction fails**"*. The code implements only the empty/non-finite fallback (`:197-201`); the construction call is unconditional (`:216`).

Construction **cannot report failure** to be caught:
- `SharedShape::trimesh_with_flags` returns `Self`, not a `Result` (`parry3d-0.17.6/src/shape/shared_shape.rs:203-209`)
- `TriMesh::with_flags` **panics** on an empty index buffer (`assert!`, `parry3d-0.17.6/src/shape/trimesh.rs:320-323`) and indexes `self.vertices[idx as usize]` unchecked thereafter

The `#1779` comment at `:192-196` calls this *"the single choke point every TriMesh source passes through"*, which makes the missing bounds check conspicuous: the two producers each carry their own copy of the guard instead (`finish_trimesh` at `crates/nif/src/import/collision/shape.rs:704-706`, `synthesize_static_trimesh` at `byroredux/src/cell_loader/spawn.rs:350-352`).

## Impact
A future TriMesh producer that skips its own bounds check panics inside `physics_sync_system` mid-frame rather than degrading to a ball. `spawn_collision_shapes`'s `catch_unwind` (`spawn.rs:1070`) would not help — it wraps only the `Clone`, and the conversion happens later in the sync system.

## Suggested Fix
Move the index-range `retain` from `finish_trimesh` down into the `TriMesh` arm so the choke-point claim is true, and correct the doc comment to describe the fallback that actually exists.

## Related
- #2285 (CLOSED — `finish_trimesh` per-buffer index bounds), #2552 (OPEN — the stale `catch_unwind` comment at the same producer), #1779
