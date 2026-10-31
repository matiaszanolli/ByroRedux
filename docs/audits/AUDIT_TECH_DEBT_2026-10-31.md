# Tech-Debt Audit — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` (HEAD fa6be4a6b; 136 commits since) · **Audited**: Dims 1, 2, 3, 7, 8 (delta-scoped to fa6be4a6b..HEAD) · **Unchanged since baseline (skimmed)**: Dims 4, 5, 6, 9 (recipes re-run, no new instances)

Part of `/audit-suite --preset comprehensive`: run synchronously with no sub-agents. Builds used the repo `target/` only (`CARGO_INCREMENTAL=0`). Nothing was committed, filed or edited outside this report and `/tmp/audit/tech-debt/`. A source-only copy of the baseline tree was unpacked under `/mnt/data/tmp/td_base` to diff function sizes and has since been removed.

In one sentence: the window was the M43 close-out (mines, attraction, crafting, lockpicking, random conversations, improving) plus the #6355-#6359 file splits, and the debt is what both leave behind: stale references to files the splits deleted, one hand-rolled hierarchy walk, a file that crossed 2000 LOC, and status prose that M43 overtook. All eight 10-27 findings are still present and already filed (#6520-#6527).

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 8 |
| **Total (new)** | **9** |

By dimension: Dim 1 ×1 · Dim 2 ×2 · Dim 3 ×4 · Dim 7 ×1 · Dim 8 ×1 · Dims 4, 5, 6, 9 ×0.

- **Gates**
  - `cargo clippy --workspace --keep-going -- -D warnings` is **red**: 3 errors, all in `byroredux-scripting` (`vm/natives/ownership_crime.rs:108`, `jail.rs:305`, `jail.rs:389`). That is **REG-2026-10-31-02** (regression of #6222), not re-reported. Because `byroredux-scripting` fails to compile under clippy, its dependents (the bin, `save`, `mod-runtime`) were **not linted** in this run, so the bin may hide more.
  - `cargo machete --with-metadata` is clean.
  - `workspace_hygiene_tests`: 26 of 27 pass. The one failure is `readme_and_roadmap_hold_their_line_ceilings` (ROADMAP.md 808 lines vs 800). That is **REG-2026-10-31-03**, not re-reported.
  - `_audit-validate.sh`: **OK** (3885 refs across 126 skill files). Advisories: 42 deleted-file basenames, 194 unmatched symbols in `docs/engine`, 10 closed-issue citations (the last is #6525's known noise).
  - The removed-symbol sweep (662 names removed in the window) found one survivor, `cmd_blas_refit_end`, already reported as REN-D12-01.
- **The 10-27 backlog is unmoved.** #6520 (mesh.rs 1864), #6521 (SceneBuffers families; its symptom is open #6506), #6522, #6523, #6524, #6525, #6526 and #6527 all still reproduce at HEAD. The window's fixes went to the 10-27 *sibling* audits' findings, not to these.
- **Cross-audit dedup.** Cited, not re-filed: ESM-2026-10-31-04 / EXT-D1-2026-10-31-02 (two displaced doc blocks), GAME-D7-2026-10-31-02 (displaced doc block), TOOL-D1-2026-10-31-02 (`crafting_menu_visible` has no caller; see TD8-01), ECS-2026-10-31-01 / REG-2026-10-31-01 (crafting lock order), REG-2026-10-31-04, PERF-2026-10-31-D7-01. TD2-01 cross-refers to GAME-D4-2026-10-31-01 (mines ignore the enable ledger).

### Delta vs the 2026-10-27 baseline

| Metric | 10-27 (fa6be4a6b) | 10-31 (670f81917) | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 26 | 38 | +12 are all the FO4 `HACK` terminal FourCC (M43 `lockpick.rs`); no real marker added; 0 shader hits |
| `allow(dead_code)` | 39 | 39 | the two new-looking `#[allow(dead_code)] // #4464` lines are the #6356 split moving `loot.rs` (see TD8-01) |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` tests | 373 | 382 | +9, all data-gated (FNV, FO3, Skyrim, Oblivion, FO4 masters); real-data pins file +3 rows (#6459, #6460) |
| production files >2000 LOC | 2 | 2 | `shader_constants_data.rs` 3169 (data table, +19) and **`crates/debug-ui/src/panels.rs` 2086** (new entry, TD1-01) |
| production fns >200 / >300 / >500 / >1000 | 206 / 79 / 25 / 0 | 207 / 78 / 24 / 0 | scanner re-run on the unpacked baseline reproduces 206/79/25/0; no function newly crossed 300 |
| test-heavy files >2000 total LOC | 93 | 91 | |
| blocking clippy gate | green | **red** (REG-02) | |
| `cargo machete` | clean | clean | |

Functions over 200 lines that are new by name in the window (`build_npc_equip_state` 330, `register_player_and_animation` 279, `register_interaction_and_crime` 220) are extraction products of #6357 and #6359. `step_streaming_session` (368) is the old `step_streaming` body renamed by #6114 and did not grow. The largest real growth is `build_world` 505 → 520.

## Baseline Snapshot (Phase 1, measured at 670f81917)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 38
allow(dead_code):      39
unimplemented!/todo!(): 0
#[ignore] tests:        382
files >2000 production LOC: 2
test files >2000 total LOC (lower priority, separate bucket): 91
```

`prod_loc_self_test`: ok. These figures equal the audit skill's own 2026-10-31 snapshot to the digit.

Watch list (prod LOC, 10-27 → 10-31):

| File | 10-27 | 10-31 | Note |
|---|---:|---:|---|
| `crates/debug-ui/src/panels.rs` | 1938 | **2086** | TD1-01 |
| `crates/plugin/src/esm/records/misc/world.rs` | 1870 | 1910 | 95.5% of the line |
| `crates/renderer/src/mesh.rs` | 1864 | 1864 | #6520 |
| `byroredux/src/asset_provider/material/merge.rs` | 1860 | 1860 | |
| `crates/nif/src/import/material/mod.rs` | 1857 | 1857 | |
| `byroredux/src/app_events.rs` | 1833 | 1833 | |
| `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` | 1818 | 1820 | |
| `byroredux/src/systems/character.rs` | n/a | 1809 | |

`gpu_timers.rs` fell 1929 → 1703 (#6358) and `inventory.rs`, `npc_spawn.rs` and `obscript_effects.rs` left the list through real splits. The largest children are `npc_spawn/resumable/runtime.rs` 1462, `fragment/vm/interp.rs` 1416 and `fragment/vm/checker.rs` 1326.

Young-crate check: `cxx-bridge` (26 + 17 C++ lines, one call `native_hello()`) and `platform` (60 lines, window only) are unchanged and have not grown a second job. No crate or tool manifest was added in the window.

## Top Quick Wins (trivial / small)

1. **TD3-01** (small): replace the ~25 live-tense `npc_spawn.rs` / `schedule/update.rs` / `obscript_effects.rs` references with symbol + directory, repair the mangled `npc_spawn/:671` and the `scripting.md:143` link, then make `stale_line_references.rs` fail on a zero-match basename (this also closes #6523's half of the problem).
2. **TD2-01** (trivial): `hide()` in `mines.rs` → `byroredux_core::ecs::collect_subtree`.
3. **TD3-03** (trivial): `feature-matrix.md:217` and the `anim_clips.rs:308` comment.
4. **TD8-01** (trivial): drop `LootOutcome.stolen` or have a production reader; fix the stale "theft consequences deferred" comment.
5. **TD3-02** (small): add the seven M43 systems to `game-loop.md` and widen the #6378 pin from three named families to every `add_exclusive`/`add_parallel` target.
6. **TD7-01** (small): name the nine sub-record length gates, on the #6393 pattern.
7. Carry-overs from 10-27 that are still trivial/small: #6524, #6527, #6525, #6522.

## Top Medium Investments

1. **TD1-01**: split `crates/debug-ui/src/panels.rs` before the next game-menu page (barter and crafting each added about 100 lines) lands.
2. **TD2-02**: one `actor_entities(world)` and one pass-clock helper in `systems/`, then migrate the six and five sites.
3. **TD3-04**: a stronger splice detector (the current guard is deliberately narrow and four splices got past it in four days).
4. Carry-overs: #6520 (mesh.rs split), #6521 (SceneBuffers family list), #6371 (compute-pass teardown).

## Findings

### MEDIUM

#### TD2-2026-10-31-01: `mines.rs::hide` hand-rolls a whole-subtree walk with no visited set or budget, the exact shape #5513 replaced with `collect_subtree`
- **Severity**: MEDIUM (duplicated logic where one copy carries the bug-fix history and the other regressed) · **Dimension**: 2 · **Effort**: trivial · **Status**: NEW
- **Location**: `byroredux/src/systems/mines.rs:141-155` (`hide`), called at `:321` from `detonate`.
- **Age**: c36ab4125 (2026-10-30, "placed proximity mines trip, burn a fuse and blast")
- **Description**: `hide` collects a mine's subtree with a bare stack:
  ```rust
  let mut stack = vec![entity];
  while let Some(next) = stack.pop() { all.push(next); if let Some(children) = world.get::<Children>(next) { stack.extend(children.0.iter().copied()); } }
  ```
  This is what #5513 retired everywhere else. `byroredux_core::ecs::collect_subtree` (`crates/core/src/ecs/hierarchy.rs:50`) carries a visited set and a `HierarchyTraversalGuard` budget because the unguarded walks hung streaming adoption and loading-cover steps on a cyclic `Children` graph. Its own test pins the history: "the hand-rolled walks this collector replaced pushed every child id onto the stack unconditionally". Every other whole-subtree walk in the bin cites it (`loading_screen.rs:580`, `cinematic.rs:750,907`, `cell_loader/unload.rs:19`, `npc_spawn/mod.rs:101`), and the two BFS walks in `ragdoll.rs` carry their own #4572 set and budget. `hide` is the only unguarded one, and it runs in `Stage::Update` on a save-loaded hierarchy.
  - No hygiene scan polices this class, which is why it passed: the guards cover `init_resource`, `PlayerEntity::get`, the pool-to-local walk and the Z-up swap, not `stack.extend(children…)`.
- **Suggested Fix**: `let all = byroredux_core::ecs::collect_subtree(world, entity);` and delete the loop. Add a `workspace_hygiene_tests` scan that fails `stack.extend(children` / `queue.extend(children` outside `hierarchy.rs` and a short allowlist (the two guarded `ragdoll.rs` walks). Do this together with GAME-D4-2026-10-31-01, which also touches the same function's spent-mine handling.

### LOW — Dim 1: Complexity

#### TD1-2026-10-31-01: `crates/debug-ui/src/panels.rs` crossed 2000 production LOC (1938 → 2086) on four M43 commits
- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium · **Status**: NEW (no filed issue; the audit skill's Phase-1 note names it)
- **Location**: `crates/debug-ui/src/panels.rs` (2086 prod LOC; 2702 total with tests)
- **Age**: 041d57333, 851636a13, d467b074c, c2742a901 (2026-10-30: dialogue speaker name, crafting page, drop and cast panels, dialogue speaking cue)
- **Description**: The file holds four jobs. The split axis falls out of the internal structure rather than from names:
  - **Snapshot and action types** (`:27-245`, 15 structs/enums): `PanelSnapshot`, `BarterSnapshot`, `CraftingSnapshot`, `InventorySnapshot`, `DialogueTopicSnapshot`, and the matching `*Action` enums.
  - **In-game HUD** (`:246-430`): `draw_hud`, vitals bars, objective lines, loading tip, player message.
  - **Native game menu** (`:430-1210`, about 780 lines): `draw_game_menu` and the Crafting, Barter, MessageBox, Dialogue, Pause, Settings and Inventory pages. This block takes one new page per M43 feature and is the growth edge.
  - **Developer studio** (`:1212-2086`, about 870 lines): `draw`, `draw_studio`, `draw_gallery`, `draw_metrics`, `draw_loader`, `draw_entities`, `draw_console`, `draw_settings`, GPU-pass helpers.
  - The first two blocks are what the shipped game draws every frame. The last is the developer overlay. They share only `PanelOutputs` and `PanelState`.
- **Suggested Fix**: `panels/{snapshots,hud,game_menu,studio}.rs` with `panels.rs` (or `panels/mod.rs`) re-exporting the existing public names so `lib.rs` and the bin's `byroredux_debug_ui::…` paths do not change. Move the existing `mod tests` cases with their code. Do it before the next menu page; `game_menu` alone would be about 780 lines and `studio` about 870.

### LOW — Dim 2: Duplication

#### TD2-2026-10-31-02: The "every live actor" roster is hand-collected at six sites and the "accumulate dt, run every N seconds" clock at five
- **Severity**: LOW · **Dimension**: 2 · **Effort**: small · **Status**: NEW
- **Location**:
  - **Roster** (`world.query::<CombatDisposition>().map(|q| q.iter().map(|(e, _)| e).collect()).unwrap_or_default()`): `byroredux/src/crime.rs:75`, `:164`, `:777`; `byroredux/src/systems/dead_body.rs:72`; `byroredux/src/systems/mines.rs:131` (`live_actors`); `byroredux/src/systems/faction_hostility.rs:424`.
  - **Pass clock**: `dead_body.rs:55`, `attraction.rs:164` (`AttractionState.elapsed`), `mines.rs:214` (`MineClock`), `faction_hostility.rs:393` (`scratch.elapsed`), `crime.rs:749` (guard-rush `elapsed`).
- **Age**: dead_body.rs and mines.rs landed 2026-10-30 (b2049af06, c36ab4125); the other four predate the window.
- **Description**: Each system re-spells the same two pieces of scaffolding. Every spelling also re-decides the details independently:
  - whether the player is added (`mines.rs` adds `PlayerEntity::get`, `crime.rs:witnesses` excludes it);
  - whether `Dead` is filtered, and in which order against the transform lookup;
  - whether `dt` is guarded with `is_finite()` (`faction_hostility.rs` and `crime.rs` do, `dead_body.rs`, `attraction.rs` and `mines.rs` rely on `dt.max(0.0)` alone; that is safe for NaN, since `f32::max` returns the non-NaN operand, but an infinite `dt` passes straight through);
  - whether the clock resets to `0.0` or carries the remainder (`attraction.rs` takes the elapsed value, `dead_body.rs` zeroes).
- **Suggested Fix**: put `pub(crate) fn actor_entities(world: &World) -> Vec<EntityId>` beside `CombatDisposition` in `faction_hostility.rs` and a small `PassClock { period, elapsed }` with `fn due(&mut self, dt) -> Option<f32>` in `systems/`. Migrate the eleven sites. The non-finite-`dt` handling then has one home.

### LOW — Dim 3: Stale documentation

#### TD3-2026-10-31-01: The #6355-#6359 splits left about 25 live-tense references to deleted files, 6 of them with line numbers, one broken link and one mangled path
- **Severity**: LOW · **Dimension**: 3 · **Effort**: small · **Status**: NEW (the same blind spot as open #6523, now on a second batch of splits)
- **Location**: `git grep` over `crates byroredux tools docs/engine` (29 occurrences in 20 files for `npc_spawn.rs`; 6 are the self-referential history in `npc_spawn/mod.rs:70` and `tests.rs`, which are fine).
  - **`npc_spawn.rs`** (deleted by 613c7f46e, #6357), live-tense: `crates/core/src/character/regen.rs:127`, `character/mod.rs:70`, `components/{sandbox.rs:32,wander.rs:40,78,travel.rs:48}`, `ecs/systems.rs:616,648`, `plugin/src/equip.rs:65`, `plugin/src/esm/records/actor/npc.rs:361,590`, `plugin/examples/pack_ambient_shape_survey.rs:3`, `byroredux/src/systems/{wander.rs:23,98,sandbox.rs:65,locomotion.rs:59}`, `scene/nif_loader.rs:705`, `cell_loader/references/attach.rs:256`, `inventory/catalog.rs:294`, `asset_provider/tests/material_path.rs:328`, and `docs/engine/npc-spawn-ai-packages.md:110,162,170,224,241,272`.
  - **With line numbers into the deleted file**: `core/src/ecs/systems.rs:616` (`npc_spawn.rs:319-323`), `plugin/.../npc.rs:361` (`:619`), `byroredux/tests/m41_phase1bx_skinning.rs:22,287` (`:402-431`), `npc-spawn-ai-packages.md:110` (`:1549`) and `:170` (`:1433-1479`).
  - **Mangled by a mechanical replace**: `npc-spawn-ai-packages.md:108` reads `` `byroredux/src/npc_spawn/:671` `` (a directory with a stale line number).
  - **`boot/schedule/update.rs`** (deleted by 4ab5766e3, #6359): `crates/core/src/combat.rs:15`, `crates/core/src/stealth.rs:39`, and the label in `byroredux/src/boot/mod.rs:600` (the label is only prose, the witness still matches, but it names a path that does not exist).
  - **`obscript_effects.rs`** (deleted by aa29774ec, #6355): `docs/engine/scripting.md:2475`.
  - **`inventory.rs`** (deleted by 258c6d9b2, #6356): `docs/engine/scripting.md:143` is a link whose text says `byroredux/src/inventory/` and whose target is still `../../byroredux/src/inventory.rs` (a dead link), plus `docs/engine/charal.md:466` and `charal-fo4-ruleset.md:106` (`inventory.rs::…`).
- **Description**: `_audit-validate.sh` does flag the bare basenames (they are among its 42 advisories, which is why they could be counted), but the advisory is "not a failure", and `crates/core/tests/stale_line_references.rs` skips any `file.rs:LINE` whose basename resolves to no file (`let Some([target]) = … else { continue }`). A reference into a *deleted* file is the one case where the line number is meaningless, and it is exactly the case the gate cannot see (the open #6523 describes the first batch of 10).
- **Suggested Fix**: replace each with the symbol and its directory (`npc_spawn/stamps.rs`, `npc_spawn/equip.rs`, `boot/schedule/update/`). Fix the `scripting.md:143` link target. Then fold #6523's gate change in: make `stale_line_references.rs` fail on a zero-match basename unless the reference has an upstream-crate prefix, and have `_audit-validate.sh` promote a deleted basename that appears with `:LINE` from advisory to failure.

#### TD3-2026-10-31-02: `game-loop.md`'s live-schedule table omits seven M43 systems, and the #6378 pin only names the three families it was fixed for
- **Severity**: LOW · **Dimension**: 3 · **Effort**: small · **Status**: NEW (recurrence of the TD3-2026-10-25-07 / #6378 class)
- **Location**: `docs/engine/game-loop.md:144-159` (the table); registrations in `byroredux/src/boot/schedule/update/quest_scene.rs:13-27`; pin `byroredux/src/workspace_hygiene_tests.rs:1740` (`the_live_schedule_table_names_the_vats_package_and_crime_systems`).
- **Age**: 2026-10-30 (mine_system c36ab4125, attraction ccccbaf03, dead_body b2049af06, random conversations 79e095131/b0591ba88, iron sights 9e17ebce1, dropped-item d467b074c)
- **Description**: Registered `Stage::Update` exclusives that `game-loop.md` never names: `mine_system`, `attraction_system`, `dead_body_discovery_system`, `iron_sights_system`, `random_conversation_system`, `make_conversation_hold_system`, `dropped_item_story_event_system`. The doc is the reference an auditor reads for stage order. #6378 pinned VATS, package and crime systems *by name*, so the next family (M43 producers) slipped straight through. Same pattern as before, just a different list.
- **Suggested Fix**: add one table row for the story-producer block (they share a stage and a purpose). Replace the by-name pin with a derived one: every function identifier passed to `scheduler.add_exclusive` / `add_parallel` in `boot/schedule/update/*.rs` must appear in `game-loop.md` or on a short documented exemption list.

#### TD3-2026-10-31-03: Two status statements that M43 and the hkx work overtook
- **Severity**: LOW · **Dimension**: 3 · **Effort**: trivial · **Status**: NEW
- **Location**:
  - `docs/feature-matrix.md:217`: "…KILL/CLOC/AHEL producers, `sm1-story-manager.sh`); SCPT/LEVL/SKIL producers remain". ROADMAP.md's M43 row (and `story-manager.md`) record `SCPT`, `LEVL`, `SKIL`, `ASSU`, `AIPL`, `JAIL`, `CHRR`, `REMP`, `CAST`, `CRFT` and `LCLD` as shipped. `feature-matrix.md` is the status floor, so a row that contradicts shipped code is exactly what the audit hunts. It was last written 2026-10-15 (ce130a7cf).
  - `byroredux/src/npc_spawn/anim_clips.rs:308`: "Animating SSE+ actors lands once a `.hkx` parser stub is wired — folded into M41.1 follow-up". The comment dates from 2026-04-28 (b16b6dbb6, moved by #6357). `byroredux-hkx` reads Skyrim and FO4 skeletons, clips and FO4 behavior graphs (`asset_provider/object_graph.rs` consumes them).
- **Suggested Fix**: rewrite the matrix row from ROADMAP's M43 list; reword the doc comment to say the function is the `.kf` path only and that Havok actors go through `object_graph.rs`.

#### TD3-2026-10-31-04: The spliced-doc-block guard stays green while spliced blocks keep landing
- **Severity**: LOW · **Dimension**: 3 · **Effort**: medium · **Status**: NEW (systemic cause behind several sibling-audit findings)
- **Location**: guard `no_doc_block_is_spliced_onto_the_wrong_item` (`byroredux/src/workspace_hygiene_tests.rs`, `byroredux_core::source_scan::spliced_doc_blocks`). Sites found this window, all reported by their own audits: `crates/plugin/src/esm/records/misc/pack.rs:336` and `byroredux/src/cell_loader/load_order.rs:1957` (ESM-2026-10-31-04), `byroredux/src/systems/story_events.rs:370-381` (GAME-D7-2026-10-31-02, where `raise_actor_attach_story_event` sits between the `#5434 — KILL` doc and `raise_kill_story_event`); plus open #6529 from 10-27.
- **Description**: The guard fires only when a doc's *first backticked name* is a sibling declared just below. Each of these four splices opens with an issue number or a record mnemonic (`#5434 — KILL`, `FO3/FNV package procedure-type index`) rather than a function name, so the guard reports green on a tree containing at least four instances. I tried the obvious widening, "a doc paragraph that opens `#NNNN —` or `M<n> —` directly under a non-blank doc line", and it matched 73 sites in `crates/` and `byroredux/`, almost all ordinary prose, so a cheap text rule will not do.
- **Suggested Fix**: add a second signal. A displaced block's doc *describes a different function than the one it precedes*, so test the doc's mnemonic or backticked identifiers against the signature and body of the item below it: the `KILL` doc names a `KILL` mnemonic that the OAAT function does not contain, while the real `raise_kill_story_event` body does. That keeps the false-positive rate down. Run it over the five known sites as the self-test.

### LOW — Dim 7: Magic numbers

#### TD7-2026-10-31-01: Nine sub-record length gates and skip offsets added in the window are unnamed (the #6393 class re-accrued)
- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Status**: NEW
- **Location**:
  - `crates/plugin/src/esm/records/misc/effects.rs:236` (FO4 `PROJ` `DNAM`, `>= 76`, then `skip_or_eof(8/12/16)` between fields), `:447` (`EXPL` `DATA` `>= 40` with `skip_or_eof(28)`, and the older `>= 16` arm just below);
  - `misc/equipment.rs:375` (`COBJ` `CNTO` `>= 8`), `:384` (`NAM1` `>= 2`); `misc/quest.rs:1167` (`CNTO` `>= 8`);
  - `misc/world.rs:1304` (`KYWD` `AOR2` `>= 14`); `misc/pack.rs:1145` (`>= 10`, `data[4]`);
  - `weather.rs:1255,1258` (Skyrim `NAM2`/`NAM3` `>= 16`).
- **Age**: 2026-10-28..30 (3bec306a0, e97707b14, 977936719, 2c55546f4 and kin)
- **Description**: #6393 (64f26db8c) named the CRVA, PACK, ACBS and Oblivion-CTDA gates, and the surrounding files now carry `PKCU_MIN_LEN`, `PKW3_MIN_LEN`, `BPND_LEGACY_LEN`, `CHAL_DATA_LEN`, `SKYRIM_DALC_SIZE`. The new gates in the same files do not. The `PROJ`/`EXPL` ones are also the layouts the ESM audit flags (ESM-2026-10-31-01/-02: FO4 EXPL read at the v97 layout, FO76 offsets wrong), so a named per-layout constant is the shape that fix needs anyway.
- **Suggested Fix**: add `const PROJ_FO4_DNAM_LEN: usize = 76;`, `EXPL_SKYRIM_FO4_DATA_LEN`, `COBJ_CNTO_LEN` and so on beside each parser. Name the `skip_or_eof` offsets in the `PROJ`/`EXPL` arms as field-offset constants (or read by offset) so the layout comment and code cannot drift.

### LOW — Dim 8: Dead code

#### TD8-2026-10-31-01: `LootOutcome.stolen` is read only by tests, behind an `allow(dead_code)` whose stated reason has been false since 2026-10-24
- **Severity**: LOW · **Dimension**: 8 · **Effort**: trivial · **Status**: NEW
- **Location**: `byroredux/src/inventory/loot.rs:144-150` (the field and its comment); live consumers at `:177-182`, `:270-281`, `:472-473`, `:529-530`.
- **Age**: 4cfcee4b9 (2026-10-24, crime faction system; `report_theft`), moved here by 258c6d9b2 (#6356)
- **Description**: The field says "read nowhere yet … the witness/bounty consumer the P3 spec defers is what will read it" and carries `#[allow(dead_code)] // #4464 — forward-latent until theft consequences`. Theft consequences are implemented: `transfer_loot` computes `stolen_from = theft_owner(..)` and calls `crate::crime::report_theft(world, player, owner, &outcome.rows)`, and `stolen` is a local that feeds the notification and the `AIPL` story event. No production code reads `outcome.stolen`; only `inventory/tests.rs:986,1063,1176` do. The sibling `LootSelection::Stack` is genuinely latent (a documented selective-take UI) and is fine.
- **Related**: `crafting_menu_visible` (`crates/debug-ui/src/lib.rs:322`) also has zero callers (not even tests). That is TOOL-D1-2026-10-31-02's root cause (`publish_ui_menus` never asks for it), so it is reported there and is fixed by calling it, not by deleting it.
- **Suggested Fix**: delete the field and have the three tests assert on `report_theft`'s effect (or on the notification), or keep it, remove the `allow` and the "deferred" comment, and read it where the notification is built.

## Dimensions with no new findings

- **Dim 4 (audit-finding rot)**: the audit skills were re-synced in 670f81917 and every number this audit samples reproduces exactly (the audit-tech-debt snapshot 38 / 39 / 0 / 382 / 2 / 91, the watch-list LOC, the npc_spawn / inventory / vm child sizes). `_audit-validate.sh` path references are all valid. The closed-issue advisory shows 10 citations (11 on 10-27), all the known noise class in open #6525.
- **Dim 5 (markers)**: 38 hits, the composition recorded in the audit skill's 2026-10-31 snapshot (18 `XXXX`, 16 `HACK` terminal FourCC, 3 quoted upstream FIXMEs, 1 documented `TBD`). The added-line grep over the window is empty for real markers and shaders have none. The `triangle.frag` MIT/Burley attribution block is intact.
- **Dim 6 (stubs)**: 0 `unimplemented!` / `todo!()`. The only `panic!("not ` is test-only (`fragment/vm/tests.rs:5001`). A sweep of 181 new `pub`/`pub(crate)` fns for production callers found every one wired; the zero-caller exception is `crafting_menu_visible` (TD8-01's related note). All new `install_*` book builders and systems are registered.
- **Dim 9 (test hygiene)**:
  - **Ignores**: 382 (+9), every new one gated on installed game data. The only non-data reasons are #6402 and #6403, still open from 10-25.
  - **Guards**: the 26 passing `workspace_hygiene_tests` include the ignore-reason, stacked-attribute, pins-lane and example-ceiling guards; the one failure is REG-03.
  - **Pins**: `.github/real-data-pins.txt` gained 3 rows (#6459, #6460). The window's other closed-HIGH fixes (#6479, #6471) are guarded by plain tests.
  - **Assertions**: a scan of every `#[test]` added or modified in the window found none without an assertion.
  - **Lanes**: no `Cargo.toml` feature or CI-lane change in the window.

## Deferred

- **TD1-01** (panels.rs split) is gated on nothing, but should land after the crafting-page lock-order fix (ECS-2026-10-31-01 touches `crafting.rs`, not `panels.rs`) and before the next native-menu page.
- **TD2-01** and GAME-D4-2026-10-31-01 should be one commit, since both rewrite `mine_system`'s spent-mine handling.
- The ROADMAP.md 808-line breach (REG-2026-10-31-03) is not re-reported. The M43 row at ROADMAP.md:282 is one table cell of several thousand characters, so trimming it is the cheapest way back under the ceiling without raising the cap.

---
Suggested next step: `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-31.md`
