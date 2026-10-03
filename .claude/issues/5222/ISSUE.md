# #5222 — FO3-D3-01: FO3 legacy LOD quads are not anchored at (0,0) in 26 of 29 worldspace/level sets — 425 of 697 object-LOD quads are never requested, including all 65 #3502 level-8-only quads

https://github.com/matiaszanolli/ByroRedux/issues/5222

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: MEDIUM
- **Dimension**: Cell Loading / exterior distant LOD. Shared mechanism, owner `/audit-exterior`. FO3 is the dominant reach; FNV is also affected.
- **Location**:
  - `byroredux/src/cell_loader/lod_support.rs:88-121` (`worldspace_lod_grid_origin` returns `(0, 0)` whenever `!combined_lod_supported(game)`);
  - `byroredux/src/cell_loader/lod_support.rs:158-163` (`quad_origin`);
  - `byroredux/src/cell_loader/object_lod.rs:182-212` (quad probe);
  - `byroredux/src/cell_loader/terrain_lod.rs:99-111` and `env_translate.rs:161-170` (legacy terrain DDS lookup, same origin);
  - `byroredux/src/cell_loader/lod_bands.rs:1001-1060` (the #3502 test).
- **Status**: NEW.
  - #2586 (closed) fixed the same defect for Skyrim/FO4 only, through WRLD NAM0.
  - #3502 (closed) added object-only coarsening. Its test never uses a real coordinate (`|level, _, _| level == 8`), so the fix reaches none of the quads it targeted.
- **Description**:
  - With a `(0,0)` origin, `quad_origin` only ever probes `<w>.level<L>.x<kL>.y<mL>.nif`.
  - FO3's authored `blocks\` and terrain quads sit on a different lattice in every worldspace except `wasteland`, `dlc03adamsafb` and `dlc03relaystation`. Every probe misses and caches an empty sentinel, and no distant objects draw. The authored terrain-LOD DDS misses too, so terrain falls back to the tiled base LTEX.
  - Some lattices are irregular: `washmontop` level-8 quads sit on three different y residues. No single origin recovers them, NAM0 included. Across the corpus, (0,0) explains 272 of 697 quads, NAM0 128, and the XCLC minimum 104.
- **Evidence**: BSA name-table walk; residue = `(qx mod L, qy mod L)`. I reproduced it independently with `strings`; counts below include terrain-mesh quads.
  ```
  Fallout - Meshes.bsa: wasteland L4/L8/L16/L32 all (0,0)          <- reachable
    dcworld03 L4 (0,2) L8 (0,6) · dcworld05 L4 (3,2) · dcworld09 L4 (0,2) L8 (4,2)
    dcworld01 L8 (0,5),(7,5) · dcworld06 L8 (1,5),(1,6) · dcworld12 L8 (2,6),(4,6) · dcworld17 L8 (0,6),(1,1)
    paradisefalls L4 (2,1) L8 (5,4) · washmontop L4 (0,3) L8 (4,2),(4,4),(4,7) L16 (4,7)
  ThePitt - Main.bsa: dlc01pittworld L4 (1,1) L8 (5,1) · dlc01steelmillexterior L4/L8 (3,1) · dlc01haven (0,3) · dlc01marketsquare (3,1)
  PointLookout - Main.bsa: dlc4pointlookout L4 (3,0) L8 (7,0) L16 (15,0) · dlc4bog (0,2)
  Agent totals: blocks 697 plain quads, 272 on the (0,0) grid, 425 unreachable (26/29 worldspace-levels);
                terrain 2 231 quads, 835 off-grid; Fallout - Textures.bsa LOD diffuse 560/1 920 off-grid
  #3502 set (dcworld01/03/06/12/17, paradisefalls, washmontop): 65 plain level-8 quads, 0 on grid
  FNV (same mechanism): blocks 107/530 off-grid (freeside*, bouldercity, nvdlc01*, road*, nuke*), terrain 367/3 567
  ```
  - Both `wasteland` and FNV `wastelandnv` are (0,0)-aligned, which is why every gate stays green.
  - The FO3 exterior gate's `MegatonWorld` ships no `landscape\lod` folder. Its PNAM `0xC5` lacks Use LOD Data, so it does not inherit Wasteland's LOD either.
- **Impact**: visual only; no crash or leak.
  - No distant buildings and no authored distant-terrain colour in: every FO3 DLC exterior except Broken Steel (The Pitt 4 worldspaces, Anchorage 6, Point Lookout 2), every DC sub-worldspace, ParadiseFalls and WashMonTop.
  - The #3502 closure and this skill's checklist line ("those worldspaces must show distant buildings inside 16 cells") are false on real data.
  - FNV loses 107 object quads (Freeside, Sierra Madre, Lonesome Road).
- **Related**: #2586, #3502, #3321, #4468, FO3-D3-02.
- **Suggested Fix**:
  - For the Fallout legacy family, stop deriving an origin.
  - Index the authored quads per `(worldspace, level)` from the archive name table, as `probe_lod_corpus` does, and select every quad whose `[qx,qx+L)×[qy,qy+L)` footprint touches the band ring. Feed the same index to the terrain DDS lookup.
  - Replace the coordinate-free #3502 test with an `--ignored` real-data test that requires a `dcworld03` / `washmontop` level-8 quad to be selected near the worldspace centre.

## Completeness Checks
- [ ] **SIBLING**: The terrain-LOD DDS lookup (`terrain_lod.rs`, `env_translate.rs`) uses the same authored-quad index as the object-LOD probe; FNV worldspaces (Freeside, Sierra Madre, Lonesome Road) are checked too
- [ ] **TESTS**: An `--ignored` real-data test requires a `dcworld03` / `washmontop` level-8 quad to be selected near the worldspace centre (replacing the coordinate-free #3502 pin)
