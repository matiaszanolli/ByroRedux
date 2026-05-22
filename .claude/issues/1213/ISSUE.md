**Severity**: HIGH
**Dimension**: Scene Graph Decomposition
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D1-NEW-02

`LocalBound` and `WorldBound` exist in `crates/core/src/ecs/components/` and the bounds propagation system documents itself as:

> **Leaf bounds** — for every entity with a `LocalBound` (set at import time), project the local sphere into world space via `GlobalTransform`, store as `WorldBound`.

But **no import-time site sets `LocalBound`**:

```
$ grep -rn "LocalBound::new\|insert.*LocalBound" byroredux/src/ crates/nif/src/
byroredux/src/systems/bounds.rs:188  — test fixture only
```

`ImportedMesh` from the NIF importer carries `local_bound_radius` per shape (used at `spawn.rs:844` for the small-STAT escalation heuristic), but the spawn loop never converts that into a `LocalBound` component row.

### Evidence
- `byroredux/src/systems/bounds.rs:43-66` iterates `world.query::<LocalBound>()` and computes `WorldBound` from it. On a fresh cell load the query is empty.
- `byroredux/src/systems/bounds.rs:127-153` Pass 2 (interior nodes) merges children's `WorldBound`. Empty leaves → empty interiors.

### Impact
- **Culling**: any future frustum / portal / occlusion cull reads `WorldBound` and sees zero spheres.
- **RT shadow / GI budgeting**: importance-sorted shadow budget (#270) and distance-based ray fallback (#271) fall back to entity position only.
- **CellRoot bound aggregation** (the comment in `byroredux/src/cell_loader/load.rs:196-199` notes "the cell's reference bounds are not yet aggregated"): water-plane centering, LOD selector, "cell bounding sphere for culling" all stay stubbed until this lands.

### Suggested Fix
At `byroredux/src/cell_loader/spawn.rs:639-647` insert `LocalBound::new(mesh.local_center, mesh.local_bound_radius)` alongside the Transform / GlobalTransform pair. The bounds-propagation system already handles the rest.

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Same insert at `scene/nif_loader.rs` (loose-NIF) and `cell_loader/precombined.rs` (precombined-spawn, when CSG geometry lands).
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Integration test that loads a fixture cell and asserts at least one `WorldBound` row exists post-load.
