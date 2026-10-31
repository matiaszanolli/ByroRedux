# SKY-2026-10-31-D4-01: Skyrim's seven "Use Land Data" child worldspaces never inherit Tamriel's landscape: 308 exterior cells load with no terrain or ground collision

Labels: medium,terrain-exterior,esm-plugin,game:skyrim,legacy-compat,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM
**Dimension**: Skyrim / Multi-master + TES5 cell load (exterior; mechanism owner `/audit-exterior` + `/audit-esm`)
**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-31.md` SKY-2026-10-31-D4-01
**Status**: same mechanism as #5505 (Oblivion), #6089 (FO3) and #6464 (FO4), all closed; no open issue covers Skyrim. Distinct from EXT-D1-2026-10-31-01 (the `exal.md` doc-rot for the same sentence after #6464), which does not measure Skyrim.

## Location
`byroredux/src/cell_loader/load_order.rs:607-615` (`needs_exterior_processing`, Oblivion | Fallout3NV | Fallout4 only); test `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` (`load_order.rs` ~:1959-1967) asserts `!needs_exterior_processing(GameKind::Skyrim)` with the comment "Skyrim has not been measured"; stale doc `docs/engine/exal.md:148-154` ("not measured on the Skyrim+ children"). Re-verified at HEAD `670f81917`.

## Description
`inherit_parent_landscapes` copies the `WNAM` parent's LAND into a child's own LAND-less cells at the same grid, for worldspaces whose `PNAM` has bit 0x01 (Use Land Data). Skyrim stamps `PNAM` (all children parent Tamriel `0x3C`), and the parse already stores `parent_flags` (`cell/wrld.rs:116`). But the pass runs only for games in `needs_exterior_processing`, and Skyrim is not one, so the step and the `exterior_ready` re-parse are skipped.

## Evidence
Python byte-walk of `Skyrim.esm`, LAND matched to its preceding CELL's `XCLC`, `VHGT` decoded to heights, median |dz| in game units, 33x33 samples:

| Child worldspace | PNAM | Exterior cells | With own LAND | LAND-less | Parent LAND exists for LAND-less |
|---|---|---|---|---|---|
| WhiterunWorld | 0x7f | 113 | 7 | 106 | 106 |
| WindhelmWorld | 0x7f | 57 | 2 | 55 | 55 |
| RiftenWorld | 0x77 | 29 | 12 | 17 | 17 |
| SolitudeWorld | 0x7f | 62 | 20 | 42 | 42 |
| WhiterunDragonsreachWorld | 0x7f | 48 | 0 | 48 | 48 |
| WindhelmPitWorldspace | 0x5f | 29 | 1 | 28 | 28 |
| KatariahWorld | 0x57 | 12 | 0 | 12 | 12 |
| **Total** | | | | **308** | **308** |

Co-location, child LAND vs Tamriel LAND at the same grid, against the same cell shifted by one cell or (2,2):

| Child | Same grid | Shifted control |
|---|---|---|
| Whiterun (7 cells) | 0 | 520 |
| Windhelm (2) | 488 | 2376 |
| Riften (12) | 24 | 364 |
| Solitude (20) | 364 | 1316 |
| WindhelmPit (1) | 0 | 312 |

Whiterun and the pit are identical to Tamriel; the others are Tamriel plus city edits. The grids are co-located as in Oblivion/FO3. Update/Dawnguard/Dragonborn/HearthFires only re-author these worlds with a few extra LAND-less cells.

## Impact
A Skyrim city exterior (`--wrld WhiterunWorld`, or a gate door transition into it) renders with no terrain in 106 of 113 cells and has no ground collision there (placed statics keep their own colliders). The FO3 equivalent (#6089) was the same defect. Interiors, Tamriel itself and the `WhiterunDragonsreach` interior bench are unaffected, which is why the control bench cannot see this. Not verified in a running engine (suite rule); the claim rests on the data census and the unchanged gate.

## Suggested Fix
Add `GameKind::Skyrim` to `needs_exterior_processing`, flip the Skyrim assertion in the gate test, add an `#[ignore]`d `Skyrim.esm` test (WhiterunWorld: 106 cells filled), and update `exal.md:148-154` with this measurement. Confirm visually on Whiterun gate approach. Check the SE-only `WhiterunDragonsreachWorld` and `KatariahWorld` worlds separately: they have no LAND of their own, so any gap there is the loader's. Note related cost: enabling this adds the interior-to-exterior re-parse described by the FO4 case (see the PERF D7-02 tracker).

## Related
#5505, #6089, #6407, #6464, EXT-D1-2026-10-31-01

## Completeness Checks
- [ ] **SIBLING**: SE-only WhiterunDragonsreachWorld and KatariahWorld checked separately
- [ ] **TESTS**: `#[ignore]`d Skyrim.esm test (WhiterunWorld: 106 cells filled) plus the flipped gate assertion
