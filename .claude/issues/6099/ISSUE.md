# FO4-SD-03: `prebaked_facegen_tint_path` encodes the Skyrim layout; FO4 ships no `facegendata\facetint` files

Labels: low,import-pipeline,bug,game:fo4,game:starfield,legacy-compat

Canonical issue for FO4-SD-03 (FO4 report) and SF-SD-05 (Starfield report): the same dead probe, same fix. Affected: Fallout 4 and Starfield (Skyrim genuinely ships `facegendata\facetint`).

## Description
`prebaked_facegen_tint_path` (`byroredux/src/npc_spawn.rs`) probes `textures\actors\character\facegendata\facetint\<plugin>\<id>.dds`, the Skyrim layout; the probe is called from `byroredux/src/npc_spawn/resumable/prebaked.rs:148` (consumed near `:296`). FO4's facegeom NIFs reference their own textures inline: probing the shipped `FaceGeom\Fallout4.esm\0001A00C.NIF` yields `Data\Textures\Actors\Character\FaceCustomization\Fallout4.esm\0001A00C_d.dds` / `_msn.dds` / `_s.dds` on the head shape, and no `facetint` name exists in any FO4 texture archive (name-table walk of `Fallout4 - Textures1..3.ba2` and `DLCCoast - Textures.ba2` returns only `FaceCustomization\...`). Starfield: listing the names of every `.ba2` in the Starfield `Data` folder, none contains `facetint` (Starfield tints faces another way). So on FO4 and Starfield `tint_path` is always `None` after one wasted archive miss per NPC, and the function's "Skyrim / FO4 / FO76 / Starfield" docblock is wrong for the texture half.

## Impact
No rendering effect (the inline texture path wins, and `select_facegen_diffuse` only fires for Skyrim skin-tint shapes); one failed archive lookup per pre-baked NPC plus misleading docs.

## Related
FO4-SD-01 (facegeom/facetint FormID segment; independent, filed separately), #2095, #4421.

## Suggested Fix
Gate the tint probe on `GameKind::Skyrim` (or make the path a per-game helper) and correct the docblock; no behaviour change expected.

Source: docs/audits/AUDIT_FO4_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
