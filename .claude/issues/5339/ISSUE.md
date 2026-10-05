# #5339: EXT-D5-2026-10-05-04: A failed distant-water rebuild leaves the stale mesh and retries a full-worldspace scan every frame; an emptied rebuild leaves a stale `water.dump` histogram

**Labels**: low,terrain-exterior,water,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5339

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D5-2026-10-05-04` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:1237-1255` (`None => return false` and the upload-error `return false`, both before `plane.center_grid = player_grid`); `:1222-1230` (emptied path).
  - `byroredux/src/streaming.rs:998-1000` (`if plane.center_grid == player_grid { return; }`).
- **Status**: NEW (introduced by #5243's rebuild path; distinct from PERF-D7-2026-10-05-02's per-crossing cost)
- **Tier Violated**: n/a
- **Game Affected**: all with LOD water
- **Description**:
  - On an upload failure (e.g. a device-memory OOM), `rebuild_lod_water_mesh` returns `false` without advancing `center_grid` and keeps the old mesh. The old mesh's hole sits around the previous grid, so the old ring now overlaps the new full-detail cells.
  - `recenter_lod_water` sees `center_grid != player_grid` on every following frame. Each retry re-walks every worldspace cell, folds 1,089 heights per cell, and attempts another blocking upload.
  - Separately, the emptied path removes `MeshHandle` but never touches `WaterLodInfo.quad_heights` / `quad_height_count`, so `water.dump` keeps reporting the last non-empty histogram.
- **Evidence**: Code path as cited. The info update at `:1266-1272` runs only on the success path.
- **Impact**:
  - Under memory pressure: a per-frame CPU spike plus failing allocations, and the old ring double-drawing over full-detail water at mismatched hole positions.
  - In the empty case: misleading diagnostics.
- **Suggested Fix**:
  - On failure, record the attempted grid, or back off, so the rebuild runs again only on the next real crossing.
  - Consider dropping the stale mesh rather than keeping a mis-holed one.
  - Zero `quad_height_count` on the emptied path.

## Publisher note

Same rebuild path as #5289 (PERF-D7-2026-10-05-02, the per-crossing cost); this issue is the failure/empty-path behaviour only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
