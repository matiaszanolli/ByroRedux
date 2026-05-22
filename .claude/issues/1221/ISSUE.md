**Severity**: HIGH (forward-looking — blocks CSG-reader milestone)
**Dimension**: Transform Compatibility (call-site placement)
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D3-NEW-02

Today's #1188 added the precombined-spawn call to the interior loader only (`byroredux/src/cell_loader/load.rs:152-159`). The exterior loader in `byroredux/src/cell_loader/exterior.rs` passes `&cell.absorbed_refs` to `load_references` (line 308) — which is correct for the fallback gate — but never invokes `super::precombined::spawn_precombined_meshes`.

When the CSG reader lands and `pc_spawned > 0` becomes the common case, the exterior loader will need the same precombined-spawn pass plus the same conditional-absorption gate. Today it's silent (paired with #(D3-NEW-01) it's correct-by-accident), but the wiring gap is structural.

### Evidence
```
$ grep -n "spawn_precombined" byroredux/src/cell_loader/*.rs
byroredux/src/cell_loader/load.rs:148
byroredux/src/cell_loader/load.rs:155
byroredux/src/cell_loader/precombined.rs:54
```

No exterior.rs call site.

### Impact
When CSG support arrives, exterior cells will skip the precombined-spawn path and render per-REFR — the FO4 headline performance feature missing on the cells it was designed for.

### Suggested Fix
Add the same Phase-3a precombined call + conditional-absorption gate to `exterior.rs`. Coord-frame consideration:
- Each exterior cell's `cell.precombined_mesh_hashes` paths are keyed by the cell's form_id.
- Exterior cell origin in world space is `(cell_x * 4096, 0, cell_y * 4096)`.
- The interior call passes `Vec3::ZERO + Quat::IDENTITY` — correct because the cell origin IS the world origin.
- The exterior call must pass `Vec3::new(cell_x * 4096.0, 0.0, cell_y * 4096.0)` so the bake lands in correct world-space position.

This depends on `spawn_precombined_meshes` growing a `cell_origin: Vec3` parameter (see D3-NEW-03).

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Interior loader at `load.rs:147-179`.
- [ ] **TESTS**: Exterior cell load test that asserts the same absorption gate fires (precombined spawn = 0 today → REFRs rendered).
