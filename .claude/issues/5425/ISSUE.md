# #5425: EXT-D4-2026-10-08-01: EXAL/SKYAL doc rot after `b4497ec1d` / `424dfe0e1` / `1162236fc` / `c60405083` / `56786698b`

**Labels**: low,terrain-exterior,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5425

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW (doc rot)
- **Dimension**: Sky, weather, sun (plus the Dim 1 docs)
- **Location**:
  - `docs/engine/exal.md:128-138` (climate resolution).
  - `docs/engine/exal.md:215-225` (image space).
  - `docs/engine/exal.md:790` (CLMT "carries only WLST").
  - `docs/engine/skyal.md` (no entry for either sky change).
- **Status**: NEW
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: all (docs)
- **Description**:
  1. exal.md says climate "has two inputs, both settled at the boundary in `env_translate`". There are now also the REGN rung, the Oblivion naming / richest rungs (in `cell_loader/exterior.rs`) and the WTHS `DefaultWeather` stand-in.
  2. The image-space description covers IMSP → INAM → identity. It omits the FO3/FNV per-TOD weather IMAD fold, the 6-slot table, and the FO3/FNV IMAD channel remap (`0x12` = contrast pivot, `0x14` = brightness). The `1162236fc` message ("Add examples …") does not mention these either, so `git log` archaeology also misses them.
  3. CLMT now parses `WSLT` (`ClimateRecord.seasonal_weathers`).
  4. skyal.md does not record the sun-disc luminance floor (`sky.glsl:425-430`, the literal `1.8`, a documented engine choice only in the code comment) or the sunset hold key (`weather.rs:95`/`:103`). Both shape every golden-hour capture.
- **Suggested Fix**:
  - Update exal.md's climate paragraph and image-space step 3 to describe the rungs and the IMAD fold, and note `WSLT`.
  - Add a skyal.md entry for the disc floor (with its 1.8 rationale) and the sunset hold.

## Completeness Checks
- [ ] **SIBLING**: watal.md / exal.md cross-references checked for the same stale climate / image-space text
