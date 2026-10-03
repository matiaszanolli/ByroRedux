# #5170: EXT-D1-2026-10-02-02: Starfield WATR noise-UV tile sizes stay unlifted beside the lifted lanes with no tracking issue (#5151 closed with the deferral in a code comment only)

**Labels**: low,terrain-exterior,water,bug,game:starfield
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (unit lift completeness)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:156-158` (deferral comment);
  - `crates/plugin/src/esm/records/misc/water.rs:1348-1357` (`noise_uv_scale_{a,b,c}` at 120/124/128);
  - `byroredux/src/env_translate.rs:806-814` (clamp `[1/4096, 1/8]`).
- **Status**: NEW. #5151's issue body says "settle it with a capture before lifting", but #5151 is closed, and no open issue tracks this (searched "noise UV Starfield" and "tile sizes water Starfield capture").
- **Tier Violated**: no-fabrication (a unit is chosen implicitly: the tile is read as BU).
- **Game Affected**: Starfield.
- **Description**:
  - Every other Starfield DNAM length is lifted. The three tile sizes (vanilla 72.11 / 39 / 13 m-or-BU) are inverted as BU, so the primary noise tile repeats every ~1 m.
  - Read as metres, the tile is 5 048 BU, which the translate clamp would cap at 4 096.
  - FO76, the same layout in BU, authors 279 / 168 / 56.
  - The Starfield-only `displacement` (72/76/80) and `normal_falloff` (52/56/60) lanes have the same unsettled status and are not listed anywhere.
- **Impact**: If the lanes are metric, Starfield water normals tile ~70× too finely. Today nothing records that an open decision exists.
- **Suggested Fix**: File a tracking issue (capture-gated) covering the tile sizes and the other Starfield DNAM lanes with an unclassified unit. List them in watal.md's per-game unit table.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
