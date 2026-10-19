# OBL-SD-03: Oblivion placement LOD (DistantLOD\<ws>_x_y.lod) is keyed on the child worldspace only, so city worldspaces get 0 to 16 of the parent's ~900 ring files

Labels: medium,terrain-exterior,bug,game:oblivion,legacy-compat

**Severity**: MEDIUM (needs one on-device confirmation of the inherit intent) · **Dimension**: 4 (exterior, legacy LOD) · **Game**: Oblivion

## Description
#5374 stamps the "use LOD data" inherit bit on all 30 Oblivion children and #5496 routes every other distant-ring key through `lod_worldspace_key()`. Placement LOD (`DistantLOD\<ws>_x_y.lod`) is the one Oblivion-only LOD scheme and still reads only the child worldspace's own key. The children ship a handful of files for their own landmarks; the surrounding countryside (the cells #5505 now fills with the parent's terrain) is covered only by the parent's files, so city worldspaces get 0 to 16 of the parent's ~900 ring files.

## Evidence
Location: `byroredux/src/cell_loader/placement_lod.rs:450` (`placement_lod_archive_path(&wctx.worldspace_key, cx, cy)`); contrast `object_lod.rs` `object_lod_quad_paths` / `ExteriorWorldContext::lod_worldspace_key` (`exterior.rs:179`). Re-confirmed present at HEAD.

Radius `PLACEMENT_LOD_RADIUS_CELLS = 16` around the child's LAND centroid, own vs Tamriel files: BrumaWorld 7 vs 800; BravilWorld 10 vs 939; ChorrolWorld 16 vs 1,034; SkingradWorld 8 vs 1,065; Imperial City districts 1 to 6 vs 926 to 962; The Fringe / Bliss / Crucible / Palace / Vitharn 0 vs 365 to 630 (SEWorld); TGTempleOfTheEmperorZero 0 vs 912. Whether the original game draws the parent's placement list inside a city worldspace is inferred from the inherit bit and #5496's policy, not observed. Not covered by #5496 (closed; terrain and `.bto` only) or #5786 (closed; it states "`placement_lod.rs` still uses the child's own key, which is right (children ship their own `distantlod\<child>_*.lod`)" - the measurement shows that is only half the picture).

## Impact
Inside a city worldspace the distant terrain (inherited) is drawn but the distant statics (farmhouses, towers, rocks, ruins) are not: ~99% fewer placements than the same view from Tamriel.

## Related
#5496, #5505, #5786, EXT-D6 siblings.

## Suggested Fix
Per cell, read the child's own `.lod` when present and fall back to `lod_worldspace_key()`'s file; a child cell that has its own file must not also load the parent's (duplicate landmarks). Pin it with an archive-path test like the `.bto` one.

Source: docs/audits/AUDIT_OBLIVION_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
