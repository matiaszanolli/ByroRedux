# FO3-D4-01: PlacementLodProvider distant-object LOD never fires on FO3 or FNV — vanilla archives ship zero distantlod .lod files

- **Severity**: MEDIUM
- **Labels**: medium, legacy-compat, bug
- **Location**: `byroredux/src/cell_loader/placement_lod.rs:283-353` (`stream_placement_lod_blocks`, gated `GameKind::Oblivion | GameKind::Fallout3NV`); `docs/engine/exal.md:240,576-613`
- **Status note**: scope-mismatch in closed #1726's premise — not a regression of the Oblivion-side code #1726 landed, which is correct.

## Description
`placement_lod.rs` implements the `DistantLOD\<World>_<x>_<y>.lod → _far.nif` per-cell placement scheme and gates it to run for both `GameKind::Oblivion` and `GameKind::Fallout3NV`, per `exal.md`'s claim that the distant-object source is "same" across Oblivion/FO3/FNV. The format itself was reverse-engineered and validated **only against Oblivion** (9,889 real `.lod` files). Direct probes of every real FO3 and FNV archive (base game + all DLC) show **zero `distantlod\` matches in any of them**. `Fallout - Meshes.bsa` has only 2 `_far.nif` entries total (`washmonumentlod_far.nif`, `ravecity_far.nif` — one-off landmark assets, not a systematic scheme). `placement_lod_archive_path` therefore always builds a path that never resolves, and `spawn_placement_lod_cell` returns `None` on every call — no crash, the streaming ring just silently inserts empty sentinels forever.

## Evidence
`stream_placement_lod_blocks` (`placement_lod.rs:294-299`) gates with `if !matches!(wctx.record_index.game, GameKind::Oblivion | GameKind::Fallout3NV) { return; }` — an active gate that does not exclude FO3/FNV. The doc comment above it explicitly frames this as "the placement scheme (Oblivion / FO3 / FNV)" with no hedge acknowledging the Fallout titles might not ship `.lod` files at all. `bsa_grep` for `"distantlod"` against `Fallout - Meshes.bsa` and all 13 FNV BSAs (base + 12 DLC) returns 0 matches everywhere. FO3's Meshes.bsa instead carries a large `meshes\landscape\lod\<worldspace>\...` terrain-LOD block tree (including named landmark sub-folders like `washmontop`, `dcworld03/08/09`), suggesting Bethesda folded landmark-object LOD into the terrain-LOD block system for Fallout titles rather than shipping Oblivion's separate per-cell object-placement format.

## Impact
Distant static-object LOD (buildings, rocks, landmark silhouettes beyond the loaded-cell radius) is completely absent on FO3 *and* FNV exteriors — the same class of gap `object_lod.rs` already documents for the Skyrim+/FO4 `.bto` path, just via the sibling provider. Visual-completeness only (terrain horizon still renders via heightmap LOD); no parse failure, no crash, no REFR mis-placement. Notably, FNV — this audit suite's own reference/baseline title — has *never* had this path runtime-validated either, only Oblivion has.

## Related
#1726 (closed; premise correct for Oblivion, incomplete for FO3/FNV), #1731 (VWD flag, same subsystem)

## Suggested Fix
Either (a) confirm via a wider GECK/xEdit cross-check that FO3/FNV genuinely ship no per-object placement LOD and narrow the `exal.md` §5 table + the `GameKind::Fallout3NV` gate to "Oblivion only," folding the Fallout landmark case into the existing terrain-LOD block path instead; or (b) if FO3/FNV do encode object LOD via the `meshes\landscape\lod\<landmark-subworld>\` convention, extend `terrain_lod.rs`'s landmark-tile handling to spawn renderable object geometry from it. Either way, update closed #1726 to note the FO3/FNV gap explicitly.

## Completeness Checks
- [ ] **SIBLING**: `object_lod.rs`'s Skyrim+/FO4 `.bto` path already documents a similar gap-awareness pattern — confirm consistent framing between the two providers once this is resolved
- [ ] **TESTS**: A regression test/documentation update pins whichever resolution direction is chosen (narrow the gate, or wire the terrain-LOD landmark path)
