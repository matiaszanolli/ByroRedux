# #6330: SF-2026-10-21-D2-01: #6200's loose `Data\` tier misses Starfield's asset shapes — no `geometries` root, and a loose `materialsbeta.cdb` is never discovered (nor could its index build re-open a directory source)

**Labels**: low,bug,import-pipeline,game:starfield,legacy-compat

**Source**: docs/audits/AUDIT_STARFIELD_2026-10-21.md · **Severity**: LOW · **Dimension**: 2 (BSGeometry `.mesh` resolution) / 3 (CDB discovery) · **Location**: `byroredux/src/asset_provider/archive.rs:47` (`LOOSE_ASSET_ROOTS = ["textures", "meshes", "materials"]`)

## Location
- `byroredux/src/asset_provider/archive.rs:47` (`LOOSE_ASSET_ROOTS = ["textures", "meshes", "materials"]`)
- `byroredux/src/asset_provider/archive.rs:524` (`normalize_mesh_path` keeps `geometries\` unprefixed)
- `byroredux/src/asset_provider/material/provider.rs:73-77, 101-105` (the loose tier is pushed, never passed to `discover_starfield_cdbs`)
- `byroredux/src/asset_provider/material/cdb.rs:162-172` (`build_cdb_material_index` → `Archive::open_shared(source)`)

## Status at filing
NEW (refines a gap the provider's own comment acknowledges as "documented, LOW" since #2621, which #6200 was positioned to close and did not). Distinct from PAR-D6-01 / PAR-D6-02 (loose indexer symlink depth / case folding).

## Description
#6200 appends a loose tier that indexes only `textures\`, `meshes\` and `materials\` under `Data`. That misses two Starfield shapes.
1. **No `geometries` root.** Starfield's external mesh data lives at `geometries\<X>.mesh` (#1292 keeps that root unprefixed). A loose `Data\geometries\…` override is never indexed, so a loose NIF override (which *is* indexed, under `meshes\`) that names new `.mesh` bodies misses every Stage-B slot. The shape is then dropped (`debug!` only).
2. **No loose CDB discovery.** `discover_starfield_cdbs` runs only on the `--materials-ba2` and `--bsa` archive arms. A loose `Data\materials\materialsbeta.cdb` or `materials\creations\<plugin>\materialsbeta.cdb` is never registered. Even if it were, `build_cdb_material_index` re-opens the source by path through `Archive::open_shared`, which only opens BSA/BA2 files; for a directory it would fail and memoise `None`.

The audit skill's Dim 3 text ("a loose CDB or `.mat` override joins the same scan") is therefore wrong for CDBs. Loose `.mat` files do resolve, through `extract` on the pushed tier.

## Evidence
- `LooseDir::index` filters on `LOOSE_ASSET_ROOTS`.
- `build_material_provider` calls `discover_starfield_cdbs` only at provider.rs:41 and :85 (archive handles).
- `Archive::open` sniffs file magic (archive.rs:171-200).
- The installed Starfield `Data` holds only `video\`, so no vanilla number moves.
- #6200's commit measured FO3, FNV, Skyrim SE and FO4 installs only.

## Impact
mod and user overrides only. A Starfield mesh replacer shipped loose loses its geometry, and a loose CDB-edited material set is ignored in favour of the archived CDB. Vanilla is unaffected.

## Related
#6200, #2621, #1292, PAR-D6-01, PAR-D6-02.

## Suggested Fix
- Add `geometries` to the loose roots, or derive the roots from the game.
- Run `discover_starfield_cdbs` on the loose tier.
- Have the CDB index build extract through the registered handle (or re-open via `Archive::open_loose` for a directory source) instead of `open_shared(path)`.
- Correct the skill's Dim 3 sentence at the next `/audit-sync`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
