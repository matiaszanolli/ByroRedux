# EXT-D2-2026-09-27-01: LAND `cover_affinity` is classified from the TXST diffuse path; the keyword table is an LTEX editor-ID table, so every NoGrass / …Grass variant is inverted

**Issue**: #4899
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: high,terrain-exterior,bug,esm-plugin

**Severity**: HIGH. This is a wrong canonical value out of the EXAL LAND translate.
**Dimension**: Terrain, splatting
**Tier Violated**: no-fabrication
**Game Affected**: Oblivion, FO3, FNV, Skyrim, FO4
**Status**: NEW
**Location**:
- `byroredux/src/cell_loader/terrain.rs:324` (`layer_affinity(texture_path.unwrap_or(""))`).
- `byroredux/src/groundcover_translate.rs` (`SUPPRESSION_KEYWORDS` / `AFFINITY_KEYWORDS`).
- `byroredux/src/groundcover_translate_tests.rs:79-88`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `exal-groundcover.md:590-600` derives the table "by tokenising every editor ID". Its first rule is that a `NoGrass` suffix suppresses cover.
- Production passes the diffuse path instead. `EsmCellIndex` carries no LTEX editor ID.
- NoGrass variants reuse the grassy sibling's texture, so suppression never matches a real path.
- `oblivion_icon_paths_resolve_like_editor_ids` asserts on a made-up path (`Dementia\DementiaMoss01NoGrass.dds`); the real ICON is `Dementia\DementiaMoss01.dds`.

## Evidence
- A census of editor ID versus real path, run through a replica of `layer_affinity`, finds disagreements on 34/229 Oblivion LTEXs, 6/51 FO3, 15/88 FNV, 18/67 Skyrim and 47/105 FO4.
- NoGrass records scoring above 0: Oblivion 26, FNV 1, Skyrim 14, FO4 14. Examples:
  - `LFieldGrass01NoGrass` → `Landscape\FieldGrass01.dds`: 0 → 0.95.
  - `CHTerrainGrass01NoGrass`: 0 → 0.95.
- Reverse cases:
  - `ChemicalBarrenWastes01Grass`: 0.95 → 0.02.
  - `LSnowRocks01wGrass`: 0.95 → 0.03.

## Impact
- Paths and building pads authored as NoGrass get maximum grass.
- Grassy variants of barren ground get none.

## Suggested Fix
- Carry LTEX editor IDs through the ESM boundary as `EsmCellIndex.landscape_texture_names`, merged in `merge_from`.
- Classify on the editor ID. Oblivion has editor IDs too, so no per-game split is needed.
- Replace the made-up-path test with a real editor-ID/path pair.

## Related
#4054, EXT-D2-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
