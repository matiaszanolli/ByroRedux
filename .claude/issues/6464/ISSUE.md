# FO4-2026-10-27-01: FO4 "Use Land Data" child worldspaces are excluded from parent-landscape inheritance; 29 LAND-less content cells have no ground

Labels: medium, terrain-exterior, legacy-compat, game:fo4, bug
Source: docs/audits/AUDIT_FO4_2026-10-27.md

# FO4-2026-10-27-01: FO4 "Use Land Data" child worldspaces are excluded from parent-landscape inheritance; 29 LAND-less content cells have no ground

**Source**: `docs/audits/AUDIT_FO4_2026-10-27.md` finding `FO4-2026-10-27-01` (severity MEDIUM, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/cell_loader/load_order.rs` - `needs_exterior_processing` (matches `Oblivion | Fallout3NV` only), `apply_exterior_processing`, and the `exterior_ready` computation in the shared-parse cache (same predicate, #6407); `crates/plugin/src/esm/cell/index.rs` `inherit_parent_landscapes` (its doc says the caller gates on game).

## Description
A WRLD whose `PNAM` has bit 0x01 set ("Use Land Data", `PNAM_INHERIT_LAND`) takes its landscape from its `WNAM` parent. #5505 (Oblivion) and #6089 (FO3) measured that such children author no LAND of their own and that their grids sit on top of the parent's terrain, and the gate was widened game by game. FO4 was never measured, so on FO4 `needs_exterior_processing` returns false and `inherit_parent_landscapes` never runs. This is the FO4 leg of #6089 / #5505 (both closed, other games; not a regression of either).

## Evidence
Census over the vanilla FO4 masters (the only FO4 WRLDs with the PNAM bit set; all carry PNAM 0x001F; DLCCoast/DLCRobot/DLCworkshop03 have none):

| Worldspace | Plugin | Parent | Cells | Own LAND | LAND-less cells with refs |
|---|---|---|---:|---:|---:|
| `Goodneighbor` 0x00054BD5 | Fallout4.esm | Commonwealth 0x3C | 109 | 0 | 14 |
| `NukaWorldMarket` 0x01053C58 (plugin-local) | DLCNukaWorld.esm | NukaWorld 0x0100290F | 59 | 0 | 8 |
| `NukaWorldAmphitheater` 0x01052931 | DLCNukaWorld.esm | NukaWorld | 63 | 2 | 7 |

Co-location test (#6089's method): compare each child STAT REFR's `DATA` z with the parent's `VHGT` height at the same (x, y); parent LAND exists at every sampled point.

| Worldspace | Co-located median \|dz\| | Shifted-control medians |
|---|---:|---|
| NukaWorldMarket | **69** | 448 / 1,783 / 2,694 |
| NukaWorldAmphitheater | **168** | 713 / 1,476 / 1,878 |
| Goodneighbor | 786 | 624 / 752 / 1,595 |

The two Nuka-World results are decisive (same strength as FO3 Oasis, 166 vs 2,480). Goodneighbor is a dense city so its median is inconclusive, but 14.2% of its 6,106 statics sit within 48 units of the parent terrain vs 0.8-4.0% for the far controls. Code: `needs_exterior_processing` is `matches!(game, GameKind::Oblivion | GameKind::Fallout3NV)`.

## Impact
Entering Nuka-World Market or the Amphitheater, and very likely Goodneighbor, gives 15-29 content cells no terrain mesh and no ground collision. The player and NPCs depend on placed statics, or fall. The parent-keyed distant LOD ring does not cover these cells because the child authors a CELL at those coordinates.

## Related
#6089, #5505, #6407 (one predicate feeds both the step and `exterior_ready`), #6043.

## Suggested Fix
Add `GameKind::Fallout4` to `needs_exterior_processing` (one gate also covers the shared-parse cache). Add an `--ignored` FO4 census pin over the three worldspaces, e.g. asserting the fill count is at least 29. Confirm Goodneighbor visually before relying on it. Update the `apply_exterior_processing` doc comment to list FO4.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

