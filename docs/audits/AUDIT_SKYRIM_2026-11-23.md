# Skyrim (SE + LE) Compatibility Audit — 2026-11-23

**HEAD**: e01ca71f0 · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-31.md` (HEAD `670f81917`) · **Audited**: Dim 1–5 (all had commits since the baseline) · **Unchanged since baseline (skimmed)**: none

Run context: five dimension agents (Dim 1 legacy-specialist, Dim 2 renderer-specialist, Dims 3–5 general-purpose/sonnet) on a detached worktree at `origin/main`, static analysis plus cargo tests, ignored real-data tests one at a time, and Python byte-walks of the SE masters. No engine launch, no GPU. The coordinator re-verified every finding against the code before writing it here. Scratch notes: `/tmp/audit/skyrim/dim_{1..5}.md`.

---

## Executive Summary

**Control bench.** Not re-run (it needs the engine). ROADMAP's bench-of-record (`d9a201c71`, 2026-10-22) has Whiterun BanneredMare at 5,853 entities, 83.7 FPS / 11.94 ms with TAA; it is 1966 commits old.

**Regression coverage is green.** Every guard the skill names passes, and the real-data gates match the skill's numbers exactly:
- NIF corpus: SE 33,468 / 33,468 clean over 8 archives, LE 22,466 / 22,466, zero truncated / recovered / realignment warnings.
- BSA: SE 23 archives / 172,961 entries and LE 9 / 132,755, all extracted with 0 errors; `folder_hash_mismatches` is 0 in all 5,698 SE and 3,919 LE folders.
- Packed SSE skin indices: 19,606 weighted lanes, 0 changed by the importer.
- Merged fold (10-plugin SE order): the LMC conformance oracle passes over 878,404 placed refs; `Forelhost01` and `SolitudeWinkingSkeever` resolve; DIAL / INFO counts equal a raw last-carrier walk; the default `merge` policy merges 0 vanilla records, so it reproduces vanilla last-write-wins.

**Skyrim-specific risk is in the newest code.**
- **One HIGH (SK-D4-01):** the LMC-REC-010 opaque collector added 2026-11-23 recurses into nested GRUPs with no depth cap. Any plugin after the first in a profile (every mod) can abort the process with a crafted 480 KB file. Vanilla data is unaffected.
- **Hand Equipment Phase 1** (also landed 2026-11-23) decodes correctly against the data and the CK pages; its residue is four LOWs (SK-D3-01…05, two of them already tracked by #7399 / #7402).
- Two LOW gaps elsewhere: a stale shader-type doc (SK-D2-01) and a real-data `ArchiveSet` test that checks only 3 SE archives (SK-D5-01).

| Severity | Count | IDs |
|---|---|---|
| HIGH | 1 | SK-D4-01 |
| LOW | 7 | SK-D2-01, SK-D3-01, SK-D3-02, SK-D3-03, SK-D5-01 (NEW); SK-D3-04, SK-D3-05 (Existing #7399 / #7402) |

---

## Dimension 1: BSTriShape Packed Geometry + SSE Skinned Reconstruction

No findings. The four delta commits do not change this dimension's behaviour: 153f611a7's `crates/nif` hunks only merge nested `if`/`if let` into let-chains with no `else` arms, and leave `skin.rs`, `sse_recon.rs`, `tangent.rs`, `import/mesh/bs_tri_shape.rs` and `blocks/tri_shape/` untouched; 62782dd3b's root-transform clear is gated to `Fallout3NV | Fallout4` by `placement_replaces_model_root` (pinned by `placement_replaces_model_root_only_for_verified_games`). Guards: 52 / 52, plus the real-data packed-index test. Observed, not filed: the SSE skinned global buffer is decoded twice per shape (load-time only).

## Dimension 2: Shader-Type Dispatch + Skyrim Material Slice

### SK-D2-01: `apply_shader_type_data` doc still says EyeEnvmap and MLP `refraction_scale` are "packed into the GPU material but not yet shaded (#5961)"
- **Severity**: LOW
- **Dimension**: Shader-type dispatch / Skyrim material slice (doc-rot)
- **Location**: `crates/nif/src/import/material/shader_data.rs:99-101` (rustdoc of `apply_shader_type_data`)
- **Status**: NEW (closed #3908 is the older sweep, predating #5961)
- **Description**: b4d073454 (#5961) shades both; #6213 / #6261 refined the eye path. The doc still says neither is shaded.
- **Evidence**: doc text "EyeEnvmap (16) and MultiLayerParallax's `refraction_scale` are packed into the GPU material but not yet shaded (#5961)"; live consumers `triangle.frag` `isEyeEnvmap` (~3147, branch ~3361-3393) and the `MLP_REFRACTION_UV_PER_UNIT` refract offset (~1575-1584).
- **Impact**: none at runtime; misleads a reader of the import boundary into thinking two Skyrim shader types are render-incomplete.
- **Related**: #5961, #6213, #6261, #3908
- **Suggested Fix**: name the live consumers (the EyeEnvmap branch gated by `pack_eye_fields`; the MLP refraction UV offset).

### Existing #6816 (Skyrim reach confirmed, not re-filed)
2ed1d91ce's classifier ceiling reaches vanilla Skyrim type-1 surfaces that bind a slot-5 env mask, have `env_map_scale > 0.3`, specular luminance ≤ 0.6 and no keyword arm: roughness becomes `min(base, 0.55)` instead of 0.8, which crosses `traceEnvironmentRay = roughness < 0.6` and fires an RT reflection ray. The per-texel blend itself is gated on `materialKind == 0` and cannot fire for Skyrim. This is #6816's unmeasured non-FNV population (already `game:skyrim`).

**Existing #6816 — Skyrim reach confirmed, not re-filed.** 2ed1d91ce's env-mask classifier ceiling reaches vanilla Skyrim type-1 surfaces that bind a slot-5 env mask with `env_map_scale > 0.3`, specular luminance ≤ 0.6 and no keyword arm: roughness becomes `min(base, 0.55)` instead of 0.8, which crosses the `roughness < 0.6` RT-reflection gate. The per-texel blend is gated on `materialKind == 0` and cannot fire for Skyrim. That population is #6816's unmeasured non-FNV set (already `game:skyrim`); whether it looks better or worse needs a capture.

Also checked: the Lambertian reference flag (f3da18aac) is written only by the Cornell harness, and `MAT_FLAG_PBR_BSDF` only by BGSM / CDB / `.mat` / Cornell, so neither reaches vanilla Skyrim; #4393 and #4256 hold; 153f611a7's `material_translate.rs` hunks are let-chain merges; ee2862cda's shared spawn step is pinned by `both_spawn_paths_attach_through_the_shared_step` and 79 `material_translate` tests. Guards: 52 / 52 (note: the filter `shader_tests::skyrim` matches no test; the module path is `blocks::shader::tests::skyrim`).

## Dimension 3: NPC Equip + FaceGen (M41)

### SK-D3-01: A partial removal from a dual-wielded stack leaves one item in both hands
- **Severity**: LOW
- **Dimension**: NPC Equip (Hand Equipment Phase 1)
- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:78` (partial branch of `remove_items`), `byroredux/src/barter.rs:452` (sell), `byroredux/src/crafting.rs:403` (improve-one-of-a-stack split); invariant set by `crates/scripting/src/hands.rs` (`equip_to_hand`, `facts.stack_count < 2`; Phase 2 `d3460fa73` moved it there from the inventory's `equip_into_hand` and kept the rule)
- **Status**: NEW
- **Description**: `equip_into_hand` lets one inventory row sit in both `EquipmentSlots.weapon` and `EquipmentSlots.left_hand` only while the row's `count >= 2` ("One item of a stack cannot be in both hands"). Nothing re-checks that invariant when the count later drops. `remove_items` releases equipment only for a row it empties (`emptied`); the partial branch (`stack.count -= take`) leaves both hands pointing at the row. Barter's sell and the smithing split reach it.
- **Evidence**: player holds `ItemStack{DAGGER, 2}`; `ToggleEquip` then `EquipLeft` on the row gives `weapon == left_hand == InventoryIndex(n)` (exercised by `hands_follow_the_equip_slot_rules`: "the stack's second dagger goes right"). `barter.rs:452` `remove_items(world, player, form_id, 1, Some(session.stock))` takes one: row count 1, `emptied` empty, no `release`. `hand_item` then returns the dagger for both `GetEquippedItemType(0)` and `(1)` and `GetEquippedWeapon(true)`/`()`. The state persists in the save (both fields are saved).
- **Impact**: after selling or scripting away one of two identical one-handers wielded in both hands, scripts and the inventory status see one dagger in two hands; `is_equipped` also blocks `drop_item` on the remaining row. Needs two identical weapons dual-wielded, so rare, and no crash; wrong reader answers until the player re-equips.
- **Related**: PRD HND-SLOT-003, #7398 (closed), #5863 (the emptied-row release this mirrors).
- **Suggested Fix**: after any count decrement of a row that is in both `weapon` and `left_hand` (central place: `remove_items` partial branch and the crafting split), if the new count is below 2 release `left_hand` (keep the right). Add a case to `hands_follow_the_equip_slot_rules`.

### SK-D3-02: The SDK inventory projection reports a left-hand weapon as unequipped
- **Severity**: LOW
- **Dimension**: NPC Equip (host contract; owner /audit-tooling)
- **Location**: `byroredux/src/extensions/capture.rs:308` (`weapon_equipped = slots.weapon == Some(index)`), `crates/sdk/src/inventory.rs:125-131` (`is_equipped = biped_slots != 0 || weapon_equipped`)
- **Status**: NEW (same family as open #6938, which is the BO64 variant of "new equipped state not folded into the SDK projection")
- **Description**: #7398 added a second weapon location, `EquipmentSlots.left_hand`, and updated the in-engine consumers that go through `equipped_indices()` (conditions, weight, DT, loot, save validation). The mod-host capture hand-rolls the equipped test from `occupants` and `weapon`, so a weapon held only in the left hand has `biped_slots == 0` and `weapon_equipped == false`, and `InventoryEntry::is_equipped()` is false. A dagger in each hand shows one entry as equipped (right) and, if they are separate rows, the other as not equipped.
- **Evidence**: `capture.rs:296-308` (mask from `occupants`, boolean from `slots.weapon`); `left_hand` appears nowhere under `byroredux/src/extensions/` or `crates/sdk/` (grep).
- **Impact**: SDK/Studio and mod-host readers of a Skyrim player's or NPC's inventory under-report equipment. No gameplay effect in the engine itself.
- **Related**: #6938, #7397.
- **Suggested Fix**: fold `left_hand == Some(index)` into the `weapon_equipped` boolean (or add a `left_equipped` field when the WIT contract is next bumped); extend the SDK capture test.

### SK-D3-03: No real-data pin for EQUP / ARMO `ETYP` / `HandCatalog`
- **Severity**: LOW
- **Dimension**: NPC Equip (test-gap)
- **Location**: `crates/plugin/src/esm/records/misc/world/keyword.rs:174` (`the_skyrim_hand_slots_take_the_hands_the_ck_describes`), `crates/scripting/src/hands.rs:37` (`HandCatalog::from_index`)
- **Status**: NEW
- **Description**: Phase 1's whole shield / two-hander classification rests on `parse_equp`, ARMO/WEAP `ETYP` FormID remap through a multi-master load order, and `slot_hands`. Every test builds `EqupRecord`s and `HandWeapon`s by hand. Nothing parses `Skyrim.esm` (+ DLC) and asserts the catalog. The design's numbers (7 slots, 237 shields, `BothHands` 1,141) came from a one-off scan that is not repeated in CI or in an `#[ignore]`d test, unlike the other Skyrim decode pins (`skyrim_bodt_body_templates_decode_their_biped_masks`, `skyrim_npc_wnam_skin_census_matches_the_byte_walk`).
- **Evidence**: grep for `equip_slots` / `HandCatalog::from_index` in `tests`: only the hand-built unit tests. My independent scan (above) shows the data does agree today, including 66 Dragonborn shields whose `ETYP` must be remapped through the master index, so the gap is latent, not a live bug.
- **Impact**: a regression in EQUP group dispatch, `remap_fid` of `ETYP`, or `editor_id` lookup (`slot_hands` identifies the hand leaves by EDID `LeftHand`/`RightHand`) would silently make every shield read as an ordinary armor and every two-hander as a one-hander, with all unit tests green.
- **Related**: #7398.
- **Suggested Fix**: an `#[ignore]`d real-data test (Skyrim SE load order, `test_paths::skyrim_se_data_dir()`) asserting 7 EQUP slots, `slot_hands` for the five hand slots, >= 237 shields in the catalog and every `ETYP: Shield` ARMO a catalog shield.

### SK-D3-04: Phase 1's readers answer "nothing" (0) for a hand that holds a spell or torch
- **Severity**: LOW
- **Dimension**: NPC Equip (Hand Equipment; readers)
- **Location**: `crates/scripting/src/hands.rs:100-113` (`item_type_code`), `crates/scripting/src/fragment/vm/natives/actor.rs:105-130`
- **Status**: STALE at publish — Hand Equipment Phase 2 (`d3460fa73`, pushed after this audit's HEAD) returns 9 for a readied spell and 11 for a torch (`HandItem::item_type_code`). Was: Existing #7399
- **Description**: PRD section 1 declined these scripts precisely because "answering 'nothing in the left hand' would be a fabrication". After 50e8e7712 the VM answers 0 for any hand with no weapon or shield, but the engine has no spell-in-hand or torch state, so the player's default Flames in the right hand, an NPC mage's spells, or a carried torch read as `0` instead of the CK's 9 / 11. The doc comment says so ("9 a spell and 11 a torch are Phase 2's"); the VM checker does not decline.
- **Evidence**: `HandItem` has only `Empty | Weapon | Shield`; `hand_item` never returns anything for spell/light. Measured side: the 567 weapon-rack references use the hand readers; whether their scripts branch on 9/11 was not decompiled here.
- **Impact**: any vanilla script that tests `== 9` / `== 11` (the oil traps, once #7403 lets them run) sees the wrong answer; scripts that only test weapon types are unaffected. Small today because the oil traps still decline at `HasMagicEffectWithKeyword` (#7403).
- **Related**: #7399, #7403, PRD HND-READ-001, acceptance 3.
- **Suggested Fix**: none beyond Phase 2; until then consider making the checker decline `GetEquippedItemType` for a script whose condition compares against 9 or 11.

### SK-D3-05: NPCs spawn with a shield and a two-handed weapon in the same loadout; the hand readers and the biped readers disagree
- **Severity**: LOW
- **Dimension**: NPC Equip (spawn path vs hand rules)
- **Location**: `byroredux/src/npc_spawn/equip.rs:374-420` (weapon = highest damage; every ARMO `equip`ped by biped mask), `crates/scripting/src/hands.rs:140-165` (left hand: a `BothHands` right weapon wins over a worn shield), `crates/scripting/src/fragment/vm/natives/actor.rs:78-100` (`GetEquippedShield` reads the biped bit), `crates/scripting/src/fragment/effects/inventory.rs:262-270` (a non-player `EquipItem` of a shield clears only `left_hand`, not a two-handed `weapon`)
- **Status**: Existing: #7399 (NPC loadouts, design section 2 rule 2) and #7402 (dual wield); measurement added here
- **Description**: spawn equips the single best-damage weapon and every armor, with no hand rules (declared gap, design section 2). Skyrim data makes the collision real: 22 `Skyrim.esm` `NPC_` records carry a `BothHands` weapon as their highest-damage direct `CNTO` weapon and also a shield in `CNTO` or the default outfit (Civil War soldiers and archers such as `CWSiegeImperialArcher`, `CWSiegeSonsSoldier`, plus `CaptainAldis`, `Brond`, `Liesl`; shield side counts every entry of a leveled outfit item, so it is an upper bound). For them `GetEquippedItemType(0)` returns the weapon's code and `GetEquippedWeapon(true)` the weapon, while `GetEquippedShield()` returns the shield, and the shield mesh is drawn beside a bow or greatsword. HND-SLOT-003 ("equipping an item MUST unequip what held the hands it takes") is applied only on the player path; the non-player scripted shield `EquipItem` leaves a two-hander in place.
- **Evidence**: Python scan of `Skyrim.esm` (`NPC_` `CNTO` + `DOFT` -> `OTFT` `INAM`, WEAP `DATA` damage at offset 8, `ETYP == 00013F45`, ARMO `ETYP == 000141E8`); readers as cited.
- **Impact**: cosmetic and script-answer inconsistency on a few dozen vanilla actors; no crash.
- **Related**: #7399, #7402, SK-D3-04.
- **Suggested Fix**: Phase 2's NPC rule (shield only beside a one-handed weapon) at spawn; make the non-player shield `EquipItem` also clear a two-handed `weapon`.


**Routed out.** /audit-gameplay: a restrained NPC (d349856b2) still chases in combat, because the combat movement systems read `collect_unconscious`, which excludes `restrained`; the CK says restrained actors do not move, the commit says they "still fight" — a decision for that audit. /audit-esm: the NPC_ / RACE list-merge fold (4870178b3) has no Skyrim real-data pin.

**Checked, no finding.** The EQUP decode and `slot_hands` match the CK *Equip Slot* page and a raw scan of the five masters (7 slots; 237 / 7 / 66 / 2 shields across Skyrim / Dawnguard / Dragonborn / Update); the 0–12 item-type codes match the CK `GetEquippedItemType` page; `left_hand` is an `InventoryIndex`, not a session handle, with the v148 bump and shape guard in place; every clear path except the partial-count one (SK-D3-01) releases `left_hand`; #6691, #4232, BO64 and the WEAP `DNAM` speed are correct for Skyrim. Guards: plugin `equip::` 65, bin 135 + 65, scripting 3, all passing; `skyrim_bodt_body_templates_decode_their_biped_masks` and `skyrim_npc_wnam_skin_census_matches_the_byte_walk` pass. `m41-equip.sh` / `p3-player-body.sh` launch the engine and were not run.

## Dimension 4: Multi-Master Load Order + TES5 Cell-Load Regression

### SK-D4-01: `EsmReader::skip_group` with opaque collection on recurses without the GRUP depth cap; a crafted mod plugin aborts the process
- **Severity**: HIGH (precedent: #3237, #3503 and #5687 are the same defect class and carry `high`)
- **Dimension**: Multi-master load order, ESM reader (mechanism owner `/audit-esm`; found through this dimension's fold)
- **Location**: `crates/plugin/src/esm/reader.rs:1181-1204` (`skip_group`, `note_skipped_content`); enabled by `byroredux/src/cell_loader/load_order.rs:1057` (`ParseOptions { collect_opaque: idx > 0 }`)
- **Status**: NEW — same class as closed #3237 / #3503 (both HIGH), at a new site (regression of the #3237 / #3503 class at a new site, introduced by 55f4f8bee, LMC-REC-010). Searched open and closed issues for `note_skipped_content`, `collect_opaque`, "nested GRUP", "skip_group recursion": nothing covers it.
- **Description**: With collection on, `skip_group` calls `note_skipped_content(end)`, which reads the skipped group's content and calls itself for every nested GRUP with `group_end = (pos + group_content_len(&group)).min(end)`. There is no `depth` parameter and no use of `bounded_group_content_end` / `MAX_GRUP_NESTING_DEPTH` (64). `reader.rs` documents exactly this shape as forbidden ("A self-recursive walker must not call this ... thread a `depth` parameter"). Collection is on for every plugin after the first in the fold, and any top-level GRUP label with no dispatch arm reaches `skip_group` (`records/parse.rs`, the `_` arm; also the FO4+-only labels on Skyrim). A nest of minimal 24-byte GRUPs therefore recurses once per 24 bytes of file.
- **Evidence**: probe crate against `byroredux-plugin` (path dependency, scratchpad), crafted Skyrim-header plugin (HEDR 1.71, form version 44) with one top-level `ZZZZ` GRUP holding N nested empty GRUPs, parsed on a 2 MiB thread (the rayon worker stack the fold uses):

  | build | N | file size | `collect_opaque` | result |
  |---|---|---|---|---|
  | dev | 20,000 | 480 KB | false | ok |
  | dev | 20,000 | 480 KB | true | `thread has overflowed its stack ... aborting`, exit 134 |
  | release | 15,000 | 360 KB | true | ok |
  | release | 20,000 and 50,000 | 480 KB and 1.2 MB | true | stack overflow, exit 134 |

  The existing table-driven guard `every_recursive_grup_walker_honours_the_shared_depth_cap` (`crates/plugin/src/esm/records/grup_walker.rs`) lists the `records/` and `cell/` walkers and never calls `skip_group` with collection on, so nothing fails.
- **Impact**: any `.esp` / `.esm` that is not the first plugin of a profile (every mod; the official DLC too) can abort the engine at load, before any cell opens, with an uncatchable error rather than a skipped group. Vanilla Skyrim data is unaffected (the real plugins nest at most a few levels; the SE set parses and folds). Hostile or corrupted mod input only, but this is the load path the LMC work opens to arbitrary mods.
- **Related**: #3237, #3503, #5687, #4644 (clamp), LMC-REC-010 / 55f4f8bee.
- **Suggested Fix**: give `note_skipped_content` a `depth` and stop descending at `MAX_GRUP_NESTING_DEPTH` (or make it iterative with an explicit stack and a depth counter), clamping to the parent end as `bounded_group_content_end` does; add `skip_group` with `enable_opaque_collection` to the table in `every_recursive_grup_walker_honours_the_shared_depth_cap`.

**Checked, no finding.**
- The LMC fold wave on Skyrim's data: conformance oracle green on the 10-plugin SE order (878,404 refs, 19,272 navmeshes, 9,532 provenance winners, 0 tombstones kept, fold 3.0 s); boot activation hoists masters and names a missing `--master`.
- `merge` vs `legacy` on vanilla: 0 records merged; the only forms two non-mastering plugins both carry are CONT `0007434B` / `0007434D` / `0007434E`, where Dawnguard's copy is an ITM, so the result is the same under both policies.
- e4a03118b keeps last-write-wins in load order (`the_master_closure_matches_a_walk_of_the_graph`); chain fold 0.88–1.04 µs per record from 500 to 4,000 plugins.
- 55f4f8bee's opaque retention changes no decoded result (`structural_digest` equal on and off for all 5 masters); cost 24.2 MB (26.0 MB with CC), about 20% more parse time per DLC.
- 970d6bb3f: no SE or LE compressed record exceeds the bound (worst ratio 94.5:1 against 2048:1).
- Use Land Data children: 308 cells from `Skyrim.esm`, 307 over the merged official set; XCCM all 214 → REGN; 71 deleted NAVMs leave the fold; Skyrim authors no partial records or EDID-sharing cells.
- DIAL `QNAM`: no empty `quest_refs`; 19,757 DIAL / 40,927 INFO equal a raw walk; `ms01_eltrys_authored_placement` passes.

**Notes, not filed.** Two Skyrim cases of open #7329 (GMST `0123C00E` authored with 0 masters; Dawnguard ACTI `02014760` VMAD value `0307B5B9`), nothing aliased in the official order. Open question: for a re-authored INFO whose `PNAM` differs from its master position (Dragonborn's override of topic `00026AF5`), 5 INFOs keep `Skyrim.esm`'s positions rather than Dragonborn's chain order; the local CK and UESP dumps do not say which order the game uses.

## Dimension 5: Archives + Corpus Gates

### SK-D5-01: The real-data `ArchiveSet` agreement test exercises 3 Skyrim SE archives, not the series the index exists for
- **Severity**: LOW
- **Dimension**: Archives + corpus gates (test-gap)
- **Location**: `byroredux/src/asset_provider/tests/archive_set.rs` (`the_index_agrees_with_the_scan_on_real_archives`, the `"Skyrim SE"` pool list: `Skyrim - Meshes0.bsa`, `Skyrim - Meshes1.bsa`, `Skyrim - Textures0.bsa`, `Dawnguard.bsa`, `Update.bsa`)
- **Status**: NEW (no open or closed issue matches; `gh issue list --state all --search "ArchiveSet index"` is empty)
- **Description**: `Dawnguard.bsa` and `Update.bsa` are Skyrim LE archives. Skyrim SE folds those DLCs into the main archives and ships neither (the SE `Data` has 23 BSAs, none by those names), so the filter `Archive::open(..).ok()` drops them silently and the SE leg of the test checks 3 archives (its own output: `Skyrim SE: 3 archives, 118728 lookups`). The commit message's "agrees on every entry of real FNV, Skyrim SE and Fallout 4 archives" therefore covers neither `Textures1`..`Textures8` (the series the sibling auto-load feeds), nor the five CC / Anniversary archives, nor `Animations`/`Misc`/`Sounds`/`Voices` (the other pools). The test cannot detect an ordering regression across a real multi-archive Skyrim pool (the thing the index changed).
- **Evidence**: test output above; `ls Data/*.bsa` on the SE install shows no `Dawnguard.bsa` / `Update.bsa`. The gap is closed for this audit by the probe: the same agreement property over the full 23-archive SE set in profile order (864,805 lookups) and the 9-archive LE set (663,775) has 0 disagreements and 0 hash collisions, so there is no live defect.
- **Impact**: none today; the index is correct on real data. A future change to push order, sibling expansion or key hashing would not be caught by the Skyrim leg.
- **Related**: ae0a768f7 (LMC-PERF-003), #6670.
- **Suggested Fix**: Name the SE series in the list (`Skyrim - Textures0..8.bsa`, `Skyrim - Animations.bsa`, a CC archive) and open the pool through `open_with_numeric_siblings` so the test also covers the real sibling order; keep the LE names under an LE leg through `skyrim_le_data_dir`.

**Checked, no finding.** `ArchiveSet` (ae0a768f7) keeps last-listed-wins (the index stores the last-pushed position and confirms with `contains`, else scans); a probe of the index logic against a linear scan over every key in profile order and five spellings gave 0 disagreements (SE 864,805 lookups, LE 663,775) and 0 xxHash64 collisions; `Textures0` still pulls `Textures1..9` in order; the HUD, terrain-height, material and sound pools go through the same `extract_first` / `contains` / `winner` calls; no archive key starts with `data\` on SE or LE. Corpus and census numbers are in the Executive Summary. Bin tests `archive_set` / `archive_siblings` / `loose_override` / `archive_precedence` 36 passed; `byroredux-bsa` 124 passed. Not run: the real-data render trace (needs the engine).

---

## Shader-Type Coverage Matrix

Render for types 1, 11 and 16 was re-read in `triangle.frag` this run; 2–7 and 14 are carried from the 2026-10-27 / 10-31 baseline (their shader paths had no commits since).

| Type | Name | Arm | Parse | Import | Render |
|---|---|---|---|---|---|
| 0 | Default | None | ✓ | ✓ kind 0 | ✓ generic lit |
| 1 | Environment Map | EnvironmentMap | ✓ | ✓ `env_map_scale` + latch; slots 4/5 → Environment/EnvironmentMask | ✓ `hasExplicitEnvironment`; classifier reach = #6816 |
| 2 | Glow | None | ✓ | ✓ slot 2 glow behind `GLOW_MAP` | generic + emissive (baseline) |
| 3 | Parallax | None | ✓ | ✓ slot 3 → Height | baseline, not re-read |
| 4 | Face Tint | None | ✓ | ✓ slot 3 → Detail, `detail_neutral_for` | ✓ baseline |
| 5 | Skin Tint | SkinTint | ✓ | ✓ | ✓ baseline |
| 6 | Hair Tint | HairTint | ✓ | ✓ | ✓ baseline |
| 7 | Parallax Occ | ParallaxOcc | ✓ | ✓ | ✓ baseline (POM) |
| 8–10, 12, 13, 15, 17–20 | landscape / LOD / snow / tree / cloud / dismember | None | ✓ | ✓ | generic (no per-type branch) |
| 11 | MultiLayer Parallax | MultiLayerParallax | ✓ | ✓ slot 6 → InnerLayer | ✓ inner layer, refraction UV, unmasked reflection |
| 14 | Sparkle Snow | SparkleSnow | ✓ | ✓ | ✓ baseline |
| 16 | Eye Envmap | EyeEnvmap | ✓ | ✓ `zup_to_yup_pos` centres, `pack_eye_fields` | ✓ `isEyeEnvmap` branch (doc stale: SK-D2-01) |

## Cell-Load Regression Status

| Check | Result |
|---|---|
| TES5 cells through the unified walker | `parse_real_skyrim_esm` passes (finds `SolitudeWinkingSkeever`, 92-byte XCLL); `--list-cells` on the merged official order shows `Forelhost01` (1,578 refs) and `SolitudeWinkingSkeever` (994 refs) with names resolved |
| Use Land Data children | 308 cells inherit Tamriel's LAND (#6594 fix holds) |
| Whiterun BanneredMare entities / FPS | not re-run; bench-of-record 5,853 entities, 83.7 FPS (`d9a201c71`, 2026-10-22) |

---

Publish with: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-11-23.md` (label every finding `game:skyrim` + `legacy-compat` plus its domain; SK-D4-01 is `esm-plugin` + `safety`, and its reach is every game's mods, not Skyrim's alone).
