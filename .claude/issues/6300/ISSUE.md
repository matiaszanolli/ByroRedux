# #6300: NIF-D1-2026-10-21-01: #6115's `note_scene_health` misses the FO4 precombine CSG decode path (caches a parsed scene in `NifImportRegistry` with no partial-scene report) and the three LOD importers

**Labels**: low,bug,nif-parser,nif,terrain-exterior,game:fo4

**Source**: docs/audits/AUDIT_NIF_2026-10-21.md · **Severity**: LOW · **Dimension**: Stream Position (recovered-scene visibility downstream) · **Location**: `byroredux/src/cell_loader/precombined.rs`, `object_lod.rs`, `terrain_lod_btr.rs`, `placement_lod.rs` (`parse_nif` call sites)

- **Game Affected**: FO4 (precombine CSG path); every title with object, terrain or placement LOD (LOD importers)
- **Location**: `byroredux/src/cell_loader/precombined.rs:345-377` (CSG decode, cached by the `reg.insert` near `:406`), `byroredux/src/cell_loader/object_lod.rs:449`, `byroredux/src/cell_loader/terrain_lod_btr.rs:239`, `byroredux/src/cell_loader/placement_lod.rs:493`
- **Status**: NEW (follow-through of #6115, closed; the commit title says "report partial NIF scenes from every consumer")
- **Description**: The skill's Dim 1 contract says every consumer that caches a scene passes it through `note_scene_health`. `bbb4dc3d7` wired that into the REFR loader, the pre-parse worker and the NPC loader (`references/import.rs:57`, `streaming/pre_parse.rs:290`, `scene/nif_loader.rs:335`). Four other production `parse_nif` consumers do not call it:
  - `precombined.rs:346` parses each `_oc.nif` itself and decodes CSG geometry from it (`decode_precombine_csg`), then inserts the result into the registry. A truncated or recovered `_oc.nif` is cached as a positive entry with no path-named log line, and `partial_nifs=` never counts it. When the CSG path is skipped, the fallback goes through `parse_and_import_nif_pub`, which does report.
  - The three LOD importers import partial scenes the same way. The only signal there is `import_nif_scene`'s path-less "importing truncated NIF scene" WARN (`import/mod.rs:174`), and nothing at all for `recovered_blocks`.
- **Evidence**: `grep -rn note_scene_health byroredux/src` returns only the three call sites above and the definition. The four `parse_nif` call sites listed under Location have no `truncated` or `recovered_blocks` check.
- **Impact**: This is observability only. Vanilla measures 0 truncated and 0 recovered on every title (table above), so it only matters for mod content. In that case a partial precombine or LOD mesh renders short with no file named, which is the gap #6115 closed for the other three consumers.
- **Related**: #6115, NIF-SD-02 (2026-10-19), #6114.
- **Suggested Fix**: Call `crate::parsed_nif_cache::note_scene_health(&path, &scene)` right after each of the four `parse_nif` successes, so all four feed the same once-per-path memo and the `partial_nifs=` counter.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
