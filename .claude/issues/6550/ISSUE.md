# EXT-D6-2026-10-31-01: the fix for the 10-27 `holes[i]` out-of-bounds panic has no regression guard

Labels: low,terrain-exterior,bug,test-gap

Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-31.md` (finding `EXT-D6-2026-10-31-01`).

- **Severity**: LOW (test-gap). The defect is fixed; nothing prevents its return.
- **Dimension**: 6 Distant LOD and trees
- **Tier Violated**: n/a
- **Game Affected**: every game with terrain LOD (Oblivion, FO3, FNV, Skyrim, FO4)
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1116-1125` (`append_lod_skirts`, then `vertex_bound(.., |i| holes.get(i).copied().unwrap_or(false))`); `byroredux/src/cell_loader/lod_support.rs:1284` (`vertex_bound_ignores_skipped_vertices`)
- **Status**: NEW. This is a guard gap behind the fixed EXT-D6-2026-10-27-01. The fix landed in `5af77a128`, an audit-report commit, with no issue number.
- **Description**: The 10-27 crash came from calling the shared `vertex_bound` with a closure indexing `holes`, which has one entry per grid sample, after `append_lod_skirts` had pushed extra vertices. `spawn_lod_block` needs a `VulkanContext`, so no unit test reaches that call. The only test that pairs `vertex_bound` with a hole mask, `vertex_bound_ignores_skipped_vertices`, builds a mask exactly as long as its vertex list. That is the shape that cannot fail, and it would stay green if someone restored `|i| holes[i]`. The fix is a single inline `.get(i)` with no named helper to pin.
- **Evidence**:
  ```rust
  append_lod_skirts(&mut vertices, &mut indices, &skirt_edges);   // vertices.len() = n*n + 2*E
  ...
  let bound = vertex_bound(&vertices, |i| holes.get(i).copied().unwrap_or(false));
  ```
  ```rust
  let holes = [false, false, true];            // same length as `verts`
  let b = vertex_bound(&verts, |i| holes[i]);
  ```
- **Impact**: A later refactor of the bound call (it has already been rewritten twice: `block_bound`, then the shared `vertex_bound`) can reintroduce a main-thread panic on every skirted block. The panic would reach the same path as before: `stream_lod_blocks` on the main thread, with no `catch_unwind`.
- **Suggested Fix**: Extract `terrain_block_bound(vertices: &[Vertex], holes: &[bool]) -> WorldBound` with the `get(i).unwrap_or(false)` rule, and call it from `spawn_lod_block`. Add a test with `vertices.len() > holes.len()` (for example the output of `append_lod_skirts` on a 2x2 grid). Also assert that a hole vertex placed far away does not stretch the sphere while a skirt vertex does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
