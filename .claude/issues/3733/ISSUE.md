# #3733: NIFAL-2026-08-30-D1-02: ESM-sourced water planes are the one cell-loader mesh spawner outside the #2444 boundary guard, so their draws shade against the hardcoded-literal arm

**Labels**: bug, low, water, nifal
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIFAL_2026-08-30.md` · **Severity**: LOW · **Dimension**: 1 (Material) · **Tier violated**: single-boundary
**Game affected**: all (every exterior/interior CELL with an XCWT / LOD water plane)

## Location
- `byroredux/src/cell_loader/water.rs` — the two `MeshHandle` spawn sites (currently `:481` CELL water plane, `:813` LOD water plane)
- `byroredux/src/material_translate.rs` — `every_exterior_spawner_inserts_a_boundary_material`, the guard whose table does not list them

## Description
#2444 established the invariant "every drawn surface's canonical material is produced at one boundary" and pinned it with the source-scan guard `every_exterior_spawner_inserts_a_boundary_material`, whose own comment says to *"keep this table in step with `cell_loader`'s spawners."* **The table lists five.**

`cell_loader/water.rs` is a sixth cell-loader spawner that inserts a `MeshHandle` and attaches `WaterPlane` + `WaterMaterial` but **no canonical `Material`** — and it carries no documented exemption.

This matters because water is not drawn by a separate collection pass: `byroredux/src/render/water.rs` looks up an **already-emitted** `DrawCommand` for each `WaterPlane` entity and merely flips `is_water = true`. So these entities do pass through `collect_static_mesh_draws`, hit the `else` arm in `byroredux/src/render/static_meshes.rs`, and are interned with `roughness 0.5`, `metalness 0.0` and the default IOR — the same 11-tuple of literals #2444 was filed to eliminate.

Contrast the NIF-sourced water path, which does not have this gap: `material_translate.rs::attach_mesh_water` attaches a canonical `Material` from `translate_material` alongside `WaterPlane`. The two water producers therefore disagree about whether a water surface has a canonical material — the "two paths silently diverge" shape the boundary exists to prevent.

## Evidence
`grep -n "MeshHandle" byroredux/src/cell_loader/water.rs` → `:481`, `:813` (re-verified 2026-08-30); the same file's only material construction is `material: WaterMaterial::default()`. The guard's array enumerates `cell_loader/terrain.rs`, `terrain_lod.rs`, `object_lod.rs`, `placement_lod.rs`, `terrain_lod_btr.rs` and stops.

## Impact
Bounded today — `water.frag` derives its optics from the WATR-driven `WaterMaterial` and never reads `mat.roughness`/`mat.metalness` (verified: no such read in `crates/renderer/shaders/water.frag`), so the primary water pass is unaffected.

The exposure is the secondary/RT path, which reads the interned `GpuMaterial` generically (`triangle.frag` shades a hit against `hitMat.roughness`/`hitMat.metalness`/`hitMat.ior`), and the durable defect is that **the guard no longer enumerates the spawner set it claims to**.

## Related
#2444 (MAT-D3-02), #3336 (added `terrain_lod_btr` to the same table for the same reason), `docs/engine/watal.md`.

## Suggested Fix
Either route both water spawn sites through `translate_texture_only_material` (they have a bound normal/noise texture path) and add `cell_loader/water.rs` to the guard's table, or add it to the table with an explicit third `boundary_fn` marker recording the deliberate `WaterMaterial`-only exemption. **The guard must enumerate the spawner, either way.**

## Completeness Checks
- [ ] **SIBLING**: re-derive the guard's table from the actual spawner set rather than adding one row — a seventh spawner would recur otherwise
- [ ] **CANONICAL-BOUNDARY**: the fix must produce the material at the boundary, not re-derive optics at render time. See `/audit-nifal`.
- [ ] **TESTS**: the existing source-scan guard is the test — it must fail before the fix and pass after
