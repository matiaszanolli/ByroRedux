# #5496: FO3-2026-10-09-D3-01: The object and terrain LOD rings ignore WRLD `PNAM` "Use LOD Data" (`0x02`) — LOD-inheriting child worldspaces draw no distant land or objects

**Labels**: bug, game:fo3, game:skyrim, legacy-compat, medium, terrain-exterior

**Source**: `docs/audits/AUDIT_FO3_2026-10-09.md` — finding `FO3-2026-10-09-D3-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. Distant content is missing on authored vista worldspaces; nothing crashes. Comparable gaps #5222 and #4913 were MEDIUM.
- **Dimension**: Cell Loading / LOD rings. The mechanism owner is `/audit-exterior` (EXAL LOD rings). Labels: `game:fo3`, `game:skyrim`, `legacy-compat`, `terrain-exterior`.
- **Location**:
  - `byroredux/src/cell_loader/object_lod.rs:207-210`: `objects_for(wctx.worldspace_key)`.
  - `object_lod.rs:419`: the archive path is keyed on the child.
  - `byroredux/src/cell_loader/terrain_lod.rs:442`, `:479`: `exterior_cells` / `terrain_for(worldspace_key)`.
  - `byroredux/src/cell_loader/legacy_lod_index.rs:132-145`: a plain key lookup.
  - `byroredux/src/env_translate.rs:269-282` and `:314-315`: `INHERIT_LOD` is consumed only by `translate_lod_water`, and is documented as the "`NAM3`/`NAM4` LOD water" bit.
- **Status**: NEW.
  - No open or closed issue covers it. Searches: "Use LOD Data", "PNAM LOD", "child worldspace LOD parent", "Whiterun distant LOD".
  - #2735 scoped the bit to LOD water only.
  - `AUDIT_FO3_2026-10-03` noted that MegatonWorld "lacks Use LOD Data, so it does not inherit Wasteland's LOD either", but never checked the children that set the bit.
- **Description**:
  - The GECK defines the bit as whole-LOD inheritance. `geck-uesp-wiki/Category/World Spaces.wiki:17-22`: "**Use LOD Data:** Check this box to inherit LOD data from the parent world space". The page shows *ChildWorldspaceLODOff/On* screenshots, and LOD water height and type are sub-fields of that group.
  - The engine reads the bit for LOD water only. No LOD module walks to the parent: a grep for parent/inherit across `object_lod`, `terrain_lod`, `terrain_lod_btr`, `lod_bands`, `legacy_lod_index` and `lod_support` finds no worldspace walk.
  - A child that sets the bit, and so ships no LOD of its own, therefore has an empty distant ring.
- **Evidence** (`py/wrld_parents.py`, `lod_names.py`, `world_cells.py`, `quad_cover.py`; the `PNAM` flags byte is shown):

  | FO3 child (parent Wasteland) | PNAM | Cells / placed refs | Own LOD names in 7 archives | Wasteland LOD over the child's cell box (terrain / object cells) |
  |---|---|---|---|---|
  | StatesmanRoofWorld | 0xC6 | 226 / 501 | 0 | 405/405 · 365/405 |
  | GNRroofWorld | 0x07 | 144 / 127 | 0 | 260/260 · 260/260 |
  | MonumentWorld | 0xC6 | 87 / 0 | 0 | 350/350 · 326/350 |
  | TestQAWorld | 0xC6 | 240 / 910 | 0 | — |

  - The children that do **not** set the bit are consistent with this reading. DCworld01–18 and WashMonTop (0xC4) ship their own quads. MegatonWorld (0xC5) ships none and inherits none.
  - Skyrim (`py/sky_lod_worlds.py`): all 7 children with the bit (WhiterunWorld, SolitudeWorld and WindhelmWorld at 0x7F, RiftenWorld 0x77, WhiterunDragonsreachWorld, KatariahWorld 0x57, WindhelmPitWorldspace 0x5F) have no `meshes\terrain\<world>` folder. `markarthworld` (0x44, bit clear) ships its own.
- **Impact**:
  - FO3: standing on the Statesman Hotel roof (Reilly's Rangers) or the GNR roof draws nothing past the loaded grid. The authored game draws Wasteland's distant terrain and buildings there.
  - The same gap reaches Skyrim's four walled hold cities and Dragonsreach.
  - LOD water already inherits under the same bit, so a water plane can draw with no land LOD around it.
- **Related**: #2735, #5222, #5387 (closed); EXT-D6-2026-10-09-01.
- **Suggested Fix**:
  - Resolve one "LOD source worldspace" per streaming state, walking `WNAM` while `PNAM & 0x02` (reuse `inherit_up_chain`).
  - Key `objects_for` / `terrain_for`, the archive and atlas paths, `worldspace_cell_bounds` and the Skyrim/FO4 `.btr`/`.bto` paths on that source. Keep the full-detail exclusion on the child's resident cells.
  - Correct the `pnam` table doc.
  - Pin it with a StatesmanRoofWorld real-data test and a synthetic Skyrim-shape test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
