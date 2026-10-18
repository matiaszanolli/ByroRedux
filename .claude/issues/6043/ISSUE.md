# ESM-2026-10-18-EXT-08: exterior CELL `XCLC` Land Flags (Hide Quad 1..4) are dropped

Labels: low,esm-plugin,terrain-exterior,bug,game:skyrim,game:fo4
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW. About 70 vanilla cells.
- **Dimension**: CELL / WRLD Walkers
- **Record / Sub-record**: `CELL` / `XCLC`
- **Location**: `cell/wrld.rs:412-426` reads the two `i32`s only
- **Status**: NEW
- **Description**: `wbCellGrid` is `X s32, Y s32, Land Flags u8, unused[3]` (`wbDefinitionsCommon.pas:8690`, `wbLandFlags`: Hide Quad 1 to 4). The flags byte is ignored, so terrain is drawn for quadrants the content hides.
- **Evidence**: Skyrim: 21 cells at 15 (all quads hidden), 3 at 12, 2 at 13, 1 each at 14, 5, 4, 3, 1. FO4: 38 at 15 plus 4 odd values. FO3/FNV third dword is uninitialised padding with a zero low byte, so the byte is safe on every game.
- **Suggested Fix**: add `land_hide_quads: u8` to `CellData` and consume it in the terrain spawner.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
