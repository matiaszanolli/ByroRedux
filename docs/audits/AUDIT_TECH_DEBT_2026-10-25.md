# Tech-Debt Audit — 2026-10-25

**HEAD**: c7463c04d · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56; 235 commits since) · **Audited**: all 9 dimensions, depth deep · One agent per dimension (Dims 5–7 shared one), read-only; the only side effects were an audit-only cargo target dir for Dim 8's clippy/machete runs and files under `/tmp/audit/tech-debt/`.

In one sentence: the 10-21 backlog is largely closed (clippy gate green, `cell_loader/exterior.rs` split, `parse_block_inner` and the ObScript `match cmd` giants gone, `core::paths`/BC4/pins repaired), and the debt of this window arrived with the crime / jail / VATS / AI-package / ObScript-table feature wave — hand-spelled accessors, stale status docs, a doc-splice class that recurred, and one file (`obscript_effects.rs`) crossing 2000 production LOC as a side-effect of the #6139 table refactor.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 4 |
| LOW | 50 |
| **Total** | **54** |

By dimension: Dim 1 ×6 · Dim 2 ×13 · Dim 3 ×11 · Dim 4 ×6 · Dim 5 ×0 · Dim 6 ×2 · Dim 7 ×5 · Dim 8 ×7 · Dim 9 ×4. Two Dim 9 findings were merged away as duplicates (TD9-02 into TD8-01, same `|| true` line; TD9-06 into TD3-02 site 8, same spliced doc block).

- **MEDIUM (4)**
  - `TD2-2026-10-25-01`: `papyrus_provider::owner_form_id` is a copy of `entity_global_form_id` made after #5588 without the player special-case (a player-owned script keys under local 1, not 0x14); eight more sites hand-spell the FormId→local step. The #5588 guard greps for the function *name*, so the rename passed.
  - `TD3-2026-10-25-01`: GPU-struct sizes in prose the size guard cannot see — ReSTIR `Reservoir` is 36 B since #6250 but `bindings.glsl:815`, `triangle.frag:3392` and the `restir.rs:859` assert message say 32; `bindings.glsl:529` says 208-byte for the 224-byte `GpuTerrainTile`.
  - `TD4-2026-10-25-01`: 11 "known-open / do not re-file" callouts in 8 skills name issues closed today; two sit over fixed code as active suppress directives (`audit-parsers:80` for #5446, `audit-starfield:66` for #5502). Fifth recurrence of the class.
  - `TD9-2026-10-25-01`: five closed-HIGH real-data guards (#5629, #5989, #5013, #6120, #5005-FO3) are absent from `.github/real-data-pins.txt`, and four run in no lane at all.
- **Gates**: the blocking clippy gate is **green** (35/35 packages, `-D warnings`); `cargo machete --with-metadata` clean. The non-blocking `--all-targets` step re-accrued **11** sites (was 36 on 10-21, re-zeroed by #6193 on 10-22), one of them deny-level (`TD8-2026-10-25-01`).
- **10-21 backlog verified**: #6272 (BC4) and #6274 (`core::paths`) hold; #6273 is closed by drift guards only (the table-driven rewrite was not done — mitigated, not consolidated); #6275 (FO4 ragdoll rows) fixed; #6176 recurred as TD4-05. **#6223 (WTHR decoders) is still open and slightly worse** (`parse_wthr` 306 → 320 lines; #6258 put the Oblivion NAM0 cloud-tint rule inside the classic arm — the "fourth arm" outcome the last report predicted).
- **Issue housekeeping the agents verified (recommendations, not actions taken)**: #6222 (clippy gate, OPEN) is fixed at HEAD; #5586 (OPEN) has a dead premise — `mq101_conformance.rs::run` is 45 lines, decomposed in `d7c47d741`; #4747 (OPEN) is already fixed in the tree; #5622's site list is stale (32 bare `allow(clippy::…)` by its own rule, 5 not on its list).

### Delta vs the 2026-10-21 baseline

| Metric | 10-21 (4a3e89d56) | 10-25 (c7463c04d) | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 26 | 26 | no new token added in 235 commits; 18 `XXXX` + 2 `HACK` FourCC + 3 quoted upstream FIXMEs + 1 documented `TBD`; 2 real TODOs cite open #6194; 0 shader hits; `triangle.frag` MIT/Burley block intact (L22–40) |
| `allow(dead_code)` | 41 | 41 | (the skill's 10-24 paragraph said 43; 12 are `cfg(test)` markers in `lock_tracker.rs`) |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` tests | 363 | 373 | +12 added, −2 removed; none bare; all new ones name their data gate |
| production files >2000 LOC | 4 | **5** | new: `scripting/src/obscript_effects.rs` 1537 → 2034 (#6139 stages 6–7). `audio/lib.rs` 2086 (+52), `nif/import/types.rs` 2011, `mq101_conformance.rs` 2157, `shader_constants_data.rs` 2972 (data table) |
| production fns >200 / >300 / >500 / >1000 | 212 / 85 / 29 / 5 | **206 / 83 / 28 / 3** | scanner re-calibrated on the base tree: reproduces the old numbers exactly |
| test-heavy files >2000 total LOC | 92 | 93 | none escalates |
| blocking clippy gate | red (REG-2026-10-21-01) | **green** | #6222 can close |
| `--all-targets` (non-blocking) unique sites | 36 | 11 | regression of closed #6193 |
| `file.rs:LINE` refs past EOF | 36 | 7 | #6167 |
| `register_update_systems` | 854 | 981 | not in #6143's list (TD1-05) |

## Baseline Snapshot (Phase 1, measured at 9634aa362; tree unchanged for counted classes at c7463c04d)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 26
allow(dead_code):      41
unimplemented!/todo!(): 0
#[ignore] tests:        373
files >2000 production LOC: 5
test files >2000 total LOC (lower priority, separate bucket): 93
```
`prod_loc_self_test`: ok. `_audit-validate.sh`: OK, all path references valid (3704 refs, 123 skill files); 371 symbol advisories in `docs/engine` (≈230 intentional EditorID / script / asset names, ≈60 in plan docs for never-built types, ≈35 false positives from the validator ignoring scripts / `.cpp` / `Cargo.lock` — TD3-10, 6 real renames — TD3-08), 1 in CLAUDE.md (`CRHoldExpansion`, a vanilla Skyrim Story Manager node name — not a finding).

## Top 10 Quick Wins (trivial / small)

1. **`TD3-01` (MEDIUM, trivial)**: fix the four prose sizes (36 B reservoir ×3, 224-byte terrain tile) and `audit-renderer/SKILL.md:75`; optionally add `Reservoir` to `pinned_sizes()`.
2. **`TD4-01` (MEDIUM, small)**: rewrite the 11 callouts in past tense with the fix SHA; delete the two `do not re-file` phrases (`audit-parsers:80`, `audit-starfield:66`). Then add the advisory `gh`-backed check from TD4-03 so the class stops recurring.
3. **`TD9-01` (MEDIUM, small)**: append the five pin rows and a `scripting-lib` lane (`run-real-data-pins.sh` arm + `real_data_pin_lane_sources` arm); verify with `scripts/run-real-data-pins.sh --list`.
4. **`TD8-01` (+ merged TD9-02, small)**: drive `--all-targets` back to zero — drop the `&`s in `actor/tests.rs`, the `clone_on_copy`, name the tuple in `texture.rs:1272`, and replace the `|| true` tautology at `parse_real_esm.rs:6467` with a real assertion or delete it.
5. **`TD2-01` (MEDIUM, small)**: delete `owner_form_id`, call `entity_global_form_id`, add `FormIdPool::local_of`, widen the #5588 guard.
6. **`TD3-02` (small)**: move eight spliced doc blocks onto their items; generalise `no_item_is_spliced_into_another_items_doc_block` into `crates/core/src/source_scan.rs` and run it over `byroredux`, `scripting`, `plugin` (three sites landed after the renderer-only #6268 sweep).
7. **`TD3-03..06` + `TD6-02` (small, one doc pass)**: reword the seven "regen is inert" sites, the six AI-procedure "parse-only" sites, the crime sentences (`scripting.md:809`, `ROADMAP.md:276`, `feature-matrix.md:206/238`, `owned.rs:19-22`) and the `RigidBodyData` save-column prose. Feeds the next `/session-close`.
8. **`TD8-02..05` (small, mechanical deletions)**: write-only `txst_textures` map; five dead `_`-prefixed params; three zero-caller pub fns (`populate_scene_fragments_from_pex_detailed`, `NiTriShape::name_str`, `bodies_overlapping_body`); five test-only pub helpers → `#[cfg(test)]`.
9. **`TD2-04` + `TD2-07` (trivial)**: `Inventory::count_of` (7 production sites + 4 test copies) and `world.init_resource::<T>()` at the 3 stragglers.
10. **`TD2-09` + `TD2-10` + `TD2-11` (trivial)**: merge `block_bound`/`world_bound`; add `depth_read_only_image_info`/`image_info` helpers; route six `mod tests {` cuts through `source_scan` and extend the weak-needle list.

## Top 5 Medium Investments

1. **`TD1-01` + `TD2-06`: shrink and split `obscript_effects.rs`** (medium). Fold the pasted 6-line caller prologue (×45, 15 with a `Binder`) into `run_row` first — the saving is mechanical and should pull the file back under 2000 on its own (estimate, ≈45 × 5 lines) — then split into `obscript_effects/{mod, getters, inventory, combat, actor_state, world_state, presentation, tests}.rs` along the domains the two tables already read in. `mod.rs` re-exports keep the 8 external call sites unchanged.
2. **`TD1-02` + `TD1-03` + `TD2-02`: `inventory.rs` (1956) and `npc_spawn.rs` (1911) before the next feature crosses 2000** (medium each). Axes: `inventory/{catalog, loot, actions}`; `npc_spawn/{stamps, fallback_physics, skeleton_paths, anim_clips, equip}`. Both are read verbatim by `include_str!` censuses in `npc_spawn/tests.rs` (3288/3373/3375/3401/3416) and `material_translate.rs:2817` — re-point them in the same commit or the pins go vacuous. In the same pass, collapse `insert_at_spawn` / `insert_at_runtime` / `clear_ambient_behavior` (~165-line twin matches that Find, UseWeapon, Flee and Ambush each edited three times).
3. **`#6223`: one Creation-era WTHR decoder** (medium; Existing, getting worse). Unchanged recipe from the 10-21 report; now with a 4th arm to fold in.
4. **`TD1-04`: `gpu_timers.rs` timer-bracket table** (small–medium). 50 near-identical `cmd_*_start/end` methods (~850 of 1929 lines) become one `TimerPass` row table; keep the `Q_*`/`BIT_*` consts the pin tests parse; verify under `BYRO_VALIDATION=1` (*feedback_speculative_vulkan_fixes*).
5. **`TD1-05` + Existing `#6143`: give the schedule registrars a regrowth budget** (small). `register_update_systems` 854 → 981 and `register_late_systems` 849 are not on #6143's list; split by contiguous order segments (registration order is the contract) and pin with a `*_LINE_BUDGET` like `draw.rs:1433`.

Honourable mention: **`TD2-03`** — a `PlayerEntity::get(world)` accessor for the binary (66 production sites in 34 files, 9 added this window; #5813 stopped at the scripting crate boundary). Mechanical sed plus a hygiene scan; also gives the two-resource split (`PlayerEntity` vs `PapyrusPlayerEntity`) exactly two accessors to reason about.



## Findings

### MEDIUM

#### TD2-2026-10-25-01: `papyrus_provider::owner_form_id` is a copy of `entity_global_form_id` that omits the player special-case; eight more sites hand-spell `pool.resolve(id).map(|pair| pair.local.0)`
- **Severity**: MEDIUM (promotion: duplicated logic where the copy lacks the fix the original carries)
- **Dimension**: 2
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:24-31` (copy); original `crates/scripting/src/condition/eval.rs:220-229`; inline FormId→local spellings: `byroredux/src/systems/vats.rs:353` (new, 7eb4de261), `byroredux/src/systems/trap_hits.rs:95`, `byroredux/src/cell_loader/spawn.rs:488,569`, `byroredux/src/cell_loader/stream_snapshot.rs:291`, `crates/physics/src/sync.rs` (2)
- **Age**: `owner_form_id` 2f4b590412 (2026-10-19) — eight days after #5588 (badc… 2026-10-11) consolidated "byte-identical private fns in two crates plus an inline third copy" into `entity_global_form_id`. The #5588 guard (`entity_global_form_id_has_one_definition`, `systems/eat_sleep.rs:624`) greps three files for the literal `fn entity_global_form_id(`, so a re-copy under another name passes.
- **Effort**: small
- **What/why**: `entity_global_form_id` first maps the player entity to `PLAYER_REFERENCE` (0x14) because the player's pool entry is local 1 (its doc: "a new writer that forgot keyed the player under the wrong id"). `owner_form_id` is the same `FormIdComponent` → `FormIdPool::resolve` walk without that arm, so a player-owned script keys under local 1. The pool→local step is separately spelled 8 more times.
- **Fix**: (1) delete `owner_form_id`, call `crate::condition::entity_global_form_id` (it is `pub`, same crate). (2) Add `FormIdPool::local_of(&self, id: FormId) -> Option<u32>` in `crates/core/src/form_id.rs:196` next to `resolve`; route the 9 call sites (incl. `entity_global_form_id` itself) through it. (3) Widen the #5588 guard to scan `papyrus_provider/execute.rs` and `systems/vats.rs`, or key it on the `component.0).map(|pair| pair.local.0)` body rather than the fn name.

#### TD3-2026-10-25-01: GPU-struct size claims in prose are stale outside `pinned_sizes()` (ReSTIR `Reservoir` 32 to 36 B; `GpuTerrainTile` "208-byte" stride)
- **Severity**: MEDIUM (a stale GPU size in prose; severity table)
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `crates/renderer/shaders/include/bindings.glsl:815`, `crates/renderer/shaders/triangle.frag:3392`, `crates/renderer/src/vulkan/restir.rs:859` (assert message), `crates/renderer/shaders/include/bindings.glsl:529`
- **Age**: #6250 `bed162262` (2026-10-24) grew `Reservoir` 32 to 36 B; #6197 `b81a43443` (2026-10-22) grew `GpuTerrainTile` 208 to 224 B
- **What**:
  - `bindings.glsl:815` says "32 B/reservoir". `triangle.frag:3392` says "would not fit the 32-byte reservoir". The `restir.rs:859` assert message says "existing 32-byte reservoir stride".
  - The live values are `RESERVOIR_STRIDE = 36` (`restir.rs:44`), pinned by `reservoir_stride_matches_shader`, and the struct's own comment at `bindings.glsl:838` ("std430 stride 36 B"). `memory-budget.md:277` is already correct.
  - `bindings.glsl:529` says the tail pads "round the record out to the 208-byte std430 array stride". The next lines (`:545`) and `gpu_terrain_tile_is_224_bytes` say 224.
  - `gpu_material_size_claims` cannot see either: `Reservoir` is not in `pinned_sizes()`, and the terrain claim does not name the type.
- **Fix**: Edit the four sites to 36 B / 224-byte. Optionally add `Reservoir` (36) to `pinned_sizes()` or to a second guard that greps `\b32[- ]?(B|byte)\b` near "reservoir". The audit-renderer skill line 75 ("reservoir stays 32 B") needs the same fix (Dim 4 hand-off).
- **Checked clean in the same sweep**: `Vertex` (104 B, ~15 sites), `GpuWaterParams` (368, 6 sites), `GpuRayBudget` (68), `GpuFogVolume` (96, header 16 + 512x96 = 49168), `GpuCombustionLightMoment` (32), `GpuFogClusterEntry` (16), `GpuSelectedRayProbe` (144), `SkinPushConstants` (32), `WaterPush` (16), `UiVertex` (20), the `memory-budget.md` bone/instance/material figures (`MAX_SKIN_SLOTS` 1364, `MAX_INSTANCES` 262144, `MAX_MATERIALS` 16384, `VERTEX_POOL_SOFT_CAP` 4M x 104).

#### TD4-2026-10-25-01: Eleven "known-open / do not re-file" callouts in eight skills name issues closed today (fifth recurrence of this class)
- **Severity**: MEDIUM (promoted: two of the sites carry an explicit `do not re-file` directive over code that is now fixed — the #3199 blindfold class `_audit-common.md` forbids; a regression there would be dedup-suppressed)
- **Dimension**: 4 — Audit-Finding Rot
- **Effort**: small
- **Age**: all eight issues closed 2026-10-25 (six by a manual close-sweep at 02:13–02:14Z after `audit-sync` baselined dcad3669d on 10-24; #6089 / #6088 by the HEAD~2 / HEAD~1 fix commits). Not an audit-sync miss — they closed after its baseline.
- **Status**: Regression of #6173 (closed TD4-2026-10-19-01, ~25 such callouts), same class as #5606, #5742, #4369, #5083
- **Sites** (skill:line — claim — truth):
  - `audit-exterior/SKILL.md:33` — "FO3 'Use Land Data' children still draw no ground, open #6089", and the inherit-all "gated by a bare `GameKind::Oblivion` check" — #6089 closed, fixed a9472103b: inheritance now runs for Fallout3NV, pinned by the real-data census. The whole parenthetical is wrong, not just the "open".
  - `audit-fo4/SKILL.md:42` — "Known open: `resolve_precombine_owner` reads the FormID slot byte as a load-order position (#6088)" — closed, fixed 9634aa362: ownership resolves through `load_order_position_of` (`byroredux/src/cell_loader/load_order.rs:363`, used at `precombined.rs:787`); legacy byte read kept only for slot-table-less fixtures.
  - `audit-legacy-compat/SKILL.md:44` — "NiControllerManager / KFM sequence state machine are the known-fragile spots (#4128 open)" — closed, fixed 92173d61f (`NamedSequences` catalogs + ObScript `PlayGroup` consumer).
  - `audit-ui/SKILL.md:46` — "Those runtime-loaded movies still reach Ruffle un-shimmed (#5480, open …)" — closed, fixed 1ac34024f (`load_archive_resource` applies `normalize_scaleform_dialect` once, `crates/ui/src/prepare.rs`).
  - `audit-save/SKILL.md:191` — "`DialogueSpokenInfoForms` — known-open #5459" (bump ledger stops at v32) — closed, fixed a4ccf3b27 (ledger restored + pin `the_bump_ledger_is_contiguous_and_current`).
  - `audit-parsers/SKILL.md:80` — "Known-open gate weakness (dedup, do not re-file): three of the four HKX dimension-gate fixtures leave `mask_size` 0 … (#5446)" — closed, fixed c78e9241e (mutation probe re-run: each clause now reddens exactly its test). **Active suppress-directive over fixed code.**
  - `audit-starfield/SKILL.md:79` — "two docs still say [Skyrim 92 + 16-byte tail], open #5534" — closed, fixed 559a76627 (game-compatibility.md and lighting-from-cells.md rewritten).
  - #5502 ×4: `audit-starfield/SKILL.md:66` ("Known open, don't re-file: the rest of #5502 … awaits a game capture") and `:113` ("rest of #5502"), `audit-nifal/SKILL.md:90` ("undecided half that keeps #5502 open"), `audit-parsers/SKILL.md:87` ("#5502 stays open only for the undecided Starfield glass-coverage signal"). #5502 was closed 2026-10-25 02:14Z "Already fixed": full Parent inheritance landed (5c14ebdf8, 1d7e2c6bd) and the glass-coverage decision is *recorded in nifal.md* (`IsGlass` routes to `bgem_glass` directly; the AlphaBlend forward only passes the classifier's early-return). The skills still call it undecided/needing a capture. **Active suppress-directive at starfield:66.** The still-open sibling is #5509 (additive-family blend modes) — keep that.
- **Suggested Fix**: rewrite each as past tense with the fix SHA (pattern already used elsewhere: "closed, `<sha>`; guard `<test>`"); delete the `do not re-file` phrase at parsers:80 and starfield:66; re-point the glass-coverage wording at `nifal.md`'s recorded decision. Run `_audit-validate.sh` after. Use the sweep script in TD4-03 to confirm zero residue.

#### TD9-2026-10-25-01 - MEDIUM - five closed-HIGH real-data guards are in `real-data-pins.txt`'s blind spot, and four of them run in no lane at all
- **Dimension**: 9 (label `test-gap`) - **Effort**: small - **Age**: guards landed 2026-09-29 .. 2026-10-19; none are in the 4a3e89d56..HEAD window
- **Location**: `.github/real-data-pins.txt`; `scripts/run-real-data-pins.sh:70-75` (lane map); `byroredux/src/workspace_hygiene_tests.rs:1053` (`real_data_pin_lane_sources`)
- **What**: each test below is `#[ignore]`d, cites a closed `high` issue in its own doc comment, and is absent from the pin list (`grep -c <fn> .github/real-data-pins.txt` = 0):

  | fn | file:line | closed-HIGH | other lane today |
  |---|---|---|---|
  | `fnv_patch04_quest_script_targets_its_own_scro_quests` | `crates/scripting/src/obscript_quests.rs:1715` | #5629 (ObScript refs resolve against SCRO-only list) | none. Its own doc says the sibling gate "was blind to references resolving against the SCRO-only list", so this is the only discriminating test |
  | `authored_actor_value_indices_resolve_in_the_vanilla_masters` | `crates/scripting/src/condition/tests.rs:2073` | #5989 (GetActorValue enum code read as FormID) | none |
  | `oblivion_starts_dead_bases_and_their_placements_match_the_audit_census` | `crates/plugin/tests/parse_real_esm.rs:5221` | #5013 (787 Oblivion corpses spawn alive) | none (the plugin `--ignored` suite is excluded from the `parsers` job, `real-data-gates.yml:242-246`) |
  | `installed_starfield_npcs_and_races_decode_acbs_and_skeletons` | `crates/plugin/src/esm/records/actor/tests.rs:2614` | #6120 (Starfield NPC_ ACBS matches no arm) | none |
  | `fo3_corpses_key_on_base_health_not_xrgd` | `crates/plugin/tests/parse_real_esm.rs:5416` | #5005 (FO3 leg; the FNV twin `fnv_corpses_key_on_base_health_not_xrgd` IS pinned, the doc calls this "the FO3 twin") | none |

  Also in the same class but already covered by a manual lane (the `parsers` job runs the whole hkx crate `--ignored`):
  `fallout_4_behavior_graph_census_is_exact` (`crates/hkx/src/corpus_tests.rs:135`, #6211, re-measured by 7e2f21482/fbccdcbff).
  Pinning it is optional; no lane gap.
- **Why it was missed**: #6224 swept only its audit window. These are older guards; the #6224/`/fix-issue` PINS line only
  catches fixes landing after it. Window check for this audit: of the 11 commits that added/changed an `#[ignore]` since
  4a3e89d56, none guards a closed-HIGH (#6089, #6090, #5753, #5750, #6329, #6252, #6251, #6241, #6350-refs are medium/low/open;
  `skeever_ragdoll_survives_six_hundred_floor_ticks`, #6185, is already pinned). The 9 closed-HIGH fixes in the window
  (#6343, #6214, #6213, #6212, #6211, #6210, #6209, #6208, #6207) all carry hosted non-ignored tests.
- **Fix**: append `plugin-esm oblivion_starts_dead_bases_and_their_placements_match_the_audit_census`, `plugin-esm fo3_corpses_key_on_base_health_not_xrgd`,
  `plugin-lib installed_starfield_npcs_and_races_decode_acbs_and_skeletons` (3.4 GB peak, one process per pin is already the
  runner's bound). The two scripting guards need a new lane: add `scripting-lib) target="-p byroredux-scripting --lib"` to the
  `case` at `run-real-data-pins.sh:70-75` and the matching `add_dir("crates/scripting/src")` arm in
  `real_data_pin_lane_sources` (workspace_hygiene_tests.rs:1058-1062) so `every_real_data_pin_names_an_ignored_test_in_its_lane`
  keeps checking them; then add the two `scripting-lib` rows. Verify with `scripts/run-real-data-pins.sh --list`.
- **Dedup**: precedent #6224 (closed; different four rows), #6183 (closed), #6182/#5905 (closed, no runner - the local run is the only run).


### LOW (by dimension)


#### Dimension 1 — Complexity (6)

##### TD1-2026-10-25-01: `crates/scripting/src/obscript_effects.rs` crossed 2000 prod LOC (1537 -> 2034, +497 in two commits) — a result of the #6139 table conversion, not a grown-back split hub
- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: `da22354f1` (stage 6, 2026-10-24: 1528 -> 1892, `run` 780 lines / 63 arms became 63 handlers + `EFFECT_COMMANDS`) and `6561576ab` (stage 7: 1892 -> 2032, `getter` 310 lines / 36 arms became 43 handlers + `GETTER_COMMANDS`). Two 2026-10-24 commits since then add +2. Was NOT on the 10-21 list (1537). This is the "newly crossed" case: the file was never a split hub (no `obscript_effects/` dir exists; `git log` on the path is a plain history, 15 commits since 10-18). #6139 (CLOSED) removed the 6 giant `match cmd` functions; their bodies landed here as ~106 small `fn(world,_fallout,cmd,args,context) -> Action` handlers.
- **Location**: `crates/scripting/src/obscript_effects.rs` (2951 total; prod 1-2034, inline `mod tests` L2035-2951 with 44 tests)
- **Structure read** (no fn >176 lines; this is breadth, not depth): module doc L1-48 · CMD id consts L64-83+ (effect ids) · `Binder`, `adjust_standing`, `form_arg`/`num`/`live` helpers L120-228 (these four are imported by 6 sibling files: `obscript_more/{extras,state,tail,actions}.rs`, `obscript_more.rs`, `obscript_quests.rs`) · mutating handlers L229-1359 (~1130 lines) · `finish_effects` L1360 + `EFFECT_COMMANDS` table L1370-1454 · getter id consts L1457-1535 · getter handlers L1537-1987 (~450) · `GETTER_COMMANDS` L1988-2032.
- **Split axis (handlers by responsibility; the two tables are the seam)**: convert to `obscript_effects/`: `mod.rs` (docs, `Binder`, `form_arg`/`num`/`live`/`adjust_standing`, `finish_effects`, and `pub(crate)` re-exports so the 8 `crate::obscript_effects::{form_arg,live,num,EFFECT_COMMANDS,GETTER_COMMANDS,adjust_standing,CMD_*}` call sites are unchanged) · `getters.rs` (L1457-2032: id consts + 43 getter handlers + `GETTER_COMMANDS`) · effect handlers split by the domain the table rows already read in: `inventory.rs` (add_item, equip/unequip, remove_all*, place_at_me), `combat.rs` (kill, enemy/ally, start/stop combat, alarm, unconscious/ghost, resurrect, reset_ai, restrained, alert, assault alarm), `actor_state.rs` (set/mod AV, essential, perk, faction/rank/reputation, reward_*, teammate), `world_state.rs` (lock, open state, destroyed, delete, ownership, weather, set_pos/angle/scale, play_group), `presentation.rs` (say, show_message, conversation, sound, idle, imod, magic shaders, cast, dispel) each with its own `COMMANDS` slice concatenated in `mod.rs`. Tests move to `obscript_effects/tests.rs` (precedent in-tree: `obscript_more/tests.rs`).
- **Hazards**: no `include_str!` reader of this file (grep clean). `obscript_cmd.rs:94-95` lists the two tables in `FAMILIES`; keep the `duplicate_rows` test green by keeping the same `&[Command]` slices (or listing the new family slices). The two row tables are order-insensitive (duplicate-id test) so splitting is safe.
- **Related**: #6139 (closed, caused the move), #5804 (condition.rs, same "table conversion concentrates rows in one file" shape).

##### TD1-2026-10-25-02: `byroredux/src/inventory.rs` 1956 prod LOC (+93 in 7 commits since 10-21, 44 lines from the line) — split before the next crime/loot feature crosses it
- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: 7 commits since 4a3e89d56: `fcd1cce4a`, `4cfcee4b9` (crime faction), `e161c45f3` (PayFine/GoToJail), `e63fa457d` (theft bounty), `e9b7bcead` (radiation gauge), `2682aa969` (#6229), `3e4bfab51`. Flat in the window before that (1863). 4659 lines total: ~2680 are inline tests, so any edit also pays the test-module diff.
- **Location**: `byroredux/src/inventory.rs` (1980 prod; `mod tests` L1981-4659)
- **Structure read**: three jobs. (a) catalog/templates/vitals: `InventoryCatalog`, `PlayerInventoryTemplate`, `PlayerVitals`, `install_catalog` L601, `build_player_character_template` L462, `build_player_template*` L742-816, `attach_to_player` L871 (L22-960). (b) loot, ownership and pickup: `is_loot_source` L962, `stamp_cell_ownership` L996, `theft_owner` L1047, `transfer_loot` L1122 (~190 lines), `container_loot_system` L1316, `PickedUp`/`PlacedItemCount`, `pickup_loot`, `take_placed_item` L1416 (L962-1500). (c) UI-driven inventory mutation: `snapshot` L1505, `consume_item` L1577 (~137), `apply_action` L1714, `toggle_equip` L1771, `reconcile_*` L1894-1980.
- **Split axis**: `inventory/{mod.rs (add_item, queue_*, drain_pending, re-exports), catalog.rs (a), loot.rs (b, the crime/ownership growth point), actions.rs (c)}`, tests as siblings per job.
- **Hazard**: `byroredux/src/npc_spawn/tests.rs:3373` does `include_str!("../inventory.rs")` and counts call names at :3401 — it must read a `concat!` of the new files or the census goes vacuous (feedback_file_split_include_str). Hub-churn caveat: after the split `git log -- inventory.rs` shows only the hub, use `inventory/` pathspec.

##### TD1-2026-10-25-03: `byroredux/src/npc_spawn.rs` 1911 prod LOC (+149 since 10-21; its two extracted siblings grew +230 and +197) — the 10-15 split did not stop growth
- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: 6 commits on the hub and 22 on `npc_spawn/` since 4a3e89d56 (`9d1a262ed` #6090, `3dc16208e`, `a6ab3625e` #5753, `1d319a942`, `1659d43c0` #6231, `cafd3ccab` #6227). Hub-hazard: the hub's own log understates churn; family sizes: `npc_spawn.rs` 1911, `npc_spawn/ai_package.rs` 1164 -> 1394 (+230), `npc_spawn/loot_appearance.rs` 1133 -> 1330 (+197), plus `resumable/`, `seam_blend/`.
- **Location**: `byroredux/src/npc_spawn.rs` (prod 1-1913, `mod tests;` external)
- **Structure read** (by line): stamps of ECS components from a `ResolvedNpc` L54-400 (faction ranks, spells, AVs, creature attack, ruleset/VATS/melee builders) · fallback collider/ragdoll install L401-560 · skeleton/body/creature **path resolution** L561-880 (`humanoid_skeleton_path`, `npc_skeleton_path`, `humanoid_body_paths`, `humanoid_body_path_icon_index`) · **animation clip loading + idle desync** L879-1320 (`load_idle_clip`, `load_kf_clip_by_path`, walk-clip tables, `idle_desync`, `SpawnIdle`) · **equipment** components + `build_npc_equip_state` L1318-1853 (the function itself is ~345 lines) · FaceGen prebaked paths + `tag_descendants_as_actor` L1854-1913.
- **Split axis**: `npc_spawn/{stamps.rs (L54-400), fallback_physics.rs (L401-560), skeleton_paths.rs (L561-880 + facegen paths), anim_clips.rs (L879-1320), equip.rs (L1318-1853)}`; the hub keeps `mod` decls + `pub(crate)` re-exports so `crate::npc_spawn::X` callers (cell_loader, scene, tests) are unchanged. Move `build_npc_equip_state` verbatim first (pure move), extract its phases only afterwards (it is the >300-line fn on this file's list).
- **Hazards**: `npc_spawn/tests.rs:3288` `include_str!("../npc_spawn.rs")` and :3375, :3416 census entries; `material_translate.rs:2817` names `"npc_spawn.rs"` in a scan list. Both must be re-pointed in the same commit.

##### TD1-2026-10-25-04: `crates/renderer/src/vulkan/gpu_timers.rs` spells 25 timer brackets as 50 near-identical methods (~850 of its 1929 prod lines); each new pass costs ~45 lines in four places
- **Severity**: LOW · **Dimension**: 1 (also Dim 2) · **Effort**: small-medium
- **Age**: 1 commit in window (`994620123`, #6316 cloud-shadow bracket, +~45 lines); file 1874 -> 1929. 640 of its lines are comments.
- **Location**: `gpu_timers.rs:1009-~1900` (`cmd_<pass>_start` / `cmd_<pass>_end`, 50 methods in `impl GpuPerFrameTimers` L683-1930), consts `Q_*_START/END` L140-213, `BIT_*` L450-477
- **What**: every `_start` is `cmd_write_timestamp(cmd, <STAGE>, self.pools[frame], Q_X_START)`; every `_end` the same plus `self.active_bits[frame] |= BIT_X`. The only variation is the stage flag: 18 `TOP_OF_PIPE`, 20 `COMPUTE_SHADER`, 12 `BOTTOM_OF_PIPE`, 2 `ACCELERATION_STRUCTURE_BUILD_KHR` (52 literals across the impl; a few methods carry extra logic, e.g. geometry-phase brackets, so check each row before folding). 73 call sites in the renderer. #6337 (closed) already made the bench-line side of these brackets a keyed table; the emit side is still hand-written.
- **Fix**: data-table shape, not a file split (skill: tables want tables). `enum TimerPass` with a `const` row `{start_q, end_q, bit, start_stage, end_stage}` and two methods `cmd_start(pass,..)` / `cmd_end(pass,..)`; keep the `Q_*`/`BIT_*` consts as they are (the pin tests `every_timer_slot_and_active_bit_is_unique_and_in_range` and `doc_table_slot_count_matches_queries_per_frame` parse `const Q_*: u32 = N;` lines and the `//! |` doc table) and migrate the 73 call sites mechanically. Timestamp writes only, no barrier/layout change, but stage flags must be preserved verbatim per row; verify with `BYRO_VALIDATION=1` (feedback_speculative_vulkan_fixes). Expected ~ -600 lines.
- **Hazard**: four `include_str!("gpu_timers.rs")` self-scans (L2244, 2326, 2372, 2413) and `renderer/src/source_scan.rs:64` allowlist row; do not move the file.

##### TD1-2026-10-25-05: `register_update_systems` grew 854 -> 981 lines (+127, 14 commits) and `register_late_systems` is 849; neither is in #6143's budget list
- **Severity**: LOW · **Dimension**: 1 · **Effort**: small
- **Age**: 14 commits on `byroredux/src/boot/schedule/update.rs` since 4a3e89d56 (AI package systems `fcd1cce4a` Find / `9aaf5e1cf` Ambush / `10f8cbb91` UseWeapon / `b540b104a` Flee, crime `4cfcee4b9` `99051a4eb` `98d6eedfe` `1a67c9f30` `0c6cd46dc` `39ffea9a8` `e63fa457d`, ObScript result scripts `6db47b0e4`). It will cross 1000 lines at the next two systems. Same `late.rs`: 836 -> 849.
- **Location**: `byroredux/src/boot/schedule/update.rs:17` (the file is essentially the one function, 997 prod lines), `late.rs:15`
- **Structure**: flat sequence of `scheduler.add_exclusive[_with_access](Stage::Update, ..)` with a 5-40 line ordering-contract comment each (producer-before-consumer: activation -> flush -> loot -> jail -> combat -> hostility -> guards -> quest/story/scene -> packages). Registration order is the contract, so the axis is **contiguous order segments**, not alphabetical: `register_interaction_and_crime`, `register_combat`, `register_quest_story_scene`, `register_ai_packages`, called in the same order from `register_update_systems`; the dt-dropping `fn x_dispatch(world,_dt)` shims (L81-145) move into the segment that registers them. No behaviour change, order pinned by calling in sequence.
- **Related**: #6143 (open: "no regrowth budget" for 11 driver fns). Add these two rows to its list rather than a new budget scheme; the same pin pattern (`draw.rs:1433` `DRAW_FRAME_LINE_BUDGET`) applies.

##### TD1-2026-10-25-06: `crates/scripting/examples/mq101_conformance.rs` is a 2157-prod-LOC single-file example whose only filed issue (#5586) is already fixed
- **Severity**: LOW · **Dimension**: 1 · **Effort**: small
- **Location**: whole file; example (not shipped, needs a Skyrim install). `main()` L2133, `run()` L397-441.
- **Premise check**: #5586 (OPEN) says `fn run()` is 1454 lines, depth 15. It is **45 lines** now; the body was decomposed into `check_*` helpers by `d7c47d741` (2026-10-15). Largest fn in the file is `check_scene_runtime` 165 lines. So #5586's premise is gone and it should be closed; the residual is purely file size (never filed; the 10-21 report listed it as "unchanged at 2157").
- **Split axis (responsibility)**: cargo builds `examples/<name>/main.rs`, and `examples/common/` already exists in this crate: `mq101_conformance/{main.rs (run, main, Checks, data_dir), ast_probe.rs (L119-340: expression/statement predicates, `mq101_player_callback_contract`), scenes.rs (L522-1285: SceneSet/SceneInventory, check_scene_*, analyze_scene), packages.rs (L928-1160: analyze_package_action, execution-kind classifiers), scripts.rs (L1286-1700: ScriptCorpus, check_scripts/fragments/imad), fragments.rs (L1700-1985), report.rs (print_inventory)}`. No `include_str!` reader (grep clean). The hygiene guard only keys on `tmp_`/"throwaway" names.


#### Dimension 2 — Duplication (12)

##### TD2-2026-10-25-02: the ambient behavior-component set is spelled three times; `insert_at_spawn` and `insert_at_runtime` are ~165-line line-for-line twins
- **Dimension**: 2
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:556-717` (`insert_at_spawn(&mut World)`), `:720-855` (`insert_at_runtime(&World)`), `:885-980` (`clear_ambient_behavior`, 30 `remove_component` lines)
- **Age**: this window added Find, UseWeapon, Flee, Ambush (b540b104a, 10f8cbb91, 9aaf5e1cf, fcd1cce4a, 7fded9423) — each edited both inserters and the remove list. The remove list has already needed a hand fix for a forgotten component (#3255 NavPath comment at `:968`).
- **Effort**: small
- **Fix**: write the variant match once. Option A: a private `trait ComponentSink { fn put<T: Component>(&mut self, actor: EntityId, c: T); }` implemented for `&mut World` (`world.insert`) and `&World` (`insert_component`), and one `fn install<S: ComponentSink>(self, sink: &mut S, actor)`; both current methods become 3-line wrappers. Option B: have `AmbientBehavior` produce a small `BehaviorBundle` enum and let the two callers insert it. Then pair the remove list with a `const AMBIENT_COMPONENT_REMOVERS: &[fn(&World, EntityId)]` so a new procedure edits one match arm and one row.

##### TD2-2026-10-25-03: the player-entity read has an accessor in scripting (#5813) but not in the binary; 66 production sites hand-spell it
- **Dimension**: 2
- **Location**: `try_resource::<PlayerEntity>().and_then(|p| p.0)` at 66 production sites across 34 files in `byroredux/src` (worst: `inventory.rs` 10, `combat.rs` 6, `systems/character.rs` 6, `systems/npc_dialogue.rs` 4, `commands/gameplay.rs` 3, `commands/view.rs` 3, `crime.rs` 2, `systems/vats.rs` 3); three local one-off wrappers `barter.rs:218 player()`, `crates/scripting/src/jail.rs:184 player_of()` (reads the *other* resource), `crates/scripting/src/obscript_more.rs:163 player()`. 9 of the 66 landed since 4a3e89d56.
- **Age**: #5813 (closed) added `PapyrusPlayerEntity::get` (`crates/scripting/src/papyrus_demo/mod.rs:83`) for "62 production sites"; its doc says the bin's `PlayerEntity` is a second, NOT-synchronised resource that "in FlyCam differs". The sweep stopped at the crate boundary.
- **Effort**: small (mechanical)
- **Fix**: `impl PlayerEntity { pub fn get(world: &World) -> Option<EntityId> }` at `byroredux/src/systems/character.rs:44`; sed the 66 sites; delete `barter::player`. Add a `workspace_hygiene_tests` scan for `PlayerEntity>().and_then` like the Papyrus one. Bonus: the two-resource split then has exactly two accessors, which is where a future FlyCam divergence must be reasoned about once (today `crime.rs` reads `PlayerEntity` while `jail.rs`/`guards.rs` read `PapyrusPlayerEntity` for the same player in one crime→jail flow).

##### TD2-2026-10-25-04: "how many of base form X does this inventory hold" is hand-spelled at seven production sites (three added by the crime/jail work) plus four test copies
- **Dimension**: 2
- **Location**: `byroredux/src/barter.rs:309-318` (`count_of`), `crates/scripting/src/guards.rs:104` (`player_gold`) and `:115` (`can_pay_bounty`), `crates/scripting/src/jail.rs:196`, `crates/scripting/src/fragment/vm/natives/favor.rs:59`, `crates/scripting/src/fragment/vm/natives/reference.rs:155`, `crates/scripting/src/obscript_more/tail.rs:577`; test copies `obscript_effects.rs:2095`, `jail.rs:351`, `obscript_objects.rs:1365`, `reference_state.rs:1599`. `crates/core/src/ecs/components/inventory.rs` has `new/push/add/get/len` and no count accessor.
- **Effort**: trivial
- **Fix**: add `Inventory::count_of(&self, base_form_id: u32) -> u32` (saturating `u32` sum) in `inventory.rs:100-150`; one-liners at each site (`player_gold` widens to `i64`). Delete `barter::count_of` and the four test helpers.

##### TD2-2026-10-25-05: four actor movers re-spell "apply the walk step to `Transform`", and two re-spell the "busy actor" gate; the #5580 guard does not see them
- **Dimension**: 2
- **Location**: `byroredux/src/systems/flee.rs:138-148`, `systems/flee_package.rs:157-168`, `systems/find_package.rs:252-262`, `systems/eat_sleep.rs:371-381` (each: `query_mut::<Transform>()` → `translation = step.position` → `if let Some(r) = rotation { rotation = r }`); busy gate `has::<Dead>||AiCombatState||AiSuspended||FleeState` at `flee_package.rs:87-93` and `find_package.rs:188-196`. (`use_weapon_package.rs` and `forcegreet.rs` apply through their own `Step::Walk` enum.)
- **Age**: flee.rs 07b… (2026-10-18), flee_package.rs b540b104a and find_package.rs fcd1cce4a (2026-10-24). #5580 (badc45309, 2026-10-12) moved the six oscillating walkers onto `navmesh_path::write_back_walk` and pinned it with `walk_movers_share_one_pass_two_write_back` (`navmesh_path.rs:995`), whose file list is exactly those six, so the next four movers passed.
- **Effort**: small
- **Fix**: add `fn apply_walk_step(world: &World, moves: impl IntoIterator<Item = (EntityId, Vec3, Option<Quat>)>)` beside `write_back_walk` (`navmesh_path.rs:821`, which can call it for its Transform half); use it in the four. Add `fn ambient_package_may_run(world, actor) -> bool` (Dead/combat/suspended/flee) next to `FleeState` in `flee.rs`; both package systems call it. Extend the guard's file list with the four files.

##### TD2-2026-10-25-06: 45 ObScript effect handlers repeat the same caller prologue; 15 of them also open a `Binder`
- **Dimension**: 2
- **Location**: `crates/scripting/src/obscript_effects.rs` — `let caller = caller_of(context); let caller_entity = live(world, caller); if caller_entity.is_none() { return Action::Declined; }` ×45 (full 6-line `+ let mut bind = Binder::default(); let me = bind.actor(caller);` ×15, e.g. `:229-237`, `:262-270`, `:274-282`); 78 `live(world, caller)` and 87 `caller_of(context)` in the file overall.
- **Age**: all of it arrived with the #6139 table refactor (bd4cabf81 … d5d13edcc, 2026-10-24); `4a3e89d56` had 0 full prologues.
- **Effort**: small
- **Fix**: `run_row` (`obscript_cmd.rs:146`) already declines a missing caller for `needs_caller` rows. Extend it to resolve the live entity once: add `Command.needs_live_caller` (or a `Context::caller_entity: Option<EntityId>` filled by `run_row`) and a helper `fn open_actor_effects(world, context) -> Option<(u32, Binder, ActorRef)>` in `obscript_effects.rs` for the 15 Binder handlers. Each handler loses 5-6 lines; the "live caller" policy is then one line in `run_row`.

##### TD2-2026-10-25-07: three `if try_resource::<T>().is_none() { insert_resource(T::new()/default()) }` stragglers survive #5806, and no guard stops a fourth
- **Dimension**: 2
- **Location**: `byroredux/src/barter.rs:274` (`FormIdPool`, has `impl Default`, `form_id.rs:217`), `byroredux/src/cell_loader/reference_state.rs:487` (`ItemInstancePool`, `impl Default` `resources/mod.rs:1549`), `byroredux/src/cell_loader/object_sequences.rs:50` (`ObjectSequenceStore`, `#[derive(Default)]`, added f3380f6c84 2026-10-18); test copies `inventory.rs:3029`, `crates/scripting/src/fragment/tests.rs:3405`. (`terrain_height.rs:149` and `scene/world_setup.rs:59` insert non-Default values — legitimate.)
- **Effort**: trivial
- **Fix**: replace with `world.init_resource::<T>()` (`crates/core/src/ecs/world.rs:686`; 154 existing callers). Add a `workspace_hygiene_tests` regex scan for the three-line shape — #6136 pinned the contract but nothing pins the call sites.

##### TD2-2026-10-25-08: three more Option-returning little-endian cursors beside `SubReader`
- **Dimension**: 2
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:835-885` (`Cursor`: `take/u8/u16/i16/u32/i32/f32/wstring`), `crates/plugin/src/esm/records/weather_settings_instance.rs:143-175` (`Cursor`: `take/u16/i32/u32/at_end`, plus `bytes.try_into().unwrap()` decode in `read_builtin` `:443-466`), `crates/plugin/src/esm/records/misc/world.rs:427-460` (`NvnmCursor`). Canonical: `crates/plugin/src/esm/sub_reader.rs` (module doc: replaces "inline `from_le_bytes` patterns"). `reflection.rs:271-332` mixes `SubReader` and raw `from_le_bytes(blob[..].try_into().unwrap())` in one function (10 sites).
- **Effort**: small
- **Fix**: add `SubReader::take(n) -> Result<&[u8]>`, `is_at_end()`, `wstring()`/`counted(stride)`; the three cursors become `SubReader` with `.ok()?`. Net: three private types gone, one bounds policy. (Net `from_le_bytes` count in `esm/` fell 34→17 this window — the trend is right, this is the remaining structural copies.)

##### TD2-2026-10-25-09: `block_bound` (terrain LOD) and `world_bound` (BTR) are the same function
- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1180-1198` (`block_bound(vertices, holes)`), `byroredux/src/cell_loader/terrain_lod_btr.rs:500-514` (`world_bound(vertices)`, 9384d4c2b 2026-06-19)
- **Effort**: trivial
- **Fix**: move one `vertex_bound(vertices, skip: impl Fn(usize)->bool) -> WorldBound` into `byroredux/src/cell_loader/lod_support.rs` (already the shared home both LOD modules import from); `terrain_lod` passes the hole mask, `terrain_lod_btr` passes `|_| false` (its test `:787` pins the empty case).

##### TD2-2026-10-25-10: residual of #6148 — depth-read-only and arbitrary-layout `DescriptorImageInfo` still hand-spelled in five passes
- **Dimension**: 2
- **Location**: `DEPTH_STENCIL_READ_ONLY_OPTIMAL` infos at `crates/renderer/src/vulkan/ssao.rs:272`, `caustic.rs:631`, `svgf.rs:925`, `composite.rs:885` and `:1235`; arbitrary-layout infos at `composite.rs:872,897,1225,1251` (`indirect_layout` / `shadow_correction.layout`). `descriptors.rs:93-116` has only `sampled_image_info` (SHADER_READ_ONLY), `general_sampled_image_info` (GENERAL) and `storage_image_info`. #6148 went 92 → 14 hand spellings; these are the remaining 9 of that class (the other 5 are `descriptors.rs` itself and `texture_registry/mod.rs`'s bindless exception).
- **Effort**: trivial
- **Fix**: add `depth_read_only_image_info(sampler, view)` and `image_info(sampler, view, layout)` to `descriptors.rs`; composite's create and recreate paths (`:872-900` vs `:1225-1254`, the same three infos twice) call them. Pure host-side struct building, no barrier/order change.

##### TD2-2026-10-25-11: six `mod tests {` cuts in renderer/bin source scans bypass `source_scan` and the weak-needle guard cannot see them
- **Dimension**: 2
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:1802` (`split("\nmod tests {")`), `vulkan/skin_compute.rs:1672,2199`, `vulkan/context/resources.rs:1178,1266`, `byroredux/src/loading_screen.rs:1077` (all `find("mod tests {")`; oldest 2026-05-23, newest 2026-10-03).
- **Effort**: trivial
- **Fix**: use `crate::source_scan::production_text` (renderer re-exports it, `crates/renderer/src/source_scan.rs:9`) or `byroredux_core::source_scan::production_text_or_whole` (bin). Add `"find(\"mod tests {\")"`, `"split(\"\\nmod tests {\")"` to the `weak` list at `byroredux/src/workspace_hygiene_tests.rs:~717`. (All else in the guard's allowlist is intact; no new weak `#[cfg(test)]` cut was added this window.)

##### TD2-2026-10-25-12: compute-pass teardown (pipeline / layout / pool / set layout) is hand-written per pass with two different guard policies
- **Dimension**: 2
- **Location**: 34 `destroy_pipeline_layout(` and 23 `destroy_descriptor_pool(` sites across 21 files in `crates/renderer/src/vulkan/`. `svgf.rs:1401-1440` null-guards each handle and resets it to null (needed for the `try_or_cleanup!` partial-init path); `ssao.rs:527-530` calls `destroy_pipeline`/`destroy_pipeline_layout` unguarded; `bloom.rs:924-945` guards per handle. Construction is already one home (`pipeline::create_compute_pipeline`, pipeline.rs:58); teardown has none.
- **Effort**: medium
- **Fix**: add `descriptors::destroy_pass_objects(device, &mut PassHandles { pipeline, layout, pool, set_layout })` (guarded + nulling, same reverse order as today's hand code) next to `DescriptorPoolBuilder`, and move passes onto it one at a time (svgf/taa/ssao/caustic first). Order-preserving host refactor; the `// SAFETY` comments the `undocumented_unsafe_blocks` deny requires then live once. Do not combine with any barrier or lifetime change (*feedback_speculative_vulkan_fixes*).

##### TD2-2026-10-25-13: `ItemKind::Weapon { 14 fields }` is built literally in ten test fixtures; the VATS work touched four of them for one new field
- **Dimension**: 2 (test fixtures)
- **Location**: `byroredux/src/systems/use_weapon_package.rs:123-140`, `systems/projectiles.rs:~425`, `systems/vats.rs` (×3), `npc_spawn/loot_appearance.rs:~1855`, `npc_spawn/tests.rs`, `inventory.rs` (×2), `crates/plugin/src/esm/records/tests.rs`, `crates/plugin/tests/parse_real_esm.rs`. 7 literals added since 4a3e89d56; ff97cdd93 (#6350, WEAP DNAM → `ap_cost`/`vats_base_to_hit`/`skill_form`/`vats`) edited `loot_appearance.rs`, `npc_spawn/tests.rs`, `projectiles.rs`, `use_weapon_package.rs` for one-line field adds.
- **Effort**: small
- **Fix**: give the variant a named payload (`ItemKind::Weapon(WeaponData)` with `#[derive(Default)]`) or add `ItemKind::weapon_stub(damage, clip_size)` in `crates/plugin/src/esm/records/items.rs` under `#[cfg(any(test, feature = "test-fixtures"))]`; fixtures then spell only the fields they pin.


#### Dimension 3 — Stale Documentation (10)

##### TD3-2026-10-25-02: eight doc blocks spliced onto the wrong item in the last 4 days (the #5809/#6064/#6268 class recurred; three landed after the #6268 sweep)
- **Severity**: LOW (contract sits on the wrong item; the new item is undocumented). Severity table has no row for this class.
- **Dimension**: 3
- **Effort**: small (8 doc moves)
- **Detector**: scanned every added run of lines in `git diff -U4 4a3e89d56..HEAD -- '*.rs'` that starts directly under a context `///` line and contains an item.
- **Sites** (each: the doc that belongs to item B now heads item A, and B has none):

| # | Doc belongs to (now undocumented) | Doc now sits on | Introduced |
|---|---|---|---|
| 1 | `scene.rs:200` `mark_scene_actor_bindings_dirty` ("Notify the alias resolver that the live candidate set changed", `:190`) | `scene_form_ids` (`:193`), whose own doc is glued under it | `43772fff8` 2026-10-22 |
| 2 | `obscript_effects.rs:179` `form_arg` ("The form a reference argument resolved to; `None` for an absent or null one", `:160`) | `adjust_standing` (`:164`) | `0539e07f8` 2026-10-21 |
| 3 | `context/post_passes.rs:83` `volumetric_sun` (the `portal_sun_radiance` / `cloud_transmittance` doc, `~:45-66`) | `caustic_sun_transmittance` (`:75`) | `7180d3109` (#6245) 2026-10-21 |
| 4 | `streaming/pre_parse.rs` `pre_parse_cell` (~`:697`; doc "Pre-parse one exterior cell's unique, uncached static NIFs...", `:652-664`) | `bake_absorbed_refs` (`:677`) | `88149ae26` (#6097) 2026-10-24 |
| 5 | `cell_loader/load_order.rs:368` `plugin_for_form_id` (the "ESL anywhere but last shifts every later regular plugin's position" doc, `~:352-362`) | `load_order_position_of` (`:363`) | `9634aa362` (#6088) 2026-10-25 |
| 6 | `load_order.rs:120` `all_regular` ("Build an order in which every plugin is a regular master...") | `has_slot_table` (`:115`) | `9634aa362` 2026-10-25 |
| 7 | `cell_loader/references/synth_child.rs:357` `spawn_synth_child` ("Dispatch one synthetic child placement... Split verbatim out of `load_references` (#2058)", `:304`) | `spawn_logical_synth_child` | `c82fcc407` (#6160 part) 2026-10-22 |
| 8 | `plugin/.../misc/dialogue.rs:1459` `parse_info_tclf_and_name_are_not_dropped` (the "#3614 TCLF/NAME" doc, `:1414`) | `parse_info_splits_begin_and_end_result_scripts_at_next` (`:1423`) | `6db47b0e4` 2026-10-22 |
- **Impact**: rustdoc and readers get the wrong contract on production items (sites 1-5, 7) and lose the contract for the right one. Sites 4, 5 and 6 landed 10-24/25, after `82f5d8919` (#6268, 10-23), because that sweep and the guard cover only the renderer crate.
- **Fix**: Move each doc block onto its item (rows 1, 3, 4 and 5 are pairs: the item that keeps its own doc is correct, the first-listed doc must move to the undocumented item). Then generalise `no_item_is_spliced_into_another_items_doc_block` (renderer-only today) into a shared helper in `crates/core/src/source_scan.rs` and run it over `byroredux`, `scripting`, `plugin` from `byroredux/src/workspace_hygiene_tests.rs`. Cheap heuristic: a `///` block whose first backticked identifier is an item name defined within 40 lines below but is not the item that follows.

##### TD3-2026-10-25-03: "Nothing inserts `PoolRegenConfig` in production" is false since `3dc16208e` (FO3/FNV Action Points regen, 2026-10-25); seven sites still say regen is inert
- **Severity**: LOW (MEDIUM if the in-flight VATS/AP work #6350 or a character audit trusts them)
- **Dimension**: 3
- **Effort**: small
- **Age**: `3dc16208e` (2026-10-25, #6350: "the ActionPoints pool goes live with load-order-priced regen"); `build_vats_model` (`byroredux/src/npc_spawn.rs:317`) returns the config and `cell_loader/references/mod.rs:362-365` does `insert_resource(regen)` on every Fallout3NV load.
- **Stale sites**:
  - `crates/core/src/character/regen.rs:121-127` ("Nothing inserts it in production on any game (#3848)... inert in effect"; also the struct now has an `action_points` lane)
  - `byroredux/src/boot/schedule/update.rs:841-849` ("No-ops today... arrives only once a live CharacterRuleset wiring reaches Oblivion")
  - `docs/engine/charal.md:243` (snippet lacks `action_points`) and `:262-266` ("Mechanism BUILT, registered, and no-op... inserted only by unit tests")
  - `docs/engine/charal-oblivion-ruleset.md:416-424`
  - `docs/feature-matrix.md:321-323` ("Pool regen remains inert")
  - `ROADMAP.md:668` ("`regen` no-ops until an Oblivion-shaped `PoolRegenConfig` exists")
- **Fix**: Reword to "inert on TES (no Fatigue/Magicka AVIFs); live for FNV/FO3 Action Points only (#6350, `build_vats_model`)". Feeds the next `/session-close`.

##### TD3-2026-10-25-04: status floors still say the six 10-24 AI package procedures are parse-only (ROADMAP is updated, feature-matrix and one ROADMAP bullet are not)
- **Severity**: LOW (the matrix is the "status floor"; no audit misled yet)
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `docs/feature-matrix.md:108-113` ("The remaining 7 procedures (Find/Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/UseWeapon) are parse-only"), `docs/feature-matrix.md:354` (gap row "Remaining PACK procedures (...)"), `ROADMAP.md:99-100` ("7 of ~17 package procedures need subsystems that do not exist (item use, package combat, magic)"). `ROADMAP.md:61-62` and `:296` are correct (only CastMagic open).
- **Age**: `b540b104a` Flee, `10f8cbb91` UseWeapon, `4b31a415c` Accompany, `9aaf5e1cf` Ambush, `7fded9423` UseItemAt, `fcd1cce4a` Find (all 2026-10-24)
- **Related**: `byroredux/src/npc_spawn/ai_package.rs:3470` test comment, see TD3-2026-10-25-09.
- **Fix**: Replace with "only CastMagic (no FO3/FNV package uses it) and Find for non-item object types remain; UseWeapon declines unmodelled conditions", mirroring `ROADMAP.md:296`.

##### TD3-2026-10-25-05: crime and VATS shipped 10-23..25 but several docs still say "no crime system" / "guards do not read it"; VATS has no matrix/ROADMAP row
- **Severity**: LOW (HISTORY Session 104 covers crime; VATS landed after the last close)
- **Dimension**: 3
- **Effort**: small
- **Location**:
  - `docs/engine/scripting.md:809` ("the engine has no crime system, so scripts' writes are all of it"; `byroredux/src/crime.rs:107-119` now writes `FactionCrimeGold`)
  - `ROADMAP.md:276` ("guards do not read it"; `99051a4eb` guards rush a wanted player, `73924db73` guard dialogue reads the bounty, `1a67c9f30` witnesses call guards)
  - `docs/feature-matrix.md:238` ("Not yet: ... crime hostility")
  - `docs/feature-matrix.md:206` ("crime/detection ... stay declined"; `e161c45f3` made `PayFine`, `PayFineThief`, `GoToJail` run, though `PlayerPayCrimeGold` stays declined)
  - VATS (`818eba7f4`, `7eb4de261`, `3dc16208e`, `077ff45d6`, `ff97cdd93`, `aebcc99a1`, 2026-10-25): `vats_mode_system`, `vats.*` console commands and `docs/smoke-tests/vats-targeting.sh` exist. `debug-cli.md:532-536` and `docs/smoke-tests/README.md:61` are updated. `docs/feature-matrix.md` and `ROADMAP.md` have no VATS or Action Points runtime row, and HISTORY has no Session 105.
- **Fix**: Add a matrix row for VATS and AP runtime ("plumbing; FNV first gate, #6350 open"). Correct the three crime sentences. The VATS part can wait for `/session-close`, but the three crime sentences contradict code that landed in Session 104.

##### TD3-2026-10-25-06: #6237 removed `RigidBodyData` from the save registry (v116), but three places still describe it as a registered, preserved column
- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Location**:
  - `docs/engine/save-load-roundtrip.md:226-231` ("a curated mutable-column set of 24 entries (... `CharacterController`, `RigidBodyData` registered *preserving* so its `serde(skip)` `part_surface_materials` survives the overlay, #5686 ...)"). `MUTABLE_DELTA_COLUMNS` (`byroredux/src/save_io/registry.rs`) has 30 entries and no `RigidBodyData`.
  - `crates/core/src/ecs/components/collision.rs:148-152` ("The live-load overlay registers this component *preserving* -- the apply hook (`keep_reimported_part_surface_materials`, #5686)"). The fn was deleted in `6765e60eb` (2026-10-21, #6237); `snapshot.rs:582` records "the `RigidBodyData` column is gone". The `part_surface_materials` doc also still reasons about the save fingerprint.
  - `docs/engine/save-load-roundtrip.md:18` (a historical "registered component count doubled (... `RigidBodyData` ...)" aside) is dated and fine. `crates/save/src/registry.rs:117` uses `RigidBodyData::part_surface_materials` as the *example* for `register_component_preserving` (still valid code, but `Transform` is now the only user, `:567`).
- **Fix**: Update the count to 30, drop the `RigidBodyData` clause, and replace the `collision.rs` sentence with "not saved (the column is gone, #6237); re-derived at import". Update the `registry.rs:117` example to `Transform`/`keep_live_translation`.

##### TD3-2026-10-25-07: `game-loop.md` live-schedule table omits the 10-22..25 systems
- **Severity**: LOW (same family as closed #5110/#5318; no guard exists)
- **Dimension**: 3
- **Effort**: small
- **Location**: `docs/engine/game-loop.md:136-176`. Not listed:
  - `vats_mode_system` (Early, exclusive + declared, `boot/schedule/early.rs:109-111`, `818eba7f4`)
  - `flee_package_system`, `find_package_system` (PostUpdate, `post_update.rs:124-127`)
  - `use_weapon_package_system` (Update, `update.rs:722`)
  - the crime systems (`jail_bed_system` `update.rs:309`, `evidence_deposit_system` `:322`, `make_guard_rush_system` `:395`)
- The PostUpdate AI row still lists only "sandbox/wander/travel/follow/escort/guard/patrol + eat_sleep + graph locomotion + console walk + walk animation".
- **Fix**: Add rows or extend the AI row. Optional guard: a test that parses `scheduler.add*` stage/system names in `boot/schedule/*.rs` and asserts each appears in `game-loop.md` (the `PARALLEL_SYSTEMS` machinery at `boot/schedule/mod.rs:760` already enumerates parallel systems).

##### TD3-2026-10-25-08: doc prose names functions and types renamed or deleted (current-tense)
- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Sites** (all verified with `git log -S` and a HEAD grep):
  - `docs/engine/archives.md:266` and `:454` name `linear_size_for()`. It is now the private `pitch_or_linear_size_for` (`crates/bsa/src/ba2.rs:1187`; renamed by `ec63d2636`, 2026-09-28).
  - `docs/engine/save-load-roundtrip.md:95` names `SaveLoadNotifications`. It was replaced by `PlayerNotifications` (`byroredux/src/notifications`; last gone from `.rs` at `ae25ade38`, 2026-09-16).
  - `docs/engine/exal-groundcover.md:673-683` says GRAS "translates to species at the EXAL boundary (`species_from_gras` / `authored_species` in `groundcover_translate.rs`)... 138 of the 168 vanilla records... become real species". `f8a900b3c` (2026-09-13, "stop translating GRAS records into blade species") deleted both functions; `groundcover_translate.rs` now has `resolve_authored_cover` / `authored_cover_record`. Re-describe the current path.
  - Tracked elsewhere, still live: `CellRootRefIndex` / `persistent_ref_index` in `exterior-readiness-plan.md` (#6171) and the `active_*_location/_target` selectors in `npc-spawn-ai-packages.md` (#6169).
- **Fix**: Rename in prose. For exal-groundcover, rewrite the paragraph to the current decode-only behaviour.

##### TD3-2026-10-25-09: stale or malformed comments in recently touched code
- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Sites**:
  - `byroredux/src/npc_spawn/ai_package.rs:3470`: test comment "Ambush (9): a procedure with no runtime, so the package wins with no behavior". Ambush has had a runtime since `9aaf5e1cf` (10-24: `AmbushBehavior`, `ai_package.rs:250-256`, `:390`). The test presumably passes because `pack(.., None)` has no `PLDT` location; reword to say that.
  - `byroredux/src/asset_provider/texture.rs:1262`: the `default_sound_candidates_hit_their_tagged_game_archive` doc opens a ```` ```sh ```` fence (`:1258`) and never closes it (the doc ends at a blank line). Age: `a6295284e` (2026-09-06, #3913).
  - `crates/plugin/examples/probe_weap_vats.rs:19` and `probe_weap_dnam.rs:12` cite `probe_weap_align.rs` as evidence. That file was deleted in this window (`git log --diff-filter=D`). Italicise or drop.
- **Fix**: As stated.

##### TD3-2026-10-25-10: the symbol validator cannot see scripts, native sources or lockfiles, producing about 35 of the 371 advisories
- **Severity**: LOW (tooling noise; the advisory list is meant to be cleared)
- **Dimension**: 3
- **Effort**: small
- **Evidence**: of the 371, 35 symbols exist in tracked non-`.rs` files: env vars in `scripts/renderer-eval*.sh` (`BYROREDUX_RENDER_EVAL_*` x5, `BYROREDUX_GROUNDCOVER_EVAL_CASES`, `BYROREDUX_RENDER_EVAL_RUNNER`), FSR native names in `crates/fsr3-sys/native/byro_fsr3.cpp` (`ffxCreateContext`, `ffxQuery`, `enableSharpening`, `jitterOffset`, `preExposure`, `motionVectorScale`, `viewSpaceToMetersFactor`), crate names in `Cargo.lock` (`cranelift`, `enumset`, `realfft`, `ruffle_render`, `xxhash64`), and bench telemetry labels in `.tsv`/scripts (`gpu_composite`, `gpu_sky_cube`, `gpu_cloud_shadow`, ...). Another ~220 are vanilla EditorIDs / ObScript+Papyrus names / GMST names / NIF texture names (intentional, see triage).
- **Fix**: Let `_audit-validate.sh` widen the haystack to `scripts/**`, `*.sh`, `*.cpp`, `*.glsl`, `Cargo.lock`. Add an allowlist file for record-name and EditorID docs (`charal-*`, `scripting.md`, `dialogue-trees.md`, `npc-spawn-ai-packages.md` EditorIDs) so the remaining ~100 are the actionable set.

##### TD3-2026-10-25-11: ROADMAP Known Issues cite closed issues as the tracker of still-unbuilt work
- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `ROADMAP.md:677` ("takes effect on the next cell load only (#5065)... live consumer needs per-placement GPU/physics teardown"): #5065 was closed 2026-10-12 by `f579785e3`, a docs-only commit that made ROADMAP itself the tracker. `ROADMAP.md:~668`-area text names #4213 (NiStencilProperty "waits for a consumer", closed 2026-09-15) and "emissive is parked (#5503)" (closed 2026-10-11). 16 of 18 issue numbers cited in Known Issues are closed; most are history (fine), but these three are the *only* record of open work.
- **Fix**: Open one forward issue per item (live Enable()/Disable() consumer; NiStencilProperty consumer; Starfield emissive) and cite those, or mark the ROADMAP lines "tracked here only".


#### Dimension 4 — Audit-Finding Rot (5)

##### TD4-2026-10-25-02: `audit-scripting` cites open #5586 as "`mq101_conformance.rs::run` oversized" — the function is 45 lines
- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-scripting/SKILL.md:84`; issue #5586 (OPEN, title "…`run` is a 1454-line, depth-15 function")
- **Status**: Existing: #5586 (premise stale)
- **Evidence**: `crates/scripting/examples/mq101_conformance.rs:397` `fn run()` … closes at `:441` = 45 lines (confirms the Dim 1 agent). The file itself is still 2157 prod LOC (Dim 1's subject), but that is not what #5586 or the callout assert. (Last commit to the file: fd70bf557, 2026-10-18.)
- **Suggested Fix**: close or retitle #5586 to whatever the 2157-LOC file's real seam now is (Dim 1 owns that); drop the callout from audit-scripting:84 (it is tagged "debt, not runtime" so no audit loses anything).

##### TD4-2026-10-25-03: No gate sees a closed issue described as open; the class has recurred five times
- **Severity**: LOW · **Dimension**: 4 · **Effort**: small
- **Location**: `.claude/commands/_audit-validate.sh` (path/symbol gate only); `.claude/commands/audit-sync/prep.sh` (closed-since-BASELINE only)
- **Status**: NEW (root cause of TD4-01; prior trackers #4369, #5606, #5742, #6173 each fixed instances, not the class)
- **Description**: `audit-sync` only finds issues closed since its BASELINE, so anything closed after the sync (today: 8 issues in 12 hours, 6 via one close-sweep) re-stales the skills immediately and nothing flags it until the next Dim 4 pass. `_audit-validate.sh` cannot see numbers or issue state.
- **Suggested Fix**: add an advisory (non-fatal, skipped when `gh` is unavailable or offline, like `SKIP_SYMBOL_CHECK`) to `_audit-validate.sh`: for each `#N` within ~160 chars of `known[- ]open|still open|stays open|open #N|#N[^.;]{0,25}open|do not re-file|don't re-file` in skill files, `gh issue list --state all --json number,state` once and print `file:line #N CLOSED` for closed ones. A working prototype (regex + window logic, 66 raw hits triaged to the 11 true sites above) is `/tmp/audit/tech-debt/dim4_issues3.py`; false-positive control: skip windows where "open" is a verb (`open conversation`, `fail-open`, `half-open`).

##### TD4-2026-10-25-04: `audit-character` says the sub-module count is 15 and "pinned"; the pin says 16
- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-character/SKILL.md:20` vs `crates/core/src/character/mod.rs:161-165`
- **Evidence**: skill: "`crates/core/src/character/` (15 modules, `mod_docstring_indexes_every_sub_module` pins the count)". The test asserts `declared.len() == 16`; `ls` shows 16 sub-modules plus `mod.rs` (affliction, attribute, components, crime, derived, fallout, leveling, profile, regen, reputation, resistance, ruleset, skill, skyrim, tes, vats).
- **Suggested Fix**: "16 modules". (6% drift; filed because it contradicts the very guard it cites.)

##### TD4-2026-10-25-05: `audit-tech-debt` Phase-1 paragraph: headline ">2000 production LOC: 4" and the watch-list are now wrong
- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:89-103`
- **Status**: recurrence of #6176 (closed TD4-2026-10-19-04); not a regression of a fix — the paragraph is dated "diff direction only" and rots each audit by design
- **Evidence** (`/tmp/audit/tech-debt/baseline.txt`, `dim1_big.txt`; prod_loc self-test ok): files >2000 prod LOC = **5**, not 4. New entry `crates/scripting/src/obscript_effects.rs` 2034 (1528 at dcad3669d → 2032 at d5d13edcc, #6139 stage 8, 2026-10-24; wc 2951). Other drift: `shader_constants_data.rs` 2964→2972, `byroredux/src/npc_spawn.rs` 1846→1911, `context/init.rs` 1920→1950; `#[ignore]` 368→373, `allow(dead_code)` 43→41 (markers 26 and test-heavy 93 unchanged). Items that checked out: audio 2086, types.rs 2011, mq101_conformance 2157, inventory 1956, panels 1938, gpu_timers 1929, world.rs 1912, merge.rs 1860; hub sizes "~70–150 prod LOC" (68–153 measured) and "largest child ~1400" (interp.rs 1396); "24 crate dirs and three of four tools unnamed" (recipe re-run: exactly 24 / 3); no new crate/tool manifests since 10-21; `cell_loader/exterior.rs` split (1d383cc5a) is already reflected in `_audit-common.md:9`, `_audit-owners.md:147` and four skills.
- **Suggested Fix**: re-baseline the paragraph (5 files; add obscript_effects.rs to the list with its #6139 cause; move npc_spawn.rs/init.rs/inventory.rs/panels.rs within-5% rows), or — better — replace the instance list with "run the recipe" and keep only the count delta, since the list is regenerated every audit anyway.

##### TD4-2026-10-25-06: `audit-tech-debt` "Recent yield: Dims 4, 3, 1, 8 dominate (then 2, 9)" contradicts the reports it cites
- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:~108-110` ("Recent yield (`grep -h '\*\*Dimension\*\*' …`)")
- **Evidence**: counting `**Dimension**` leading digit over all 40 `AUDIT_TECH_DEBT_*.md`: Dim 2 = 106, 1 = 104, 3 = 81, 8 = 60, **4 = 54**, 7 = 33, 9 = 30, 6 = 23, 5 = 8. Last 5 reports: 2 = 63, 3 = 28, 1 = 26, 7 = 18, 4 = 17, 8 = 17, 9 = 15. Dim 2 is the top yield (the skill ranks it fifth); Dim 4 is fifth, not first; Dim 7 outranks 9. ("5 and 6 rarely surface real debt" holds.)
- **Suggested Fix**: "Dims 2, 1, 3 dominate (then 8, 4; 7, 9)", or drop the sentence — it steers agent ordering/effort.


#### Dimension 5 — Stale Markers (0)

No findings. See Checked-clean.


#### Dimension 6 — Stubs & Placeholders (2)

##### TD6-2026-10-25-01: 13 EDID+FULL-only record types are parsed into `EsmIndex` maps that nothing outside tests/examples ever reads
- **Severity**: LOW · **Dimension**: 6 · **Effort**: small (decide per type) · **Age**: #810/#458 (2026-05); table re-confirmed through 2026-10
- **Location**: dispatch `crates/plugin/src/esm/records/parse.rs:443-449` -> `dispatch_misc_stub.rs` (arms use `parse_minimal_esm_record`); maps in `records/index.rs`.
- **What**: Of the 31 minimal-tier record types, these `EsmIndex` fields have **zero non-test, non-parse readers** (`grep -w <field>` outside `tests/`, `examples/`, `index.rs`, `parse.rs`, `dispatch_misc_stub.rs`): `animation_objects` (ANIO), `camera_shots` (CAMS), `camera_paths` (CPTH), `default_objects` (DOBJ), `menu_icons` (MICN), `ammo_effects` (AMEF), `load_screen_types` (LSCT), `placeable_waters` (PWAT), `dehydration_stages`/`hunger_stages`/`sleep_deprivation_stages` (DEHY/HUNG/SLPD), `recipe_categories`/`recipe_records` (RCCT/RCPE), `birthsigns` (BSGN), `sigil_stones` (SGST). Per-record `HashMap<u32, MinimalEsmRecord>` + String allocs on every FNV/FO3/Oblivion load. The dispatch comment ("when a real consumer arrives, replace the arm") makes this a deliberate parking lot, but nothing in ROADMAP/docs lists which of these are genuinely owed: BSGN (Oblivion birthsign abilities never reach the player ruleset), HUNG/DEHY/SLPD (FNV Hardcore needs; `charal-fnv` has no consumer), RCPE/RCCT (crafting), CPTH/CAMS (the load-screen MOD2 camera paths in open #6194), DOBJ (engine default forms).
- **Fix**: (a) add one line per type to the ROADMAP "Known Issues -> Correctness and content" naming it as parsed-not-consumed with the owning future milestone, or (b) drop the maps for types with no owner (ANIO, MICN, AMEF, LSCT, PWAT, SGST) and let them fall to `skipped_unconsumed_groups`, which already gives the telemetry. Not a bug; the cost is parse time + memory for dead data.

##### TD6-2026-10-25-02: `owned.rs` still says guards, arrest and the other crimes "are not modelled"
- **Severity**: LOW · **Dimension**: 6 (stub-status doc; Dim 3 overlap) · **Effort**: trivial · **Age**: line written e63fa457d 2026-10-23, made false by 99051a4eb (guards rush) / e161c45f3 (PayFine/GoToJail) / 39ffea9a8 (assault + murder), all 2026-10-24
- **Location**: `crates/core/src/ecs/components/owned.rs:19-22`
- **What**: "... a theft seen by an NPC adds its bounty (`byroredux/src/crime.rs`); guards, arrest and the other crimes are not modelled." Today `crime.rs` models assault and murder, `systems/guard.rs` + `make_guard_rush_system` rush a wanted player, and `scripting/src/jail.rs` implements PayFine/GoToJail/serve-sentence/evidence-chest (Oblivion; Skyrim/FO4 jail is explicitly "parsed and unused" per `docs/engine/charal-skyrim-ruleset.md:811`). The doc steers readers away from a working subsystem. `byroredux/src/crime.rs:40-44` carries the accurate "Not modelled yet" list (sneak/stealth roll, disposition drop, hostile spells, creatures-as-victims, Shivering Isles).
- **Fix**: replace the clause with a pointer: "bounty, witnesses, guards and Oblivion jail live in `byroredux/src/crime.rs` and `scripting/src/{jail,guards,crime_knowledge}.rs`; see that module's *Not modelled yet* list".


#### Dimension 7 — Magic Numbers (5)

##### TD7-2026-10-25-01: `VatsModel::ap_regen_per_second` hard-codes the 60 Hz tick that `POOL_REGEN_DT` already names
- **Severity**: LOW · **Dimension**: 7 · **Effort**: trivial · **Age**: 3dc16208e 2026-10-25
- **Location**: `crates/core/src/character/vats.rs:292` `self.ap.restore_rate * 60.0`; the constant is `crates/core/src/character/regen.rs:45` `pub const POOL_REGEN_DT: f32 = 1.0 / 60.0;` (documented "Fixed regen tick — 60 Hz"). The FNV struct doc (vats.rs:211) also restates "0.06 x 60 Hz = 3.6/s".
- **What**: The per-frame -> per-second conversion is the inverse of the tick the regen system consumes the result at. If the fixed tick is ever retuned, ActionPoints regen silently desyncs from Fatigue regen (same `PoolRegenConfig`, same `POOL_REGEN_DT`).
- **Fix**: `self.ap.restore_rate / POOL_REGEN_DT` (import from `super::regen`).

##### TD7-2026-10-25-02: combustion temperature clamps/floors live as shader-local literals; the exemption's "no Rust counterpart" is false for the 4200 K ceiling
- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: b3ee51921 2026-08-18 (ceiling); regimes in core unchanged since
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:421` `const float MAX_COMBUSTION_TEMPERATURE_K = 4200.0;` (exempted at `shader_constants.rs:225`), `:1419` inline `nuclearExplosion ? 3800.0 : 2300.0`, `:420` `IGNITION_TEMPERATURE_K = 720.0`, `:422` `MAX_COMBUSTION_SPEED_MPS = 28.0`; core side `crates/core/src/combustion.rs:282-283` `CombustionRegime::NUCLEAR_EXPLOSION { temperature_k: 4200.0, .. }` (others: FLAME 1850, EXPLOSION 2800, GAS_FLAME 3400, ADIABATIC 2350).
- **What**: `sanitizedChemistry` (`:1730-1737`) and the source-temperature clamp (`:1417-1421`) clamp transported chemistry to `[AMBIENT, 4200]`. 4200 is exactly the nuclear regime's authoring temperature in core, so raising any regime above 4200 K (e.g. a plasma/magic regime — the `GAS_FLAME` doc anticipates one) is silently clamped by the shader with no test or constant tying the two. Everything else in the combustion model flows core -> `shader_constants_data.rs` (`ADIABATIC_FLAME_TEMPERATURE_K`, `COMBUSTION_SOOT_OXIDATION_*`, `COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2`, ...); these four did not. The 3800/2300 K floors match no core regime (core: 4200 / 1850-2800), i.e. they are already a divergent duplicate.
- **Fix**: add `COMBUSTION_MAX_TEMPERATURE_K = CombustionRegime::NUCLEAR_EXPLOSION.temperature_k()` (const fn) and the floors to `shader_constants_data.rs`, drop the `MAX_COMBUSTION_TEMPERATURE_K` exemption, and add a core test that no `CombustionRegime` exceeds the ceiling.

##### TD7-2026-10-25-03: the water rain-control bit-pack is a bare-literal CPU/GPU contract with no shared constants and no round-trip test
- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: c5612af8d 2026-08-20
- **Location**: pack `byroredux/src/render/water.rs:73-84` (`* 1023.0`, `<< 10`, `<< 20`, `clamp(0.0, 0.999)`); unpack `crates/renderer/shaders/water.frag:929-934` (`& 1023u`, `>> 10u`, `>> 20u`, `/ 1023.0`, then `x / max(1 - x, 0.001)`).
- **What**: Three 10-bit lanes are quantized with a `v/(v+1)` map in the binary crate and decoded in the shader; the lane width, shifts, scale and the 0.999 clamp (which bounds the decode at ~999) exist only as duplicated literals on both sides. The only guard is a source-substring assert (`vulkan/water.rs:1325`, checks `floatBitsToUint(push.normal_falloff.w)` exists). The file that owns the push-constant layout (`vulkan/water.rs`) does not own the encoding.
- **Fix**: move the pack/unpack pair beside `GpuWaterParams`, export `RAIN_CONTROL_BITS/LANES` through `shader_constants_data.rs`, generate the GLSL mask/shift defines, and add a Rust round-trip test (pack -> unpack in Rust mirroring the GLSL math) pinning the 0.999 clamp.

##### TD7-2026-10-25-04: inline clamp bounds on material-authored values are repeated across shader includes, outside the provenance gate
- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Age**: lighting.glsl clamp 2026-08-25; ray_hit.glsl 9ade75063 2026-07-30; material_sampling.glsl 218b425be 2026-06-16
- **Location**:
  - Fresnel exponent `clamp(x, 0.25, 16.0)` x3: `shaders/include/pbr.glsl:112,129` and `include/lighting.glsl:155` (the rim-light exponent; also the fallback `2.0` at `:154`).
  - Parallax layer budget: `include/material_sampling.glsl:115-116` `clamp(maxPasses, 4.0, 32.0)` / floor `8.0` (primary) vs `include/ray_hit.glsl:347-348` `clamp(mat.parallaxMaxPasses, 4.0, 12.0)` / floor `4.0` (secondary). CPU default is `DEFAULT_PARALLAX_MAX_PASSES = 4.0` (`core/ecs/components/material.rs:21`) and nothing on the CPU clamps to the shader's range.
  - `presentation.frag:188-193` radial-blur: loop `i <= 6`, `/ 6.0`, `/ 7.0` — three coupled literals for one tap count.
- **What**: The gate only inspects declared SCREAMING constants, so repeated inline clamp bounds slip through; changing the Fresnel range in `pbr.glsl` but not `lighting.glsl` would make rim light and Schlick disagree on the same authored exponent. The primary/secondary parallax caps (32 vs 12) are intentional per the comments but are bare numbers on both sides.
- **Fix**: `FRESNEL_POWER_MIN/MAX`, `PARALLAX_PASSES_MIN/PRIMARY_MAX/SECONDARY_MAX` (+ floors), `PRESENTATION_RADIAL_TAPS` in `shader_constants_data.rs`; optionally have the importer clamp `parallax_max_passes` to the same range so the contract is stated once.

##### TD7-2026-10-25-05: sub-record length gates added since 10-21 are unnamed, and PACK repeats `12` six times
- **Severity**: LOW · **Dimension**: 7 · **Effort**: trivial · **Age**: 2026-10-24 (PKW3 10f8cbb91, CRVA 4cfcee4b9, CTDA 73924db73); ACBS #6120
- **Location**: `plugin/src/esm/records/actor/faction.rs:329,336` `CRVA` `>= 14` / `>= 20`; `misc/pack.rs:1266` `PKW3` `>= 20` (its own doc says "24 bytes"); `pack.rs:1232,1259` `PLD2`/`PTD2` `>= 12` plus the same `12` re-checked inside `parse_location`/`parse_target`/`parse_data_target` (`pack.rs:832,855,882`) and the PLDT/PTDT arms; `actor/npc.rs:866-867` ACBS `>= 20` (FO4/76) / `>= 16` (Starfield); `condition.rs:398` `data.len() == 24` (Oblivion CTDA) beside the already-named `CTDA_RUN_ON_MIN_LEN = 28` (`:166`).
- **What**: The tree already names these (`CHAL_DATA_LEN`, `GRAS_DATA_LEN`, `CTDA_RUN_ON_MIN_LEN`, `weather.rs SKYRIM_*_SIZE`, `PackUseWeapon` flags). The new gates are the exceptions, and in PACK the double gate (arm guard + helper guard) means two edits per layout change. The PKW3 `>= 20` vs the documented 24-byte body is deliberate tolerance but undocumented in code.
- **Fix**: `const CRVA_MIN_LEN: usize = 14; CRVA_TAIL_LEN = 20; PACK_LOCATION_LEN = 12 (shared by PLDT/PLD2/PTDT/PTD2/PDTO); PKW3_MIN_LEN = 20; ACBS_FO4_LEN = 20; ACBS_SF_LEN = 16; CTDA_OBLIVION_LEN = 24`, and drop the arm-level duplicates where the helper already checks.


#### Dimension 8 — Dead Code (7)

##### TD8-2026-10-25-01: `--all-targets` re-accrued 11 lints in 3 files, one deny-level — the `||  true` is also a vacuous assertion
- **Severity**: LOW (non-blocking CI step) · **Dimension**: 8 (overlaps 9) · **Effort**: small · **Status**: Regression of #6193 (CLOSED, bbda0c975)
- **Location / sites** (all test/example targets; `-D warnings` gate does not see them):
  - `crates/plugin/tests/parse_real_esm.rs:6467` — **error** `clippy::overly_complex_bool_expr`: `.all(|race| !race.body_part_textures.is_empty() || true)`. Added by 9d1a262ed (#6090, 2026-10-25). The assertion cannot fail, so the "Skyrim+ authors none — the parser's own gate, pinned" comment is a lie; and the deny-level error aborts that test target's lint (`byroredux-plugin` test "parse_real_esm" "could not compile").
  - `crates/plugin/src/esm/records/actor/tests.rs:56,59,1478,2673-2677` — 8x `needless_borrows_for_generic_args` (`sub(b"MODT", &[0u8; 12])` etc.; lines from 4a7963156 #6297 10-22, 1a67c9f30 10-24, 4cfcee4b9 10-24).
  - `crates/scripting/src/condition/tests.rs:2480` — `clone_on_copy` on `Condition` (1a67c9f30, 10-24).
  - `byroredux/src/asset_provider/texture.rs:1272` — `type_complexity` on the `[(&str, fn() -> PathBuf, &str); 4]` table (3e4bfab51 #6184, 10-22).
- **Fix**: drop the `&` on the byte-array args and the `.clone()`; name the tuple (`type GameArchiveCase = …`). For 6467 either assert something real (`race.body_part_textures` is empty for Skyrim+ races per the comment: `.all(|race| race.body_part_textures.is_empty())`, guarded to non-Fallout races) or delete the block. Re-run `--all-targets --keep-going` to zero.
- **Merged from Dim 9 (TD9-2026-10-25-02, `test-gap`)**: the `|| true` at `parse_real_esm.rs:6467` is also a vacuous assertion — the test parses only `Fallout3.esm`, so no Skyrim+ race is ever visited and the "Skyrim+ authors none" gate is not exercised. Fix both in one commit.

##### TD8-2026-10-25-02: `txst_textures` is a write-only map plumbed through `parse_txst_group`
- **Severity**: LOW · **Effort**: small · **Age**: reader deleted 65add6a2e (#6036, 2026-10-20); missed by the 10-21 report
- **Location**: `crates/plugin/src/esm/records/parse.rs:162,254` (declared, passed `&mut`, never read); `crates/plugin/src/esm/cell/support.rs:621-767` (`parse_txst_group` / `_inner` param, recursion at :655, the `txst_textures.insert(header.form_id, diffuse.clone())` at :767 under the comment "Backward-compat LTEX resolver: legacy diffuse-only map"); callers: 14 sites in `cell/tests/txst.rs` + `grup_walker.rs:665`.
- **What**: #6036 moved LTEX→TXST resolution to the merged load order (`landscape_texture_txsts: ltex.ltex_to_txst`, parse.rs:536) and deleted the only loop that read `txst_textures` (`txst_textures.get(txst_id)` -> `landscape_textures`). The map is still built per TXST (a `String` clone per record) and the doc at support.rs:621 still says "kept for the LTEX resolver downstream". `txst.rs:293` even pins that MNAM-only TXSTs are absent from "the legacy map".
- **Fix**: delete the `HashMap<u32,String>` param from `parse_txst_group(_inner)`, the insert, the local in parse.rs, and the `diffuse_only` plumbing in the 14 tests (assert on `texture_sets[...].diffuse` instead, which already carries TX00).

##### TD8-2026-10-25-03: five `_`-prefixed parameters that survived refactors (delete, do not rename)
- **Severity**: LOW · **Effort**: small · **Status**: NEW (sibling of closed #4386 / #6192 `_receiver_kind`)
- **Location** (each param unused in the body; all callers inside the same crate):
  - `crates/nif/src/import/material/walker.rs:21-27` `extract_vertex_colors(_scene, _shape, data, _inherited_props, mat)` — three dead params; the doc itself says "Pre-#438 this function ignored its `_mat` parameter and re-scanned…" i.e. the re-scan was removed and the parameters were left (26e11db0a, 2026-04-26 split). One caller: `import/mesh/ni_tri_shape.rs:158`.
  - `crates/nif/src/import/material/mod.rs:153` `is_decal_from_modern_shader_flags(flags1, _flags2, …)` and `:175` `is_two_sided_from_modern_shader_flags(_flags1, flags2, …)` (c8a0f313e, 2026-04-26, #712). Callers `dedicated_shader.rs:336,348,661,671` + tests.
  - `byroredux/src/material_translate.rs:1188-1189` `normal_alpha_spec_applies(material_kind, _metalness, _env_map_scale, …)` — body is `material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0`; its two callers (:1245, :1296) and 6 test asserts still compute and pass `metalness`/`env_map_scale`. `:1266` `normal_alpha_spec_roughness(…, _glossiness, …)` (9 params, already under `allow(too_many_arguments)`; 10 call sites).
- **Not flagged (legitimate)**: `write_rgba_inplace(_ctx: GpuUploadCtx)` and `SkinComputePipeline::dispatch(_bound)` (capability-token params), `natives/{alias,forms,location}.rs` `_frame/_args/_statement` (fn-pointer table signatures), trait-impl `_args`/`_event_loop`.
- **Fix**: drop the params and update callers/tests; the `normal_alpha_spec_*` pair also sheds an `allow(clippy::too_many_arguments)` need.

##### TD8-2026-10-25-04: three pub items with zero callers
- **Severity**: LOW · **Effort**: trivial · **Status**: NEW
- `crates/scripting/src/fragment/populate.rs:502` `populate_scene_fragments_from_pex_detailed` — only mention outside its body is the re-export `crates/scripting/src/lib.rs:163`. Production uses `populate_owned_scene_fragments_from_pex_detailed_with_providers` (`byroredux/src/asset_provider/script.rs:635`). #6189 deleted this one's non-`_detailed` sibling (`populate_scene_fragments_from_pex`) but left the `_detailed` wrapper behind (added f5127c1cc, 2026-09-10). Delete fn + `lib.rs:163` name. (The quest twin `populate_quest_fragments_from_pex_detailed` is still called by `populate_quest_fragments_from_pex`.)
- `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:41` `NiTriShape::name_str` under the comment "Backward-compatible field access" — no caller anywhere (`.name_str()` hits nothing); `HasObjectNET::name` (:67) already provides it. Delete the `impl` block.
- `crates/physics/src/world/queries.rs:572` `PhysicsWorld::bodies_overlapping_body` — all three production sites call `bodies_overlapping_body_where` (`systems/script_queries.rs:152`, `systems/trap_hits.rs:246,352`). Added 6dc948ac4 (2026-10-14). Only mentions are docs (`docs/engine/scripting.md:1860`, `AUDIT_PHYSICS_2026-10-16.md`) — update that doc line if deleting.

##### TD8-2026-10-25-05: pub helpers reachable only from tests are not `#[cfg(test)]`
- **Severity**: LOW · **Effort**: trivial · **Status**: NEW (same class as closed #6192, whose `vdf.rs string_map` fix was exactly `#[cfg(test)]`)
- `crates/audio/src/lib.rs:843` `AudioWorld::is_script_music_active` (567bfe593, 2026-10-21; sole caller `byroredux/src/systems/script_requests.rs:462` inside `#[cfg(test)]`).
- `crates/plugin/src/esm/records/misc/equipment.rs:242` `first_part_name` ("the old stub's sanity-check field"; sole caller its own test :559; the stub it checked is gone).
- `crates/scripting/src/condition/catalog.rs:648` `ConditionFunction::oblivion_dialect` (73924db73, 10-24; callers only `condition/tests.rs`).
- `crates/scripting/src/crime_faction.rs:69` `CrimeFactionData::from_map` (4cfcee4b9, 10-24; doc "for a caller that stands in for a load order"; sole caller `byroredux/src/crime.rs:590` in tests).
- `crates/plugin/src/esm/records/reflection.rs:217` `weather_root_class` (callers: its unit test + `tests/parse_real_esm.rs`; plugin-crate public API, note only).
- `byroredux/src/interaction.rs:187` `bind_key` and `save_io/player.rs:68` `queued` already use `#[cfg_attr(not(test), allow(dead_code))]` — the tidy equivalent is a plain `#[cfg(test)]`.
- **Fix**: `#[cfg(test)]` on each (or delete `first_part_name`).

##### TD8-2026-10-25-06: `LightKind` is a "Backward-compatible name" alias with ~25 production uses
- **Severity**: LOW · **Effort**: small · **Status**: NEW
- **Location**: `crates/core/src/ecs/components/light.rs:6-7` `pub type LightKind = EmitterKind;` Consumers still spell the alias: `byroredux/src/render/lights.rs:15,80-84`, `cell_loader/spawn.rs:648`, `spawn/mesh_instance.rs:572`, `spawn/{authored_cover,precombined}.rs`, `precombined.rs:500`, `cornell/builders.rs:211` (76 whole-repo hits incl. tests vs 12 for `EmitterKind`). The canonical name is `EmitterKind`; the engine has no external consumers, so "backward-compatible" protects nothing and leaves two names for one type.
- **Fix**: `sed` the alias to `EmitterKind` across `byroredux/` + `crates/core`, delete the alias and its re-export.

##### TD8-2026-10-25-07: `worldspace_climates` docs call a live input "back-compat" and point at a deleted line
- **Severity**: LOW (doc rot, Dim 3 overlap) · **Effort**: trivial · **Status**: NEW
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:15-17` ("preserved for back-compat with the cell loader — see byroredux/src/cell_loader.rs:778"; that file is 271 lines) and `cell/index.rs:73-77`. The map is the live input of `env_translate::resolve_worldspace_climate` (`byroredux/src/env_translate/climate.rs:37,61,98`) and two examples.
- **Fix**: reword both docs to "CLMT FormID per worldspace, consumed by `env_translate::climate::resolve_worldspace_climate`"; drop the stale line reference.


#### Dimension 9 — Test Hygiene (3)

##### TD9-2026-10-25-03 - LOW - #6184 class re-grown inside the allowlist: `animation.rs` hard-codes the Steam root and silently skips four times
- **Dimension**: 9 (`test-gap`) - **Effort**: small - **Age**: new site 051179b7b (#6241, 2026-10-23); other three older
- **Location**: `byroredux/src/asset_provider/animation.rs:768-775` (`fallout_4_turn_in_place_clip_carries_its_turn_as_root_yaw`, new),
  `:1262`, `:1353`, `:1425` (three Skyrim tests)
- **What**: all four resolve `BYROREDUX_*_DATA` with a literal `/mnt/data/SteamLibrary/...` fallback and, on a missing archive, `return`
  (or `let Ok(..) else { return }`) with no `require_game_data` / `test_paths::require_or_skip`. Under `BYROREDUX_REQUIRE_GAME_DATA=1`
  they report `ok` for an absent install: the #3850 false green. `no_test_region_hardcodes_the_dev_machine_game_path`
  (workspace_hygiene_tests.rs:1370) cannot see it because the ratchet is **per file** and `animation.rs` is on
  `HARDCODED_GAME_PATH_ALLOWLIST`; since base, 4 more literals were added to allowlisted files (animation.rs +1, npc_spawn/tests.rs +1,
  ragdoll_pose_tests.rs +1; of these only animation.rs's skips silently, the other two assert under REQUIRE).
- **Fix**: use `test_paths::fnv_data_dir()`-style helpers (`fo4_data_dir()`, `skyrim_se_data_dir()` exist in `crates/plugin/src/esm/test_paths.rs`)
  and `require_or_skip` in the four tests, then drop `animation.rs` from the allowlist (the guard's `stale` arm forces that). Longer term make
  the ratchet per-occurrence (count per file) so allowlisted files cannot grow.
- **Dedup**: Existing #6184 (closed 3e4bfab51; the ratchet is its remedy). This is a regression of the class, not of the ratchet.

##### TD9-2026-10-25-04 - LOW - three assertion-free ignored "tests" are probes in test clothing
- **Dimension**: 9 (`test-gap`) - **Effort**: small
- **Location**: `byroredux/tests/skinning_e2e.rs:877` (`fnv_dump_global_skin_transform`), `:927` (`fnv_vertex_skin_dump_arms1`),
  `crates/nif/tests/sequence_names_census.rs:36` (`object_sequence_names`); also `skinning_e2e.rs:412` (`oblivion_vertex_world_check`,
  deliberately `eprintln!`-only with a stated reason, so not counted but the same shape)
- **What**: bodies print and `unwrap()` only; nothing can fail except a missing file. They are investigation dumps from M41 phase 1b
  and a name census, and `fnv_vertex_skin_dump_arms1`'s comment still reads as an open hunt ("rendering produces a long-ribbon vertex
  artifact"). Precedent: #5857 (VM timing census got a ceiling) and #6185 (skeever probe turned into a pinned test).
  The previous audit's assertion-free scan counted `unwrap()` as "has an assertion" and so never listed them.
- **Fix**: move the three to `examples/` (the example ceilings guard, `example_targets_hold_their_per_crate_ceilings`, applies), or pin one
  measured fact each; delete `fnv_vertex_skin_dump_arms1` if the ribbon artifact is long closed.

##### TD9-2026-10-25-05 - LOW - the only operation-order test for the VATS to-hit chain is a placeholder that fails when enabled
- **Dimension**: 9 (`test-gap`) - **Effort**: medium (blocked on an in-game capture) - **Age**: 077ff45d6, 2026-10-24 (Refs #6350)
- **Location**: `crates/core/src/character/vats.rs:586-622` (`to_hit_chance_matches_recorded_in_game_percentages`)
- **What**: `#[ignore = "ground-truth captures pending..."]`; `GROUND_TRUTH` is an empty slice whose single fixture row is commented out, and the final
  `assert!(!GROUND_TRUTH.is_empty())` makes the test fail the moment the `#[ignore]` is removed. Its own doc says it is "the only test that can catch a wrong
  operation ORDER". `gh issue view 6350` (open) does not list the capture as an acceptance item, so nothing tracks it. It is the sole non-data-gate
  `#[ignore]` added in the window.
- **Fix**: add the capture checklist (the doc's scenario table) to #6350's acceptance list; when the captures exist, put them in the const and remove
  the attribute. Until then keep it, but it should not count as coverage. Deferred: gated on the VATS runtime milestone (#6350).


## Existing (tracked by OPEN issues; not re-filed)

- **#6223** WTHR decoders (TD2-01 of 10-21): worse, see Executive Summary.
- **#6140** `audio/lib.rs` 2086 prod LOC (+52); **#6141** `nif/import/types.rs` 2011 (flat); **#6143** eleven frame/Vulkan-driver fns with no regrowth budget (`build_pipelines_and_finish` 1404 and `build_and_upload_instances` 1322 keep growing; only `draw_frame` is pinned) — TD1-05 proposes adding the two schedule registrars.
- **#6194** the two `loading_screen.rs` TODOs (5 days old, ticketed; both strings must go when it closes).
- **#6276** new surfaces with no reference doc: `BYRO_CACHE_DIR` is now documented, but the loose `Data\` override tier still is not, and 46 of 87 distinct `"BYRO*"` literals appear in no doc. **#6277** `groundcover_bench.rs:1611` `push_bytes` doc + `skyal.md:373` memory slug (unchanged). **#6171 / #6170 / #6169 / #6168** doc-symbol rot (still live). **#6167** past-EOF refs (down 18 → 7). **#6166 / #6165** CLAUDE.md smoke-test list (20 of 40 scripts) and workspace-tree nesting (user-owned, unchanged).
- **#6350** (VATS umbrella): `VatsMode::{MenuMode, ScanMode, Playback}` never constructed and `as_u32` with no caller under `allow(dead_code)` reasons that say "constructed by the 3c lane" although 3c landed without constructing them — reword the reasons now. The ground-truth to-hit test (`vats.rs:586`) is a placeholder that fails if un-ignored and is not on the issue's acceptance list.
- **#5622** bare `allow(clippy::…)`: re-measured 32 sites by the issue's own rule; 5 not on its list (`merge.rs:758`, `graph_nodes.rs:227`, `synth_child.rs:356`, `nif_loader.rs:542`, `pre_parse.rs:692`); 10 `too_many_arguments` allows added since 4a3e89d56 pass only because a `///` parameter doc sits above them.
- **#6278** `metrics_response_mirrors_core_snapshot` guard cited by `DebugResponse::Metrics` does not exist. **#6347 / #6348** two source-scan guards scanning too narrow a region. **#6184** (closed) ratchet is per file — see TD9-03.
- Closed and verified holding: #6271 (exterior split), #6272, #6274, #6275, #6144, #6139, #6148 (residual: TD2-10).

## Deferred (gated on an in-progress milestone)

- VATS ground-truth captures for `to_hit_chance_matches_recorded_in_game_percentages` (TD9-05) and the VATS runtime dead-code reasons: gated on **#6350**.
- The 13 parsed-never-read EDID+FULL record maps (TD6-01): BSGN / HUNG-DEHY-SLPD / RCPE-RCCT / CPTH-CAMS are owed to future character-ruleset, crafting and load-screen milestones (**#6194** for the camera paths); decide per type whether to list or drop.
- Golden-frame baselines (regenerated 2026-10-22; 12 renderer/shader commits since): currency cannot be judged without a GPU — `/audit-runtime` or a local `cargo test --release -p byroredux -- --ignored golden`.
- Pre-2026-06-07 audit reports: 17 of 142 CRITICAL/HIGH blocks have no issue trace (predate `/audit-publish`); of 6 checked against code, 5 are fixed. `RL-01` ("sRGB light colours not linearised", 2026-04-12c) is a **wrong premise** under the project colour-space policy — nobody should act on it. The other 10 were not individually verified.

## Checked, clean

- **Dim 1**: `cell_loader/exterior.rs` 1687 → 210 prod LOC (split in `1d383cc5a`; family 1713 LOC, largest member 635). The 5 `match` expressions with >50 arms are exhaustive enum maps (`ui_input.rs` ×2, `Effect::kind`, `Token` Display, condition function-id dispatch); the 3 `mod.rs`/`lib.rs` with >20 `pub use` are single-job facades.
- **Dim 2**: single-home guards hold — descriptor helpers, `upload_ctx!`, `with_extension_host` (9 callers), `bytes::` (zero `from_le_bytes` in `byroredux/src` production), `paths::`, `submit_initial_layouts`, `coord.rs`, archive access via `asset_provider`, `source_scan` allowlist. `nif/blocks/dispatch.rs` (new) shows no duplicated string arms.
- **Dim 3**: `README` / `CLAUDE.md` / `contributing.md` flags and `--test` names resolve; `docs/engine/index.md` complete (82 docs); `ecs.md` stage list, `architecture.md` "35 members", `debug-cli.md` "80 components", `testing.md` "~106 dispatch tests", `shader-pipeline.md` binding table 0–25, `GpuMaterial` / `GpuLight` / `GpuRayBudget` tables, `Vertex` 104 B and `GpuWaterParams` 368 B all match code.
- **Dim 4**: 81 `/audit-x … Dim N` cross-references and all `Dim N` owner-map rows resolve; 25 "default all N" counts match; ~20 sampled numeric claims within 20%; all guards named in skills exist; `docs/audits/` 90+-day reports: 77 of 80 CRITICAL/HIGH traced to issues, the other 3 resolved in code. `ROADMAP.md` is at its 800-line cap exactly — the next growth breaches it.
- **Dims 5/6/7**: no stale marker; `todo!()` 0; every new Component/Resource/system/installer/console command since the baseline has a production writer and reader; `cxx-bridge` (26 lines) and `platform` (60 lines) placeholders unchanged; NIF version gates have zero bare literals in production; no inline Vulkan limit; no shader `#define`/`const` bypasses `shader_constants_data.rs`.
- **Dim 8**: `allow(dead_code)` — the remainder are justified (documented reserved fields, schema vocabulary, layout mirrors); `#[deprecated]` 0; `// removed:` 0; every `[features]` branch live.
- **Dim 9**: no `workspace_hygiene_tests` guard is `#[ignore]`d; all 58 pin rows name a real ignored fn; every feature has a CI lane; no commented-out assertions or smoke-only tests among the 378 tests added; the 9 closed-HIGH fixes in the window all carry hosted, non-ignored tests.

## Cleanup note

Report files under `/tmp/audit/tech-debt/` and the audit cargo target dir (`/mnt/data/tmp/td-audit-target`) are removed after this report is written. Suggested next step: `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-25.md`.
