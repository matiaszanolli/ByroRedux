## # FO3-D3-01: FO3 legacy LOD quads are not anchored at (0,0) in 26 of 29 worldspace/level sets — 425 of 697 object-LOD quads are never requested, including all 65 #3502 level-8-only quads
State: OPEN
Labels: bug, medium, legacy-compat, game:fnv, game:fo3, terrain-exterior

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

## # FO4-D2-01: the #3905 spawn neutral-roughness pass overwrites the glass classifier's mirror roughness (0.04 → 0.5) on BGSM mirror panes
State: OPEN
Labels: bug, low, legacy-compat, game:fo4, nifal

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D2-01)

- **Severity**: LOW (latent; 0 vanilla FO4 shapes and 0 FO76 shapes). It escalates to HIGH under the NIFAL divergent-`Material` rule the day a BGSM mirror pane exists.
- **Dimension**: BGSM/BGEM merge (the #3639/#3905 neutral-roughness pair)
- **Location**:
  - `byroredux/src/material_translate.rs:1360-1379` (`unresolved_gloss_neutral_roughness`).
  - `byroredux/src/helpers.rs:11,183-196` (`MIRROR_ROUGHNESS = 0.04`, the mirror-pane branch).
  - The spawn call sites: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1367` and `byroredux/src/scene/nif_loader.rs:1504`.
- **Status**: NEW. The interaction is pre-existing: 7e2abb730 (#3905) landed on top of the mirror branch e5d02f83d. No matching issue exists.
- **Description**:
  - The spawn pass infers "the BGSM's `(1 − smoothness)` hit the clamp floor" from `roughness <= 0.04`.
  - The glass classifier's mirror-pane branch also writes exactly 0.04 (with metalness 1.0) on a kind-0 material, and `from_bgsm` does not block it.
  - The spawn pass then runs later, sees 0.04 with `bgsm_pbr_scalars_authored` and no gloss handle, and rewrites roughness to 0.5.
  - The result is metalness 1.0 / roughness 0.5: a blurred chrome sheet that is neither authored nor the classifier's mirror.
- **Evidence**: `helpers.rs:183-195` versus `material_translate.rs:1369-1378`; no test combines a mirror pane with `bgsm_pbr_scalars_authored`. FO4 corpus probe, 226,068 NIFs: 26 mirror-named shapes, 0 mirror panes. Vanilla BGSM mirrors are authored kind 1 (EnvironmentMap), and the branch returns early for those. FO76: 62 mirror-named NIFs, 0 panes.
- **Impact**: none on vanilla. Modded BGSM panes named `*mirror*` on a glass-keyword texture with no smooth-spec map would hit it.
- **Related**: #3639, #3905, #4255, #5012.
- **Suggested Fix**: carry an explicit "roughness at the BGSM floor" provenance bit from the merge, and gate the spawn pass on that rather than on the value. Alternatively, have the classifier's forced mirror state exempt itself. Pin it with a BGSM mirror-pane fixture.

## # FO4-D4-02: PKIN expander models CNAM as a content base; every vanilla CNAM is a template CELL and 0 vanilla REFRs place a PKIN
State: OPEN
Labels: bug, low, legacy-compat, game:fo4, esm-plugin, doc-rot

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D4-02)

- **Severity**: LOW (0 vanilla reach; doc/data-model rot plus a mod-only silent miss)
- **Dimension**: ESM architecture records + cell expansion
- **Location**: `crates/plugin/src/esm/records/pkin.rs:1-60`, `byroredux/src/cell_loader/refr.rs:529-614`
- **Status**: NEW. #589, #815, #1180, #2611 and #2612 (all closed) assumed the LVLI/CONT/STAT/MSTT/FURN content model.
- **Description**:
  - Docs and expander treat `CNAM` as "the content base record" and emit one synthetic child with `child_form_id = CNAM`.
  - On real data the CNAM is the pack-in's template CELL.
  - The CK bakes pack-in contents into ordinary REFRs at placement time, so vanilla carries no PKIN-based REFR.
- **Evidence**:

  | Plugin | PKIN | CNAM → CELL | REFRs with a PKIN base |
  |---|---|---|---|
  | Fallout4.esm | 872 | 872 | 0 |
  | DLCRobot | 21 | 20 | 0 |
  | DLCCoast | 64 | 64 | 0 |
  | DLCworkshop03 | 6 | 6 | 0 |
  | DLCNukaWorld | 46 | 46 | 0 |

  The census scripts are in `/tmp/audit/fo4/d4/pkin*.py`.
- **Impact**:
  - Vanilla: none. The expander is never reached, and nothing is double-spawned.
  - Mods: a mod-placed PKIN REFR would emit a child whose base is a CELL, miss every base lookup and spawn nothing.
  - `pkin_expansion_tests` encode the wrong model.
- **Related**: #589, #1180, #2611, #2612.
- **Suggested Fix**: correct the docs ("CNAM = pack-in template CELL"); then either instance that CELL's references under the outer transform, or log CELL-typed CNAMs as an explicit miss; add a real-data assert that the CNAMs resolve to CELLs.

## # FO4-D5-01: a present-but-unopenable BA2 makes every FO4 real-data NIF gate skip green (#4660's fix missed this harness)
State: OPEN
Labels: low, legacy-compat, nif, game:fo4, test-gap

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D5-01)

- **Severity**: LOW (test harness; no runtime impact)
- **Dimension**: Archives + real data
- **Location**:
  - `crates/nif/tests/common/mod.rs:428-442` (`open_all_mesh_archives`)
  - `crates/nif/tests/common/mod.rs:487-506` (`open_ba2_by_name`)
  - `crates/nif/tests/parse_real_nifs.rs:316-364` (`run_all_meshes_gate`)
- **Status**: NEW. This is a sibling site that #4660's fix (ec63d2636) did not reach. #4660 named the bgsm, facegen, bsa, hkx and menuxml harnesses; `crates/nif/tests/common` was not among them.
- **Description**:
  - Both helpers treat "file exists, but `open` returns `Err`" exactly like "file missing": they print `skipping: failed to open …` and return `None`.
  - `run_all_meshes_gate` then `continue`s and never asserts `walked > 0`. `run_game` returns early.
  - The strict `BYROREDUX_REQUIRE_GAME_DATA` lane (#3850) only hardens the missing-data-dir case.
- **Impact**: a BA2 reader regression that rejects vanilla FO4 mesh archives would turn `parse_rate_fo4_all_meshes`, `parse_rate_fallout_4` and the baseline harnesses into passing no-ops. That is the class #5008 just widened, by making a former warn a hard `InvalidData`. `crates/bsa/tests/ba2_real.rs` `expect`s `Fallout4 - Meshes.ba2` and `Textures1.ba2` to open. Nothing covers `MeshesExtra.ba2` or the six DLC `Main.ba2`.
- **Related**: #4660 (parent sweep), #5008, #4628 (missing-file variant on FO76), #3850, #2334.
- **Suggested Fix**: panic on an `open` error when `is_file()` is true; keep `None` only for absent files. Assert `walked > 0` in `run_all_meshes_gate` whenever the data dir resolved.

## # FO4-D2-02: the #3639 "template parent supplies the gloss map" case is unpinned; the sibling test claims coverage it lacks
State: OPEN
Labels: low, legacy-compat, game:fo4, test-gap

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D2-02)

- **Severity**: LOW (test gap; production is correct)
- **Dimension**: BGSM/BGEM merge
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs:1052-1090`
- **Status**: NEW
- **Description**:
  - The fallback at `merge.rs:1291` runs after the full chain walk, so a parent's `smooth_spec` correctly disarms it.
  - The guard's doc claims "even from a template parent", but its fixture puts `smooth_spec_texture` on the leaf with `parent: None`. No test has a smooth-spec supplied only by a parent.
  - Two regressions would stay green: moving the check inside the loop, or keying it on the leaf's own path.
- **Impact**: guard gap only. 0 vanilla BGSMs are in this state.
- **Related**: #3639, #5012.
- **Suggested Fix**: add a chain fixture (leaf: spec on, smoothness 1.0; parent: `smooth_spec_texture`) that asserts `roughness_override == Some(0.04)`, and fix the sibling doc.

## # RT-2026-10-03-02: Oblivion GildedCarafe TSV — newest regen blocks are contract-derived one-row edits buried under an older block that contradicts them; entities_total lags the live capture by one
State: OPEN
Labels: documentation, low, tech-debt, game:oblivion, doc-rot

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-03.md` RT-2 · **Severity**: LOW · **Dimension**: Baseline integrity · **Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe

**Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`: header lines 1–24 (09-30 #5125 block), 38–44 (10-03 #5189 block), 45–58 (09-29 #5123 block); rows `light_count_directional`, `entities_total`

| | Baseline | Current (HEAD `2c36c29d8`, live capture) |
|---|---|---|
| light_count_directional | 0 | 0 |
| entities_total | 929 | 928 |

## Description
The runtime skill's integrity rule is that each TSV's newest `# regenerated:` block explains the rows and that every row comes from one capture. This file breaks both parts:
- **Header order.** Line 1 is still the 2026-09-30 #5125 block. Its text says "light_count_directional stays 1 (RT-2, #5123, fixed)", but the row says 0.
- **Buried blocks.** The two blocks that actually set the row are #5189 (2026-10-03, 1→0) and #5123 (2026-09-29, 2→1). They are inserted at lines 38 and 45, inside the 09-16 narrative, so a reader taking `head` sees the wrong newest block.
- **No live capture behind the row.** Both edits are self-described "CONTRACT-DERIVED regen, not live-captured", so `light_count_directional` came from no capture.
- **Stale entity count.** Today's live run confirms the directional row at 0. It also shows the one fewer spawned light entity (`entities_total` 929→928) that the one-row edit left behind. The −1 is consistent with #5189's zero-spawn, but that is inferred, not bisected.

## Evidence
`grep -n '^# regenerated' .claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` lists 2026-09-30 (l.1), 2026-09-16 (l.25), **2026-10-03 (l.38)**, **2026-09-29 (l.45)**. The 2026-10-03 capture's `light.dump` shows 8 `kind=Point`, 0 `kind=Directional`, and `bench: entities=928`.

## Impact
None on the gate today (−0.1 % is in the ±2 % band; directional is exact). The risk is that the next reader attributes the directional row from the wrong block, and the file sets a precedent of editing gate rows without a capture.

## Related
#5189, #5123 (closed); #5125 (09-30 regen)

## Suggested Fix
Live-regen the Oblivion TSV (`/audit-runtime --game oblivion --regen`). Put a single newest block at line 1 that cites #5123/#5189 for the directional row and #5189 for the −1 entity. Correct the 09-30 block's "stays 1" sentence, or move it below.

## Completeness Checks
- [ ] **SIBLING**: The other baseline TSVs (and their newest `# regenerated:` block) checked for the same pattern
- [ ] **TESTS**: `cargo test -p byroredux --bin byroredux bench::` (runtime baseline schema tests) stays green after the edit

## # RT-2026-10-03-03: audit-runtime SKILL.md presents #5118/#4987/#5131/#5133 as open and its gate matrix omits the P4–P6 smokes and m48-5-fnv-hud
State: OPEN
Labels: documentation, low, tech-debt, doc-rot

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-03.md` RT-3 · **Severity**: LOW · **Dimension**: Harness integrity (doc rot)

**Location**: `.claude/commands/audit-runtime/SKILL.md:20`, `:24`, `:36`, `:108`

## Description
- **:108 (Oblivion neutralisation).** Says `scripts/check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`. `e04fa1ef6` (Fix #5118) now derives the neutralisation list from each fixture's `FIXTURE_DATA_ENV`, and `oblivion.env` declares `BYROREDUX_OBLIVION_DATA`. The script also fails if any harness `*_DATA` variable is left un-neutralised.
- **:24 (#4987).** Calls #4987 "known-open: the lane has not yet been shown to reach a device". It was closed by `6d5d8fa5f` ("make vulkan-validation prove it selected a device").
- **:36 (#5131–#5133).** Says the TSVs carry "still-unattributed" moves of #5131–#5133. `6fcf313e2` attributed #5131 and #5133, and both are closed. Only #5132 (the FO4 spot-light schema row) is open.
- **:20 (gate matrix).**
  - The FO3 `FIXTURE_GATES` example ("p0,p5,p2") omits `p5-f5-f9-quicksave` (the fixture declares `p0-door-interaction p5-save-restart p5-f5-f9-quicksave p2-melee-core`).
  - The playable-slice row omits `p4-quest-route`, `p5-door-transition`, `p5-f5-f9-quicksave`, `p5-quest-persistence`, `p5-soak` and `p6-loading-model`, all of which CLAUDE.md lists as gates.
  - The milestone row omits `m48-5-fnv-hud.sh`.

## Evidence
`grep FIXTURE_DATA_ENV docs/smoke-tests/fixtures/*.env`; `gh issue view 4987` / `5131` / `5133` → CLOSED, `gh issue view 5132` → OPEN; `ls docs/smoke-tests/`.

## Impact
An auditor could file a duplicate for the Oblivion contract, under-run the bless-a-build gate set (the skill says to run the gates you have data for), or treat the vulkan-validation lane as inert.

## Related
#5118, #4987, #5131, #5133, #5132

## Suggested Fix
Delete the :108 bullet. Replace the #4987 parenthetical with a pointer to `6d5d8fa5f`. Rewrite :36 as "#5132 open (spot row)". Extend the :20 gate lists from `docs/smoke-tests/` and the fixtures, then run `.claude/commands/_audit-validate.sh`.

## Completeness Checks
- [ ] **SIBLING**: Other audit skills that quote the smoke-gate list or #4987 (`grep -rn '4987\|FIXTURE_GATES' .claude/commands`) checked for the same staleness
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes with no new advisories

## # SAFE-D5-2026-10-05-01: `with_one_time_commands_inner` frees the command buffer and destroys/releases the fence on the `MaybeInFlight` wait-failure arm, though its own error says the commands may still be pending
State: OPEN
Labels: bug, renderer, low, vulkan, safety

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` — `SAFE-D5-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW.
  - The decision tree's "Vulkan spec violation → at least HIGH" floor was weighed and not applied. The violation is reachable only when `vkWaitForFences` with an infinite timeout fails with `VK_ERROR_OUT_OF_HOST_MEMORY` or `VK_ERROR_OUT_OF_DEVICE_MEMORY`. After a true `VK_ERROR_DEVICE_LOST`, destroying and freeing these objects is valid per the spec's "Lost Device" section.
  - #5201 rated this exact window LOW for the AS objects the same command buffer writes. This finding is that issue's sibling inside the helper itself.
- **Dimension**: Vulkan Spec Compliance
- **Location**: `crates/renderer/src/vulkan/texture.rs:931-942`, the `wait_for_fences` error arm of `with_one_time_commands_inner`. The class contract is at `:671-674`, and the reusable-fence guard at `:869`.
- **Status**: NEW.
  - #5201 (CLOSED, `65b0217f3`) made `build_blas_batched` consult `OneTimeCommandError::may_be_in_flight` before unwinding its ASes.
  - #4891 (CLOSED, `9cc77cec0`) introduced the `NotSubmitted` / `MaybeInFlight` split for the callers.
  - Neither touched the helper's own `cmd` / fence disposal, which dates from #1861. No issue or audit names it (searched `OneTimeCommandError free_command_buffers` and `MaybeInFlight fence`).
- **Description**:
  - #4891 defines `MaybeInFlight` as "`vkQueueSubmit` or the fence wait failed… the commands may be pending, so a host-side destroy could race an in-flight transfer: the caller must keep what they reference alive". Every caller now honours that:
    - `buffer.rs:914/1528/1634` and `texture_registry/upload.rs` leak their staging and destinations;
    - `blas_static.rs` calls `mem::forget` on `prepared` and `compact_accels`.
  - The helper that returns the error does the opposite with the objects it owns. On the fence-wait failure it:
    - calls `free_command_buffers(pool, &[cmd])`, which is invalid while the buffer is pending (`VUID-vkFreeCommandBuffers-pCommandBuffers-00047`);
    - destroys the fence it created (`VUID-vkDestroyFence-fence-01120`);
    - or, on the reusable-fence path, drops the guard, so the *next* caller's `reset_fences` hits a fence possibly still tied to pending work (`VUID-vkResetFences-pFences-01123`).
  - The `vkQueueSubmit`-failure arm (`:920-929`) is spec-fine. A failed submit leaves the command buffer not pending, except on device loss. Only the wait arm contradicts its own class.
- **Evidence**:
  ```rust
  if let Err(e) = device.wait_for_fences(&[fence], true, u64::MAX) {
      if owned {
          device.destroy_fence(fence, None);
      }
      drop(fence_guard);
      device.free_command_buffers(pool, &[cmd]);
      return Err(OneTimeCommandError::maybe_in_flight(
          e,
          "wait for one-time commands",
      ));
  }
  ```
- **Impact**: None in normal operation. On an OOM-from-wait (the renderer is already failing), the helper frees or reuses objects the GPU may still be executing. That is undefined behaviour of the same class #5201 closed one layer up, and it leaves the "one helper, one failure contract" policy inconsistent.
- **Related**: #5201, #4891, #1861, #1713. Owner overlap: `/audit-renderer` Dim 1 raised #5201; `/audit-concurrency` covers fence discipline.
- **Suggested Fix**: On the wait-failure arm, branch on the `vk::Result`:
  - `ERROR_DEVICE_LOST` keeps today's free and destroy;
  - any other code leaks `cmd` and the owned fence, and poisons or marks the reusable fence so it is recreated rather than reset.

  Pin the arm with the existing `one_time_failure_class_tests` needle scan. Per the *Speculative Vulkan Fixes* rule, confirm the classification under `BYRO_VALIDATION=1` with fault injection before landing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

## # GAME-D2-2026-10-05-01: FO3/FNV dialogue ignores each INFO's own owning quest — a multi-quest topic can speak an INFO whose quest is not running
State: OPEN
Labels: bug, low, gameplay, quests, dialogue, game:fnv, game:fo3

- **Severity**: LOW (mostly masked by `GetIsID`; misroutes the residue only)
- **Dimension**: 2 — NPC dialogue (P4 route, extended to FO3/FNV by #5224)
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:112-128` (`owned_topic_records`: any running bound quest in the DIAL's `quest_refs`)
  - `:226` (`select_first_info` walks every INFO of the DIAL)
  - `:240` (`owning_quest = quest_refs.first()`)
  - `crates/plugin/src/esm/records/misc/dialogue.rs:249-` (`InfoRecord` has no quest field)
- **Status**: NEW. The decode half belongs to `/audit-esm`, and the INFO/CTDA semantics to `/audit-scripting`.
- **Trigger**: FNV or FO3. Activate an NPC bound by a running quest that owns a shared Top-level topic, for example `DoctorMedical` `0x1DCEE` or `FollowersHired` `0x37084`.
- **Description**: in FO3/FNV, each INFO names its own quest (`QSTI`), and one DIAL can list several. In the GECK, an INFO counts only while its quest is running. That quest's priority orders the INFOs, and its dialogue conditions gate them all ("Quest conditions are checked first; only if those are true are the conditions on the infos evaluated", GECK *Quest Data Tab*). None of this is decoded or applied: once any one running bound quest owns the DIAL, INFOs from every listed quest compete in file order.
- **Evidence** (Python census of `FalloutNV.esm`):
  - All 23,247 INFOs carry `QSTI`.
  - 138 DIALs list more than one quest. In 28 Topic-type DIALs, the INFOs span more than one quest.
  - The Top-level (`DATA[1] & 0x02`) ones are `DoctorMedical`, `FollowersHired`, `FollowersFired`, `DoctorSupplies`, `DoctorRadiation` and four Gomorrah/Primm topics. Most of their INFOs have `GetIsID` (fn 72) conditions, which mask the problem: `DoctorMedical` has 34 INFOs, 5 without `GetIsID`; `FollowersHired` 26 and 2; `FollowersFired` 11 and 1.
  - `GREETING` (`0xC8`, 209 quests, 5,300 INFOs) has flags `0x00`, so #5224 makes it link-only, and it never opens a list.
- **Impact**: on these topics, an NPC can speak a line that belongs to a stopped quest, or skip a higher-priority quest's line. The NPC-side log also names the wrong owning quest. Skyrim (one `QNAM` per DIAL) is unaffected by the per-INFO half, but Skyrim quest dialogue conditions are not applied either.
- **Suggested Fix**:
  - Decode the INFO `QSTI` (FO3/FNV/Oblivion) into `InfoRecord.quest`.
  - In `select_on_topic` / `top_level_menu`, filter INFOs to running quests that bind or own the speaker, and order them by quest priority.
  - Have `/audit-scripting` decide on applying quest-level dialogue conditions.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D2-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix

