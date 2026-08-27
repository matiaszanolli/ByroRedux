# SKY-2026-08-27-D4-01: a Deleted-REFR tombstone authored under a different CELL than the base placement never removes it — 5 vanilla Skyrim SE placements still spawn

Labels: medium,esm-plugin,bug,game:skyrim,legacy-compat

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (code read + reproduced against the five shipped SSE masters)
- **Location**: `crates/plugin/src/esm/cell/mod.rs:1142` (`merge_placed_references`), fed from `crates/plugin/src/esm/cell/walkers.rs:691` and `crates/plugin/src/esm/cell/walkers.rs:160` / `crates/plugin/src/esm/cell/wrld.rs:314`
- **Description**: `parse_refr_group` correctly detects `RECORD_FLAG_DELETED` (0x20) and records the tombstoned FormID, but it pushes it into the **enclosing `CellData`'s** `deleted_refs`, and `merge_cell_references` only ever applies `over.deleted_refs` against **that same cell's** base references:

  ```rust
  fn merge_cell_references(base: &CellData, over: &mut CellData) {
      merge_placed_references(&base.references, &mut over.references, &over.deleted_refs);
  }
  ```

  In the real Creation Engine a REFR is a globally-unique FormID and deleting it removes the object wherever it lives; the cell GRUP the deletion happens to be authored under is not part of the identity. Bethesda's own DLCs routinely author the tombstone under a *different* CELL from the one the base master placed the REFR in (a REFR that was moved across a cell boundary before being deleted, or a home interior that HearthFires re-authored). When that happens the tombstone lands on `CellData` A while the live placement sits in `CellData` B, the `retain` in `merge_placed_references` matches nothing, and the object the DLC deleted still renders.
- **Evidence**: raw GRUP walk of the five masters (tombstone census), then the same five parsed through `parse_record_indexes_in_load_order` and every merged reference checked against the tombstone set. Deleted-flagged record census (raw walk, flag `0x20`):

  ```
  Skyrim.esm:        deleted-flagged total=0
  Update.esm:        total=97   [NAVM 3/244, REFR 93/9618, STAT 1/379]
  Dawnguard.esm:     total=154  [ACHR 10/908, IDLE 2/388, NAVM 59/1922, NPC_ 1/623,
                                 REFR 80/66095, SMQN 1/36, STAT 1/930]
  HearthFires.esm:   total=16   [NAVM 5/137, REFR 11/12463]
  Dragonborn.esm:    total=12   [EXPL 1, INFO 1, NAVM 1, REFR 8/78754, SPEL 1]
  _ResourcePack.esl: total=0
  ```

  Merged 5-plugin load (order `["skyrim.esm","update.esm","dawnguard.esm","hearthfires.esm","dragonborn.esm"]`):

  ```
  tombstoned REFR/ACHR FormIDs across the 5 masters = 202
  merged refs total=871810; tombstoned survivors=5
    ["SolitudeProudspireManor:0010C205", "POIReach24:00023C63",
     ":000DDD36", ":000CE60E", ":000CE60D"]
  ```

  Provenance of each survivor (raw walk, printing the enclosing children-GRUP label):

  ```
  Skyrim.esm:      REFR 0010C205 deleted=false in cell 00016A06 grup_type=9
  HearthFires.esm: REFR 0010C205 deleted=TRUE  in cell 00016778 grup_type=9
  Skyrim.esm:      REFR 000DDD36 deleted=false in cell 00007158 grup_type=9
  Update.esm:      REFR 000DDD36 deleted=TRUE  in cell 00007157 grup_type=9
  Skyrim.esm:      REFR 00023C63 deleted=false in cell 0000711B grup_type=9
  Dawnguard.esm:   REFR 00023C63 deleted=TRUE  in cell 0000713B grup_type=9
  Skyrim.esm:      REFR 000CE60D deleted=false in cell 00000D74 grup_type=8
  Skyrim.esm:      REFR 000CE60E deleted=false in cell 00000D74 grup_type=8
  Dawnguard.esm:   REFR 000CE60D deleted=TRUE  in cell 0000BC34 grup_type=9
  Dawnguard.esm:   REFR 000CE60E deleted=TRUE  in cell 0000BC34 grup_type=9
  ```

  Every one is a deletion authored under a *different* CELL than the base placement
  (and `000CE60D/E` additionally cross the persistent(8) → temporary/VWD(9) group
  boundary). What actually survives, with its base object resolved from the merged
  index:

  ```
  SURVIVOR 0010C205 in 'solitudeproudspiremanor' base=001010B3 CraftingCookingPotSm
                    [FURN] Furniture\SmallCookingPot.nif pos=[652.39,-970.58,0.00]
  SURVIVOR 000DDD36 in 'tamriel(-39, 2)'  base=0002BCA7 FXSplashSmallParticlesLong [MSTT]
  SURVIVOR 00023C63 in 'tamriel(-40, 4)'  base=00000032 COCMarkerHeading [STAT]
  SURVIVOR 000CE60E in 'tamriel<persistent>' base=000140BD pos=[135365.9,-75910.6,11338.2]
  SURVIVOR 000CE60D in 'tamriel<persistent>' base=000140BD pos=[135897.3,-76105.9,11338.2]
  ```

  197 of 202 tombstones are honoured correctly; these 5 are not.
- **Impact**: 5 vanilla-authored deletions are ignored on a full Skyrim SE + 3-DLC load.
  The user-visible one is `0010C205` — HearthFires deletes a small cooking pot from
  Proudspire Manor (Solitude player home) and it still spawns, so the reworked HF
  interior renders with a leftover/duplicate cooking pot at `[652, -971, 0]`. The other
  four are an exterior splash effect, a COC marker (invisible), and two persistent-cell
  placements. Blast radius grows with mods: any mod that relocates then deletes a
  vanilla REFR hits the same hole, and there is no workaround short of not loading the
  deleting plugin.
- **Suggested Fix**: promote the tombstone from per-`CellData` to per-plugin/global.
  Collect `deleted_refs` into an index-level `HashSet<u32>` on `EsmCellIndex` as each
  plugin merges (the FormID is already remapped by `read_record_header`), and have the
  merge — or a single post-merge sweep over `cells` / `exterior_cells` /
  `worldspace_persistent_cells` — drop any placement whose FormID is tombstoned by a
  *later-or-equal* plugin. Keep the existing per-cell path as the fast case; the
  un-delete semantics already pinned by
  `three_plugin_chain_composes_refr_merge_and_cross_plugin_delete` must still hold (a
  later plugin re-placing the FormID wins), so the global set has to be consulted in
  load order, not as a flat union. A regression test asserting these 5 FormIDs are
  absent from the merged 5-master index is the natural pin.
- **Related**: #1660 (the original tombstone skip — correct, but per-cell), #2370 EX-09/17
  item 7 (`0cef6fc0`, added `deleted_refs` + cross-plugin removal — same per-cell scope),
  #1546. No OPEN issue in the 300-issue dedup baseline (84 open, fetched 2026-08-27) covers this.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
