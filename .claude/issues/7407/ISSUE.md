# #7407 SK-D3-03: No real-data pin for EQUP / ARMO `ETYP` / `HandCatalog`

**Labels**: low,esm-plugin,test-gap,bug,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D3-03` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: LOW
- **Dimension**: NPC Equip (test-gap)
- **Location**: `crates/plugin/src/esm/records/misc/world/keyword.rs:174` (`the_skyrim_hand_slots_take_the_hands_the_ck_describes`), `crates/scripting/src/hands.rs:37` (`HandCatalog::from_index`)
- **Status**: NEW
- **Description**: Phase 1's whole shield / two-hander classification rests on `parse_equp`, ARMO/WEAP `ETYP` FormID remap through a multi-master load order, and `slot_hands`. Every test builds `EqupRecord`s and `HandWeapon`s by hand. Nothing parses `Skyrim.esm` (+ DLC) and asserts the catalog. The design's numbers (7 slots, 237 shields, `BothHands` 1,141) came from a one-off scan that is not repeated in CI or in an `#[ignore]`d test, unlike the other Skyrim decode pins (`skyrim_bodt_body_templates_decode_their_biped_masks`, `skyrim_npc_wnam_skin_census_matches_the_byte_walk`).
- **Evidence**: grep for `equip_slots` / `HandCatalog::from_index` in `tests`: only the hand-built unit tests. My independent scan (above) shows the data does agree today, including 66 Dragonborn shields whose `ETYP` must be remapped through the master index, so the gap is latent, not a live bug.
- **Impact**: a regression in EQUP group dispatch, `remap_fid` of `ETYP`, or `editor_id` lookup (`slot_hands` identifies the hand leaves by EDID `LeftHand`/`RightHand`) would silently make every shield read as an ordinary armor and every two-hander as a one-hander, with all unit tests green.
- **Related**: #7398.
- **Suggested Fix**: an `#[ignore]`d real-data test (Skyrim SE load order, `test_paths::skyrim_se_data_dir()`) asserting 7 EQUP slots, `slot_hands` for the five hand slots, >= 237 shields in the catalog and every `ETYP: Shield` ARMO a catalog shield.

## Completeness Checks
- [ ] **SIBLING**: the WEAP `ETYP` × `DNAM` both-hands classification is pinned in the same test
- [ ] **TESTS**: an `#[ignore]`d real-data test over the Skyrim SE load order
