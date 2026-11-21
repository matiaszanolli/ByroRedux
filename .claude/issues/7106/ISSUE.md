# #7106 — ESM-D2-10: Stale comments and docs: `armor_type` enum order and pre-Skyrim claim, REGN `RDMD`, `ALOC`/`MUST` in esm-records.md, `merge_from` line citations

Labels: low,esm-plugin,documentation,doc-rot
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D2-10)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D2-10**

- **Severity**: LOW (doc rot)
- **Dimension**: Sub-Record Byte Accounting (doc)
- **Record / Sub-record**: `ARMO` `BOD2`/`BMDT` `armor_type`; `REGN` `RDMD`
- **Location**:
  - `crates/plugin/src/esm/records/items.rs:792` says "`armor_type`: 0=light, 1=clothing, 2=heavy, 3=none, 4=gauntlets". xEdit `wbArmorTypeEnum` (`wbDefinitionsTES5.pas:2250`) is 0 Light, 1 Heavy, 2 Clothing; the field doc at `:195-199` and every consumer (`byroredux/src/armor.rs:81-83`, `combat.rs:548`, `skill_use_emit.rs:88`: `Some(1)` is Heavy) agree with xEdit, so `:792` is wrong and has been since 2026-04-17 (`cfdc222b1b`).
  - `items.rs:195-199` says `armor_type` is "`None` on pre-Skyrim games"; since `44cd08fdf` (#7051) Oblivion `BMDT` sets `Some(0)` / `Some(1)` from General Flags bit 7 (`:782-789`).
  - `crates/plugin/src/esm/records/misc/world/region.rs:161-162` ("no consumer reads it yet (#3816)" for `RDMD` `music_type`) contradicts the same doc block's earlier sentence and `byroredux/src/music/translate.rs:408-416`, which reads it as the exterior region music category ("region RDMD"). Line `:154` ("Skyrim's `RDMO` music waits on xWMA playback (#3816)") sits against `translate.rs:473` reading `music: Some(form)`; re-check it with the line above.
- **Status**: NEW (the last item is the residue of ESM-D2-05)
- **Impact**: a reader of the BOD2 arm gets clothing and heavy swapped; no behaviour change.
- **Suggested fix**: rewrite the three comments to the current behaviour (cite `wbArmorTypeEnum`).

### Merged: ESM-D4-08 (the same doc-rot found by Dim 4)

- **Severity**: LOW
- **Dimension**: 4
- **Record / Sub-record**: `ARMO` `BOD2`/`BMDT` `armor_type`, `ALOC`, `MUST`
- **Status**: NEW
- **Location and evidence**:
  - `crates/plugin/src/esm/records/items.rs:792`: "armor_type: 0=light, 1=clothing, 2=heavy, 3=none, 4=gauntlets". xEdit `wbArmorTypeEnum` (`wbDefinitionsTES5.pas:2251-2253`, `wbDefinitionsFO4.pas:2250`) is 0 Light, 1 Heavy, 2 Clothing. The field doc at `items.rs:195-198` was already corrected (`31aca901d`) and every consumer (`armor.rs`, `combat.rs:548`, `skill_use_emit.rs:88`) keys heavy on `Some(1)`. The same field doc still says "`None` on pre-Skyrim games", false since `fba412ebb`/`44cd08fdf` set `Some(0|1)` from Oblivion's `BMDT` General Flags bit 7 (correct against `wbDefinitionsTES4.pas:1697-1706`).
  - `docs/engine/esm-records.md:~470`: ALOC is listed among the "26 arms [that] remain on the minimal-stub path" (`ALOC/ANIO/ASPC/...`). `ALOC` is a typed `parse_aloc` row since `782e23d0a`, so the list is 25 and ALOC belongs with the graduated MSET/MUSC group.
  - `docs/engine/esm-records.md:~446` and `index.rs` `music_tracks` doc say "Skyrim MUST"; since #6790 it is Skyrim, FO4 and FO76.
  - `index.rs` (the `MANUALLY_MERGED` table inside `every_index_map_is_a_category_or_a_recorded_exclusion`) cites `index.rs:1010-1029` and `:1041-1042` for `merge_from` lines that are now ~1667-1699. `records/tests.rs:1949` still says "80 typed maps on EsmIndex + 2 from cells" above a pin of 118.
- **Impact**: documentation only.
- **Suggested fix**: one doc pass. Replace the line-number citations with function names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
