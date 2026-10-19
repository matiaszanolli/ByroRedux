**HEAD**: cb5368348 · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-17.md` (HEAD a071ed217) · **Audited**: Dim 4 (cell-loader attach path and reference identity), Dim 5 slice (`cell_loader/unload.rs` script hooks), Dim 6 slice (ObScript attach + the cell-loader-fed `OnLoad`/`OnReset`/`GameMode` state), plus the lock-order lane the concurrency sibling handed over; scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: none (106 commits in scope since the baseline, most of them the M47.3 ObScript wave)

**This report is AREA-SCOPED (streaming-deep), not a full-codebase scripting audit.** Do not use it as the next run's full baseline. Dims 1 (recognizer chain), 2 (fragment dispatch internals), 3 (event runtime internals), 7 (provider layer) are out of area, except for the one lock finding below. No engine launch, no source edits, no sub-agents. Scratch: `/tmp/audit/streaming-deep/scripting/` (test logs only). No private cargo target dir was used.

## Build and test state

```
cargo test -q -p byroredux-scripting                                       1168 passed; 0 failed; 9 ignored
cargo test -q -p byroredux --bin byroredux -- cell_loader::references boot::schedule   73 passed; 0 failed; 1 ignored
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-scripting --lib         1168 passed; 0 failed
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib
                                                                           scripting: 1166 passed; 2 FAILED  (4 of 4 runs, same two tests)
```

The last line is SCR-SD-01 below. The failure the concurrency sibling saw "once, not reproduced in 45 reruns" is deterministic per build configuration.

## Executive Summary

**1 HIGH, 1 MEDIUM, 1 MEDIUM extension of an open issue. 0 CRITICAL, 0 LOW. 2 NEW, 1 Existing.**

The attach path itself held up. Every identity-only stub path goes through `spawn_logical_quest_reference_with_scripts` (source pin in `references/source_pin_tests.rs`). The actor job attaches after the root is stamped and after `apply_starts_dead`. The interior job installs the ObScript tables before its references spawn (`ensure_obscript_tables`, `load.rs:556`). The exterior path populates the scene runtime in `assemble_exterior_streaming` (`world_setup.rs:1257`) before any exterior reference spawns. `OnCellLoadEvent` inserted by the post-scheduler streaming step is consumed the next Update and drained in Late, so it fires exactly once. The new defects are in what the 10-18 ObScript wave hung on the loader.

## Findings

### SCR-SD-01: `evaluate_guard_guard_free` nests `QuestStageState` then `QuestObjectiveState` in code order; every other nested site is TypeId-sorted, so the ABBA lane flips red or green with the build configuration
- **Severity**: HIGH (ECS deadlock potential floor; latent in production, see Impact)
- **Dimension**: Fragment Dispatch & Locks (lane handed over by `AUDIT_CONCURRENCY_2026-10-19.md`, which could not attribute it)
- **Location**: `crates/scripting/src/fragment/effects/evaluate.rs:614-617` (`let stages = world.try_resource::<QuestStageState>()?; let objectives = world.try_resource::<QuestObjectiveState>()?;`), against `fragment/effects/deferred.rs:496` and `:1226` and `byroredux/src/commands/quest.rs:221`, all `world.resource_2_mut::<QuestStageState, QuestObjectiveState>()`
- **Status**: NEW. Introduced by `70832c690` (2026-10-18, "Run ObScript item, combat, faction, lock and actor-value commands on the effect runtime"). Same pair as closed #3819 family 4, but that was a test-hygiene fix and does not cover this site. Not #6003 (that is `StoryManagerNodeState`) and not #5986.
- **Untrusted-Input**: No
- **Description**: `resource_2_mut` acquires in TypeId order (`world.rs:810`). `evaluate_guard_guard_free` acquires in a fixed code order, Stage then Objective. When `TypeId(QuestObjectiveState) < TypeId(QuestStageState)` in a given binary, the sorted sites record Objective to Stage and the guard-free site records Stage to Objective. The process-wide graph (`lock_tracker::record_and_check`) then holds both edges and panics in whichever test closes the cycle. TypeIds are hashes of crate identity, and a crate's identity changes with the unified feature set, so the sorted order, and with it the verdict, differs between `cargo test -p byroredux-scripting` and `cargo test -p byroredux-core -p byroredux-physics -p byroredux-scripting`. That is why it looked transient.
- **Evidence** (run at HEAD cb5368348):
  ```
  BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-scripting --lib                       -> 1168 passed
  BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux-core -p byroredux-physics -p byroredux-scripting --lib
    obscript_effects::tests::get_is_reference_compares_form_ids --- FAILED
    obscript_effects::tests::stop_combat_with_a_target_stops_only_that_fight --- FAILED
    attempted acquisition of QuestObjectiveState while holding QuestStageState ... cycle:
    QuestObjectiveState -> QuestStageState -> QuestObjectiveState
  ```
  Four runs, same two tests each time. Those two tests are the ones that route a getter through `obscript_effects.rs:1299` (`evaluate_guard_guard_free`). I grepped every other file that names both resources (`obscript_quests.rs:855-880`, `obscript_more/actions.rs:21-40`, `byroredux/src/objectives.rs`, `commands/quest.rs:436-460`): they take one guard at a time, so this is the single unsorted nesting.
- **Impact**: Production deadlock is latent. The ObScript getter runs from exclusive systems (`update.rs:491`, `:523`, `:611`, `:622`) and the sorted sites from exclusive systems and the debug drain, so no second thread holds the opposite order today. The cost is that the CI lock-order lane turns red or green on unrelated dependency or feature changes. That is the lane-blinding precedent of #4603 and the safety sibling's SAFE-2026-10-19-01.
- **Related**: #3819, #4603, #6003, #6062 (same lane, different pair), CONC-SD sibling note.
- **Suggested Fix**: Take the pair in one call that sorts by TypeId (a read counterpart of `resource_2_mut`), or copy what `evaluate_guard` needs from `QuestStageState` into a snapshot and drop that guard before taking `QuestObjectiveState`. Add a guard test that runs the guard-free getter and a `resource_2_mut` apply in one process under the lane.

### SCR-SD-02: `mark_cell_reset` drops a cell's pending `OnReset` at the first scripted reference it sees, and never drops it for a cell with none
- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript (loader-fed state)
- **Location**: `crates/scripting/src/obscript_state.rs:187-220` (`mark_cell_reset`), fed by `byroredux/src/cell_loader.rs:637` (`note_player_entered_cell`), called at cell-root creation in `load.rs:571`, `exterior.rs:1156` and `exterior.rs:2033`
- **Status**: NEW. Introduced by `d1b298921` / `da2676b70` (OnReset). No open or closed issue found (searched "OnReset", "PendingCellResets").
- **Untrusted-Input**: No
- **Description**: `note_cell_entered` pushes the cell onto `PendingCellResets` when the 72-hour (or `iHoursToRespawnCell`) expiry has passed. `mark_cell_reset` runs in Update and, per pending cell, marks every `ObScriptInstance` entity whose `CellRoot` resolves to that cell's `CellFormId`, then drops the entry if it marked at least one (`found`). It retries next frame only when it marked none. Two consequences:
  1. **Partial load.** References spawn in budgeted slices across frames (`exterior.rs:2263`, `load.rs:652`), and `stamp_cell_root_range` stamps per slice (`exterior.rs:2276`). The first frame in which any scripted reference exists in the cell consumes the entry. Scripted references spawned in later slices of the same load never receive `OnResetEvent`. The doc comment says "keeps its entry until it has references", which is the defect.
  2. **No scripted reference at all.** A cell with no `ObScriptInstance` (most exterior cells, and every cell on Skyrim/FO4, where the query is absent) never sets `found`, so the entry is re-pushed forever. Each frame rebuilds the `scripted` list and does two lookups per instance per stuck cell. When such a cell is later reloaded legitimately, the stale entry resets its scripted references although the cell was not expired on that visit.
- **Evidence**: `unresolved.push(cell)` at `obscript_state.rs:211-213`, re-extended into `PendingCellResets` at `:215-219`; nothing else drains the resource (grep: only `mark_cell_reset` and `note_cell_entered` touch it). The only test (`obscript_objects.rs:1396-1417`, `on_reset_runs_when_the_cell_expired_between_visits`) puts `CellFormId` on the scripted entity itself, spawns it before the mark, and never loads a second slice or a scriptless cell. I did not measure the stuck-entry cost on game data; it is `stuck cells x resident scripted references x 2` lookups per frame, bounded by the number of distinct cells expired in a session.
- **Impact**: `OnReset` is documented in the module as the trap and animating-object reset. On a multi-frame cell load some of those objects do not reset. In a long session (more than 72 game hours between visits) permanent per-frame work accumulates on FNV/Oblivion, and a later visit can fire a spurious reset.
- **Related**: GAME-SD-01 (script state not carried across eviction), `AUDIT_SAVE_2026-10-19.md` (`CellVisits` is saved).
- **Suggested Fix**: Key the pending entry to the cell-load completion (the job's `Complete` step raising the reset for the whole cell), or stamp `OnResetEvent` at attach time when the cell is flagged expired, instead of polling for "any scripted reference". Drop an entry whose cell root has finished loading regardless of whether it held scripted references.

### SCR-SD-03: ObScript object scripts run `GameMode` every frame and `OnLoad` on 3D-less stubs and disabled placements; #5997's proposed fix would not stop `GameMode`
- **Severity**: MEDIUM
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:176-185` (unconditional `OnCellLoadEvent`), `crates/scripting/src/obscript_objects.rs:329` (`BLOCK_ON_LOAD if loaded.contains`) and `:345` (`BLOCK_GAME_MODE => runs.push`, no residency test); stub producers `exterior.rs:393` (`spawn_logical_quest_reference_with_scripts`), disabled branch `references/mod.rs:674-690`
- **Status**: Existing: #5997 (open). That issue is filed for the Papyrus VM consumers; the ObScript consumers were added after it (`da2676b70`, `571900465`) and make it worse.
- **Untrusted-Input**: No
- **Description**: The remote persistent actors the exterior persistent-cell job stubs (`prepare_logical_actor_stubs`, `exterior.rs:429-471`) and the `placement_disabled` actors and statics get the full script attach, including `ObScriptInstance` and `OnCellLoadEvent`. `obscript_object_system` then runs their `OnLoad` once and their `GameMode` block every frame. Local GECK page `(main)/OnLoad.wiki`: the block runs "whenever the scripted object's 3D loads" and "won't run on most disabled objects, because their 3D data isn't loaded". The module doc (`obscript_objects.rs:15`) itself says `GameMode` runs "for every loaded scripted reference". A stub is by construction not loaded.
- **Evidence**: no 3D or cell-residency check in `collect_due`; stubs are `Transform` + `FormIdComponent` + `SceneAliasCandidate` only. I did not verify against the original engine whether `GameMode` of a persistent reference in an unloaded cell runs, and did not count stubs with `SCRI` on FNV data (no game data run). The `OnLoad` half is backed by the GECK page above; the `GameMode` half is a doc-versus-code divergence that needs a ruling.
- **Impact**: `OnLoad` bodies (`PlaceAtMe`, `Enable`, `MoveTo`, `AddScriptPackage`) fire at world entry on actors that have no body, and per-frame `GameMode` work scales with the persistent cell's scripted-actor count.
- **Related**: #5997, #5998 (non-primary SCOL/PKIN children have no FormID, so `collect_due` skips them at `obscript_objects.rs:350` for the ObScript path too, silently, which is benign for the ObScript runtime), ESM-SD-01 from the esm sibling (an override-moved REFR keeps its base copy, so both entities would attach and run the script).
- **Suggested Fix**: Update #5997 to name the ObScript consumers. Pass a `has_3d` flag into `attach_script_for_refr` as it proposes, and additionally gate `BLOCK_GAME_MODE` on the entity having a built body (or a loaded `CellRoot`), pending the ruling above.

## Considered and dropped

- **Per-REFR `translate_pex_detailed_with_providers`** (`attach.rs:844`): already #5999 and PERF-SD sibling; no cache exists, unchanged.
- **ObScript attach re-runs `analyze_obscript_compatibility`, `legacy_script_fingerprint`, `compile_legacy_obscript_program` per REFR** (`attach.rs:318-351`): same shape as #5999, linear in script size, estimated millisecond-scale per cell; not worth a separate finding, mention under #5999.
- **`LegacyObscriptProgram` and `ObScriptInstance` both attach to one reference** (`attach.rs:350-356`, deliberate `|`): the VM declines the extension and load-order calls the static program owns (`GetModIndex` is not modeled in `obscript_vm`), so no double side effect found.
- **`populate_scene_runtime` reinstalls the object table, cloning every `ScriptRecord`, on each interior load**: bounded, subsumed by PERF-SD-2026-10-19-01 (whole load order re-parsed per door).
- **Dedup forwards received** (character CHAR-SD-01..03, esm ESM-SD-01..03): none overlaps SCR-SD-01/02; ESM-SD-01 noted under SCR-SD-03 Related.
- **Siblings not re-reported**: GAME-SD-01 (object locals not surviving unload), GAME-SD-02/04, SAVE-SD-02 (`OnUnload` firing in the loaded session), SAFE-2026-10-19-01 (`head_look`/`magic_shader`, #6062).

## Dimension coverage

| Dim | Verdict |
|---|---|
| 4 Attach path | Held, except SCR-SD-03. Verified: stub helper pairing, once-per-reference claim, `stamp_quest_reference` before attach on every path, `EditorLocation` stamped before snapshot restore, interior/exterior table ordering, `OnCellLoadEvent` one-shot timing. |
| 5 `unload.rs` hooks | `raise_unload_events` and `retire_active_packages` run on the post-retention victim set before `release_entities_timed`. Held. |
| 6 ObScript attach | Held for attach ordering; SCR-SD-02 for the `OnReset` feed. |
| 1, 2, 3, 7 | Out of area (SCR-SD-01 is the one lane carry-over). |

## Could not check

Anything needing the engine, a Vulkan device or game data: `m47-3-obscript.sh`, `m47-triggers.sh`, `refr_script_coverage` (stub counts per game), the original-engine `GameMode` semantics for persistent references in unloaded cells.

Next step: `/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-19.md`
