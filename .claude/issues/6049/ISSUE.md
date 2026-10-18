# PHYS-EXT-D1-2026-10-18-02: The terrain tile collider's upward winding is load-bearing under `FIX_INTERNAL_EDGES | ORIENTED`, is pinned by no test, and its explanatory comment states the opposite

Labels: low,physics,terrain-exterior,test-gap,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-18.md

- **Severity**: LOW (test gap and doc rot; behaviour is correct by arithmetic)
- **Dimension**: Shape Translation
- **Location**: `byroredux/src/cell_loader/terrain.rs:966-985` (comment `:969-971` and the index loop), `byroredux/src/cell_loader/spawn/synthesize_trimesh_tests.rs` (no orientation test), `crates/physics/src/config.rs:34-49` (`TriMeshFlagBits::DEFAULT = FIX_INTERNAL_EDGES | ORIENTED`)
- **Status**: NEW
- **Description**: The tile is emitted as `(tl, tr, bl), (tr, br, bl)` and the same index buffer is handed to the collider. In the Y-up frame: `tl=(x,h,-y)`, `tr=(x+128,h,-y)`, `bl=(x,h,-y-128)`, so `(tr-tl) x (bl-tl) = (128,0,0) x (0,0,-128) = (0, +16384, 0)`: the triangle normal points up. That is the orientation `ORIENTED` needs for a floor (a mesh with its normals pointing down would treat the walkable side as the interior). The comment says the Z-up to Y-up transform "negates Z, flipping winding" and so the indices are emitted "CW so it becomes CCW". `zup_to_yup_pos` is `(x,y,z) -> (x,z,-y)`, a proper rotation (determinant +1) that does not flip winding, and the emitted order is already CCW seen from above in both frames. The code is right; the reasoning written beside it is wrong, which invites a "fix" that would invert the floor. No test builds a LAND-shaped tile, registers it, and asserts that a downward ray or the KCC grounds on it; the existing ghost tests use one-triangle XZ fixtures and check only AABB/scale/ownership.
- **Evidence**: arithmetic above; `zup_to_yup_pos` at `crates/core/src/math/coord.rs:105`; tests listed in `synthesize_trimesh_tests.rs` (single triangle, floor ghost static/renderer-free, ownership backlink, scale once). The `ORIENTED` semantics were not probed in this pass (read-only, no scratch crate); live walking on exterior terrain (W1 smoke, normal play) is the empirical evidence that the current orientation works.
- **Impact**: None today. A future winding edit (for example to "fix" front-face culling on the render side, since the index buffer is shared) would silently drop the player through every exterior tile with no red test.
- **Trigger Conditions**: a change to the terrain index loop or the shared Y-up conversion.
- **Related**: the `terrain.rs` comment at `:969`, `config.rs` ORIENTED note.
- **Suggested Fix**: correct the comment (rotation, not reflection; the order is CCW from above), and add a test that builds a small LAND-shaped grid through the same index pattern, registers it via `spawn_trimesh_collider_ghost` + `physics_sync_system`, and asserts `cast_ray_down` hits at the surface height and a capsule `move_character` is grounded on it.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
