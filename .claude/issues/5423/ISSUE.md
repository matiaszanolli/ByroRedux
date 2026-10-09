# #5423: EXT-D1-2026-10-08-02: Three new `game ==` logic branches bypass the table-shaped scheme functions, and climate resolution is now split across `env_translate.rs` and `cell_loader/exterior.rs`

**Labels**: low,terrain-exterior,bug,tech-debt
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5423

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D1-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW (discipline; behaviour is correct today)
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:1894`: `(record_index.game == GameKind::Oblivion).then(..)`.
  - `byroredux/src/cell_loader/terrain_lod.rs:462`: `let desired = if game == GameKind::Fallout3NV {`.
  - `byroredux/src/streaming_helpers.rs:119`: `&& wctx.record_index.game == GameKind::Fallout3NV`.
- **Status**: NEW (introduced by `b4497ec1d` and `b7987d813`)
- **Tier Violated**: no-render-time-fallback (table-shape rule) and single-boundary (climate)
- **Game Affected**: Oblivion, FO3/FNV (structural)
- **Description**:
  - The skill allows `GameKind` only as table-shaped `match` returning data.
  - `object_lod.rs` correctly gates on `ObjectLodScheme::FalloutLegacyBlocks`. The terrain ring and the index scan instead hard-code `Fallout3NV`, bypassing `terrain_lod_layout` and its FalloutLegacy layout. A future title that adopts the legacy layout would silently take the descent path in one ring and the authored path in the other.
  - The Oblivion naming / richest rungs live beside the cell loader rather than next to `resolve_worldspace_climate` in `env_translate.rs`. exal.md says that is where climate resolution is "settled at the boundary".
- **Suggested Fix**:
  - Gate both rings and the scan on the layout/scheme tables.
  - Move `region_climate_for_center` / `named_or_richest_climate` into `env_translate.rs` as one `resolve_exterior_climate`, with a table-shaped per-game rung list.

## Completeness Checks
- [ ] **SIBLING**: All three `game ==` branches (exterior.rs, terrain_lod.rs, streaming_helpers.rs) moved to the layout/scheme tables together
- [ ] **TESTS**: A regression test pins this specific fix
