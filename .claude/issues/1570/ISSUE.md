**Severity**: LOW · **Dimension**: ESM + Cell Bring-up (dead code / doc rot)
**Location**: `byroredux/src/components.rs:107-126`; `byroredux/src/cell_loader/precombined.rs:24-25`; query/count sites `byroredux/src/render/static_meshes.rs:135,205` and `byroredux/src/commands.rs:105,140`
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D5-01)

## Description
When R6a-stale-14 converted the synthesized-trimesh collider to a separate ghost entity (`spawn.rs:1073-1080`), the `IsCollisionOnly` insert was removed. `grep` confirms it is never inserted anywhere — defined, queried, counted, but the query is always empty and the `static_meshes.rs:205` gate is dead. The `components.rs:107-126` comment and `precombined.rs:24-25` cross-ref still describe the OLD pattern ("the entity keeps its `MeshHandle`… Set in `crate::cell_loader::spawn` after `synthesize_static_trimesh`") — exactly what the ghost pattern replaced.

## Evidence
`grep -rn "IsCollisionOnly" byroredux/src crates` → only the decl (`components.rs:123`), two query/count sites (`commands.rs:105,140`, `static_meshes.rs:135`), and stale doc cross-refs; **zero insert sites**. Live spawn site spawns a no-MeshHandle ghost; `physics-only (no MeshHandle)` is the real diagnostic count.

## Impact
No functional bug — collider-cost fix is correctly achieved via the absent MeshHandle. Pure doc-rot + dead code; the stale comment misleads anyone reasoning about why synthesized colliders stay out of BLAS.

## Related
ROADMAP R6a-stale-14; #1317/#1324 (other dead code, different scope).

## Suggested Fix
Delete `IsCollisionOnly` and its query/count/import sites and rewrite the comments to point at the ghost-entity pattern (`spawn.rs:1073`); or, if retained for a future combined path, replace the doc body with "currently unused; synthesized colliders use a MeshHandle-free ghost entity instead."

## Completeness Checks
- [ ] **SIBLING**: The `commands.rs` "expect 0" diagnostic and the `static_meshes.rs` BLAS-exclusion gate are both updated/removed in lockstep with the marker
- [ ] **TESTS**: No test asserts a non-zero `IsCollisionOnly` count (would pin the dead behaviour)
