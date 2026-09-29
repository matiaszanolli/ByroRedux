# #5098: SKY-D5-2026-09-29-01: ROADMAP's compat matrix and game-compatibility.md still give Skyrim SE as 33,424 NIFs across 7 archives and have no Skyrim LE row; the gate measures 33,468 across 8, plus 22,466 LE

**Labels**: documentation, nif-parser, low, legacy-compat, game:skyrim, doc-rot

**Source report**: `docs/audits/AUDIT_SKYRIM_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: Archives + corpus gates (reporting)

## Location
- `ROADMAP.md` compat matrix, Skyrim SE row ("100% (**33 424** across 7 archives … 2026-08-29)").
- `ROADMAP.md` project-stats line ("Skyrim SE 33 424").
- `docs/engine/game-compatibility.md` Skyrim SE row ("33 424, 7 archives").

## Description
#3712 added `Skyrim - Animations.bsa` (44 NIFs) to `Game::optional_mesh_archives` (`crates/nif/tests/common/mod.rs`). The gate now sweeps 8 archives and 33,468 NIFs; the delta is exactly those 44. The authoritative matrix still says "33 424 across 7 archives". It also has no Skyrim LE row, although `parse_rate_skyrim_le` has gated 22,466 LE NIFs at 100% since `fb8173fe0`.

## Evidence
This audit's `parse_rate_skyrim_se`: 33,468 / 33,468 across 8 archives (Meshes0 18,862, Meshes1 13,847, _ResourcePack 149, CC 231/266/65/4, Animations 44). `parse_rate_skyrim_le`: 22,466 / 22,466.

Validated at HEAD 9fcfdc3fc: all three "33 424" sites are present; the ROADMAP compat matrix has no Skyrim LE row.

## Impact
Cosmetic. The authoritative matrix gives a stale SE count and does not record the LE gate.

## Related
- NIF-D3-2026-09-29-02 (refreshes `nif-parser.md` and FO76/FO4 rows, not these lines), #3712.

## Suggested Fix
In the next `/session-close`: set the SE row, the stats line and `game-compatibility.md` to 33,468 / 8 archives, and add a Skyrim LE row (BSA v104 zlib, 22,466 / 22,466, 1 archive).

## Completeness Checks
- [ ] **SIBLING**: The seven-game NIF total on the stats line recomputed after the SE change

