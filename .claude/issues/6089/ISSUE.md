# FO3-SD-03: FO3 child worldspaces with "Use Land Data" draw no ground; #5505's landscape inheritance is Oblivion-only

Labels: medium,terrain-exterior,esm-plugin,bug,game:fo3,legacy-compat

**Severity**: MEDIUM · **Dimension**: 3 (worldspace context, terrain) · **Game**: FO3

## Description
WRLD `PNAM` bit `0x01` ("Use Land Data") makes a child worldspace use its `WNAM` parent's landscape. Four FO3 worldspaces carry it: `Oasis` (`0x0005`), `ParadiseFalls` (`0x003d`), `GNRroofWorld` (`0x0007`), `MegatonWorld` (`0xffc5`), all with parent `Wasteland`. Their cells are authored without LAND. #5505 added the landscape inheritance but gated it to Oblivion, so on FO3 those cells spawn their references with no terrain mesh and no ground collision.

## Evidence
Location: `byroredux/src/cell_loader/exterior.rs:1795-1805` (`if record_index.game == GameKind::Oblivion { inherit_parent_landscapes() }`), `crates/plugin/src/esm/cell/index.rs:358-425` (the function; its doc says "FO3+ child grids are not assumed to share the parent's coordinates"). Re-confirmed present at HEAD.

Raw scan of `Fallout3.esm` (`LAND` attributed to the preceding `CELL`):

| Worldspace | cells | LAND-less | of those with refs | parent LAND at same (x,y) |
|---|---:|---:|---:|---:|
| Oasis | 34 | **34** | 7 | 34 |
| ParadiseFalls | 13 | 11 | 10 | 11 |
| GNRroofWorld | 143 | 58 | 4 | 58 |
| MegatonWorld | 16 | 5 | 1 | 5 |
| total | 206 | 108 | 22 | 108 |

The co-location test #5505's author asked for: static objects of the child cells against the parent's `VHGT` terrain height at the same position. `Oasis`: median |dz| **166** units vs **2,480** for a control shifted by (+7,+5) cells. `ParadiseFalls`: **46** vs **4,126**. So both towns stand on the Wasteland terrain at those coordinates. `GNRroofWorld` (1,547 vs 936) and `MegatonWorld` (431 vs 717) are inconclusive (rooftop and offset geometry), not disproved. Disproof attempted: LOD ring is parent-keyed only for coordinates the child has no CELL for; the cells are not hidden-quad cells (#6043 is about Hide Quad flags); the PNAM bit values have bit 0 set and the parent chain exists. Not verified live.

## Impact
Entering Paradise Falls or the Oasis puts the player in a world whose ground is missing (22 cells with placed content have none).

## Related
#5505 (closed; Oblivion leg, which "measured on Oblivion only"), `EsmCellIndex::inherit_parent_landscapes`, EXT-D6 sibling (terrain LOD hole), #6043.

## Suggested Fix
Run the inheritance for `Fallout3NV` too (the CS rule is the same), keep the cell's own LAND winning, and pin it with an `--ignored` test over `Oasis` / `ParadiseFalls`; confirm the roof and Megaton worlds visually before relying on them.

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
