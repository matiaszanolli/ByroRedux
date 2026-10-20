# Tech-Debt Audit — 2026-10-19

**HEAD** 12677d718 at audit start (Dim 7+9 ran at fc4a15104 and HEAD reached 28e1f413e before the merge; both later commits are FO4 ragdoll/XRGD work, so no finding below is expected to move) · **Prior report** `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (HEAD f4ae42a6f, 338 commits / 626 files / +73.8k lines earlier) · **Depth** deep · **Dimensions** 1–9, one agent each (5+6 and 7+9 paired), all read-only.

The one-sentence story: **the 10-15 report's whole backlog was worked off in four days** — every prior HIGH/MEDIUM is fixed (the Papyrus-VM unbounded-allocation HIGH #5791, the seven duplication MEDIUMs #5792–#5798, the red clippy gate), the five scripting/save/cell monoliths were genuinely split, and the blocking clippy gate and `cargo machete` are green. What is left is (a) the M47.3 ObScript wave re-creating the same shapes one layer over, (b) **audit-infrastructure rot that accumulated after the last `/audit-sync`** (three MEDIUMs, all fixed by one re-sync), and (c) one real tooling gap: **the lanes added to run the `#[ignore]`d tests cannot execute because the repo has zero registered runners.**

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 8 |
| LOW | 49 |
| **Total** | **57** |

By dimension: Dim 1 ×9 · Dim 2 ×15 · Dim 3 ×12 · Dim 4 ×6 · Dim 5 ×1 · Dim 6 ×2 · Dim 7 ×2 · Dim 8 ×5 · Dim 9 ×5. (Two cross-dimension overlaps kept in the reporting dimension: `TD7-02` is also Dim 2 duplication; `TD9-03` is also a Dim 7 hardcoded path. `TD4-2026-10-19-05`'s "Gamebryo 2.3 source drive not mounted" note is wrong — see TD3-12, the tree exists at `/run/media/matias/Respaldo 2TB/…`, only the documented `/media/…` prefix is stale.)

- **MEDIUM (8):**
  - `TD2-01` — eight `initialize_layouts` bodies / sixteen callers in the renderer; #6063 (landed today) fixed the in-flight-destroy premise for the shadow filter only, and **five sibling callers still destroy images a submitted command buffer may touch** (`init.rs:848,1223,1320,1526`, `resize.rs:1674`). Duplication with divergent bug-fix history; the bug itself belongs to the renderer/concurrency audits.
  - `TD3-01` — `docs/feature-matrix.md` contradicts shipped code and itself in five rows (demote to LOW if no audit was misled).
  - `TD4-01/-02/-03` — post-sync audit-skill rot: ~25 "known-open / do not re-file" callouts name now-CLOSED issues; five split hub files still carry old symbol attributions and path-scoped `git log`/`grep` commands that silently return nothing; `_audit-owners.md` routes the split `fragment/vm/` tree to `scripting` alone at MEDIUM instead of HIGH with concurrency/performance owners. **One `/audit-sync` fixes all three.**
  - `TD9-01/-02/-03` — `gh api …/actions/runners` = 0: the `closed-high-pins` lane (#5799) and the nightly golden (#5854) target runners that do not exist, so all 344 `#[ignore]`s still run only by hand; the 50-row pins array has drifted (two rows name the wrong lane, three closed-HIGH pins are missing); ~35 real-data test files re-inline `/mnt/data/SteamLibrary` and skip silently, including the #5489 and #5005 pins.
- **Gates:** blocking clippy `--workspace --keep-going -D warnings` **green** (all 34 packages linted), renderer `undocumented_unsafe_blocks` re-lint green, `cargo machete` (+ `--with-metadata`) clean. Non-blocking `--all-targets` re-accrued 20 lints (`TD8-05`).
- **Closed since 10-15 (verified in code, not just by issue state):** every prior TD1 (8 of 10; one partial, one live), TD2-01..07 + 11 of the 16 LOWs, TD3 (11 of 13), TD4 (5/5), TD5/6 (3/3), TD7 (6/6), TD8 (5 of 7), TD9 (3 of 5 effective).
- **Still live from 10-15:** #5802 (hub registration lists; `register_update_systems` 763 → 852), the unfinished half of closed #5803 (`apply_effect` 578, `evaluate_guard` 313 — closed issue + live problem), the five dead `populate_*`/`watr_to_params`-class items (#5745 closed without them), ROADMAP tripwire (`TD3-03`), the weak-`#[cfg(test)]`-cut guard blind spot (`TD2-04`, regression of #5824).
- **Process note:** the Dim 4 agent ran one `cargo test --workspace -- --list` against the repo's default `./target` before noticing the pinned-target rule; no repo file was touched.

### Delta vs the 2026-10-15 baseline

| Metric | 10-15 | 10-19 | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 24 | 23 | only 2 real TODOs (`loading_screen.rs:10,46`); 15 `XXXX` + 2 FO4 `HACK` + 3 upstream FIXME quotes + 1 documented `TBD`; 0 shader hits; `triangle.frag` MIT block intact (L24–40) |
| `allow(dead_code)` | 41 | 41 | every production site has a live reason; 12 are `cfg(test)` markers |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` tests | 322 | 343 (344 by Dim 9's count) | +21, all data/GPU gates; none runs on a hosted runner (`TD9-01`) |
| production files >2000 LOC | 6 | 4 | `vm.rs`, `translate/effects.rs`, `save_io.rs`, `esm/cell/mod.rs`, `condition.rs` left via real splits (#5800/#5801/#5804/#5805/#5585; largest child 1538); `mq101_conformance.rs` 2157, `audio/lib.rs` 2034, `nif/import/types.rs` 2001 crossed; `shader_constants_data.rs` 2868 is a data table |
| production fns >200 LOC | 199 | 210 (same scanner: 204 → 210) | `apply_at_depth` 621 → 127, `object_read` 1259 → 72; new: six M47.3 ObScript `match cmd` functions |
| fns >500 / >1000 LOC | 31 / 7 | 29 / 5 | |
| test-heavy files >2000 total LOC | 84 | 86 | none escalates |
| blocking clippy gate | red | **green** | |

## Baseline Snapshot (Phase 1, measured at 12677d718)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 23
allow(dead_code):      41
unimplemented!/todo!(): 0
#[ignore] tests:        343
files >2000 production LOC: 4
test files >2000 total LOC (lower priority, separate bucket): 86
```

## Top 10 Quick Wins (trivial / small)

1. **Run `/audit-sync`** — fixes `TD4-01`, `-02`, `-03`, `-04`, `-06` in one pass (closed-issue callouts, split-hub attributions and pathspecs, owner-map row, this skill's own baseline paragraph, stale LOC figures).
2. **`TD2-01` (small)** — one `initialize_layouts_once` helper + one `destroy_unless_in_flight(pass, err)` replacing the sixteen caller blocks; closes the five sibling callers still carrying the premise #6063 disproved. Pin with a source-scan test modelled on `shadow_filter.rs:485`.
3. **`TD9-02` (trivial)** — relabel two pins to `nif-corpus`, add the three missing closed-HIGH pins (#5489, #5005, #5238), delete the dead `schedule` clause, and add a hosted-runnable hygiene test that every pin title is an `#[ignore]`d `fn` in its lane's target.
4. **`TD9-01` (small decision)** — move the pin list to `.github/real-data-pins.txt` + `scripts/run-real-data-pins.sh`; drop `schedule:` from `rt-correctness.yml` (3 cancelled / 1 queued runs, queue growing); regenerate `cube_demo_60f.png` (last 09-15, 121 shader commits since).
5. **`TD3-02` / `TD3-01` (trivial/small)** — ROADMAP: delete the two fixed Known-Issues checkboxes (#5765 listed as open HIGH, FO76 GeneratedMeshes); `docs/feature-matrix.md`: delete two gap rows, fix the #4747 mis-citation, re-date the heading.
6. **`TD8-01` (trivial)** — delete the five zero-caller public items (`watr_to_params`, `QuestGates::conditions_of`, `started_quests`, `populate_scene_fragments_from_pex`, `PackageFragmentSet`) and their three re-export names.
7. **`TD7-01` (trivial)** — `saturating_add` on `crime_gold` and `disposition_mods` (the sibling `MiscStats` already does); mod-supplied bytecode can overflow them (panic in dev, wrap negative in release).
8. **`TD6-01` (trivial)** — insert/remove `PlayingScriptedIdle` next to `IdleResume` in `systems/cinematic.rs`; today ObScript `IsIdlePlaying` answers 0 always, under a "Shipped" M47.3 row.
9. **`TD2-04` (small)** — add the two newline-prefixed weak needles to `no_source_scan_cuts_with_the_weak_cfg_test_needle`, then migrate the 9 weak and ~11 inline-strong sites to `source_scan::production_text*`.
10. **`TD3-09/-10/-11/-08` + `TD3-04/-05/-06/-12` (trivial, doc-only)** — stale `npc-spawn-ai-packages.md` selectors (#3042), `scripting.md:1593` "no running actor graph", `exterior-readiness-plan.md` `CellRootRefIndex`, renamed symbols, `architecture.md` "27 members" → 34. **CLAUDE.md is user-owned**, so `TD3-05` (three mis-nested tree groups under `spt/`, `hkx/`, `texture-upscale/`), `TD3-06` (smoke list names 19 of 35 scripts) and `TD3-12` (Gamebryo path `/media/…` → `/run/media/…`) need the user's own edit.

## Top 5 Medium Investments

1. **`TD1-01` — ObScript command table** (medium): six chained `match cmd` functions (~270 arms, 150–780 lines each, falling through `_ => next::run`) become one `static COMMANDS` row table built from per-family files; removes order-dependent shadowing and makes a duplicate id a unit test. 21 commits touched the chain in two days, so do it before it grows. Folds in `TD2-02/-06/-14` (the three statement-frame parsers, the three reputation blocks, the If/ElseIf twin).
2. **`TD1-06` + `TD1-05` — finish #5803 / #5802** (medium + large, incremental): per-family files for `apply_effect`/`evaluate_guard`/`guard_reading`/`check_call`, and `register_systems`/`register_saved` per subsystem for the 852/826-line schedule functions. Reopen #5803 or file new.
3. **`TD1-07` — budget the eleven 580–1373-line frame/Vulkan driver functions** (small to budget, medium to extract): copy the `draw.rs:1403` budget test pattern first; `build_pipelines_and_finish` and `build_and_upload_instances` each grew ~40 lines in four days. Skinned-BLAS/`build_blas_batched` extractions are recording-adjacent — extract verbatim, validate under `BYRO_VALIDATION=1`.
4. **`TD9-03` — `test_paths::require_or_skip` + migrate the 127 `/mnt/data/SteamLibrary` literals** (small, mechanical), starting with the two closed-HIGH pin files; add a hygiene scan so it stays closed.
5. **`TD1-08` / `TD1-02..04` — split the registry-as-control-flow and the three newly-over-2000 files**: `parse_block_inner` (255-arm, 1102-line `match type_name`) into family dispatchers; `mq101_conformance.rs`, `audio/lib.rs`, `nif/import/types.rs` along the axes proposed (each lists the `include_str!`/source-scan hazards to re-point in the same commit — `count_example_targets` only counts a directory target's `main.rs`).

## Deferred

- **`TD8-02` (ObScript ledger state nothing reads back; `GetTalkedToPC` can never be true)** and **`TD6-02` (eight typed ESM maps with no readers)** are gated on the M47.3 follow-through / CHARAL + AI consumers — annotate now, wire later (decline-on-unmodeled rule applies to the commands).
- **`TD5-01`** `loading_screen.rs` TODOs: crosses the 30-day triage line 2026-11-01; file one LSCR tracking issue or fold into the P6 ROADMAP row (milestone P6).
- **`TD1-02..04`** file splits are low urgency (`audio/lib.rs` is 48% comments, `types.rs` is pure data); do them opportunistically with the M47.3 / M47.2 churn they collide with.
- **Unverified here:** golden images are not confirmed current (no engine launch); per-game corpus counts in the per-game skills need game data; 65 pre-06-07 audit reports whose findings are not under headings were not parsed (Dim 4 does not claim "no gap" there).

## Findings

Severity order is in the index below; the full finding text follows grouped by dimension (each dimension keeps its own prior-status table, "Verified clean" list and evidence).

### MEDIUM
| ID | Title | Effort |
|---|---|---|
| TD2-2026-10-19-01 | Eight `initialize_layouts` bodies; 16 callers keep the premise #6063 proved false | small |
| TD3-2026-10-19-01 | `docs/feature-matrix.md` contradicts shipped code in five rows | small |
| TD4-2026-10-19-01 | ~25 "known-open" skill callouts name CLOSED issues | small |
| TD4-2026-10-19-02 | Skill layout rows / pathspecs name five split hubs; delta scoping silently empty | small |
| TD4-2026-10-19-03 | `_audit-owners.md` owns `fragment/vm.rs`, split `vm/` tree loses HIGH floor + owners | trivial |
| TD9-2026-10-19-01 | Zero registered runners: closed-HIGH pin lane + nightly golden cannot execute | small / medium |
| TD9-2026-10-19-02 | `closed-high-pins` list drifted (2 wrong-lane rows, 3 closed-HIGH pins missing) | trivial |
| TD9-2026-10-19-03 | ~35 real-data tests re-inline `/mnt/data/SteamLibrary`, skip silently | small |

All other findings are LOW. Detail per dimension:

---


## Dimension 1 — File / Function / Module Complexity

Dim 1 (File / Function / Module Complexity), HEAD 12677d718: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 9 LOW (all default LOW; no promotion trigger applies).
Prior (10-15) TD1-01..10: 8 fixed (01, 02, 05, 06, 07, 08, 09, 10), 1 partial (04: `apply_at_depth` fixed, siblings live), 1 live (03 = open #5802). This run: 5 new, 2 prior-live (#5802, remainder of #5803), 1 stale-premise-but-live-in-new-form (#5586), 1 regression note (#5096 budgets).
Notable: the 10-15 scripting monoliths are all split and no child re-grew (largest child 1538 prod LOC); the churn moved to the M47.3 ObScript command chain (six chained 150-780 line `match cmd` functions). Primary bucket today = 4 files (shader_constants_data.rs data table, mq101_conformance.rs example, audio/lib.rs, nif/import/types.rs); fns >200 LOC: 204 -> 210 (same scanner, base f4ae42a6f vs HEAD).

## Method and deltas

- `prod_loc` self-test: ok. Primary bucket (prod_loc > 2000), measured today vs base f4ae42a6f:

| prod LOC | base | file | note |
|---:|---:|---|---|
| 2868 | 2900 | `crates/renderer/src/shader_constants_data.rs` | data table (51% comments), generator already split out by #5808; not filed |
| 2157 | 1974 | `crates/scripting/examples/mq101_conformance.rs` | crossed (TD1-02) |
| 2034 | 1977 | `crates/audio/src/lib.rs` | crossed (TD1-03) |
| 2001 | 1997 | `crates/nif/src/import/types.rs` | crossed (TD1-04) |

- Left the list since 10-15 (all via real splits, none by shrinkage): `fragment/vm.rs` 6213, `translate/effects.rs` 5295, `byroredux/src/save_io.rs` 2136, `plugin/esm/cell/mod.rs` 2108, `scripting/src/condition.rs` 2005. Children today: `obscript_effects.rs` 1538 (new, not a split child), `fragment/vm/interp.rs` 1381, `vm/checker.rs` 1298, `effects/deferred.rs` 1280, `translate/effects/model.rs` 1121, `condition/eval.rs` 677, `save_io/live_load.rs` 714, `save_io/registry.rs` 619, `esm/cell/walkers.rs` 1194. No split child re-crossed 2000, and none is within 5% of it.
- Watch list (>= 1900, within ~5% of the line): `renderer/src/vulkan/context/init.rs` 1919 (base 1879, +40 in four days; volumetrics/ReSTIR commits `cf1a234d0`, `a03cf9174`), `debug-ui/src/panels.rs` 1921 (base 1952, shrinking), `plugin/src/esm/records/misc/world.rs` 1912 (flat). Next tier 1800-1900: `byroredux/src/inventory.rs` 1863 (base 1780, **+83 in four days**, fastest riser), `asset_provider/material/merge.rs` 1860, `nif/import/material/mod.rs` 1834.
- Secondary bucket (total LOC > 2000, tests inflate): 86 files (baseline.txt); none has prod_loc > 2000 beyond the four above, none escalates.
- Function scan: `/tmp/audit/tech-debt/fnscan.py` (brace-aware, strings/comments blanked, `#[cfg(test)]` / `#[test]` items and `tests/` + `*tests.rs` excluded; nesting counts only if/else/for/while/loop/match/unsafe/arm/closure blocks; arms = `=>` directly under a `match` brace). Output `fn_head.tsv` (HEAD) and `fn_base.tsv` (`git archive` of f4ae42a6f in `base_src/`). Calibration: the scanner reproduces the prior report's headline rows exactly at base (`apply_at_depth` 621, `object_read` 1259, `check_call` 773, `register_late_systems` 808, `register_update_systems` 763, `build_save_registry` 368).

| metric | prior report | base, this scanner | HEAD | delta |
|---|---:|---:|---:|---:|
| production fns > 200 LOC | 199 | 204 (194 excluding examples/tools) | **210** (201 excl.) | +6 |
| fns > 300 | n/a | 81 | 84 | +3 |
| fns > 500 | n/a | 31 | 29 | -2 |
| fns > 1000 | n/a | 7 | 5 | -2 |
| nesting > 5 (all / non-example) | n/a | 114 / 86 | 122 / 93 | +8 / +7 |
| match with > 50 arms | n/a | 8 | 8 | different members (below) |
| `pub use` hubs > 20 (mod.rs/lib.rs/facade .rs) | 5 | 5 | 5 | 0 |

  The prior 199 vs my base 204 is scanner scope (mine includes `examples/` and `tools/`: 10 fns); headline rows agree to the line.
  Gone from the > 200 list: `apply_at_depth` 621 -> 127, `object_read` 1259 -> 72, `check_call` 773 -> 372 (still > 200), `evaluate_function` 410 -> 95, `register` (scripting lib) 241 -> `registration.rs` 169, `mq101::run` 1454 -> 45, `apply_environment` 216 -> 28, `decode_spline_animation` 238 -> 4, the two pasted 95-arm `effect_kind` copies (#5807, one `Effect::kind` remains). New or crossed: the six ObScript `run` fns, `build_save_registry` moved to `save_io/registry.rs` 379, `execute_pending_save_loads` 316, `obscript_quests::call` 162 -> 359, `getter` 310, `guard_reading` 234 (moved), `ambient_ai_package_system` 172 -> 210, `footstep_system` 198 -> 212, `apply_environment_with_sources` 217.
  Growth in functions that were already > 200: `register_update_systems` 763 -> 852, `refresh_scene_actor_bindings` 484 -> 555, `build_pipelines_and_finish` 1333 -> 1373, `build_and_upload_instances` 1250 -> 1289, `trigger_detection_system` 227 -> 264, `recreate_gbuffer_dependent_passes` 235 -> 277, `scene_trigger_actor_approach_system_inner` 290 -> 323, `register_late_systems` 808 -> 826, `apply_effect` 562 -> 578.
- match > 50 arms today: `nif/blocks/mod.rs::parse_block_inner` 255 (1102 LOC), `byroredux/src/ui_input.rs::physical_key` 134 and `logical_key` 80 (key tables, fine), `translate/effects/model.rs::Effect::kind` 95 (the single remaining copy), `papyrus/src/token.rs::fmt` 84, and three new ObScript command matches: `obscript_more/tail.rs::run` 71, `obscript_more/state.rs::run` 69, `obscript_effects.rs::run` 63.

## Findings

#### TD1-2026-10-19-01: The M47.3 ObScript command surface is a chain of six `match cmd` functions (150-780 lines each, ~270 arms), each ending `_ => next::run(..)`
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms > 50, fn > 200)
- **Location**: `crates/scripting/src/obscript_effects.rs:301` (`run`, 780 lines, 63 arms) then `obscript_more.rs:296` (233 / 24) -> `obscript_more/state.rs:19` (435 / 69, falls through at L451) -> `actions.rs:11` (260 / 26, L268) -> `extras.rs:157` (157 / 15, L311) -> `tail.rs:89` (483 / 71, nest 7); plus `obscript_quests.rs:481` `call` (359 / 28) and `obscript_effects.rs:1228` `getter` (310 / 36)
- **Status**: NEW (the five-file chain did not exist at the 10-15 snapshot; `obscript_more/` landed 2026-10-18, 21 commits touch these files in two days)
- **Effort**: medium
- **Description**: each command id is dispatched by trying up to six `match` blocks in a fixed order, so which function owns a command is only discoverable by reading the chain; a command added to an earlier block silently shadows a later one (e.g. `state.rs:259` forwards `0x1170 if !fallout` back into `obscript_effects::run`, `tail.rs:460` and `extras.rs:300` call `obscript_actions::run` / `obscript_effects::run` with synthetic ids). `obscript_effects::run` also opens with a 30-entry `matches!(cmd, CMD_SET_ENEMY | CMD_SET_ALLY | ...)` (`needs_caller`) that restates, per command, a fact the arm already knows. This is the shape the 10-15 report fixed on the Papyrus side by family files, re-created in a flat form on the ObScript side.
- **Evidence**: `fnscan.py` rows above; `sed -n 296,330p obscript_effects.rs`; `grep -n '_ => super::' obscript_more/*.rs` shows the five fall-through links.
- **Proposed fix**: one row table, `static COMMANDS: &[Command { id: u16, games: Games, needs_caller: bool, handler: fn(&World, &[ObScriptValue], &Context) -> Action }]`, built from per-family files (`obscript/cmd/{inventory,combat,faction_perk,lock_open,world_pos,player,ui_message,quest,state_query,ledger,...}.rs`, one `fn` per command or small family; the bodies are already 5-40 lines), looked up by sorted-slice `binary_search_by_key` or a `HashMap<u16, _>` built once; `handles()` (obscript_effects.rs:117) becomes a table lookup. Order-dependence disappears and a duplicate id becomes a unit test over the table. Convert in chain order so `obscript_more/tests.rs` (683 lines) stays the safety net. Re-check `include_str!` readers of the `obscript_*` files before moving text (none seen at this snapshot).

#### TD1-2026-10-19-02: `crates/scripting/examples/mq101_conformance.rs` is a 2157-production-LOC example (was 1974); existing #5586's premise (`run` is 1454 lines, depth 15) is fixed, the file-level problem is not
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/examples/mq101_conformance.rs` (2184 lines; tests from L2158); `run` is now L397-441 (45 lines)
- **Status**: Existing: #5586 (open, stale: no commit carries "Fix #5586"; the decomposition landed under other work, `fd70bf557`, `314580045`). Re-scope the issue to the file crossing.
- **Age**: +183 prod LOC since 10-15 (`f2d559a79`, `5dc24a272`, `02f22d793`, `f597efa83`, `fd70bf557`)
- **Effort**: small (the helpers are already separate `fn`s; this is a move)
- **Description**: the checks are now ~45 well-named functions, but in one file with no module boundary. Layers by line: AST/pex shape helpers L119-340; data-dir/args L342-396; quest record + IMAD runtime checks L445-521, L1511-1611; scene analysis + package analysis L522-1285 (`check_scene_runtime` 165, `check_scene_linkage` 91, `analyze_package_action`, `package_execution_kind`, `travel_execution_kind`); script corpus + fragments L1286-1510, L1638-1985; animation/voice L1986-2046; report L2047-2157.
- **Evidence**: `prod_loc` 2157; `grep -nE '^(fn|struct|const)'` listing; `fn_head.tsv` (largest remaining fn 165 lines).
- **Proposed fix**: directory target `examples/mq101_conformance/{main.rs, pex_shape.rs, quest.rs, scenes.rs, packages.rs, scripts.rs, report.rs}` (cargo auto-discovers `main.rs`). Hazard: `byroredux/src/workspace_hygiene_tests.rs:895 count_example_targets` counts only `main.rs` of a directory target, so the per-crate LOC ceiling (`scripting` 13 files / 4853 LOC, exactly at the ceiling today) would silently stop counting the sibling modules; count the whole directory in that helper in the same commit. Shares helpers with `fragment_coverage.rs` (686) and `obscript_tools.rs` (976) via `examples/common/`.

#### TD1-2026-10-19-03: `crates/audio/src/lib.rs` crossed 2000 production LOC (1977 -> 2034); it holds seven responsibilities and is 48% comments
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/audio/src/lib.rs:1-2036` (tests already external, `mod tests;` L2036; `pub mod fuz;` L2033)
- **Status**: NEW (watch-listed at 1977 on 10-15)
- **Age**: +57 since 10-15: `acd577a79` (#5912 .fuz voice), `74d841165` (orphaned dialogue voice)
- **Effort**: medium
- **Description**: 969 of the 2036 lines are comments, ~970 are code, so this is a low-urgency crossing. By line: module docs L1-153; DSP/unit helpers (`bu_to_audio_space`, underwater filter, reverb send gates, constants) L155-335; sound bookkeeping types (`ActiveSound`, `SoundSource`, `PendingOneShot`) L336-472; `AudioWorld` + `impl` L474-911 (376 lines); ECS components (`AudioListener`, `Attenuation`, `AudioEmitter`, `OneShotSound`) L912-1070; the system and its sub-steps (`audio_system`, `sync_emitter_positions`, `update_underwater_filters`, `sync_listener_pose`, `build_spatial_track`, `drain_pending_oneshots`, `dispatch_new_oneshots`, `prune_stopped_sounds`) L1077-1653; loaders + `SoundCache` + `VoiceSoundCache` L1654-2031.
- **Evidence**: `prod_loc` 2034; `grep -cE '^\s*//' lib.rs` 969; offsets from `grep -nE '^(pub(\(crate\))? )?(fn|struct|enum|impl)'`.
- **Proposed fix**: `lib.rs` keeps docs and `pub use`; new `dsp.rs` (units + filter/reverb gates), `world.rs` (`AudioWorld`, `ActiveSound`, `SoundSource`, `PendingOneShot`), `components.rs`, `system.rs` (the per-frame steps; #5738 already shares `build_spatial_track` between the two dispatch loops), `cache.rs` (`SoundCache`, `VoiceSoundCache`, loaders). Hazard: four source-scan tests read `lib.rs` verbatim (`audio/src/tests.rs:1350, 1417, 1502, 1632`: `drain_pending_oneshots`, both dispatch failure arms, the `bu_to_audio_space` seam, `FilterBuilder::new()` count == 1); each must read `concat!` of the new files or the pins go vacuous (feedback_file_split_include_str).

#### TD1-2026-10-19-04: `crates/nif/src/import/types.rs` sits on the line (2001 production LOC) and mixes eight type families plus five inline test modules
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/nif/src/import/types.rs:1-2442` (tests inline at L554, L1179, L1835, L2294, L2355)
- **Status**: NEW (watch-listed 1997 on 10-15; 44% comments)
- **Effort**: medium
- **Description**: pure data + small accessors, no function over ~130 lines, so the cost is navigation and merge collisions (every M47.2/FO4 import field lands in `ImportedMaterial`/`ImportedMesh`/`ImportedScene`). Families: lights/texture effects/collision/nodes L31-300; `MaterialTextureSet<T>` + impl + tests L326-657; `ImportedMaterial` + Default L658-959; `ImportedMesh`, morphs, skin, bones, dismember helpers + impls L960-1575; attach points/furniture/`ImportedScene` + `Similarity`/`compose_geometry_bounds` L1576-1908; ragdoll L1909-2028; particle emitters + curves + flat form L2029-2293.
- **Evidence**: `prod_loc` 2001; `grep -nE '^(pub )?(struct|enum|impl|fn|mod)'`.
- **Proposed fix**: `import/types/{mod.rs (re-exports), light.rs, node.rs, material.rs (+MaterialTextureSet tests), mesh.rs (mesh/skin/bone + skin_partition tests), scene.rs (+geometry_bounds tests), ragdoll.rs, particles.rs (+role_enumeration tests)}` with `pub use` in `types/mod.rs` so every `super::types::X` caller is unchanged. `include_str!("types.rs")` has no reader in `crates/nif` (checked); `crates/core/src/ecs/components/material.rs:757, 840` cite the path in comments, refresh them.

#### TD1-2026-10-19-05: Existing #5802 still live: the six hub lists kept growing after the first install slice (`register_update_systems` 763 -> 852)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `byroredux/src/boot/schedule/update.rs:17` (852, was 763), `late.rs:15` (826, was 808), `byroredux/src/save_io/registry.rs:263` `build_save_registry` (379, was 368), `byroredux/src/asset_provider/script.rs:1046` `populate_scene_runtime` (431, was 420), `crates/scripting/src/scene/quest_alias.rs:835` `refresh_scene_actor_bindings` (555, was 484, nest 6)
- **Status**: Existing: #5802 (open). Progress verified: `World::init_resource` exists (`world.rs:668`); `scripting/registration.rs` is 182 lines with 9 `::install(` calls (`register` 241 -> 169); 6 scripting modules expose `install` + `register_saved` (`crime_gold`, `crime_faction`, `obscript_ledger`, `challenges`, `magic_effects`, plus `ownership`'s first-slice hooks) and `obscript_more` is install-only; `scripting/lib.rs` is now a pure 326-line facade (#5806). Not done: the two schedule functions (the bulk) and the registry still hand-list every system/row (`save_io/registry.rs` carries ~104 `register*` lines, `late.rs` ~66 `Access` rows).
- **Effort**: large (a seam), done incrementally
- **Proposed fix** (no change to the issue's axis): `pub fn register_systems(&mut Scheduler)` per subsystem for the 826/852-line schedule functions (the M47.3 `obscript_*` systems are the newest rows), and `register_saved` for every remaining registry row; add `refresh_scene_actor_bindings` (a four-phase fixed point: sort candidates, filler resolution, location/created-object aliases, retry pass) as a second item under the issue. Suggest posting these 10-19 deltas on #5802.

#### TD1-2026-10-19-06: The remainder of closed #5803 is live: `apply_effect` (578, grew from 562), `evaluate_guard` (313), `check_call` (372), `guard_reading` (234), `call_statement` (227)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects.rs:152` `apply_effect` (578 lines, ~35 inline arms for the older families); `fragment/effects/evaluate.rs:228` `evaluate_guard` (313 / 30 arms); `fragment/vm/checker.rs:722` `check_call` (372); `translate/effects/guards.rs:357` `guard_reading` (234); `fragment/vm/interp.rs:1154` `call_statement` (227)
- **Status**: Existing: #5803 (closed 10-17 by `20e14944a`, whose subject covers only `apply_at_depth`, now 127 lines and fixed; the issue title also named `apply_effect`/`evaluate_guard`). Closed issue + live problem for those two: regression of its stated scope, reopen or file new.
- **Effort**: medium
- **Description**: the family-file precedent (`effects/{inventory,lock,scene,ai_combat,player_control,placement,vehicle_cinematic,quest_scoped}.rs`) covers the newest arms only. The inline arms group into clear families: quest/local/reference variables (`SetQuestVariable`, `CopyQuestVariable`, `ModQuestVariable`, `SetLocal`, `SetReferenceVariable`, L294-312, L620-667), actor values (`SetActorValue`/`Named`/`DamageActorValue`, L496-575), perk/faction/spell/race (L576-619, L679-720), location + alias (L444-495), player stats/economy (L315-335, L368-397), messages/UI (L252-283).
- **Proposed fix**: move each group to `effects/{quest_vars,actor_values,actor_lists,location_alias,player_stats,messages}.rs`, so `apply_effect` becomes a ~120-line dispatcher; mirror the same families for `evaluate_guard` (guard kinds by family) and `guard_reading`. Keep `fragment.rs::SOURCES` valid (add the files to its `concat!` and its `sources_completeness` test).

#### TD1-2026-10-19-07: Frame and Vulkan driver functions of 580-1373 lines have no regrowth budget (only `draw_frame` does); two are still growing (regression of #5096)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (fn > 200, nesting > 5)
- **Location** (HEAD LOC / nesting): `renderer/src/vulkan/context/init.rs:527` `build_pipelines_and_finish` 1373 / 4 (+40 in 4 days); `context/build_and_upload_instances.rs:153` 1289 / 3 (+39); `byroredux/src/app_events.rs:681` `about_to_wait` 1032 / 7; `byroredux/src/render/static_meshes.rs:326` `collect_static_mesh_draws` 1031 / 4; `context/skinned_blas_refit.rs:78` 978 / **10**; `acceleration/blas_static.rs:266` `build_blas_batched` 975 / 5; `context/geometry_pass.rs:21` 761 / 8; `byroredux/src/app_frame.rs:110` `render_one_frame` 701 / 7; `physics/src/water.rs:726` `apply_buoyancy_with_scratch` 682 / 7; `cell_loader/references/synth_child.rs:284` 661 / 6; `cell_loader/references/mod.rs:296` `load_references_budgeted` 580 / **9**
- **Status**: Existing: #5096 (closed; the budget there is `DRAW_FRAME_LINE_BUDGET = 720`, `draw.rs:1403`, which `draw_frame` sits exactly on at 720). The other eleven have no pin; `build_pipelines_and_finish` and `build_and_upload_instances` regrew since 10-15.
- **Effort**: small per function (budget), medium per extraction
- **Description / extraction proposals** (outlines read; none touches barrier order):
  1. `about_to_wait`: the `--bench-frames` summary branch L1170-~1650 (~480 lines, 60 print/format sites, nest 7, includes the screenshot-claim state machine at L1590-1650) becomes `fn emit_bench_summary(&mut self)` + `fn advance_screenshot_request(&mut self)`; leaves the per-tick driver near 450.
  2. `build_pipelines_and_finish`: numbered construction steps (12c cluster cull, 12d skin, 12d.5 palette, 14 graphics, 15a water, groundcover blade/model tiers, ...) become one `build_<stage>(..) -> Result<..>` per stage returning the owning struct; construction only, no recording.
  3. `build_and_upload_instances`: instance-SSBO walk L188-~760, ReSTIR/caustic rig-key folding L760-~880, BLAS-key creation pass L1055-1230, TAA/skin recovery L1233-1372 become four private methods around the existing output type (L87).
  4. `collect_static_mesh_draws`: ~14 hoisted query bundles + per-entity resolve; split `StaticDrawQueries::new(world)` from `fn emit_draw(&mut out, entity, ..)`.
  5. `record_skinned_blas_refit` (nest 10) and `build_blas_batched` (phases 1-7 are numbered in its comments): one fn per phase/bracket (`#1194` skin-dispatch bracket L527-609, refit bracket L799-901). **Recording-adjacent**: extract verbatim, keep call order and barrier/bracket placement identical, verify under `BYRO_VALIDATION=1` (feedback_speculative_vulkan_fixes).
  6. `load_references_budgeted` (nest 9) and `import_embedded_animations` (`nif/src/anim/entry.rs:368`, 433 / nest 9): flatten with early-continue helpers per phase.
- **Evidence**: `fn_head.tsv`; section markers from the comment headers of each function.
- **Proposed fix**: first, copy the `draw.rs:1403-1457` budget test pattern (source-scan of the fn's line count) for these eleven at their current size so growth is a deliberate edit; then extract in the order above.

#### TD1-2026-10-19-08: `nif/src/blocks/mod.rs::parse_block_inner` is a 1102-line, 255-arm `match type_name` (the largest match in the workspace)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms > 50)
- **Location**: `crates/nif/src/blocks/mod.rs:265` (fn spans L265-1366; ~289 arm-head lines incl. or-alternatives)
- **Status**: NEW to this report series (identical size at base; never filed)
- **Effort**: medium
- **Description**: nearly every arm is `"X" => Ok(Box::new(x::X::parse(stream)?))` interleaved with rationale comments, grouped by family only by comment. It is a registry written as control flow, and the live arm count is quoted in `CLAUDE.md`. Two tests scan this file's text (`import/collision/mod.rs:788`, `import/walk/lights.rs:264` via `include_str!("../../blocks/mod.rs")`), and `every_dispatched_bhk_shape_has_resolve_arm` (collision/mod.rs:858) depends on that scan.
- **Evidence**: `fnscan.py` row; `awk 'NR>=265 && NR<=1370' blocks/mod.rs | grep -cE '^        "[A-Za-z0-9_]+"( \| "[A-Za-z0-9_]+")* =>|^        \| "'` -> 289.
- **Proposed fix**: family dispatchers chained like the collision ones (`fn dispatch_node(..) -> Option<io::Result<Box<dyn NiObject>>>`, `dispatch_geometry`, `dispatch_shader`, `dispatch_collision`, `dispatch_anim`, `dispatch_particles`, ...), each in the matching `blocks/<family>.rs`, with `parse_block_inner` keeping the groupID prologue and trying families in order; or a `static BLOCK_PARSERS: &[(&str, ParseFn)]` sorted slice. Re-point the two `include_str!` scans (and the dispatch-count doc) in the same commit.

#### TD1-2026-10-19-09: `byroredux/src/cell_loader.rs` is a 36-`pub use` facade that also owns two unrelated jobs
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (mod.rs/lib.rs with > 20 `pub use`)
- **Location**: `byroredux/src/cell_loader.rs:229` `pack_effect_shader_flags`, `:279` `pack_imported_material_flags` (~130 lines, 34 call sites outside the file), `:637` `note_player_entered_cell` (new, M47.3 respawn clock hook)
- **Status**: NEW (35 `pub use` at base; the M47.3 hook is new)
- **Effort**: trivial
- **Description**: the module doc promises a split entry-point facade; the material-flag packers (#890 Stage 2) and a gameplay respawn hook are neither. Other `pub use` hubs are clean: `scripting/lib.rs` 67 (pure facade, `register` moved out by #5806), `core/ecs/components/mod.rs` 43, `byroredux/src/systems.rs` 28, `plugin/esm/records/mod.rs` 25 contain no logic.
- **Proposed fix**: `cell_loader/material_flags.rs` (the two packers, re-exported from `cell_loader.rs`) and `note_player_entered_cell` into `cell_loader/load.rs` or the M47.3 caller's module; no caller edits.

## Verified clean

- No file other than the four above exceeds 2000 prod LOC; no split child of the 10-15 splits re-crossed (max 1538) or entered the >= 1900 band.
- `shader_constants_data.rs` (2868, data table): the generator types and renderer now live in `shader_constants_gen.rs` (161 lines, #5808 landed) and rows name each constant once through `shader_define!`; the remaining const + `SHADER_DEFINES` row duplication is a two-place edit, not a split candidate. Not filed.
- The 10-15 TD1-01/02/05/06/07/08/09 fixes hold: `object_read` 72, `evaluate_function` 95, `scripting/lib.rs` has no fn/struct items, one `Effect::kind` (no pasted copies), `save_io/` and `esm/cell/` are directories.
- `apply_at_depth` is now a 127-line ordered flush list (nest 2).
- `scripting/lib.rs`, `core/ecs/components/mod.rs`, `byroredux/src/systems.rs`, `plugin/esm/records/mod.rs`: facade-only hubs.
- Nesting > 5 is 122 functions across all scopes (93 outside examples/tools); depth histogram: 6 x82, 7 x25, 8 x10, 9 x4, 10 x1; the depth >= 9 set is `record_skinned_blas_refit` 10, `load_references_budgeted` 9, `import_embedded_animations` 9 plus probe examples; no new depth-9+ production function since 10-15.
- Test-heavy bucket (86 files) has no member with its own prod_loc above 2000.
- `cargo +nightly clippy -W clippy::cognitive_complexity` not run (no cargo invocation needed; the scanner above covers the same worst-offender ranking).


---

## Dimension 2 — Logic Duplication

# Tech-debt audit 2026-10-19 — Dimension 2: Logic Duplication (HEAD 12677d718)

**Counts:** 1 MEDIUM, 14 LOW (0 HIGH/CRITICAL). Prior: all 7 MEDIUMs (TD2-2026-10-15-01..07) **fixed**; of the 16 prior LOWs (-08..-23) 11 verified fixed and 5 leave residue (-06 tail, -08, -10, -22, -23), re-filed as 4 "prior-partial" findings (04, 08, 09, 10). New: 11 genuinely new findings (01-03, 05-07, 11-15).
**Notable:** the one MEDIUM is a live copy of the bug #6063 fixed one copy of today; the guard that closed TD2-23 (#5824) has a blind spot (newline-prefixed weak cuts), so the source-scan one-home rule is being re-broken at ~20 sites.
**Method:** discovery recipe + side-by-side reads + a 5/7/8-line normalized-window clone scan (script in scratchpad, production code only, test modules cut) over every `.rs` under crates/ byroredux/ tools/; flagged files changed since 2026-10-15.

## Prior-report status (verified in code, not just by closed issue)

| Prior ID | Status | Evidence |
|---|---|---|
| TD2-01 shape cull gate x8 | FIXED | `shape_is_culled` / `node_is_culled` at `crates/nif/src/import/walk/mod.rs:278,288`; 12 call sites use it (#5792). `particle_system_is_culled` keeps its own flag test on purpose (target-keyed NiVisController fallback). |
| TD2-02 twin tangent synthesizers | FIXED | `synthesize_tangents` is a 20-line wrapper over `synthesize_tangents_yup` (`tangent.rs:162-186`, 89a0b6ccd #5793; d81f7501f docs). |
| TD2-03 two NPC Finalize tails | FIXED | `attach_skeleton_runtime` / `attach_walk_animation` / `finish_actor` / `spawn_armor_or_log` in `npc_spawn/resumable/mod.rs:591-680`, both arms call them (2ba5c8c12 #5794). |
| TD2-04 CachedNifImport built twice | FIXED | `CachedNifImport::from_scene` (`cell_loader/nif_import_registry.rs:444`) called from `partial.rs:126` and `references/import.rs:156` (e49533a4c #5795). The `import.rs:472` literal is the SpeedTree shell, deliberate. |
| TD2-05 four WATR decoders | FIXED | `HeadLayout` + `decode_legacy_head` (`water.rs:424,440`), four decoders call it, a cross-layout test pins it (626be5d12 #5796). `decode_dnam_fo4` has a genuinely different layout and keeps its own reads. |
| TD2-06 ten Papyrus dispatch tails | FIXED, tail residue | `fragment/dispatch.rs` `run_papyrus_call` / `flush_advances`; 9 dispatchers use it (e27c93a97 #5797). Residue: TD2-2026-10-19-09. |
| TD2-07 NIF/core animation twin | FIXED | `nif::anim::types` re-exports the core key/channel types (`types.rs:55-58`); `convert_nif_clip` only interns names and resolves the flip role (ce08ec0cf #5798). |
| TD2-08 15 clamp samplers | FIXED, residue | `descriptors::clamp_sampler` has 14 callers (31f4919d5 #5809). Three clamp samplers still hand-built: TD2-2026-10-19-10. |
| TD2-09 extension dispatch tails | FIXED | `with_extension_host` in `byroredux/src/extensions/systems.rs` (78cfa46df #5810). |
| TD2-10 populate / lower_* tails | HALF FIXED | `finish_lowered_fragment` (`populate.rs:272`, c1983a81d #5811) took the populate loops; the four `lower_*_fragment` Scope builds are untouched: TD2-2026-10-19-08. |
| TD2-11 StartScene/StopScene | FIXED | `stop_with_shutdown` (`quest_stages.rs:274`, 32bebaa19 #5812). |
| TD2-12 player-entity read x62 | FIXED | `PapyrusPlayerEntity::get` (106f636be, #5813); no `try_resource::<PapyrusPlayerEntity>().map` left. |
| TD2-13 CellLightingRes | FIXED | `CellLightingRes::bare` (`components/environment.rs:94`, 2e5f462db #5814). |
| TD2-14 CELL walkers | FIXED | `CellData` build shared (`cell/helpers.rs:270`, d6d216b91 #5815). |
| TD2-15 apply_batch arms | FIXED | e634fbf9b #5816. |
| TD2-16 FormID parsers | FIXED | `parse_console_form_id` (`commands/shared.rs:101`, #5817). |
| TD2-17 tree-LOD LE reader | FIXED | 83fb9700e #5818. |
| TD2-18 GRUP walks in examples | FIXED | `EsmReader::for_each_record`, 24 uses in `crates/plugin/examples` (0b3ae0d13 #5819). |
| TD2-19 four ring reconcilers | FIXED | `reconcile_lod_rings` / `retain_resident` (83fb9700e #5820). |
| TD2-20 MaterialProvider failed path | FIXED | `record_failed` (`material/provider.rs:280`, 5fb7af99f #5821). |
| TD2-21 metrics snapshot | FIXED | one copy per side of the wire seam, deliberate (f64a299ec #5822). |
| TD2-22 GpuUploadCtx literal x22 | HALF FIXED | `upload_ctx!` macro (`vulkan/mod.rs:102`) has 18 callers (69963c09e #5823); 8 macro-shaped literals remain: TD2-2026-10-19-10. |
| TD2-23 weak cfg(test) cuts | REGRESSED / guard blind spot | 18 sites migrated (8bd4f3b8b #5824), but the guard needle list misses newline-prefixed forms: TD2-2026-10-19-04. |

---

## MEDIUM

#### TD2-2026-10-19-01: Eight `initialize_layouts` bodies are the same UNDEFINED→GENERAL one-time submit, and their sixteen callers all carry the "nothing has referenced these images" destroy premise that #6063 just proved false for the one copy it found
- **Severity**: MEDIUM (duplicated logic, one copy fixed and the siblings not — severity table row "divergent bug-fix history")
- **Dimension**: 2 - Logic Duplication
- **Location**: bodies `vulkan/bloom.rs:536`, `taa.rs:563`, `svgf.rs:1005`, `water_caustic.rs:151`, `caustic.rs:798`, `gbuffer.rs:331`, `shadow_filter.rs:279`, `volumetrics/init.rs:914` (the last two take `GpuUploadCtx` / extra staging); callers `context/init.rs:848,1097,1158,1223,1320,1391,1526`, `context/resize.rs:621,820,915,977,1674` and `taa.rs:818`, `svgf.rs:1431`, `caustic.rs:1190`
- **Age**: copies since #949/#1100 (2026-05); barrier helper funnel `13e00a24b` (2026-09-15) and #4221 shared the barrier but not the submit; `4a6128389` #6063 (2026-10-19, 18:37) fixed only `shadow_filter`
- **Effort**: small (helper) / small (caller policy)
- **Description**: seven of the eight are: `with_one_time_commands(...)`, collect `image_barrier_undef_to_general(img)` for the pass's images, one `cmd_pipeline_barrier(NONE -> COMPUTE_SHADER, ...)`. Only the destination stage (water_caustic: FRAGMENT; gbuffer: UNDEF->SHADER_READ) and the image list differ. `caustic.rs:803-813` still hand-builds the barrier (layered range) after #4221 funnelled the rest.
- **Evidence**: #6063's message: "destroyed the pass's images on any failure, including a post-submit fence-wait error from the UNDEFINED -> GENERAL layout walk, which may still be executing against them"; it split the walk out and added the `OneTimeCommandError::may_be_in_flight` leak arm (`shadow_filter.rs:96`, `resize.rs:737`) for that one pass. The sibling callers still `destroy` straight after a failed `initialize_layouts` under the comment "no frame command buffer has yet referenced its images": `init.rs:1223-1235` (SVGF), `init.rs:1320-1329` (caustic), `init.rs:1526-1535` (TAA), `init.rs:848-859` (water caustic), `resize.rs:1674-1681` (TAA, "nothing has referenced these images"). That premise is exactly what #6063 showed false for a `MaybeInFlight` error. (Bug-class ownership: renderer/concurrency audit; filed here because the cause is the unshared wrapper.)
- **Impact**: the in-flight-leak policy (#4891 / #5947 / #6063) has to be re-applied per pass; it has now been applied to 1 of 8, and the remaining 7 free GPU memory a submitted command buffer may still touch.
- **Related**: #6063, #4891, #5947, #4221, #949/#1100
- **Suggested Fix**: `fn initialize_layouts_once(ctx: GpuUploadCtx, images: impl IntoIterator<Item = vk::Image>, dst_stage: vk::PipelineStageFlags) -> Result<(), OneTimeCommandError>` beside `image_barrier_undef_to_general` in `vulkan/descriptors.rs` (or `texture.rs` next to `with_one_time_commands`); the eight bodies shrink to an image list. Then one `fn destroy_unless_in_flight(pass, err)` (the #6063 arm) replaces the sixteen caller blocks, and a source-scan test pins "no `initialize_layouts` caller destroys without consulting `may_be_in_flight`", modelled on the #6063 test at `shadow_filter.rs:485`.

---

## LOW

#### TD2-2026-10-19-02: The SCDA statement frame (`[op][0x1c prefix][len][payload]`) is parsed three times, with the opcode constants spelled three ways
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript.rs:14-17,95-125` (diagnostic scan), `obscript_runtime.rs:28-40,331-345` (`compiled_line`), `obscript_vm.rs:69,434-450,484-640` (`next_statement`, `payload_span`, raw `0x16/0x17/0x18/0x19/0x1c` literals)
- **Age**: obscript.rs/runtime 2026-09-01; VM 2026-09-18 (6229e7d6e)
- **Effort**: small
- **Description**: three independent walkers of the same compiled-statement framing; the `0x1c` reference-function escape adds a 4-byte prefix in each (`offset + 4/6/8` literals at `obscript.rs:100-106`, `runtime.rs:333-337`, `vm.rs:435-438`). `SET_TO/IF/ELSE_IF/REFERENCE_FUNCTION` are `const`s in the first two files and bare hex in the VM (`OP_SET` at `vm.rs:69` is the only named one there).
- **Evidence**: hardening has already landed per copy: #5596 (cce3d59b5) touched all three; #5655 (291af4528, argument region bound) and #6016 (9a7298283, If/ElseIf condition bounded by payload, 2026-10-19) landed in the VM only, while `obscript_runtime.rs:327` enforces `expression_end == payload.len()` independently.
- **Proposed fix**: `pub(crate) struct StatementFrame { opcode, command_offset, payload: Range<usize>, next: usize }` + `fn statement_frame(bytes, offset) -> Option<StatementFrame>` and one `pub(crate) mod opcode { SET_TO, IF, ELSE, ELSE_IF, END_IF, REFERENCE_FUNCTION, ... }` in `obscript.rs` (the decode home); runtime and VM import both. `next_statement`/`payload_span` become thin wrappers.

#### TD2-2026-10-19-03: `vk::DescriptorImageInfo::default().sampler(..).image_view(..).image_layout(..)` is hand-spelled 92 times across 15 renderer files, after #5593 consolidated everything around it
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: counts by file: `composite.rs` 22, `svgf.rs` 17, `volumetrics/init.rs` 14, `taa.rs` 9, `bloom.rs` 8, `caustic.rs` 4, `sky_cube.rs` 3, `scene_buffer/descriptors.rs` 3, others 1-2 (`ssao`, `sky_cube/*`, `presentation`, `exposure_meter`, `groundcover_bench`, `texture_registry/mod.rs`, `water.rs`). Shapes: 37 x (sampler, view, SHADER_READ_ONLY_OPTIMAL), 22 x (sampler, view, GENERAL), 20 x (view, GENERAL), 13 other.
- **Age**: pre-#5593 (c6d36b4e7 consolidated `layout_binding` / `write_*` but not the image-info element)
- **Effort**: small (two helpers, mechanical migration)
- **Description**: `descriptors::write_combined_image_sampler` / `write_storage_image` take `&[DescriptorImageInfo]` that every caller builds by hand; the layout is the only thing that varies and it is determined by the descriptor type.
- **Evidence**: no divergence history found (checked `git log -S` on layout constants); this is breadth, not drift.
- **Proposed fix**: add `descriptors::sampled_image_info(sampler, view)` (SHADER_READ_ONLY_OPTIMAL) and `descriptors::storage_image_info(view)` (GENERAL) next to `write_storage_image` (`descriptors.rs:76`), migrate in place, and extend `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` to count the raw chain.

#### TD2-2026-10-19-04: The weak `#[cfg(test)]` cut is back at 9 sites the guard cannot see, and ~11 more re-spell `production_text` inline (prior-partial: TD2-2026-10-15-23, closed #5824)
- **Severity**: LOW (second recurrence, carried)
- **Dimension**: 2 - Logic Duplication
- **Location**: weak, newline-prefixed (truncates at a `#[cfg(test)] fn`/`use`): `crates/debug-server/src/evaluator.rs:986`, `crates/physics/src/water.rs:3919`, `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:2047`, `byroredux/src/app_step.rs:1726,1749`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:1761,1792,1843,1946`. Strong needle spelled inline instead of calling `source_scan`: `crates/ui/src/prepare.rs:1004` (ui does not depend on core), `crates/physics/src/sync.rs:2684`, `physics/src/world/mod.rs:1715`, `renderer/src/vulkan/context/resources.rs:815,866,911`, `byroredux/src/systems/cinematic.rs:2272`, `systems/combat_ai.rs:689`, `boot/schedule/mod.rs:810`, `material_translate.rs:2840,2887`, `npc_spawn/tests.rs:2909`.
- **Age**: #5824 (8bd4f3b8b, 2026-10-15) migrated 18 sites; two of them (`cinematic.rs:2272`, `combat_ai.rs:689`) were rewritten to the inline strong needle rather than to `production_text`
- **Effort**: small
- **Description**: `no_source_scan_cuts_with_the_weak_cfg_test_needle` (`workspace_hygiene_tests.rs:708`) lists `find("#[cfg(test)]")`, `rfind`, `split`, `split_once` with and without a leading `\n` — but not `split("\n#[cfg(test)]")` / `find("\n#[cfg(test)]")`, which is the spelling of the nine weak sites. The strong inline copies are not weak but break the one-home rule (`crates/core/src/source_scan.rs:22,36`).
- **Proposed fix**: add the two newline-prefixed needles to the guard (and let it also flag an inline `"\n#[cfg(test)]\nmod "` outside `source_scan.rs` plus the `ui`/`pex`/`facegen` allowlist), then migrate the 9 weak sites to `production_text_or_whole` and the in-workspace strong copies to `production_text`. For `byroredux-ui`, either allowlist it or add the already-approved-style dev-dependency.

#### TD2-2026-10-19-05: Eight request-drain systems open with the same query-collect / query_mut-remove preamble, and `flight_target` / `push_direction` are the same "level away vector" twice
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: preambles `byroredux/src/systems/flee.rs:73-80`, `knockdown.rs:65-72`, `revive.rs:32-39`, `script_requests.rs:43-51,174-181`, `cinematic.rs:132-139,397-445,460-507`; vectors `flee.rs:108-109` vs `knockdown.rs:84-85`
- **Age**: M47.3, 2026-10-17/18 (all new this window)
- **Effort**: trivial
- **Description**: `match world.query::<R>() { Some(q) if !q.is_empty() => q.iter().map(..).collect(), _ => return }` then `if let Some(mut q) = world.query_mut::<R>() { for .. { q.remove(..) } }`. The vector twin is `Vec3::new(a.x-b.x, 0.0, a.z-b.z).normalize_or_zero()` + "if zero use `Vec3::X`", byte-identical.
- **Evidence**: none yet; the family grew by five systems in two days and each re-spelled the preamble. (`scripting/src/cleanup.rs:239 drain_component` is the discard-only sibling.)
- **Proposed fix**: `pub(crate) fn take_requests<T: Component + Copy>(world: &World) -> Vec<(EntityId, T)>` in `systems/mod.rs` (query, collect, remove, one lock each), and `fn level_away(from: Vec3, to: Vec3) -> Vec3` in `systems/locomotion.rs` beside `step_toward_detailed`; both callers `flee_target`/`push_direction` keep only their own scale/lift.

#### TD2-2026-10-19-06: `AddReputation`, `SetReputation` and `AddReputationExact` each carry the same 12-line "get-or-insert the player's `FactionReputation` standing, branch on the fame/infamy axis" block, one per sibling file
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript_effects.rs:972-985`, `obscript_more/actions.rs:157-170`, `obscript_more/tail.rs:374-387`
- **Age**: 2026-10-18 (all three)
- **Effort**: trivial
- **Description**: `world.query_mut::<FactionReputation>()` -> `if get_mut(player).is_none() { insert(Default) }` -> `get_mut(..).expect("present or inserted")` -> `if axis == 1 { add_fame/set_fame } else { add_infamy/set_infamy }`. Differences: add vs set, and `AddReputation` maps the 1-5 amount through `reputation_bump_points`.
- **Proposed fix**: `fn adjust_standing(world, player, repu, fame: bool, points: u16, set: bool) -> Option<()>` in `obscript_effects.rs` (the first home; `obscript_more` already imports `form_arg`/`num` from it); the three arms keep argument validation and the effect text.

#### TD2-2026-10-19-07: Five scripting resources each hand-build a lowercased-EDID -> FormID table with their own `from_index`, lookup and `install_*`
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/idle_names.rs:19-35` (`IdleNames`), `sound_requests.rs:30-62` (`SoundEditorIds`), `actor_value_names.rs:12-41` (`ActorValueNames`), `obscript_more/extras.rs:34-58` (`MusicTracks`), `messages.rs:88-98` (globals in `MessageCatalog`)
- **Age**: 2026-10-13..18
- **Effort**: small
- **Description**: each is `HashMap<String, V>` keyed by `editor_id.to_ascii_lowercase()`, filtered on non-empty EDID, queried with a second `to_ascii_lowercase()`, installed by a `pub fn install_*` that returns the count. Case folding on both sides is the part that must stay consistent.
- **Proposed fix**: `pub struct EditorIdTable<V>(HashMap<String, V>)` with `from_records(iter)` and `get(name)` in a new small `crates/scripting/src/name_table.rs`; the five resources wrap it (their extra indexes — `by_form`, the `av` prefix alias — stay local).

#### TD2-2026-10-19-08: The four `lower_*_fragment` entry points still repeat the `Scope` build and the prune/never-suspend/barriers pipeline (prior-partial: TD2-2026-10-15-10 second half)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/translate/effects/lower.rs:71-76,92-98,116-122,222-228` (Scope) and the `.map(prune_unread_locals).filter(|e| locals_never_suspend(e) && provider_barriers_fit(e))` tail at `:77-79,99-101,123-125,235-237`
- **Age**: M47.2, 2026-10-12..16; #5811 (c1983a81d) fixed the populate half only
- **Effort**: trivial
- **Description**: they differ in `library`, `vmad_is_quest`, and (info) the speaker-parameter seeding plus a latent-tail check; everything else is the same 4-field Scope literal and pipeline.
- **Proposed fix**: `fn lower_with(body, providers, scope: Scope) -> Option<Vec<Effect>>` in `lower.rs` that owns the pipeline, and `fn scope_from(quest_props, argless, per_instance, library) -> Scope`; each entry point then sets its two or three distinguishing fields.

#### TD2-2026-10-19-09: Three fragment dispatchers still hand-roll `flush_advances`, and `Effect::Interpret { .., resume: None }` is built by hand at 11 sites (prior-partial: TD2-2026-10-15-06 tail)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: epilogue copies `crates/scripting/src/fragment/systems.rs:161-166` (pending-fragment system), `:259-264` (scene dispatcher, with the #3580 lock-order comment), `crates/scripting/src/quest_stages.rs:1134-1139`; constructors at `fragment/{distance,events,hits,package,perk,populate,reference,terminal,updates}.rs`
- **Age**: epilogues 2026-09/10; the dispatch module landed 2026-10-15
- **Effort**: trivial
- **Description**: `advances.is_empty()` -> `PapyrusPlayerEntity::get` -> `push_quest_stage_advances` is `dispatch::flush_advances` verbatim (it is `pub(crate)` and re-exported at `fragment.rs:82`). The two in `systems.rs` additionally carry a copy of the #3580 comment that now lives once in `dispatch.rs:41-46`.
- **Proposed fix**: call `flush_advances` at the three sites; add `Effect::interpret(script, function, args)` beside `Effect::Interpret` in `translate/effects/model.rs` and use it at the 11 literals.

#### TD2-2026-10-19-10: Residue of #5809 and #5823: three clamp samplers and eight macro-shaped `GpuUploadCtx` literals are still hand-built (prior-partial: TD2-2026-10-15-08, -22)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: samplers `crates/renderer/src/vulkan/context/helpers.rs:612` (depth-history: NEAREST/NEAREST + `max_lod(0.0)`, i.e. `clamp_sampler(NEAREST, NEAREST)` for a 1-mip image), `placeholder.rs:106`, `groundcover_bench.rs:556`; upload ctx `context/resize.rs:667,718,1284`, `byroredux/src/cell_loader/terrain.rs:994`, `byroredux/src/scene.rs:784`, `byroredux/src/cornell/builders.rs:388,434`, `cornell/oracle.rs:433`
- **Age**: sampler/ctx macros 2026-10-15; the stragglers were never migrated
- **Effort**: trivial
- **Description**: `clamp_sampler`'s doc says the two different samplers (shadow, wrap/repeat) are the only exceptions, but the three above are plain clamp samplers. Each of the eight literals has the exact `device/allocator/queue/transfer_pool` shape `upload_ctx!($ctx)` expands to (the `scene.rs`/`builders.rs` ones pre-bind `alloc` locally, which the macro's own doc says keeps compiling).
- **Proposed fix**: migrate the three to `descriptors::clamp_sampler` (the extra `.mipmap_mode` is a parameter) and the eight to `upload_ctx!` / `byroredux_renderer::upload_ctx!`.

#### TD2-2026-10-19-11: The Skyrim `.fuz` voice-path builder copies the truncation block of the FO3/FNV one, written one day after #5521 changed it
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/systems/dialogue_voice.rs:97-122` (`voice_path_candidates`) vs `:136-165` (`fuz_voice_path_candidates`); third sibling `:172` (`oblivion_voice_path_candidates`)
- **Age**: 4d6294bb1 (#5521, 2026-10-11) rewrote the rule; acd577a79 (#5912, 2026-10-19) copied it
- **Effort**: trivial
- **Description**: lowercase both EDIDs, `if quest.chars().count() + topic.chars().count() > VOICE_NAME_CHARS { cut quest to 10, topic to the rest }`, then `format!("sound\\voice\\{plugin}\\{voice_type}\\{quest}_{topic}_{:08x}_{response}.<ext>")` — 16 identical lines. The two differ in the empty-guard (`||` vs `&&`) and the extension; the doc comment of the fuz copy re-measures the same 25-char boundary, i.e. the rule is shared knowledge held in two places.
- **Proposed fix**: `fn voice_stem(quest_edid, topic_edid) -> (String, String)` (lowercase + truncate) in `dialogue_voice.rs`; both builders keep their guard and extension.

#### TD2-2026-10-19-12: Skyrim class skill and pool allocation share a 20-line leftover-distribution loop, copied with its tie-break
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:416-448` (`skyrim_class_skill_points`, N=18) vs `:462-500` (`skyrim_class_pool_points`, N=3)
- **Age**: 7c740eca0 (2026-10-14), tie-break reversed in 91d5e8d5c (2026-10-17), pool copy b9e60846e (#5914, 2026-10-18)
- **Effort**: trivial
- **Description**: `sets = total / sum`, per-slot `sets * weight`, then `order` sorted by (weight desc, index desc) and a `taken[]` round-robin until the leftover is gone. The pool doc says "distributed exactly like the skills" and carries its own re-statement of the index-descending tie rule that 91d5e8d5c had to change on the skill copy.
- **Proposed fix**: `fn distribute_by_weight<const N: usize>(weights: &[u8; N], total: i32) -> [i32; N]` in the same file; the skill fn passes `8 * (level-1)`, the pool fn `10 * (level-1)` and adds the Health term after.

#### TD2-2026-10-19-13: `DELIVERED_EVENTS` (18) and `DELIVERED_ALIAS_EVENTS` (16) share 14 hand-copied literals
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/reference.rs:36-57`, `fragment/alias_scripts.rs:33-50`
- **Age**: 2026-10-13 (alias list), the four trap/destruction events were added to the reference list only (2026-10-18/19)
- **Effort**: trivial
- **Description**: the alias list is the reference list minus `OnDestructionStageChanged`/`OnTrapHit`/`OnTrapHitStart`/`OnTrapHitStop`, plus `OnAliasInit`/`OnAliasShutdown`. A new reference-side event must be added to two arrays and nothing records whether the alias omission is a decision (`raise_reference_event` only reaches `PapyrusReferenceScripts`).
- **Proposed fix**: one `const SHARED_REFERENCE_EVENTS: [&str; 14]` in `reference.rs`; build each public list from it (concat in a `const fn`, or a `LazyLock<Vec<&'static str>>`), with a comment on why the four are reference-only.

#### TD2-2026-10-19-14: `obscript_runtime.rs` has two copies of the If/ElseIf/Else recursion (compiled lines vs source tokens)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript_runtime.rs:242-317` (`parse_compiled_sequence` + `parse_compiled_conditional`) vs `:475-543` (`parse_source_sequence` + `parse_source_conditional`)
- **Age**: 2026-09-01 (aa0fa1b5d "lower ObScript elseif chains")
- **Effort**: small
- **Description**: `parse_*_conditional` are 35 identical lines (depth cap, then-branch, `match terminator { EndIf, Else, ElseIf, End }`, build `If`); the `*_sequence` loops differ only in how a line is classified. The depth cap `MAX_LEGACY_OBSCRIPT_NESTING` is enforced in both copies by hand.
- **Proposed fix**: one generic `fn parse_conditional<L>(parse_sequence: &mut impl FnMut(&mut usize, usize) -> Option<(Vec<Stmt>, StatementTerminator)>, cursor, depth, condition) -> Option<Stmt>` (the sequence closure captures `script`/`dialect`/`lines`); the two `*_conditional` functions disappear.

#### TD2-2026-10-19-15: Small same-file twins on new code (each trivial; grouped to keep the report short)
- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location / fix** (each names its consolidation):
  - `crates/plugin/src/esm/records/misc/challenge.rs:97-98`, `music.rs:41`, `cell/wrld.rs:127-152`: hand-indexed `u32/f32::from_le_bytes([data[0], ..])` reads where `SubReader` (`esm/sub_reader.rs`) or `read_u32_sub` / `read_f32_sub` (`records/common.rs:287,295`) exist; `challenge.rs` SNAM/XNAM want a `read_form_sub(subs, code, remap)` helper next to them (14 raw reads remain in `records/`, 3 are new).
  - `crates/scripting/src/obscript_quests.rs:490,863`: two `let num = |i| args.get(i).map(..).unwrap_or(0.0)` closures that are `obscript_effects::num` (`pub(crate)`, `obscript_effects.rs:291`); and four `obscript_actions::Context { caller, known, action_ref, owner, elapsed }` literals (`:593,737,759,812`) for a `fn context(&self, caller) -> Context` on `ObScriptCommandHost`.
  - `byroredux/src/cell_loader/references/synth_child.rs:445-470` vs `:527-552`: the stamp / attach-script / count / `spawn_logical_quest_reference` tail, kept in step by a "same split as the LIGH-only branch above" comment (#3016); extract `fn finish_synth_child(...)`.
  - `crates/scripting/src/dialogue.rs:592-602` vs `package.rs:670-680`: the `ScenePlayer -> scene_form_ids` map; extract `scene::scene_form_ids(world)`.
  - `crates/audio/src/lib.rs:698-715` vs `:728-745`: `play_oneshot` / `play_oneshot_following` differ only in `SoundSource`; `fn queue_oneshot(&mut self, source, ...)`.
  - `crates/nif/src/blocks/collision/constraints.rs:266-313`: `LimitedHingeCInfo::parse_fo3` and `parse_hinge_fo3` read the same eight Vec4s; share `read_hinge_axes(stream) -> HingeAxes`.

---

## Verified clean (checked, nothing to file)
- Descriptor/binding boilerplate (#5593): zero `WriteDescriptorSet::default()` outside `descriptors.rs`; `DescriptorSetLayoutBinding::default()` only in the bindless texture registry and `reflect.rs` (the documented exception); no local cleanup macro (`macro_rules!` list: `try_or_cleanup!`, `upload_ctx!`, `impl_no_uninit!`, shader-define rows, a test `assert_layout!`).
- Image create/bind/destroy chains: `ImageCreateInfo::default()` only in `vulkan/image.rs` and `texture.rs`.
- Walk-to-point systems: the six `WalkDecision` systems share `navmesh_path::write_back_walk` (#5580); no new copy.
- coord.rs one-home: no reimplementation of `zup_to_yup_*` outside `crates/core/src/math/coord.rs` found in the new nif collision / ragdoll code (`np_ragdoll.rs`, `ragdoll.rs` call it).
- ESM sub-record scaffolding for the new `CHAL` / `MUSC` records: both use `CommonNamedFields` and `SubReader`; only the raw reads above.
- `ShaderTypeFields` (core) vs `MaterialInfo` fields (nif) and `WaterParams` (core vs plugin): mirrored by design at the NIFAL / WATAL translation boundary, documented; not filed.
- `core::ecs::metrics` vs `debug-protocol`: the one-per-side copy is deliberate (#5822).
- Graphics-pipeline fixed-function state (input assembly / viewport / rasteriser) is nine independent pipelines with different values; not a candidate.


---

## Dimension 3 — Stale Documentation & Comments

# Dimension 3 — Stale Documentation & Comments (2026-10-19, HEAD 12677d718, depth=deep)

Counts: 0 CRITICAL / 0 HIGH / 1 MEDIUM / 11 LOW = 12 findings. `_audit-validate.sh`: 0 STALE refs (3352 refs, 122 skill files; Dim 4 count = 0), 37 deleted-basename advisories, 343 docs/engine symbol advisories, 1 workspace-tree advisory (`CRHoldExpansion`, a Skyrim SM node name = data, italicise).
Prior (10-15) Dim 3 had 13 findings: 11 fixed (01,03,04,05,06,07,08,10,11,12,13), 2 still live (02 narrowed to one site -> TD3-2026-10-19-10; 09 worsened -> -03). New this run: 10. Cause of the new ones: the #5800/#5801/#5804 splits (vm.rs, translate/effects.rs, condition.rs became 136/177/224-line hubs) and the 10-15 CLAUDE.md tree rewrite, which shifted entries under the wrong parent.
Notable: GPU-struct sizes are CLEAN (Vertex 104, GpuWaterParams 368, GpuCamera 368, GpuMaterial 432, GpuFogVolume 96, GpuRayBudget 68 all match layout tests/definitions). ecs.md stage order, shader-pipeline.md Set 1 binding table (0-23), memory-budget.md constants all verified.

## MEDIUM

### TD3-2026-10-19-01: `docs/feature-matrix.md` (the "status floor") contradicts shipped code and itself in five rows
- Severity: MEDIUM (stale status doc that audit-oblivion / audit-gameplay / audit-scripting read as a floor; promoted per the stale-baseline row, demote to LOW if no audit is shown to have been misled)
- Dimension: 3 · Effort: small · Status: NEW (the 10-15 TD3-05 fixes landed; this is a different set of rows)
- Locations: `docs/feature-matrix.md:341` (heading "live gaps as of 2026-08-19"), `:350`, `:358`, `:238`, `:215-216`
- Description / Evidence:
  - `:350` "Oblivion exterior (TES4 worldspace + LAND)" is a gap, while `:21-26` and the paragraph at `:28-30` say it shipped (closed by #2368, 2026-08-12).
  - `:358` "UV scroll animated materials ... audited, not prioritised" vs `:130,134` (`AnimatedUvTransform` written by `animation_system`, consumed in `render/`, shipped).
  - `:238` "Authored attack/hit/death animation + sound | X | Open - combat sound tracked as #4747": `byroredux/src/systems/combat_anim.rs` (added `ec3a18d2f`, 2026-09-21, "Wire the P2 combat tail: Draugr attack/hit/death takes plus combat sounds") ships both; and #4747 is an unrelated open audio-docs issue (REGN music status). Wrong issue number + wrong status.
  - `:216` "Created-object spawning from aliases | X": `crates/scripting/src/created_references.rs` (`eeb6793b7` 2026-10-12 "Created Object 'At' aliases spawn their references", `3299c7dcb` PlaceAtMe) ships it. `:215` "Reference collections ... X": script-filled `RefCollectionAlias` shipped (`alias_collections.rs`, 2026-10-13); only condition/fill-type-filled collections are missing.
  - Heading date 2026-08-19 is two months old.
- Proposed fix: delete rows `:350` and `:358`; flip `:238` to partial/shipped for Draugr-family (cite `combat_anim.rs`, drop #4747); split `:215-216` into shipped (script-driven collections, created-object "At") vs open (condition-filled collections, LCTN); re-date the heading.

## LOW

### TD3-2026-10-19-02: ROADMAP Known Issues lists two fixed items as open (one is an open "HIGH")
- Severity: LOW · Effort: trivial · Status: NEW
- Locations: `ROADMAP.md:646-649` (ReSTIR claim reads a `writeonly` binding, #5765, HIGH), `ROADMAP.md:650-653` (FO76 `GeneratedMeshes` truncation tail); inconsistency also at `ROADMAP.md:720` ("re-measured 2026-09-29") vs `:219`/`:719` ("2026-10-03")
- Evidence: #5765 CLOSED 2026-10-15; `a03cf9174` added the `ReservoirClaimBuffer` (binding 22); `triangle.frag:4233` only stores `reservoirsCurr`; `bindings.glsl:613-618` and `shader-pipeline.md:~742` describe the fix. FO76: `crates/nif/tests/parse_real_nifs.rs:555-558` ("#4628 ... floors were re-tightened from 0.945/0.0"), #3461 and #4628 CLOSED, `ROADMAP.md:219` already says 102 980 clean. The item's own "then raise the 0.0 floor" is done.
- Proposed fix: delete both checkboxes (move #5765 to a Completed one-liner); change `:720` date to 2026-10-03.

### TD3-2026-10-19-03: ROADMAP "Growth discipline - Tripwire today" names three files that are now 136-224-line hubs and understates five others (carried from 10-15 TD3-09, worse)
- Severity: LOW · Effort: trivial · Status: STILL LIVE (worsened by #5800/#5801/#5804)
- Location: `ROADMAP.md:446-452`
- Evidence: `wc -l` today: `translate/effects.rs` 177 (claimed 10 117), `fragment/vm.rs` 136 (6 215), `condition.rs` 224 (3 691) - split into `translate/effects/*`, `fragment/vm/*`, `condition/*`. Others drifted: `physics/world/mod.rs` 5 491 (5 209), `material_translate.rs` 4 468 (4 422), `inventory.rs` 4 337 (4 202), `volumetrics.rs` 4 281 (4 168), `physics/water.rs` 4 093 (3 963), `esm/records/misc/world.rs` 3 508 (3 508). The "re-measured 2026-10-15" stamp is four days old and wrong for 8 of 9 rows.
- Proposed fix: regenerate the list from `git ls-files '*.rs' | xargs wc -l` with the >3500 cut (today's over-the-line production files: those six; test-only files `shader_contract_tests.rs` 7 606, `fragment/tests.rs` 6 595, `parse_real_esm.rs` 6 082, `vm/tests.rs` 5 867, `effects/tests.rs` 5 337, `extensions/tests.rs` 4 863 are the test bucket), or replace the numbers with the command.

### TD3-2026-10-19-04: `docs/engine/architecture.md` says the workspace is "27 members"; it is 34
- Severity: LOW · Effort: trivial · Status: NEW
- Location: `docs/engine/architecture.md:17-22` (count + tree)
- Evidence: `Cargo.toml` members = 34 (29 crates + `byroredux` + 4 tools; ROADMAP `:716` states the same). The doc's tree omits crates `menuxml`, `game-detect`, `boot-request`, `settings-io`, `sdk` and tools `byro-detect`, `byro-launcher`; it says "two tools".
- Proposed fix: update the sentence and tree (copy the roster from `CLAUDE.md:300-372`), or replace the count with "see ROADMAP Project Stats".

### TD3-2026-10-19-05: CLAUDE.md Workspace Structure tree has three mis-nested entry groups, a stale module list and a missing-directory gap (after the 10-15 rewrite)
- Severity: LOW · Effort: trivial (user-owned file: needs the user's edit) · Status: NEW (regression introduced while fixing 10-15 TD3-10)
- Locations: `CLAUDE.md:327-340` (`spt/`), `:355-366` (`debug-ui`/`hkx`/`debug-server`), `:368-374` (`tools/`), `:135-136`
- Evidence (resolved with a path-walk of the tree block against the checkout):
  - Under `spt/` sit `src/parse.rs eval.rs layout.rs raster.rs tex.rs font.rs menu.rs profile.rs` and `examples/render_hud.rs`; none exist in `crates/spt/src` (it has `parser.rs scene.rs stream.rs tag.rs version.rs import/ recon/`). They are `crates/menuxml/src/*` and `crates/menuxml/examples/render_hud.rs`; `menuxml/` (`:319`) has no children listed.
  - Under `hkx/` sit `src/lib.rs`, `listener.rs`, `system.rs`, `evaluator.rs`, `registration.rs` (the DebugDrainSystem / register_component text); those are `crates/debug-server/src/*`. `crates/hkx/src` actually has `animation.rs behavior.rs packfile.rs expression.rs`.
  - Under `texture-upscale/` sit `src/main.rs`, `src/display.rs` (TCP client / REPL); those are `tools/byro-dbg/src/*`.
  - `:135-136` lists 9 domain modules for `components/`; there are 12 (`explicit_start`, `projectile_muzzle`, `game_time` unlisted). Hub is 89 lines (31 production) - "~30 LOC" is fine.
  - `byroredux/src/` directories `boot/` (cli, registries, schedule/*), `streaming/`, `save_io/`, `npc_spawn/`, `scene/`, `systems/` (45 files) are not in the tree; the tree is described as a map, so this is informational.
- Proposed fix: move the three child groups under `menuxml/`, `debug-server/`, `byro-dbg/`; add the three components modules; optionally add `boot/`, `streaming/`, `save_io/` one-liners.

### TD3-2026-10-19-06: CLAUDE.md smoke-test list is incomplete ("Currently:" names 19 of 35 scripts)
- Severity: LOW · Effort: trivial (user-owned file) · Status: NEW
- Location: `CLAUDE.md:51-110` vs `docs/smoke-tests/`
- Evidence: on disk, absent from CLAUDE.md: `interior-godrays.sh m34-day-night.sh m41-ragdoll.sh m43-quest-runtime.sh m47-3-obscript.sh m47-triggers.sh m48-5-fnv-hud.sh m-exteriors.sh m-trees.sh p0-door-interaction.sh p1-character-traversal.sh p2-melee-core.sh p3-hud.sh p3-player-body.sh p5-save-restart.sh r6a_stale_15_bench.sh` (16). Every script on disk IS in `docs/smoke-tests/README.md` (checked), and everything CLAUDE.md names exists.
- Proposed fix: replace the enumeration with "see `docs/smoke-tests/README.md` (every script is indexed there)" and keep only the pattern description; avoids the list drifting again.

### TD3-2026-10-19-07: 18 `file.rs:LINE` references point past the end of their (now split) target
- Severity: LOW · Effort: small · Status: NEW (the split wave; `components.rs` 89 lines, `cell_loader.rs` 643, `save_io.rs` 107, `condition.rs` 224, `records/mod.rs` 169, `cell/mod.rs` 198, `material/walker.rs` 194)
- Evidence (out-of-range detector, `git ls-files` x regex, unique basenames only; in-range-but-wrong is invisible, so this is a floor):
  - docs: `dependency-update-followups-plan.md:405` (`components.rs:1241`), `exterior-grid-streaming.md:199` (`components.rs:968`; also `:174` `components.rs:60-73`), `exal-groundcover.md:102` (`components.rs#L914`), `m47-3-quest-alias-design.md:55` (`condition.rs:326`; fn now `condition/eval.rs:171`), `npc-spawn-ai-packages.md:129` (`records/mod.rs:587`), `per-game-translation-survey.md:142,177` (`walker.rs:172-257`, `:331-345`), `save-load-roundtrip.md:35,47` (`save_io.rs:783`, `:223`), `exterior-readiness-plan.md:898` (`cell/mod.rs:1271-1316`), `sdk-v0.1-next-action-plan.md:230` (`compatibility.rs:4252`).
  - source comments: `parsed_nif_cache.rs:70` (`cell_loader.rs:1322`), `variant_pack_gating_tests.rs:224` (`render.rs:603`), `nif/import/material/legacy_properties.rs:308-309` (`render.rs:573`, `cell_loader.rs:2463`), `texture_slot_3_4_5_tests.rs:1653`, `plugin/src/esm/cell/wrld.rs:14` (`cell_loader.rs:778`), `renderer/src/texture_registry_tests.rs:60` (`cell_loader.rs:945`), `plugin/tests/parse_real_esm.rs:1619`. Also `render/mod.rs:588,618` and `scene/procedural_fallback_tests.rs:6` cite `systems.rs:1446/1437-1442/1136` (`systems.rs` is 88 lines; `weather_system` is `systems/weather.rs:789`).
- Proposed fix: replace each with a symbol name (`cell_loader::...::fn`) - line numbers into split files will rot again. A hygiene test over `\.rs:\d+` against `wc -l` (my 40-line script) would hold the class.

### TD3-2026-10-19-08: docs/engine names renamed/split symbols and paths as live
- Severity: LOW · Effort: trivial · Status: NEW
- Evidence / fix:
  - `scripting.md:2282` `QuestCommandHost` -> `ObScriptCommandHost` (`obscript_quests.rs:376`).
  - `archives.md:266,454` `linear_size_for` -> `pitch_or_linear_size_for` (`crates/bsa/src/ba2.rs:1187`).
  - `m47-0-design.md:113` `quest_advance_on_activate_system` -> `quest_advance_system` (`papyrus_demo/quest_advance.rs`); doc is retained history, so just rename in the table.
  - `boot.rs` (now `boot/mod.rs`): `launcher.md:618`, `npc-spawn-ai-packages.md:16`, `exterior-readiness-plan.md:712`, `charal-oblivion-ruleset.md:416`; `streaming.rs` (now `streaming/mod.rs`): `coordinate-system.md:341`. `_audit-validate.sh` lists these as deleted-basename advisories and no one clears them.
  - The other ~30 basename advisories are legitimate (data files, struck-through, rapier's `sap_axis.rs`) - italicise to retire them.

### TD3-2026-10-19-09: `npc-spawn-ai-packages.md` describes eight PACK `active_*_location/_target` selectors that #3042 deleted
- Severity: LOW · Effort: trivial · Status: NEW (doc is the authoritative reference for `/audit-gameplay`)
- Locations: `docs/engine/npc-spawn-ai-packages.md:184-193` (lists `active_sandbox_location ... active_patrol_location`, `active_follow_target`, `active_escort_target` as "independent mirror groups"), `:366-370` ("dead code tracked as #3042")
- Evidence: #3042 CLOSED 2026-08-27; `rg active_follow_target crates byroredux` -> 0 hits; what remains is the `active_package_is!` macro generating the seven `active_package_is_*` test helpers (`crates/plugin/src/esm/records/misc/pack.rs:1040-1067`). No `*_location` / `*_target` fn exists.
- Proposed fix: rewrite both passages to "the seven `active_package_is_*` helpers (test-only macro)"; drop the location/target names and the "tracked as #3042" clause.

### TD3-2026-10-19-10: `scripting.md` says no actor behavior graph runs; FO4 actors' graphs do (carried from 10-15 TD3-02, narrowed to one site)
- Severity: LOW · Effort: trivial · Status: STILL LIVE (the other four sites - `anim_events.rs`, `animation_graph.rs`, `actor_graph.rs`, `scripting.md:1475-1524` - are fixed)
- Location: `docs/engine/scripting.md:1592-1593` ("What is **not** built is a *running* actor behavior graph (variables, blends and events of `MTBehavior` are not evaluated)")
- Evidence: same file `:1544` ("graphs run too (FO4 actors carry the same `ObjectAnimationGraph` component") and `:1621` ("an `ObjectAnimationGraph` runs it with `animated_root` the actor's skeleton"); `3bb6806b5`, `49529db7c`.
- Proposed fix: reword to what is unbuilt (root motion, pose modifiers: look-at/twist/foot IK/ragdoll/jiggle per `animation_modifiers.rs`).

### TD3-2026-10-19-11: `exterior-readiness-plan.md` reports `CellRootRefIndex` / `persistent_ref_index` as landed; both were deleted in #3884
- Severity: LOW · Effort: trivial · Status: NEW
- Locations: `docs/engine/exterior-readiness-plan.md:700-716` ("§7 step 1 landed"), `:1154` (`owned_by_persistent_cell`), `:1216-1219`, `:1288-1305` (`regn_active_entries`, `ai_package_rows` pieces)
- Evidence: `git log -S CellRootRefIndex`: deleted by `9bd432f8f` (2026-09-06, "Fix #3884, fix #3885: delete the dead single-root ref index and load_interior_cell"); `rg -w CellRootRefIndex crates byroredux` -> 0; `byroredux/src/components/navmesh.rs:17` already records "deleted in #3884". Also the plan's EX-16 items (#2737, #3299) are CLOSED.
- Proposed fix: add a one-line "superseded: index family deleted in #3884" under §7 step 1 and at `:1216`; the plan is historical, so no further rewrite.

### TD3-2026-10-19-12: Gamebryo 2.3 source path points at a mount that no longer exists
- Severity: LOW · Effort: trivial (CLAUDE.md is user-owned) · Status: NEW
- Locations: `CLAUDE.md:418`, `docs/legacy/key-files.md:4`, `docs/legacy/gamebryo-2.3-architecture.md:3`
- Evidence: `/media/matias/Respaldo 2TB/...` does not exist (`/media/matias/` holds `Respaldo 4TB`, `ROMS`); the tree is at `/run/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/` (the user's memory notes the same move, 2026-04-19). Reference copies of v2.6/v3.2 are under `/mnt/data/src/reference/`.
- Proposed fix: state the path as `/run/media/matias/Respaldo 2TB/...` (or "mount point varies; look under /run/media/$USER") in all three.

## Verified clean (checks that came up empty)
- `_audit-validate.sh`: 0 STALE path refs in skills and docs/engine; no skill-symbol advisories (Dim 4 note: count 0).
- GPU struct sizes: `Vertex` 104 B (`vertex.rs:329`; prose in memory-budget.md:832, bindings.glsl:407, caustic_splat/skin_vertices comments, COMPATIBILITY.md:83 all agree), `GpuWaterParams` 368 (`water.rs:188,1071`; shader-pipeline.md:479/748, water.vert:141 agree), `GpuCamera` 368, `GpuFogVolume` 96 B (6 x vec4; memory-budget.md:527 49 168 B = 16 + 512 x 96 correct), `GpuRayBudget` 17 words/68 B, `GpuLight` 64 B, light header 4 112 B, `MAX_SKIN_SLOTS` 1 364, `MAX_TOTAL_BONES` 196 608, `MAX_MATERIALS`/`MAX_INSTANCES`/`MAX_TERRAIN_TILES`/pool caps. `attribute_descriptions()` = 9 as CLAUDE.md says.
- `docs/engine/ecs.md`: `Stage` enum + order, `PARALLEL_SYSTEMS` in `boot/schedule/mod.rs` (9 entries = "nine parallel-stage systems"), lock-order functions named (`actor_quest_trigger_is_in_sequence`, `definition_arc`, `collect_newcomers`, `pool_regen_tick_system`, `melee_damage_charal_bonus`, `animation_system_inner`, `ragdoll_writeback_system`, `validate_animation`) all exist.
- `shader-pipeline.md` Set 0/1/2 binding table is complete through binding 23 and matches `bindings.glsl`.
- ROADMAP Project Stats: 34 workspace members, 1 503 `.rs` files, 6 100 issue dirs re-count exactly; bench-of-record "515 commits stale" is 518 now (3 commits after the stated HEAD, fine).
- CLAUDE.md / README / smoke-test flags: all 47 engine `--flags` found in the doc set exist in source; all 12 named shader files and all symbols cited in the tree exist; every tree path resolves except the mis-nesting in TD3-05; `docs/smoke-tests/README.md` indexes every script; CLAUDE.md names no smoke script that is missing. The toolchain note is accurate in substance (distro `/usr/bin/cargo` is 1.97.1; rustup stable and the repo pin are 1.96.0; `rust-toolchain.toml` comment is consistent).
- Deleted-symbol sweep since 2026-10-15 (`git log -p` removed `fn|struct|enum|const|trait|type` minus re-added = 71 names): only `QuestCommandHost` (TD3-08) appears in docs; none in source doc comments. No reverts in the window, so no HISTORY entry cites reverted work. No ROADMAP milestone is marked in progress with a closed issue other than the two in TD3-02.
- docs/engine/game-compatibility.md per-game numbers match ROADMAP's matrix (Skyrim SE 33 468, FO76 102 980, Starfield 120 543, FO4 235 082).
- Prior-fixed and not re-litigated: grass-LOD stochastic story (code comments + exal-groundcover supersession note), vm.rs "Not modelled: structs", LCTN dispatch note, m42 smoke text, ci.yml unsafe count, space-run `main.rs` exemption, M42 procedure counts, CLAUDE.md toolchain note.


---

## Dimension 4 — Audit-Finding Rot

# Dim 4 (Audit-Finding Rot), 2026-10-19, HEAD 12677d718

**Counts**: 0 CRITICAL / 0 HIGH / 3 MEDIUM / 3 LOW = 6 findings, all NEW. Prior report (2026-10-15) Dim 4: 5/5 FIXED (by `88ea1a2f6` "audit-infra rot swept, symbol gate made real"): validator now prints 0 STALE and 0 skill-symbol advisories (was 7), `#5838-#5840`, `#5742`, `#5606`, `#5607` all CLOSED, and the shim descriptions re-diff clean. Not re-litigated.
**Notable**: the skills were last synced at `f4ae42a6f` (2026-10-15, `audit-sync/BASELINE`); HEAD is 338 commits / 626 files / +73.8k lines past it. The rot is all post-sync, from two events: the 10-15/16 monolith splits (`d7c47d741`, `c0de708ae`, `ef3dafe38`, `83fce5388`) and the 10-15..10-19 fix wave closing issues the skills still call open. A fresh `/audit-sync` (prep.sh) fixes TD4-01..03 in one pass.
**Process note**: I ran one `cargo test --workspace -- --list` (to size a test-count claim) before noticing the brief's CARGO_TARGET_DIR rule; it used the repo's default `./target`. Nothing else was run, no repo file was touched.

## MEDIUM

### TD4-2026-10-19-01: ~25 "known-open / do not re-file" callouts in 14 skills name issues that are now CLOSED (and several premises are fixed in the tree)
- **Severity**: MEDIUM (a "do not re-file" on a closed issue suppresses a regression finding; severity-table row "stale audit baseline that misleads")
- **Dimension**: 4 | **Effort**: small (one sweep) | **Age**: closures 2026-10-15 .. 2026-10-19, after the sync
- **Locations** (callout line -> issue, closing commit; all `gh` state CLOSED/completed unless noted):
  - `audit-nifal/SKILL.md:90` and `audit-skyrim/SKILL.md:61` #4256 (`2266f7778`, `Material::source_shader_type` now crosses the boundary)
  - `audit-audio/SKILL.md:19` #4743 (`519bb02c4`); `:18` "Oblivion voice (#5526 open)" (`fa7591ca2`, 10-16); `audit-oblivion/SKILL.md:28` #5526, #5505 (`75b921cf4`); `audit-exterior/SKILL.md:33` #5505
  - `audit-fo3/SKILL.md:29` #4772 (`fc9622f6f`)
  - #5015 closed 10-19 after `e1bd40bcf` "decode FO4 skeleton ragdolls and pose human/feral-ghoul corpses from XRGD": `audit-fnv/SKILL.md:64`, `audit-fo4/SKILL.md:92` ("FO4 corpses are still unposed (open #5015)" is now false), `audit-skyrim/SKILL.md:88`, `audit-physics/SKILL.md:80`
  - `audit-ecs/SKILL.md:37` #5032, `:127` #5713, `:253` #5714
  - `audit-fo4/SKILL.md:89` #5523; `audit-starfield/SKILL.md:77` #5533 (`bfea48f82`); `audit-parsers/SKILL.md:87` and `audit-nifal/SKILL.md:165` #5525
  - `audit-parsers/SKILL.md:61` #5009 ("`debug_load.rs`'s `resolve_nif_bytes` keeps one"): fixed `44219e881` (10-15); `resolve_nif_bytes` no longer exists in the tree
  - `audit-esm/SKILL.md:287` "instance-value reader is open tracker #5627" (`2c203278f` decodes WTHS instance values, 10-19); `:427` "#5609 (open)" (`490e76397`, 10-15)
  - `audit-scripting/SKILL.md:70-79` block headed "Known-open - cite, don't re-file (verified 2026-10-15)" lists #5682, #5752, #5787-#5790 (and #5734 as the live cause), all CLOSED; `:668` "#5790 is open ... #5789 on `%0.f`" (both closed 10-17)
  - Premise half-fixed, issue still open: #5502's "remaining half" (texture-set slot / flat-colour Parent inheritance) was fixed 10-19 by `1d7e2c6bd` ("no texture 2,049 -> 217"; the issue stays open only for the glass-coverage decision). Still written as known-open with the old 2,051/10,158 figure at `audit-parsers/SKILL.md:87`, `audit-nifal/SKILL.md:90,165`, `audit-starfield/SKILL.md:66,111`.
- **Evidence**: batch state from `gh issue list --state all --limit 9000` (6026 issues) joined against every `#N` within an open/known/still/Existing clause in `.claude/commands/` (126 candidates); each line above re-read. Verified correctly OPEN (kept): #3307, #4113, #4115, #4137, #4232, #5365, #5446, #5459, #5534, #5646, #5647, #5750, #5764, #5778.
- **Proposed fix**: rerun `/audit-sync` (the closed-issue sweep is its first step); for each line either move the issue to the "Fixed ... verify, don't re-file" list or rewrite as a regression guard. Treat #5015's FO4 and #5502's slot-inheritance claims as behaviour changes needing a re-read of the code, not just a relabel.

### TD4-2026-10-19-02: Skill layout rows and `First step` commands still name five files that were split into thin hubs, so symbol attributions are wrong and delta scoping silently reports nothing
- **Severity**: MEDIUM (same silent-miss class as closed #5473; the validator passes because the hub file still exists)
- **Dimension**: 4 | **Effort**: small | **Age**: `d7c47d741` (10-15), `c0de708ae` / `ef3dafe38` / `83fce5388` (10-16)
- **Hubs now** (prod LOC): `fragment/vm.rs` 134 (was 6213) -> `vm/{interp,checker,registrations,script_model,value,natives/}`; `translate/effects.rs` 97 (was 5295) -> `effects/{model,lower,guards,receivers,scope,inline,vm_bridge,prims/}`; `condition.rs` 123 -> `condition/{eval,catalog,context,requirement}`; `byroredux/src/save_io.rs` 86 -> `save_io/{registry,commands,live_load,player}`; `plugin/src/esm/cell/mod.rs` 68 -> `lighting.rs` etc.
- **Wrong attributions**: `audit-scripting/SKILL.md:11,36,68,90,267,282,710` (`fragment/vm.rs` "interpreter + `check_*`"), `:128,129,135` (`translate/effects.rs`; `every_effect_primitive_bounds_its_argument_count` is in `effects/tests.rs:4856`), `:340` (`condition.rs::evaluate` is `condition/eval.rs:627`); `audit-concurrency/SKILL.md:280` (test is `condition/tests.rs:1087`); `audit-save/SKILL.md:8,28,100,251,283` and `audit-gameplay/SKILL.md:86` (`build_save_registry`/`MUTABLE_DELTA_COLUMNS` in `save_io/registry.rs`, `PRE_RELOAD_RESOURCES`/`execute_pending_save_loads`/`reload_*_session` in `live_load.rs`, `SaveCommand`/`validate_form_ids` in `commands.rs`, `apply_player_pose` in `player.rs`); `audit-starfield/SKILL.md:79` (`decode_starfield_lighting_tail` is `esm/cell/lighting.rs:148`).
- **Silent-miss commands**: `audit-save/SKILL.md:101` (`git log -p -S'register_' --since=2026-10-15 -- byroredux/src/save_io.rs` returns 0 commits; the dir returns 6), `:252` (`grep 'fn validate_' ... save_io.rs` misses `commands.rs:24,79`), `:284` (`git log -- save_io.rs`: 0 since 10-16 vs 24 for the dir); `audit-scripting/SKILL.md:128` (`git log --since=2026-10-16 -- translate/effects.rs`: 1 commit vs 11 for `effects/`).
- **Proposed fix**: repoint each pathspec to the dir (`-- byroredux/src/save_io`, `-- crates/scripting/src/translate/effects`) and each symbol to its new file; add the five hubs to the "thin-dispatch file + directory pairs" list in `_audit-common.md:9` (today it names `save_io`/`components` but not `vm`, `effects`, `condition`, `esm/cell`). Prefer a one-line rule in `_audit-common.md` ("a path-scoped `git log`/`grep` on a hub file is wrong; scope to `<stem>/`") over per-site fixes.

### TD4-2026-10-19-03: `_audit-owners.md:70` owns `fragment/{vm,budget,reference}.rs`, so the split `vm/` tree (~3.8k prod LOC, the hottest file family of the last week) routes to `scripting` alone at MEDIUM
- **Severity**: MEDIUM | **Dimension**: 4 | **Effort**: trivial | **Age**: `ef3dafe38` (10-16)
- **Evidence**: `_audit-route.sh` output: `fragment/vm.rs` -> `HIGH scripting, concurrency, performance Dim 1, tech-debt Dim 1`; `fragment/vm/interp.rs`, `fragment/vm/checker.rs`, `translate/effects/lower.rs` -> `MEDIUM scripting`. `/audit-incremental` and `/audit-suite --changed|--area` therefore drop the concurrency, performance and tech-debt owners and the HIGH floor for the interpreter, whose own audit text calls out lock/budget/allocation risk (`audit-scripting/SKILL.md:282,710`).
- **Proposed fix**: change the row to `crates/scripting/src/fragment/{vm,budget,reference}` (stem form, as `byroredux/src/save_io` at line 149 already is); add `crates/scripting/src/translate/effects` to the scripting row if it should carry a floor. Re-run `_audit-route.sh` on `git ls-files crates/scripting/src/fragment/vm` to confirm HIGH.

## LOW

### TD4-2026-10-19-04: `audit-tech-debt/SKILL.md` baseline paragraph (lines 81-97) is stale in the same split-driven way the brief predicted
- **Severity**: LOW | **Dimension**: 4 | **Effort**: trivial | **Age**: written 2026-10-15 at `f4ae42a6f`
- **Evidence** (today's `baseline.txt` / `prodloc_all.txt`): ">2000 production LOC: **6**" is now **4**: `shader_constants_data.rs` 2868 (data table), `scripting/examples/mq101_conformance.rs` 2157, `audio/src/lib.rs` 2034, `nif/src/import/types.rs` 2001. The five named large files are now hubs: `fragment/vm.rs` 134 (text says 6213, "a single file"), `translate/effects.rs` 97 (5295), `save_io.rs` 86 (2136), `esm/cell/mod.rs` 68 (2108), `condition.rs` 123 (2005). Watch-list rows crossed the line instead: types.rs 1997->2001, mq101 1974->2157, audio 1977->2034 (these are Dim 1 findings). `#[ignore]` 322 -> 343, markers 24 -> 23, test-heavy >2000 84 -> 86; `allow(dead_code)` 41 and `todo!` 0 unchanged.
- **Proposed fix**: replace the measured-numbers sentence with today's snapshot, or (better, it already says "never quote") delete the per-file LOC list and keep only the directional counts plus the recipe; note the split-hub hazard from TD4-02 next to the Dim 1 recipe.

### TD4-2026-10-19-05: `.claude/agents/*.md` residuals after #5839 (still outside the validator)
- **Severity**: LOW | **Dimension**: 4 | **Effort**: small | **Age**: `88ea1a2f6` fixed the renderer facts; these were missed
- **Evidence**: `ecs-specialist.md:20` lists `resources.rs` (it is `resources/mod.rs`); `:32-33` says `transform_propagation_system` lives in `animation.rs` (it is `make_transform_propagation_system` at `crates/core/src/ecs/systems.rs:65`; only `animation_system` is in `byroredux/src/systems/animation.rs:1057`); `:50` "current workspace target is 1979+" (a `cargo test --workspace -- --list` listing shows ~11.2k test lines). `legacy-specialist.md:38` maps `m_kWorld` to `WorldTransform` (the ECS component is `GlobalTransform`; `WorldTransform` is an SDK projection type, `crates/sdk/src/projection.rs:21`), `:43` `CollisionObject` (no such component; `components/collision.rs` has `CollisionShape`/`RigidBodyData`), `:49` "34.1.1.3 (Skyrim SE)" (Skyrim SE is 20.2.0.7 / BSVER 100; 34.x is Starfield). Its header counts (NiMain 245, SDK 1,592) cannot be re-measured: see "Unverifiable".
- **Proposed fix**: point the layout bullets at `CLAUDE.md` (as `renderer-specialist.md` now does), delete the test-count invariant, fix the three legacy-table names; add `.claude/agents/*.md` to `command_files` in `_audit-validate.sh`.

### TD4-2026-10-19-06: `audit-scripting/SKILL.md:32` "~92k LOC incl. tests" for `crates/scripting/src/` is now 110.9k (+20.5%)
- **Severity**: LOW | **Dimension**: 4 | **Effort**: trivial
- **Evidence**: `find crates/scripting/src -name '*.rs' | xargs cat | wc -l` = 110,872 (the line itself says re-measure). Other sampled figures hold: `crates/plugin/src` ~73k (75.6k), `crates/save/src` ~3.2k (3,208), `crates/spt/src` ~2.8k (2,841), `byroredux/src/extensions` ~10.8k (10,662), `papyrus_provider/`+`compatibility.rs` ~7.5k (8.7k, +16%, under threshold), `cxx-bridge` 26-line lib.rs + 9-line C++ stub, `platform` 60 LOC, `GpuMaterial` 432 B, 4112 B light header, 142 Skyrim catalog entries, "default all N" dimension counts for all 23 skills that state one.
- **Proposed fix**: bump to ~111k or drop the figure.

## Verified clean
- `_audit-validate.sh`: exit 0, 3352 refs, 0 STALE, 0 skill-symbol or skill-basename advisories (remaining 37 basename / 343 symbol / 1 workspace advisories are all in `docs/engine/` or `CLAUDE.md`: Dim 3).
- Dimension cross-references: 81 `/audit-x ... Dim N` references checked against each target's actual headings; 0 broken (one regex hit was a same-skill "Dim 8 here").
- Skill `--focus` default counts match heading counts for every skill that states one; `groups.txt` still covers all 34 `audit-*` dirs once; 34/34 `.agents/skills` shim descriptions re-diff within 4 words of the canonical.
- `session-close` README/ROADMAP caps: README 585 <= 600, ROADMAP 799 <= 800 (one line of headroom; Dim 3 should note it).
- `docs/audits/` older than 90 days (384 reports before 2026-07-21): parsed 215 CRITICAL/HIGH finding headings in 105 reports and matched IDs to issue titles or in-report `#N`. Post-2026-06-07 (should have a trace): 2 untraced IDs, both fixed in the tree (`ECS-2026-06-23-01` missing `fragment/tests.rs`: file exists; `D7-4` FO3 SCPT not executed: M47.3 ObScript VM shipped). The three low-ref post-06-07 reports (`AUDIT_FNV_2026-07-15_DIM9`, `AUDIT_SAVE_2026-06-23`, `..._RENDERER_2026-07-15_DIM18`) trace to #2080, #1696 (CLOSED) and "no HIGH". Pre-06-07 (no trace expected, verified against code): 15 untraced; checked in code and fixed: NIF-009 (`extra_data.rs:136` reads `u8`), NIF-008 (`shader/legacy.rs:71` parallax fields), R-7 (`texture.rs:481` view/image before allocation), CONC-D2-NEW-01 (`draw.rs:584` holds the queue guard), SAFE-02 (`init.rs:463` propagates the error), SAFE-03 (`lock_tracker.rs` exists), MEM-002 (#26), SAFE-01 (#84), PD-02 (#332). Not individually verified: NIF-301, PERF-04-11-H3, RL-01 (premise contradicts the monitor-space colour policy), RL-02, FO4-D6-GAP-05, FO4-D8-NEW-01 (likely #1358), CONC/ECS 2026-04-12..07 and the 65 reports whose findings use non-heading formats (tables / inline) were not parsed, so "no gap" is not claimed for them.
- Struct sizes and counts re-measured in `audit-renderer`/`audit-safety`/`audit-ui`/`audit-exterior` (see TD4-06 list): all match.

## Unverifiable here
- The Gamebryo 2.3 source path (`/media/matias/Respaldo 2TB/...`, in `CLAUDE.md`, `_audit-common.md`, `legacy-lookup/SKILL.md`, `legacy-specialist.md`) does not exist on this machine today (`/media/matias` holds only `Respaldo 4TB` and `ROMS`); header-count claims and `/legacy-lookup` cannot be checked. (Correction at merge: the tree exists at `/run/media/matias/Respaldo 2TB/…`; only the documented `/media/…` prefix is stale — see TD3-2026-10-19-12. The header-count claims are therefore checkable.)
- Corpus counts in the per-game skills (FNV/FO3/Oblivion/Starfield census figures) need game data and were not re-measured.


---

## Dimensions 5+6 — Stale Markers, Stub & Placeholder Implementations

Dim 5 + Dim 6 (HEAD 12677d718, 2026-10-19): 0 HIGH / 0 MEDIUM / 3 LOW. Dim 5: 1 finding (new). Dim 6: 2 findings (new).
Prior Dim 5/6 findings: TD5-2026-10-15-01, TD6-2026-10-15-01, TD6-2026-10-15-02 are all FIXED (reworded / docs rewritten; #5841-#5843 CLOSED, no regression). Prior-live: 0.
Notable: 23 marker hits (prior 24; the scene.rs "TODO" reword landed), only 2 are real TODOs; 0 shader hits; 0 `unimplemented!`/`todo!`; the production-unreachable yield is one dead marker component and one block of parse-only typed ESM maps.

## Findings

#### TD6-2026-10-19-01: `PlayingScriptedIdle` is never inserted, so ObScript `IsIdlePlaying` (0x1070) always answers 0; its doc says the app's idle runtime adds it
- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/scripting/src/obscript_state.rs:377-383` (component + doc), `crates/scripting/src/obscript_more.rs:506-509` (the only reader); would-be writer sites `byroredux/src/systems/cinematic.rs:~101-109` (IdleResume insert) and `:~209` (IdleResume remove)
- **Age**: `6e9d55aa9`, 2026-10-18 (M47.3 close-out; ROADMAP M47.3 row says "Shipped")
- **Effort**: trivial
- **Status**: NEW
- **Description**: The doc says "the app's idle runtime adds it when the idle starts and removes it when the actor goes back to what it was doing". No code in `byroredux/` or `crates/` constructs or inserts it (`rg -nw PlayingScriptedIdle` -> the component def, `register`, the `has::<>` read at `obscript_more.rs:508`, and a save-ledger string). `IsIdlePlaying` therefore reads 0 on every actor even mid-`PlayIdle`. It is registered in the save ledger as "session-local like the IdleResume it follows", so the ledger entry also describes a writer that does not exist.
- **Evidence**: `rg -nw PlayingScriptedIdle crates byroredux --glob '*.rs'` (non-test): 4 lines, zero writes. `IdleResume` (the intended twin, `components/ai_anim.rs:35`) is written at `systems/cinematic.rs` where `PlayIdle` one-shots are installed and removed in `idle_resume_system`.
- **Proposed fix**: In `cinematic.rs`, insert `PlayingScriptedIdle` alongside each `IdleResume` insert and remove it where `idle_resume_system` removes `IdleResume` (same `done` list), or, if the getter is not wanted, drop the component and make 0x1070 decline. Add a one-line test that `IsIdlePlaying` flips during a one-shot.

#### TD6-2026-10-19-02: eight typed ESM record maps (+3 side maps) are parsed and retained on every load but have zero readers anywhere in the workspace; `EsmIndex` docs describe them in the present tense
- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/plugin/src/esm/records/index.rs:406-412` (`enchantments` ENCH), `:497-531` (`reputations` REPU "Drives the faction-reputation system and quest gating", `explosions` EXPL, `combat_styles` CSTY "per-NPC AI behavior profile", `impacts` IPCT, `impact_data_sets` IPDS, `recipes` COBJ), `body_parts` BPTD; side maps `navi_info`, `object_mod_loose_items`, `deleted_record_metadata`. Insert sites only: `dispatch_misc_gameplay_b.rs:32,149-183`, `dispatch_misc_gameplay_a.rs:94`, `dispatch_items.rs:85`, `parse.rs:460`.
- **Age**: `089ba022cf`, 2026-05-16 (#808/#809; 5 months); ENCH from #629
- **Effort**: small (doc annotation) / large (consumers)
- **Status**: NEW (same family as closed #4376 TD6-009 / #4214; those covered SPEL/MGEF/LVSP/IMGS only)
- **Description**: `rg '\.(enchantments|reputations|body_parts|recipes|combat_styles|explosions|impact_data_sets|impacts)\b'` over crates/byroredux/tools, minus tests and `dispatch_*`, returns nothing (the only non-test mention is a comment at `actor/faction.rs:180`; `explosions` is read only by `crates/plugin/examples/probe_form.rs:45`). The typed parsers (`parse_expl`, `parse_csty`, `parse_ipds`, `parse_cobj`, `parse_bptd`, `parse_repu`) run on every ESM load for FNV/FO3/Skyrim/FO4 and the maps live for the session. `docs/engine/esm-records.md:408-435` lists them as wired maps; only the 29 EDID-only stubs get the "no consumer" caveat (`:440`, `memory-budget.md:124`). Weapon enchantments (ENCH) resolve nowhere at runtime. ROADMAP M24 line lists PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF as "fully parsed"; none of these eight is claimed wired there, so no roadmap lie, only a doc-honesty gap.
- **Evidence**: grep above; `EsmIndex` field docs at `index.rs:509-531` ("Drives ...", "per-NPC AI behavior profile") with no consumer caveat.
- **Proposed fix**: Add a "parsed, no runtime consumer yet" sentence per field in `index.rs` and a typed-but-unconsumed sub-list in `esm-records.md` (mirroring the stub caveat), so a consumer author and the next audit see it. Do not delete the parsers (they back `probe_form`/census tooling and cross-ref resolution). Optionally track REPU/CSTY as the next CHARAL / AI consumers.

#### TD5-2026-10-19-01: two untracked `TODO`s in `loading_screen.rs` name unbuilt LSCR work with no issue (watch item, crosses 30 days on 2026-11-01)
- **Severity**: LOW
- **Dimension**: 5 Stale Markers
- **Location**: `byroredux/src/loading_screen.rs:10` ("Full legacy XML/NIF menu animation, TRNS \"Around Origin\" semantics, ONAM/ZNAM interactive zoom bounds and the MOD2 camera-path NIFs remain TODO"), `:46` ("authored camera path NIFs, MOD2, are a separate TODO")
- **Age**: `e609118644`, 2026-10-02 (17 days; under the 30-day triage floor, reported because it is the only real production marker and names no issue)
- **Effort**: trivial
- **Status**: NEW
- **Description**: These are the only two genuine TODOs in the tree. They are not tied to an issue or ROADMAP row (`rg "MOD2" ROADMAP.md` and an issue-title search return nothing), so when they age past six months nothing will say whether the work is wanted. Not reachable from a broken path: the LSCR cover renders with a fixed 0.3 rad elevation (`STAGE_CAMERA_ELEVATION_RAD`).
- **Evidence**: `git blame -L10,10 -L46,46` -> `e609118644` 2026-10-02.
- **Proposed fix**: File one tracking issue for the four remaining LSCR items and cite it as `(#N)` in both comments, or note them in the P6 ROADMAP row.

## Dim 5 marker triage (23 hits, identical token list on sources and `crates/renderer/shaders/`)

| Class | Hits | Verdict |
|---|---|---|
| ESM `XXXX` extended-size escape (`plugin/src/esm/reader.rs` x11, `cell/wrld.rs:206`, `records/misc/magic.rs:2116,2159`) + `sdk/.../storage_util/mod.rs:729` doc | 15 | false positive (key on content) |
| FO4 `HACK` terminal FourCC (`plugin/tests/parse_real_esm.rs:5388,5390`) | 2 | false positive |
| Quoted upstream FIXME (`bgsm/src/bgem.rs:137`, `nif/src/blocks/bs_geometry.rs:646`, `plugin/.../misc/world.rs:279` quoting OpenMW) | 3 | documents upstream, not stale |
| `TBD` with its own resolution (`records/items.rs:378`, FNV WEAP DNAM offset 20: not stored, NAM6 authoritative) | 1 | documented unknown |
| Real TODO (`loading_screen.rs:10,46`) | 2 | TD5-2026-10-19-01 |
| Shader hits (`crates/renderer/shaders/`) | 0 | clean (grep re-run with the same token list) |

## Verified clean
- `unimplemented!` / `todo!()` / `panic!("not ` in production: 0 (sole hit `fragment/vm/tests.rs:4713` is a test assertion message).
- triangle.frag third-party attribution block (GLSL-PathTracer, Copyright (c) 2019 Asif Ali, MIT; Burley citation) intact at `crates/renderer/shaders/triangle.frag:24-40`.
- Stub-comment grep (`// stub|TODO: real|placeholder|not yet`): 58 hits, all legitimate prose (history of removed stubs, placeholder-texture/billboard semantics, tests); none marks a live stub body.
- Empty trait-impl bodies in production: only trait default no-ops (`obscript_vm.rs:174,186`, `core/ecs/storage.rs:113`); none a doc-promised behaviour.
- Console commands in `byroredux/src/commands/`: no no-op/"TODO"/"not implemented" bodies; `depth.rs` unsupported-format message is a real error path.
- Code added since 2026-10-15 (ObScript VM, challenge/music, magic effects/shaders, flee/head_look/knockdown/revive/script_requests/script_queries, fuz, named/object sequences): every new `pub fn *_system` has a non-test reference and the byroredux ones are registered at `boot/schedule/update.rs:624-642,845`; all `EngineQueries` fields are installed at `systems/script_queries.rs:158-168`; `PendingVideos`/`ExitGameRequest`/`PendingMenus`/`PendingSleep`/`PendingMusic`/`PendingTextureSwaps`/`ReviveRequest`/`PendingScriptedLines`/`ai_disabled_actors`/`IgnoreCrime`/`ButtonPresses`/`HitShaderKicks`/`PendingBarterMenu` all have both a production writer and reader; `fuz_to_wav_bytes` is called from `systems/dialogue_voice.rs:280`.
- Honest-by-design dormant items re-checked and left alone: `audio` `OneShotSound` (doc at `audio/src/lib.rs:14-18` says no engine producer, #5612), `core/src/stealth.rs` (doc says unscheduled math), `AttachPoints`/`ChildAttachConnections` (doc now carries "No consumer yet (#5843)"), `RecurringUpdate` (docs rewritten to "dormant"), `Effect::is_placeholder` set incl. `AddAchievement`.
- ESM coverage vs ROADMAP: the ROADMAP compat matrix is NIF-parse-rate only (no ESM column), and its ESM line (M24 Phase 1) claims only parse-level coverage; every record it names as "fully parsed" has a reader. The only gap is the doc-honesty one in TD6-2026-10-19-02.


---

## Dimensions 7+9 — Magic Numbers, Test Hygiene

# Tech-debt audit 2026-10-19 — Dimensions 7 (Magic Numbers) + 9 (Test Hygiene), HEAD fc4a15104
Counts: HIGH 0, MEDIUM 3, LOW 4 (7 findings: TD7 x2, TD9 x5). Prior report (2026-10-15): all 6 TD7 and all 5 TD9 findings are CLOSED (#5791, #5844-#5848 / #5799, #5854-#5857); 8 of 11 re-verified fixed in code, 3 "fixed" ones are not effective in practice (TD9-01..02 below, a regression of #5799/#5854); 7 findings new.
Notable: the prior HIGH (unbounded `new T[n]` / `Array.Add` / string `+` in the Papyrus VM) IS fixed and the new ObScript VM has no step/size budget that can overflow; but every automated lane added for the ignored tests (closed-HIGH pins #5799, nightly golden #5854) targets a self-hosted runner that does not exist (runner count 0), so 344 `#[ignore]`s (322 last report, +22) still run only by hand.

## MEDIUM

#### TD9-2026-10-19-01: The lanes added to run the ignored tests (#5799 pins, #5854 golden) cannot execute — zero registered runners — so every `#[ignore]` still runs by hand
- **Severity**: MEDIUM (promotion: `#[ignore]`d tests guarding fixes of closed HIGH issues #5005/#5228/#5238/#5243/#5487/#5489 sit in no running lane). Regression of closed #5799 / #5854 / #5905 (each closed on a workflow that never runs).
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:1-6,281-283` (`closed-high-pins`, `runs-on: [self-hosted, linux, x64, byroredux-game-data]`, manual-only since `fde8feec2`), `.github/workflows/rt-correctness.yml:6-9,80-83` (`on: schedule` + `runs-on: [... byroredux-rt]`), `byroredux/tests/golden/cube_demo_60f.png`
- **Age**: `ceca6eaa8` (2026-10-15, #5799), `10f2f276b` (2026-10-15, #5854), `fde8feec2` (2026-10-17, schedule disabled on the data lane only)
- **Effort**: small (decision) / medium (runner)
- **Description**: `gh api repos/matiaszanolli/ByroRedux/actions/runners --jq .total_count` = `0`. `rt-correctness.yml` kept its nightly `schedule:`; its last 4 runs: 3 `cancelled` after 24 h in queue, 1 `queued` (39243043594) — nightly golden coverage claimed by `10f2f276b` has never produced a result, and queued runs accumulate. The `real-data-gates.yml` header and `docs/contributing.md:314-318` honestly say "run locally", but the 50-row `pins=(...)` array exists only inside the YAML, so the "local run" is a hand-copy of 50 lines, and nothing else automates it. Every other ignore (344 total: byroredux 97, plugin 94, nif 46, bsa 34, scripting 20, ...) is data/GPU gated; none is runnable on a hosted runner, so effective automated coverage of the ignored set is 0. `cube_demo_60f.png` was last regenerated 2026-09-15 (`b23c92e2f`); 121 commits have touched `crates/renderer/shaders` since, and no run could have noticed (the combustion golden was regenerated 10-15, 15 renderer commits since).
- **Evidence**: `gh run list --workflow rt-correctness.yml --limit 30` -> `cancelled/schedule x3, queued/schedule x1`; `git log --oneline $(git log -1 --format=%h -- byroredux/tests/golden/cube_demo_60f.png)..HEAD -- crates/renderer/shaders | wc -l` = 121.
- **Impact**: the closed-HIGH regression nets have only ever been exercised when an audit remembers the right `--ignored` filter; docs/skills (`audit-runtime`, `audit-nif`, `audit-scripting`) still describe these as "nightly".
- **Existing**: #5905 (closed, low; recorded the runner gap for the parsers lane only), #5799, #5854 (both closed).
- **Suggested Fix**: (1) move the pin list to a checked-in data file (`.github/real-data-pins.txt`) read by both the YAML and a new `scripts/run-real-data-pins.sh` (one process per pin, same refuse-the-no-op check) so the maintainer's local run is one command; (2) drop `schedule:` from `rt-correctness.yml` the way `fde8feec2` did for the data lane (or register the runner) so the queue stops filling; (3) have `/audit-runtime` run the golden pair + pins script and record the result; (4) regenerate or confirm `cube_demo_60f.png` on the dev box in the same pass.

#### TD9-2026-10-19-02: The `closed-high-pins` list drifted from the code: two entries name the wrong cargo lane (red on arrival), three named closed-HIGH pins are absent, and nothing on a hosted runner checks the list
- **Severity**: MEDIUM (closed-HIGH pins #5489 / #5005 / #5238 unlisted; the check is a hosted-runnable gap)
- **Dimension**: 9 - Test Hygiene
- **Location**: `.github/workflows/real-data-gates.yml:367-368` (`"bin dark_texture_role_population_matches_the_documented_census"`, `"bin real_archive_torch_meshes_surface_particle_emitters"`) vs `crates/nif/tests/translation_completeness.rs:975` and `crates/nif/tests/parse_real_nifs.rs:675`; absent: `byroredux/src/cell_loader/pkin_expansion_tests.rs:579` (`starfield_cydonia_pack_ins_instance_their_template_cells`, #5489), `byroredux/src/cell_loader/reference_state.rs:910` (`fo3_fnv_script_killed_corpses_match_the_measured_census`, #5005), `crates/plugin/tests/parse_real_esm.rs:360` (`fo3_fnv_named_npc_health_matches_the_authored_curve`, #5238)
- **Age**: `ceca6eaa8` (2026-10-15, #5799) — the 2 wrong rows were added by the #5784 follow-up in the same commit family
- **Effort**: trivial
- **Description**: the `bin` lane runs `cargo test -p byroredux --bin byroredux <title> -- --ignored`; the two `bin` rows above live in `byroredux-nif` integration tests (`translation_completeness`, `parse_real_nifs`), so on a runner they match nothing and the lane's own "matched nothing (green no-op)" check turns the job red permanently. The `nif-corpus` target list also omits `--test parse_real_nifs` (needed for the torch test). #5799's finding named three closed-HIGH pins (#5489, #5005, #5238 cases above); they are not in the 50-row array (`grep -c <name> real-data-gates.yml` = 0 for each). The line `if: github.event_name == 'schedule' || ...` on the job (`:283`) refers to a trigger the file no longer has. I verified the other 48 rows: each `fn` exists, is `#[ignore]`d, and sits in the cargo target its lane names.
- **Evidence**: `grep -rn "fn dark_texture_role_population_matches_the_documented_census" crates byroredux` -> `crates/nif/tests/translation_completeness.rs:975` only.
- **Impact**: the first run on a registered runner would fail on two stale rows while 3 HIGH pins still go unrun; and because the YAML cannot execute today nobody sees the drift.
- **Suggested Fix**: relabel the two rows `nif-corpus` and add `--test parse_real_nifs` to that lane; append the three missing pins; add a hosted-runnable test in `byroredux/src/workspace_hygiene_tests.rs` that parses the pins list (or the data file from TD9-01) and asserts each title is a `fn` carrying `#[ignore]` inside the source tree its lane names (same style as `every_ignore_attribute_carries_a_reason`); delete the dead `schedule` clause.

#### TD9-2026-10-19-03: ~35 real-data test files re-inline `/mnt/data/SteamLibrary/...` defaults and skip silently, bypassing `test_paths` and `BYROREDUX_REQUIRE_GAME_DATA` — including the #5489 and #5005 pins
- **Severity**: MEDIUM (guards closed-HIGH #5489 and #5005; also Dim 7 hardcoded constant)
- **Dimension**: 9 - Test Hygiene (and 7: an environment-specific path literal copied 127 times)
- **Location**: `byroredux/src/cell_loader/pkin_expansion_tests.rs:579-585` (Starfield; reads `BYROREDUX_STARFIELD_DATA` or `/mnt/data/SteamLibrary/...`, `eprintln!("skipping")`, `return`), `byroredux/src/cell_loader/reference_state.rs:926-947` (FO3/FNV `continue`), `byroredux/src/cell_loader/load_order.rs:1220-1230`, `byroredux/src/cell_loader/legacy_lod_index_tests.rs:35-39,91-99,127-134`, `crates/nif/tests/ragdoll_import.rs:23-33,139-146,179-186,202-212,221-230`, `crates/pex/tests/r5_fidelity.rs:155`, `byroredux/src/asset_provider/texture.rs:1136-1143`, `byroredux/src/inventory.rs` (6 literals), `byroredux/src/save_io/consumable_tests.rs` (4), `crates/audio/src/tests.rs` (7), and about 25 more
- **Age**: structural; `pkin_expansion_tests.rs` 2026-10-14 (#5489 pin), `legacy_lod_index_tests.rs` / `ragdoll_import.rs` touched 2026-10-19
- **Effort**: small (mechanical)
- **Description**: `crates/plugin/src/esm/test_paths.rs` (`pub mod`, `data_dir(env, default)` + the per-game `*_data_dir()/*_esm()`) already centralises the default install paths and makes a set override binding (#3850); `crates/hkx`, `crates/nif/tests/common`, `crates/menuxml/tests/common` and `crates/scripting/tests/common` add the `BYROREDUX_REQUIRE_GAME_DATA` fail-instead-of-skip. `grep -rn /mnt/data/SteamLibrary` finds 127 occurrences in 68 files outside `test_paths.rs` (35 of them test files, excluding examples/game-detect/boot). These tests carry their own default, a typo'd `BYROREDUX_*_DATA` override silently skips, and an absent install reports `ok` even in the strict lane (#3850's exact false green, re-grown after #5784/#5855 fixed three sites).
- **Evidence**: `let Ok(bytes) = std::fs::read(&path) else { eprintln!("[Starfield PKIN] skipping: {path:?} unavailable"); return; };` over a hardcoded `unwrap_or_else(|_| "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".into())`.
- **Impact**: moot until a lane runs them (TD9-01), then a false green on exactly the tests the lane exists for; the hardcoded path is also the dev machine's layout.
- **Existing**: #5784 and #5855 (both closed, three sites each), #3850 (closed).
- **Suggested Fix**: add `test_paths::require_or_skip(what)` (panic when `BYROREDUX_REQUIRE_GAME_DATA` is set, else eprintln) beside `data_dir`, and convert these sites to `test_paths::{fnv,fo3,skyrim_se,starfield}_data_dir()` + that helper; start with the five files above (the two closed-HIGH pins). A one-line source scan in `workspace_hygiene_tests` ("no `/mnt/data/SteamLibrary` in a test file outside `test_paths.rs`") keeps it closed.

## LOW

#### TD9-2026-10-19-04: `skeever_ragdoll_survives_six_hundred_floor_ticks` is an isolation probe in test clothing that cannot run on any machine as shipped
- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/src/ragdoll_skeever_probe_tests.rs:88-100` (`#[ignore = "real-data isolation probe (needs BYRO_SKEEVER_NIF)"]`, default `/tmp/byro-5161-probe3/skeever_skeleton.nif`), `:132-` (`BYRO_PROBE_NOFLOOR`, `BYRO_PROBE_PENETRATION`, `println!` instrumentation)
- **Age**: `441d90c2f` (2026-10-10), guards closed #5161 (labelled high-class multi-SAP/bone-pose fix)
- **Effort**: small
- **Description**: the input file lives in `/tmp` and is not in the repo or an archive (`ls /tmp/byro-5161-probe3` -> missing), so the test `panic!`s with "set BYRO_SKEEVER_NIF" on every machine that has not hand-extracted it, three env knobs steer the scenario, and it is in no pin list. It is the same shape the `no_committed_example_self_describes_as_disposable` guard exists to stop, but as a `#[test]` where the guard cannot see it.
- **Evidence**: `let Ok(nif_bytes) = std::fs::read(&path) else { panic!("set BYRO_SKEEVER_NIF to the extracted skeever skeleton (missing: {path})") };`
- **Suggested Fix**: read `skeleton.nif` of the skeever from the Skyrim archive through the shared archive helper (like `race_skeletons_match_authored_paths_on_real_skyrim_data`), drop the probe env knobs/`println!`, add it to the pins list; if #5161 already has a unit-level regression, delete this probe.

#### TD9-2026-10-19-05: `manual_bench_draw_sort_serial_vs_parallel` is a 58-line print-only benchmark in `#[test]` clothing (sibling of closed #5857)
- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/src/render/draw_sort_key_tests.rs:726-790` (`#[ignore = "manual timing bench, environment-dependent"]`, body is `eprintln!` timing rows; scan of all `#[test]` bodies finds it as the only assertion-free ignored body besides helper-wrapped false positives)
- **Age**: older than 2026-09; #5857 fixed only the VM census (`vm_cost_idle_burst_polling_runaway` now has one ceiling assertion, `vm/tests.rs:4486`) although its description named both
- **Effort**: trivial
- **Description**: nothing fails if the parallel sort regresses; the sibling `manual_bench_draw_sort_decorate_sort_undecorate` has asserts.
- **Suggested Fix**: move to `byroredux/examples/` or assert serial==parallel output order (the part that is deterministic) and keep timings printed.

#### TD7-2026-10-19-01: ObScript ledger accumulators use unchecked `+=` where the sibling `MiscStats` saturates
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (silent over/underflow)
- **Location**: `crates/scripting/src/obscript_more/state.rs:37` (`l.crime_gold += num(args, 0) as i32`), `crates/scripting/src/obscript_ledger.rs:131` (`*self.disposition_mods.entry(actor).or_default() += delta`, fed by `obscript_more/state.rs:113` `... as i32`)
- **Age**: `6e9d55aa9` (2026-10-18)
- **Effort**: trivial
- **Description**: `as i32` of the script's f32 argument saturates, so two `ModCrimeGold 2147483647` statements overflow; `profile.dev` has overflow checks (panic inside a scheduler system on the default `cargo run`), `profile.release` wraps to a negative bounty. The neighbouring `ModMiscStat` path was written correctly (`crates/scripting/src/misc_stats.rs:51`, `saturating_add`). Documented-use values (hundreds) are far below the limit, hence LOW, but both sit on mod-supplied bytecode (ObScript quest scripts).
- **Suggested Fix**: `saturating_add` in both (same one-liner as `MiscStats::increment`); a two-line test per site.

#### TD7-2026-10-19-02: 13 ESM sub-record decoders pair a bare `len() >= N` literal with hand-indexed `from_le_bytes([sub.data[0], ...])` instead of `SubReader` / `core::bytes::le_*_at`
- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers (ESM sub-record sizes; also Dim 2 duplication, reported here)
- **Location**: `crates/plugin/src/esm/records/misc/challenge.rs:85,95-96` (new, `317320437` 2026-10-18), `misc/music.rs:35`, `weather.rs:1043,1053`, `misc/effects.rs:73`, `scol.rs:163`, `misc/quest.rs:547`, `misc/dialogue.rs:693,735`, `misc/world.rs:1839,1870`, `misc/water.rs:1402`
- **Effort**: trivial
- **Description**: the gate literal (`>= 4`, `== 4`, `>= 2`) and the index literals (`[0]..[3]`) must be edited together; `SubReader::u32()/u16()/f32()` and `byroredux_core::bytes::le_u32_at` (used all over `obscript_vm.rs`) make the length check part of the read. The same files use named sizes elsewhere (`CREATURE_DATA_LEN`, `GRAS_DATA_LEN`, `WIRE_SIZE`), so the convention exists; the 2026-10-18 `challenge`/`music` modules arrived with the hand-indexed form. The prior PROJ finding (#5846) is fixed (`effects.rs` no longer re-slices `SubReader::new(&sub.data[N..])`).
- **Suggested Fix**: replace each with `SubReader::new(&sub.data).u32()` / `le_u32_at(&sub.data, 0)` guarded by `if let Some`; keep a named `*_LEN`/`WIRE_SIZE` only for `DATA` structs that are checked as a whole (`challenge.rs` `DATA` 24).

## Prior findings (diff direction)
- TD7-2026-10-15-01 (HIGH, Papyrus VM unbounded `new T[n]` / `Add` / `+`): FIXED #5791. `vm.rs:102-111` names `MAX_ARRAY_ELEMENTS = MAX_SCRIPT_ARRAY_ELEMENTS (512)` and `MAX_STRING_BYTES (4096)`; enforced in `interp.rs:744-752` (`New`, array literal), `natives/collections.rs:319-330` (`Add` uses `count > MAX - len`, `Insert`), `value.rs:423-428` (`concat_texts`, also reached from `arithmetic` Add-with-string). I read every other allocating site in `vm/`: no uncapped `vec!`/`repeat`/`resize`/`with_capacity` from script values. Int arithmetic is `wrapping_*` (documented 32-bit), division/modulo by zero stop the run.
- TD7-02 (link-chain literals): FIXED #5844 (`MAX_LINK_CHAIN_HOPS = 100`, `vm.rs:111`, used in `natives/linked_refs.rs` x6). TD7-03 (ground-cover pools): FIXED #5845 (`from_layout_bindings` at `groundcover/construct.rs:404`, `groundcover_models.rs:423`, `groundcover_bench.rs:698`). TD7-04 PROJ: FIXED #5846. TD7-05 hkx: FIXED #5847 (`MAX_SKELETON_BONES` ... `animation.rs:111-119`; the `behavior.rs` `MAX_BONES = 1024` vs 4096 reconcile is not visible in the diff, low value). TD7-06 decals: FIXED #5848 (`MATERIAL_DECAL_SLOTS` in `shader_constants_data.rs:989`, used by `ray_hit.glsl:441,516`, `triangle.frag:382`).
- TD9-2026-10-15-01 (218 ignores in no lane): closed #5799 by adding a lane that cannot run -> new TD9-01 / TD9-02. TD9-02 (golden): combustion golden regenerated `10f2f276b`; lane has no runner -> TD9-01. TD9-03 (3 scripting files skip green): FIXED #5855 (`common::require_or_skip` calls at `perk_entry_points_real_data.rs:50`, `pex_recognize_e2e.rs:43`, `location_aliases_real_data.rs`). TD9-04 (examples): FIXED #5856 (162 examples now, was 168). TD9-05: half fixed (TD9-05 above).

## Verified clean
- Dim 7 NIF version gates: `crates/nif/src/version_literal_tests.rs` scans non-test source for bare `NifVersion(0x..)` and `bsver <op> N`; my own grep (`version|bsver|user_version` compared to a bare decimal in `crates/nif/src`) hits only comments and `header.rs` pins.
- Dim 7 shader provenance: the only top-level `#define NAME value` outside the generated header are push-constant aliases (`groundcover_blade.vert`), two `pbr.glsl` AA constants, `RENDER_DEBUG_NON_PARTICIPANT_GREY` and the interaction texel-count derived defines; every shader-local `const int/uint` loop ceiling (`NUM_SAMPLES`, `NUM_RESERVOIRS`, `MAX_GLASS_INTERFACES`, `MAX_DIFFUSE_BOUNCES`, `BLUE_NOISE_TILE`, `REFLECTION_LIGHT_CANDIDATES`, groundcover `SLOT_*`) is listed in the `shader_constants.rs:216-338` provenance gate, and the groundcover counter layout is pinned by `groundcover.rs:676-700` and `:783-800`. Literal loop bounds left in GLSL are geometric (4 corners, 3 channels, 6 neighbours, 9 SH).
- Dim 7 Vulkan limits: `device.rs:690-701` derives the bindless ceiling from `maxPerStageDescriptorUpdateAfterBind...` with the named `BINDLESS_CEILING`; `MESA_MAX_3D`/`MAX_2D` in `volumetrics.rs:3076` are test constants feeding `fit_froxel_divisor_to_device`; all GPU struct size literals are `assert_eq!`/`const` pins; the only `.size(N)` push ranges are `caustic.rs:502` (8, `push_bytes`) and `sky_cube/filter.rs:70` (4, one `f32`).
- Dim 7 ObScript VM (`obscript_vm.rs`, `obscript_runtime.rs`, `obscript_effects.rs`, `obscript_more/*`): no loop/goto in the bytecode, each `next_statement` advances at least 4 bytes, nesting is capped by `MAX_OBSCRIPT_VM_NESTING = 32`, all offset math is `checked_add` or bounded by `bytes.len()`; `Inventory::add` saturates (`inventory.rs:131`); `MiscStats` saturates; `updates.rs` interval/timer clamps are documented (`MIN_GAME_TIMER_HOURS`) and NaN-safe through `f32::max`.
- Dim 9 guards: all `workspace_hygiene_tests` guards are plain `#[test]` (no `#[ignore]` in the file); `every_ignore_attribute_carries_a_reason` holds (344 ignores, every one carries a reason). Triage grep leaves 6 non-data reasons, all explained: `extender_compat_e2e.rs:32` (mod archive), `vm/tests.rs:4461` (timing census, now asserts), `ragdoll_skeever_probe_tests.rs:90` (TD9-04), three "vanilla ... on disk" reasons.
- Dim 9 feature gates: every `cfg(feature = "...")` names a declared feature (`save`, `inspect`, `dhat-heap`, `debug-server`, `parallel-scheduler`, `tracing-tracy`, `recon`; the one `byroredux` hit for `inspect` is inside a string fixture); every non-default state has a `ci.yml` lane (`spt --features recon` check + lib test, `byroredux --no-default-features` / `tracing-tracy` / `dhat-heap` checks, `core --no-default-features --lib --tests`, `nif --features dhat-heap` x3).
- Dim 9 smoke assertions: 161 `is_ok()/is_some()` assertions repo-wide (none is the only assertion of a data test); no commented-out `assert!`; scan of all `#[test]` bodies without any assert/panic/expect/`?` yields only should-panic tests, helper-wrapped asserts (`approx`), cargo-corpus wrappers and the TD9-05 bench.
- Dim 9 `golden_frames.rs`: `.capture` manifests match `CAPTURE_ARGS` (`--bench-mode renderer-static --upscaler taa`, `--combustion-lab`); flags exist in `cli_args.rs` / `cornell.rs`. Not run (no engine launch allowed).
- Dim 9 named regression tests: all test names in `audit-*/SKILL.md` that I extracted (389 candidates) resolve to an existing `fn`; the 9 that did not are fields, modules, example names or test binaries, not tests.


---

## Dimension 8 — Dead Code & Backwards-Compat Cruft

# Dimension 8 — Dead Code & Backwards-Compat Cruft (2026-10-19, HEAD 12677d718, depth=deep)

Summary: 0 HIGH / 0 MEDIUM / 5 LOW. Prior (10-15) dimension-8 set: 5 fixed (TD8-01 gate, -02 all-targets re-zeroed, -03 machete/dev-deps, -05 GC_FRAME_SERIAL, -06 import_nif doc), 1 still-live residue (TD8-04, 5 of 7 items; 2 became used), 3 new (ledger dead state, `presets` module, `_receiver_kind`) plus 1 re-accrual (all-targets, new sites).
The blocking CI clippy gate is GREEN at HEAD (`cargo clippy --workspace --keep-going -- -D warnings`, 1.96.0, zero diagnostics, every one of the 34 workspace packages linted); the renderer-only `undocumented_unsafe_blocks` re-lint is also green; `cargo machete` and `cargo machete --with-metadata` are both clean. The 10-15 red gate (renderer build script + 11 scripting lints + 11 hidden bin lints) was fixed by 251d491ef / 12677d718.
Notable: nothing is red; the remaining debt is small dead public surface and a non-blocking `--all-targets` bucket of 20 lints in test targets. The `allow(clippy::…)` reason-comment backlog is unchanged and already tracked as open #5622.

## Prior findings (dimension 8, from AUDIT_TECH_DEBT_2026-10-15)
- TD8-2026-10-15-01 (red gate): FIXED — gate output empty at HEAD (log `/tmp/audit/tech-debt/dim8_clippy.log`, ends `Finished`, 0 warning/error lines). Closed #5849, verified.
- TD8-2026-10-15-02 (`--all-targets`): FIXED for the 26 old sites (#5850), but a new wave exists — see TD8-2026-10-19-05.
- TD8-2026-10-15-03 (machete dev-deps): FIXED — `ci.yml:~80` now runs `cargo machete --with-metadata` + `git diff --exit-code Cargo.lock` (#5851); local `--with-metadata` run reports nothing, Cargo.lock untouched.
- TD8-2026-10-15-04 (#5745 zero-ref list): PARTLY live — `message_box_menu_visible` / `barter_menu_visible` are now called (`byroredux/src/systems/script_queries.rs:36-37`); five items remain, see TD8-2026-10-19-01. `ActorValues::set_permanent` already resolved (#5762 closed).
- TD8-2026-10-15-05 (`GC_FRAME_SERIAL`): FIXED — 0 references to `GC_FRAME_SERIAL`, `GROUNDCOVER_FRAME_SERIAL_BITS`, `byroGcWorley` (#5852).
- TD8-2026-10-15-06 (`import_nif` doc): FIXED — `crates/nif/src/import/mod.rs:409-413` now says "harness/example entry point (#5853)".

## Findings

#### TD8-2026-10-19-01: Five dead public items from the 10-15 list were never deleted (no open issue; #5745 closed without them)
- **Severity**: LOW — **Dimension**: 8 — **Status**: still-live (prior TD8-2026-10-15-04, residue)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1498` (`watr_to_params`, `record.params`; re-export `misc.rs:106`); `crates/scripting/src/dialogue.rs:153` (`QuestGates::conditions_of`, superseded by `shared_conditions_of` `:160`); `crates/scripting/src/fragment/alias_scripts.rs:254` (`started_quests`); `crates/scripting/src/fragment/populate.rs:506` (`populate_scene_fragments_from_pex`, only other mention is the `lib.rs:154` re-export); `crates/scripting/src/fragment/package.rs:36` (`PackageFragmentSet`, only other mention `lib.rs:173` re-export)
- **Age**: `watr_to_params` fa309d8d6 (2026-05-12); `conditions_of` 6709eb65e (10-12); `started_quests` caa81f212 (10-13)
- **Effort**: trivial
- **Description**: `git grep -nw <name> -- '*.rs'` returns only the definition (and the `pub use` line for the three re-exported ones). `watr_to_params` is a one-line field read ("adapter ... just returns the structured view"). The superseded-by-`shared_*`/`_detailed` pattern is the same one that left the first batch behind.
- **Evidence**: counts above re-run today; scripts in `/tmp/audit/tech-debt/d8w/`.
- **Proposed fix**: delete the five items and the three re-export names (`misc.rs:106`, `lib.rs:154`, `lib.rs:173`). Closed-issue cousin: #5745 (regression-adjacent, those five were added to its list in the 10-15 report but not fixed).

#### TD8-2026-10-19-02: `ObScriptLedger` stores state the engine never reads back (five fields, one with no production writer)
- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/scripting/src/obscript_ledger.rs`: `item_values` (`item_value` `:155`, zero callers anywhere; writer `obscript_more/state.rs:199` SetItemValue logs "done"), `tag_skills` getter `tag_skill` (`:188`, zero callers; `is_tag_skill` is the live reader), `known_topics` (`knows_topic` `:98`, test-only `obscript_more/tests.rs:149`; AddTopic writes it), `quest_objects` (`is_quest_object` `:114`, test-only `:151`; `state.rs:125` writes), `disposition_mods` (`disposition_mod` `:126`, test-only `tests.rs:141`; `state.rs:113` writes), `talked_to_player` (`note_talked_to_player` `:110` has NO production caller — only `tests.rs:160`; the reader `obscript_more.rs:324` GetTalkedToPC therefore can never be true outside a test)
- **Age**: 6e9d55aa9 (2026-10-18)
- **Effort**: small
- **Description**: the module doc says the ledger keeps values "nothing else in the engine owns", but for these five nothing reads them either: the commands report `done(...)` and the value is serialized into saves (FORMAT_MAJOR 112) with no effect. `talked_to_player` is the sharpest case: a getter wired to a condition with a setter nobody calls.
- **Evidence**: `git grep -nw` of each accessor excluding `obscript_ledger.rs` and test paths (table in session log); production-vs-test counts `knows_topic prod=0 test=1`, `is_quest_object 0/1`, `disposition_mod 0/1`, `item_value 0/0`, `tag_skill 0/0`, `note_talked_to_player 0/1`.
- **Proposed fix**: per field either (a) wire the consumer (dialogue topic gate, barter value lookup, disposition in greet/persuasion, call `note_talked_to_player` from the dialogue-open path) or (b) make the command decline (`Action::Declined`) per the decline-on-unmodeled rule and drop the field + getter. Public API of a workspace-internal crate, so note rather than delete the getters if a consumer is planned.

#### TD8-2026-10-19-03: `vulkan::material::presets` (five Disney presets, ~200 lines with tests) has had no consumer since it landed
- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/renderer/src/vulkan/material.rs:~854-1060` (`polished_metal` `:867`, `car_paint` `:911`, `lacquered_plastic` `:926`, `painted_matte` `:942`, `skin_wax_marble` `:958`; private `#[cfg(test)] mod tests` `:973`)
- **Age**: c09d63a6f (2026-05-23, #1251)
- **Effort**: trivial
- **Description**: `git grep -n 'presets::'` finds only the unrelated `byroredux_settings_io::presets`; every call to the five fns is inside material.rs's own test module. Doc justifies it as the citable reference table (`feedback_no_guessing`), but nothing — including the Cornell `mat.*` harness — builds a `GpuMaterial` from them.
- **Evidence**: `git grep -nw -e polished_metal -e car_paint -e lacquered_plastic -e painted_matte -e skin_wax_marble` → material.rs only.
- **Proposed fix**: either have the Cornell/`mat.*` harness use the presets (gives them a consumer) or delete the module and keep the Hyperion numbers in the doc comment where they are citable.

#### TD8-2026-10-19-04: Zero/test-only-caller `pub` helpers and one unused `_`-parameter that survived the #5800 split
- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/scripting/src/fragment/vm/checker.rs:1163` (`check_native(.., _receiver_kind: Kind, ..)`: unused in the 140-line body; all three callers `checker.rs:967,1078`, `natives/game_utility.rs:272` pass one — twice a dummy `Kind::None`; the sibling `check_forms/_location/_alias` `_statement` params are legit — they are `Self::check_*` fn-pointer table entries `checker.rs:1013-1022`); `crates/renderer/src/vulkan/allocator.rs:340` (`log_fragmentation_report`, zero callers; the `mem.frag` command uses `fragmentation_report_lines` directly, `world_info.rs:1364`); test-only callers: `crates/audio/src/lib.rs:626` (`manager_mut`), `crates/core/src/ecs/components/bsx.rs:53` (`contains_marker_children`), `scene_flags.rs:101,106` (`is_node`, `sorting_disabled`), `crates/core/src/form_id.rs:104` (`as_uuid`), `crates/game-detect/src/vdf.rs:67` (`string_map`), `crates/plugin/src/esm/records/mod.rs:86` (`CHALLENGE_FLAG_SHOW_ZERO_PROGRESS` alias, zero consumers)
- **Age**: `_receiver_kind` ef3dafe38 (2026-10-16, #5800 split); others older
- **Effort**: trivial
- **Description**: `_receiver_kind` is a pure survivor (delete the param, do not rename); the helpers are the same "reachable only by tests" class as closed #5745. `is_oil_explosion` (`fog_volume.rs:187`) is deliberately kept per AUDIT_RENDERER_2026-09-05 — not flagged. Bit-flag vocabulary consts (`shader_flags.rs`, `bsx.rs`, `challenge.rs`) are decoded-field vocabulary — not flagged.
- **Evidence**: `git grep -nw` per name; scan scripts `/tmp/audit/tech-debt/d8w/scan*.py`, outputs `zero.txt`, `testonly.txt` (the 384-row test-only list is noisy — shared method names — only the verified rows are listed here).
- **Proposed fix**: drop `_receiver_kind` from the signature and its three call sites; delete `log_fragmentation_report` and the dead items, or gate the test-only ones `#[cfg(test)]`.

#### TD8-2026-10-19-05: `--all-targets` (non-blocking CI step) re-accrued 20 lints in test/example targets
- **Severity**: LOW — **Dimension**: 8 — **Status**: re-accrual of TD8-2026-10-15-02 (#5850, closed; different sites — regression class, not same lines)
- **Location**: `field_reassign_with_default` x6: `crates/sfmaterial/src/settings.rs:279`, `crates/scripting/src/condition/tests.rs:204`, `magic_effects.rs:439,546`, `obscript_actor_values.rs:153`, `obscript_effects.rs:1575`; `items_after_test_module` x4 (bin test): `byroredux/src/asset_provider/texture.rs:1002`, `commands/quest.rs:1255`, `npc_spawn/resumable/runtime.rs:401`, `save_io/live_load.rs:193`; `empty_line_after_outer_attr` x3: `crates/scripting/tests/{perk_entry_points_real_data.rs:46,pex_recognize_e2e.rs:39,location_aliases_real_data.rs:38}`; `type_complexity` x2: `crates/hkx/src/animation.rs:2876`, `crates/plugin/src/esm/reader.rs:2109`; `needless_borrows_for_generic_args` x2: `crates/plugin/src/esm/records/actor/tests.rs:98,110`; `crates/pex/src/decompile/lower.rs:868` (`needless_update`); `crates/plugin/examples/dump_npc_subs.rs:57` (`borrow_deref_ref`); `crates/scripting/src/obscript_more/tests.rs:110` (`erasing_op`, `0x10c7 + 0`)
- **Age**: newest 6e9d55aa9 (10-18), `reader.rs`/`animation.rs` sites 10-15; four are older than 10-12 and show only because 1.96 reports them under `--all-targets` (e.g. `texture.rs:1002` 2026-06-28)
- **Effort**: small
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) finishes; 30 warning lines, 20 distinct sites, zero errors, no deny-level lint. Renderer, physics, save, ui test targets are clean. Informational (CI step is `continue-on-error`).
- **Evidence**: `/tmp/audit/tech-debt/dim8_clippy_all.log`.
- **Proposed fix**: `cargo clippy --fix --workspace --all-targets` handles most; reorder the four `items after test module`; `0x10c7 + 0` -> `0x10c7`; `type_complexity` pair gets a `type` alias.

## Already tracked
- `#[allow(clippy::…)]` without a reason: Existing #5622 (open, "28 sites"). Comment-adjacency heuristic today: 33 sites (29 production — dominated by `too_many_arguments` in `byroredux/src/cell_loader/*`, `npc_spawn/resumable/*`, `crates/renderer/src/vulkan/{caustic,bloom,composite,frame_upscaler,groundcover*}.rs`, `crates/core/src/settings/mod.rs:91`, `crates/plugin/src/esm/cell/walkers.rs:483`; plus `large_enum_variant` at `cell_loader/load.rs:525`; 4 in examples). `graph_nodes.rs:224` is the nested recursive `make` helper. Drift is +1..+5 since 10-11; no new issue needed, extend #5622's list.

## Verified clean
- `allow(dead_code)`: 41 grep hits, every production site has a live reason and is genuinely unused-by-construction: `ba2.rs:166` `end_mip` (write-only M40 reservation, cited #1761/#1049), `lock_tracker.rs:65` (`cfg_attr(not(debug_assertions))`), `query.rs:26/96/234` (RAII lock guards), `renderer/build.rs:20` (layout mirror), `scene_buffer/buffers.rs:28` (byte-copied SSBO header), `bs_tri_shape.rs:244-276` (vertex-flag vocabulary), `env_translate/mod.rs:91` (`INHERIT_MAP`), `inventory.rs:1003/1016` (#4464, forward-latent with constructor/consumer named), `interaction.rs:181` + `save_io/player.rs:68` (`cfg_attr(not(test), …)` test-only helpers). The rest are tests/examples. No stale allow found (the #3887/#3888 class is gone).
- `#[deprecated]`: none. `// removed:` breadcrumbs: none. `_unused` hits are `let _unused = r.u16()` byte-skips in record parsers (intentional) and `evict_unused_blas` (a live name).
- Feature flags: all 12 declared features have both branches reachable (`spt/recon`, `tracing-tracy`, `dhat-heap`, `debug-server`, `parallel-scheduler` both cfg arms at `scheduler.rs:499/505`, `core/save|inspect`, `scripting/save` enabled by the bin); no `cfg(feature = …)` names an undeclared feature (the one `inspect` hit in `byroredux/src` is a string fixture).
- `allow(unused_imports)` in production (`cell_loader.rs`, `scene.rs`, `nif/import/material/mod.rs`): per-block with documented reasons since #3891; the `cfg_attr(not(test), …)` form keeps the lint armed for test builds.
- `_`-prefixed params: trait-impl / fn-pointer-table / system-signature (`_dt`, `_world`, `_scope: &Scope` in the `prim_*` table, `NavigatorBackend`) conformers only, apart from `check_native` above.
- pub re-export scan (names in `pub use` with no code occurrence anywhere): only `ui/src/input.rs:9` ruffle re-exports (public API alias), `CREATURE_DATA_LEN` (used by `actor/npc.rs`), and the challenge alias in TD8-04 — no re-exports of deleted types.
- Renderer-only `-D clippy::undocumented_unsafe_blocks` re-lint (#5121): clean. Crates left unlinted by the gate: none.


---
