# Incremental Audit — 2026-11-22

**Scope**: `--range 2b40f76b6..f9f65373a` (the working tree was clean against HEAD, so the default `--working` scope was empty; the range is every commit since the 2026-11-22 gameplay audit's HEAD). **Commits**: 33. **Files**: 133 (6,349 insertions, 311 deletions). Read-only: no repo edits, no engine launch.

## 1. Change summary

| Theme | Commits | Main paths |
|---|---|---|
| TSD-4.2 terrain patches in the TLAS (#7221) | `ff339f658`, `972875cc0`, docs `413d96645`, `f9f65373a` | `crates/renderer/src/vulkan/acceleration/{blas_terrain_patch,tlas,mod}.rs`, `crates/renderer/src/vulkan/context/*`, `crates/renderer/src/vulkan/{terrain_patch,groundcover,gpu_timers}.rs`, the ground-cover shaders + `.spv`, `shader_constants.glsl` / `shader_constants_data.rs`, `byroredux/src/{terrain_patches,commands/terrain,systems/metrics}.rs` |
| Legacy Mod Compatibility: plugin activation (#6670, PR #7239) | `285633f27`, `d497bc3e2`, `884627235`, `2f03fc4f2`, `c2dcd5a29`, `15d5d459b`, `b590fd4bb`, `1623b64f4`, merges | `crates/plugin/src/{activation,activation_tests,archive_pairing,lib}.rs`, `crates/plugin/src/esm/{raw_record,mod}.rs`, `crates/plugin/src/esm/records/provenance.rs`, `crates/core/src/form_id.rs`, `byroredux/src/{plugin_activation,commands/plugins}.rs`, `byroredux/src/boot/`, `byroredux/src/cell_loader/load_order.rs`, `docs/smoke-tests/lmc-activation.sh` |
| Gameplay audit fixes #7224–#7237 | 14 `Fix #N` commits | `byroredux/src/{npc_spawn,systems,inventory,save_io,commands}/`, `byroredux/src/{combat,bow}.rs`, `crates/core/src/ecs/components/{travel,wander,inventory}.rs` |
| Hygiene | `3e14962a7` (clippy `-D warnings`), `ace99c164` (sandbox seat pass lock order), `f15700baa` (PRD link), `fb6fd4256` (issue snapshots) | various |

## 2. Routing map

`git diff 2b40f76b6..HEAD --name-only | .claude/commands/_audit-route.sh`: 35 HIGH, 41 MEDIUM, 57 LOW-risk paths; **0 unrouted**. Owner dimensions applied (changed hunks only):

| Group | Files | Owner audits / dimensions applied |
|---|---|---|
| A — renderer | acceleration, context, terrain_patch, groundcover, gpu_timers, shaders, shader constants, terrain commands/metrics | `/audit-renderer` (RT/AS, sync, shaders, memory), `/audit-safety` (unsafe, Vulkan lifecycle), `/audit-concurrency` (GPU sync), `/audit-performance` (hot-path allocs, telemetry), `/audit-exterior` Dim 2 (terrain patches) |
| B — plugin activation | plugin activation/pairing/raw record/provenance, `form_id.rs`, boot, load order, plugins commands | `/audit-esm` (untrusted-input reading, load order), `/audit-save` Dims 1-2 (plugin identity in saves), `/audit-concurrency` Dim 7, `/audit-performance` Dim 7, `/audit-tooling` (CLI report), `/audit-ecs` Dim 5, `/audit-legacy-compat` |
| C — gameplay fixes | npc_spawn, systems, inventory, save_io, combat, commands, core components | `/audit-gameplay` (Dims 1, 2, 4, 5, 7), `/audit-save`, `/audit-concurrency` (lock order, access declarations), `/audit-ecs`, `/audit-character` Dim 4, `/audit-scripting` |

Tests run by the reviewers (pinned 1.100.0, never `--ignored`), all green: `byroredux-renderer --lib` 1,678; `byroredux --bin byroredux` 4,034 (also green under `BYRO_LOCK_ORDER_CHECK=1`); `byroredux-plugin --lib` 1,403; `byroredux-core --features inspect --lib` 1,030; `check-prd-conformance` 0 errors. Fresh `glslangValidator -V` builds of `groundcover_models.comp` / `groundcover_scatter.comp` are byte-identical to the committed `.spv`.

## 3. Findings

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 12 |

The MEDIUM (IA-C-01) was re-verified by the orchestrator against `byroredux/src/npc_spawn/ai_package.rs` (`EvaluatePackageRequest` only marks the actor due; `select_active_package` still receives the running package as current, so `package_flags::kept_past_window` holds) and the GECK *EvaluatePackage* page ("If the actor is on a package flagged as must complete or must reach location, EVP will still drop that package and force a reevaluation"). IA-C-02's ordering (drain after the cell reload) was re-verified against `live_reload_tests.rs` (`reload < discard < pose`). IA-B-05 was re-verified: both skills cite `script_winners` / `script_winner`, and no tracked Rust source defines either.

### IA-C-01: The #7224 docs say `EvaluatePackage` ends a Must Complete hold, but the ambient pass never lets it

- **Severity**: MEDIUM
- **Dimension**: gameplay Dim 5 (package flags)
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:9-12` (module doc), `:51-60` (`procedure_done` comment, `_ => false`); `byroredux/src/npc_spawn/ai_package.rs:1359-1374` (`requested`), `:1456-1458` (`select_active_package(..., runtime.active_package_form_id, ...)`)
- **Changed in**: `package_flags.rs` / commit 94e785c7e (#7224, comment + test only)
- **Status**: NEW (the premise of closed #7224 was kept by the owner; this is the second half it left behind)
- **Description**: #7224 keeps a Must Complete package whose procedure has no Done state (Sleep, Eat, Wander, Sandbox, Guard) holding past its window "until a higher-priority package, `EvaluatePackage`, or a script package ends it, as the GECK describes". The GECK page for EvaluatePackage says the opposite of what the code does: "If the actor is on a package flagged as must complete or must reach location, EVP will still drop that package and force a reevaluation of the package list" (`/mnt/data/src/reference/geckwiki/(main)/EvaluatePackage.wiki`, Notes). In the engine, `EvaluatePackageRequest` only adds the actor to `due` so the pass runs now; the winner is still `select_active_package(.., current = runtime.active_package_form_id, ..)`, whose `kept_past_window` returns true for the running Must Complete package. So EVP does not end an ambient hold. Only the script-package slot is dropped (`request_package_evaluation` -> `drop_script_package`).
- **Evidence**: `requested.contains(&actor)` is read once, at `ai_package.rs:1371`, to widen `due`; nothing passes it to `select_active_package` / `scan_with_flags` / `kept_past_window`. The new test `a_must_complete_package_with_no_done_state_holds_past_its_window` has no EVP case, so the comment's claim is unpinned.
- **Impact**: After the #7224 decision, the only GECK-documented release from a no-Done Must Complete package (a quest script or console `evp` waking a Sleep/Eat actor) does nothing; the actor stays pinned until a higher-priority package opens. The comment tells the next reader the opposite, citing the GECK as authority. FNV `VSpecialViperShadow*` Sleep/Eat packages named in #7224 are the exposed shape.
- **Related**: #7224 (closed), #6656, `crates/scripting/src/package.rs:227-247`
- **Suggested Fix**: Make an explicit EVP (not `queue_package_evaluation`, which sets up a script package and must not end it) clear the running-package hold for that pass, e.g. pass `current = None` or skip `kept_past_window` when the request came from `request_package_evaluation`; add a test beside the #7224 one. Otherwise correct the comments to say EVP does not release it.

### IA-A-01: `terrain_patch.rs` module header and `record_terrain_patches` doc and SAFETY text still describe the 4.1 frame order and "raster only"

- **Severity**: LOW
- **Dimension**: audit-safety Dim 4 (is the stated invariant true) and audit-renderer Dim 4 (documentation of recording order)
- **Location**: `crates/renderer/src/vulkan/terrain_patch.rs:10-14`; `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:589-591` and `:612-613`
- **Changed in**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` and `crates/renderer/src/vulkan/terrain_patch.rs`, `ff339f658` (the code motion happened here; the comments were left behind)
- **Status**: NEW
- **Description**: #7221 moved the patch pass ahead of `dispatch_skin_and_cluster` (and so ahead of the TLAS build and the instance upload) and put the draws in the TLAS. Three comments still say otherwise.
- **Evidence**:
  - `terrain_patch.rs:10-14`: "re-displaces each resident slot in place with `terrain_patch.comp`, between the instance upload and the main render pass ... (the host clones the base cell's draw command onto the patch mesh with `in_tlas = false`); they do not enter the TLAS until stage 4.2."
  - `dispatch_skin_and_cluster.rs:589-591`: "after the instance upload (which also copies any dirty terrain tiles the pass reads) and before the geometry pass".
  - `dispatch_skin_and_cluster.rs:612`: `// SAFETY: outside any render pass (between the instance upload and the geometry pass); ...`.
  - The actual order, in `draw.rs:192-195`: `upload_terrain_tiles_if_dirty`, `record_terrain_patches` (compute, then patch BLAS), then `dispatch_skin_and_cluster`. The terrain-tile copy no longer lives in the instance upload at all (`build_and_upload_instances.rs:1385-1391`, `:1448-1451`). The host side now sets `patch.in_tlas = base.in_tlas && state.rt` (`byroredux/src/terrain_patches.rs`).
  - The "outside any render pass" half of the SAFETY text is still true; the position parenthetical is not.
- **Impact**: A reader who trusts the SAFETY text will reason about the wrong predecessor commands (the tile copy and the TLAS build are the real neighbours). No runtime effect.
- **Related**: #7221. The `.claude/commands/audit-{renderer,concurrency,performance,safety}` skills carry the same stale 4.1 prose ("between the instance upload and the geometry pass", "raster-only, `in_tlas = false`", `QUERIES_PER_FRAME` 70); that is `/audit-sync` territory, noted here only so it is not lost.
- **Suggested Fix**: Rewrite the three comments to say "after the terrain-tile upload, before the patch BLAS and the TLAS build; patch draws enter the TLAS once their BLAS matches the frame".

### IA-A-02: The all-slots-wait rider list in `sync.rs` does not name the new patch BLAS set (shared AS refit in place, shared scratch)

- **Severity**: LOW
- **Dimension**: audit-concurrency Dim 1 (cross-frame resource rider list)
- **Location**: `crates/renderer/src/vulkan/sync.rs:97-98` (rider 11), `:693-714` (`frames_in_flight_contract_names_every_dependent_resource`); new resources at `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:113-121` (`TerrainPatchBlasSet`: one AS per mesh, one `scratch`, not per frame in flight)
- **Changed in**: `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs`, `ff339f658` (`sync.rs` was not touched)
- **Status**: NEW
- **Description**: Rider 11 records that the skinned BLAS is "refit in place ... so an earlier frame's readers must retire before the next refit", and rider 10 names the shared skin output buffer, so that a future narrowing of the all-slots fence wait (#4606 / #5365) migrates them first. The patch BLAS set has the same shape: one AS per patch mesh updated in place by `UPDATE` (`src == dst`), one scratch buffer shared by every frame in flight, and a vertex pool written in place. It is not on the list, and the pin test iterates a hand-written resource list that does not include it.
- **Evidence**:
  - `blas_terrain_patch.rs:395-410` records `UPDATE` with `.src_acceleration_structure(entry.accel)` into `entry.accel`.
  - `blas_terrain_patch.rs:130-134` (`ready_address`) and `:288-293` (the BUILD and UPDATE batch) use the one `terrain_patch_blas.scratch`.
  - `sync_and_acquire_frame` waits both slots' fences today, so there is no live hazard. The cross-frame ordering is also covered on-queue: the patch compute's entry barrier has src stages FRAGMENT|COMPUTE (`terrain_patch.rs`, `record`), its exit barrier now has dst `ACCELERATION_STRUCTURE_BUILD_KHR`, and `record_terrain_patch_blas` opens with an AS_BUILD to AS_BUILD barrier. That is a transitive chain, which I derived by reading, not by running validation.
- **Impact**: None today. If the wait is narrowed, the rider list is what tells the implementer which resources to re-derive; this set would be missed and its safety would rest only on a transitive barrier chain nobody pinned.
- **Related**: #4601, #4606, #5365.
- **Suggested Fix**: Add a rider line for the patch BLAS set and scratch (and the patch compute's in-place vertex rewrite) to `sync.rs`, and add the resource name to the pin list in `frames_in_flight_contract_names_every_dependent_resource`.

### IA-A-03: About ten small heap allocations per exterior frame in the patch BLAS record path

- **Severity**: LOW
- **Dimension**: audit-performance Dim 1 / Dim 5 (per-frame allocations in the record path)
- **Location**: `crates/renderer/src/vulkan/terrain_patch.rs:803-831` (`blas_requests` returns a fresh `Vec`); `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:264-411` (`live`, `plan_input`, the planner's returned `Vec`, `regions`, `recorded`, `geometries`, `ranges`, `infos`, `range_refs`)
- **Changed in**: `blas_terrain_patch.rs` and `terrain_patch.rs`, `ff339f658`
- **Status**: NEW
- **Description**: Every frame with at least one resident patch allocates the request `Vec` plus nine more in `record_terrain_patch_blas`. The renderer's other recording paths keep such scratch on `self.scratch.*` and shrink it through the existing shrink policy; this one does not. The cost is microseconds at 16-24 elements, so it is hygiene, not a hitch.
- **Evidence**: `Vec::with_capacity(...)` / `.collect()` sites at the lines above; none are stored on the manager or the pass. With zero requests nothing is allocated (empty `collect` does not allocate and `live` is `with_capacity(0)`), so interiors pay nothing.
- **Impact**: Allocator traffic in a hot recording path, exterior only. Not measured.
- **Related**: #2923 (hot-path discipline).
- **Suggested Fix**: Keep the vectors on `TerrainPatchBlasSet` (cleared each frame), and have `blas_requests` fill a caller-owned buffer.

### IA-B-01: `FoldProvenance` production fold leaves overridden injected forms in `injected`, contradicting its doc and the fixture path

- **Severity**: LOW
- **Dimension**: ESM / load order (audit-esm Dim 3)
- **Location**: `crates/plugin/src/esm/records/provenance.rs:44-60` (`note_plugin`), `:80-110` (`injected`, `counts`, `note_writer`)
- **Changed in**: `crates/plugin/src/esm/records/provenance.rs` (884627235, 15d5d459b)
- **Status**: NEW (part of #6670 Phase 1)
- **Description**: `injected()` is documented as "The injected forms that no later plugin overrode". `note_writer` (the fixture path) honours that: it `remove`s the form from `injected` when a second writer arrives. `note_plugin` (the production fold) does not. When a later plugin overrides a form an earlier plugin injected, the branch at `:47-58` reads `self.injected.get(&form_id)` as the definer and inserts into `overridden`, but never removes the `injected` entry.
- **Evidence**: the test `an_injected_form_names_its_injector_and_a_later_override_follows_it` folds `[0x1], [0xF00], [0xF00]`. After the fold `carriers(0xF00) == [1, 2]` and `injected` still holds `(0xF00, 1)`. The test asserts only `carriers` and `winner`, never `injected()`. By code reading `counts()` then reports `(1, 1)` where the doc implies `(1, 0)`. `summary_lines` builds its "injected" column from `injected()`, so it counts forms a later plugin overrode, while `note_writer`-built fixtures do not.
- **Impact**: the "N injected into a master's ID space" figure (fold log line and `plugins.conflicts` summary) and the per-plugin injected column over-count forms that were also overridden. Report-only today, but LMC-REC-005 is the requirement the column evidences, and the production and fixture paths disagree.
- **Related**: #6670, LMC-REC-005, `byroredux/src/commands/plugins.rs` `summary_lines`.
- **Suggested Fix**: in `note_plugin`'s overridden branch `self.injected.remove(&form_id)` after taking the definer (`winner()` already prefers `overridden`), and assert `injected()` in the test.

### IA-B-02: `diff_fields` compares raw record flags, so a compression-bit difference defeats the ITM/identical verdict (LMC-REC-006)

- **Severity**: LOW
- **Dimension**: ESM records / conflict report (audit-esm Dim 7)
- **Location**: `crates/plugin/src/esm/raw_record.rs:163-165`; consumer `byroredux/src/commands/plugins.rs` `chain_verdicts`
- **Changed in**: `crates/plugin/src/esm/raw_record.rs` (884627235)
- **Status**: NEW
- **Description**: `diff_fields` begins with `if winner.flags != other.flags { diffs.push(Flags …) }` over `header.flags` exactly as stored. That word includes `FLAG_COMPRESSED` (0x00040000, `reader.rs:20`), a storage detail of how the saving tool wrote the record, and `read_sub_records` already decompresses before the sub-record bytes are compared. An override byte-identical to the record it replaces but stored compressed by one tool and uncompressed by another yields `Flags { … }`, a non-empty diff, and is therefore reported as a change instead of "identical (ITM)". It also cannot be classified as a revert.
- **Evidence**: `chain_verdicts` treats `diffs.is_empty()` as `Identical`. PRD LMC-REC-006 defines identity over the records' values (xEdit's ITM: "values identical to the values without the edit"), not their on-disk encoding.
- **Impact**: ITMs and reverts can be under-reported for mixed-tool load orders. The output is a console/`--plugin-report` line today. The same predicate will gate the "no-op must not take ownership" rule when `merge` lands.
- **Related**: LMC-REC-006, #6670.
- **Suggested Fix**: compare `flags & !FLAG_COMPRESSED` (and any other pure-encoding bits), and keep the compressed bit out of the printed flag words.

### IA-B-03: `plugin_key` folds with Unicode `to_lowercase`, but the previous load-order key was ASCII-only, so identities of non-ASCII-uppercase plugin names change

- **Severity**: LOW
- **Dimension**: Save / identity (audit-save Dim 2; audit-legacy-compat)
- **Location**: `crates/core/src/form_id.rs:100-103` (`plugin_key`), `byroredux/src/cell_loader/load_order.rs:29-31` (`plugin_basename_lc`)
- **Changed in**: `crates/core/src/form_id.rs` (285633f27), `byroredux/src/cell_loader/load_order.rs` (884627235)
- **Status**: NEW
- **Description**: before this range `plugin_basename_lc` was `Path::file_name().to_ascii_lowercase()`, and the load order hashed those names into `PluginId` (`load_order.rs:196,1031`). `plugin_key` is `basename.to_lowercase()`, which also lowercases non-ASCII letters (the new test pins `Ünïcödé.esp` to `ünï…`). For a plugin whose name has a non-ASCII uppercase letter, the registered `PluginId` and the load-order key therefore change from `hash("Ünï.esp")` to `hash("ünï.esp")`. The commit message and docs say "a lowercase name keeps the identity it always had, so nothing saved changes". That holds for ASCII-lowercased names, not for these.
- **Evidence**: `plugin_identity_folds_case_and_path` asserts only the ASCII case against `Uuid::new_v5(.., b"skyrim.esm")`. Saved `FormIdPair.plugin` values for such a plugin no longer resolve against the new `LoadOrderIdentity`. `str::to_lowercase` also applies context-sensitive final-sigma and multi-codepoint (`İ`) rules, which NTFS-style simple case folding does not.
- **Impact**: only mods named with non-ASCII capitals (rare; most are ASCII). A save made before the change would fail to match that plugin's forms. The project accepts save-format bumps, so this is a wording and awareness item. The practical risk is the claim "nothing saved changes" being relied on.
- **Related**: LMC-ID-001, LMC-ID-009, `docs/engine/plugin-loading.md:24,157`.
- **Suggested Fix**: either state the non-ASCII exception in the doc and commit trail, or fold per-character with simple case mapping (`chars().flat_map(char::to_lowercase)` still has the İ expansion, so pick the mapping to match the target filesystems deliberately) and record the decision under LMC-ID-009.

### IA-B-04: `FoldProvenance::note_plugin` resolves the slot owner with a linear scan per record

- **Severity**: LOW
- **Dimension**: Performance (audit-performance Dim 7, parse cost)
- **Location**: `crates/plugin/src/esm/records/provenance.rs:46` (`owner_of(form_id)`), closure at `byroredux/src/cell_loader/load_order.rs:1008-1012` (`slots.iter().position(..)`)
- **Changed in**: both files (884627235)
- **Status**: NEW
- **Description**: `owner_of` is evaluated for every record a plugin carries that is not yet in `overridden` (it is computed before the `merged.record_types.contains_key` test), and the closure does `slots.iter().position(|owner| *owner == slot)`, which is O(plugins). The fold is serial, after the parallel walk. LMC-ID-004 requires profiles with no ceiling (more than 254 plugins must load).
- **Evidence**: the commit measures 0.02 s of provenance in a 0.48 s fold on 10 plugins. Cost scales as records x average owner position: at 250+ plugins and millions of mod records it grows to hundreds of millions of enum compares on the load thread. I did not measure it.
- **Impact**: seconds-scale worst case on very large profiles, load-time only. Not a per-frame cost.
- **Related**: LMC-ID-004, #6670.
- **Suggested Fix**: build a `HashMap<GlobalSlot, usize>` once (or index regular slots directly), and compute `owner` lazily only in the two branches that use it.

### IA-B-05: audit skills still cite the removed `script_winners` / `LoadOrder::script_winner`, so `_audit-validate.sh` now fails

- **Severity**: LOW
- **Dimension**: Tooling (audit infrastructure)
- **Location**: `.claude/commands/audit-fo3/SKILL.md:73`, `.claude/commands/audit-gameplay/SKILL.md:64`
- **Changed in**: `byroredux/src/cell_loader/load_order.rs` (884627235 renamed `script_winners`, `note_script_writer` and `script_winner` to `provenance`, `note_record_writer` and `record_winner`)
- **Status**: NEW
- **Description**: both skills describe the #5428 last-writer table by its old names. `git grep script_winner` finds no source hit. `bash .claude/commands/_audit-validate.sh` prints `script_winners … audit-fo3` and "FAILING".
- **Evidence**: validator output above. Historical `docs/audits/*` mentions are fine.
- **Impact**: the audit-skill gate is red. It is not in CI (`grep _audit-validate .github scripts` is empty), so it surfaces only when a skill is next edited.
- **Related**: `_audit-common.md` Path-Reference Convention.
- **Suggested Fix**: re-word both skill lines to `LoadOrder::record_winner` / `FoldProvenance` (`crates/plugin/src/esm/records/provenance.rs`), keeping the #5428 and #5304 references.

### IA-B-06: new LMC modules sit under catch-all ownership rows

- **Severity**: LOW
- **Dimension**: Tooling (audit ownership map)
- **Location**: `.claude/commands/_audit-owners.md` (no row for `byroredux/src/plugin_activation.rs`, `byroredux/src/commands/plugins.rs`, `crates/plugin/src/{activation,archive_pairing}.rs`, `crates/plugin/src/esm/{raw_record.rs,records/provenance.rs}`, `crates/core/src/form_id.rs`)
- **Changed in**: files added in d497bc3e2, 884627235 and 1623b64f4
- **Status**: NEW
- **Description**: the validator advisory lists `byroredux/src/plugin_activation (387 LOC)` as reached only by the catch-all row. `crates/plugin/` falls to the generic `esm, performance Dim 7` row (`_audit-owners.md:89`), so these untrusted-input readers and the boot/argv rewrite are not routed to audit-tooling Dim 4, audit-save Dim 2 or audit-parsers.
- **Evidence**: routing `crates/plugin/src/activation.rs` and `byroredux/src/plugin_activation.rs` through `.claude/commands/_audit-route.sh` resolves both by general prefix rows, not an LMC-specific owner.
- **Impact**: a later `/audit-incremental` routes them thinly.
- **Related**: IA-B-05.
- **Suggested Fix**: add rows such as `byroredux/src/plugin_activation.rs, commands/plugins.rs | esm, tooling Dim 4; per-game` and `crates/plugin/src/{activation,archive_pairing}.rs, esm/{raw_record,records/provenance}.rs | esm, performance Dim 7`.

### IA-C-02: The #7230 drain resets `PackageDayBlocks` after the cell reload, wiping the loaded session's own spawn-time Once Per Day notes

- **Severity**: LOW
- **Dimension**: gameplay Dim 5 / save Dim 5 (drain ordering)
- **Location**: `byroredux/src/save_io/live_load.rs:480-500` (`reset_resource::<PackageDayBlocks>`), called at `:787` after `reload_*_session`; recorder `byroredux/src/npc_spawn/ai_package.rs:1211-1214` (`note_selected` at spawn)
- **Changed in**: `save_io/live_load.rs` / 48c6ea693 (#7230)
- **Status**: NEW
- **Description**: The map is keyed by the outgoing session's EntityIds, so a reset is right; but `discard_outgoing_session_requests` runs after the reload (the test `live_load_drain_discards_the_outgoing_sessions_requests` pins `reload < discard < pose`). `apply_ai_package_behavior` calls `package_flags::note_selected` at spawn for the winner of an Once Per Day package, so actors spawned by the reload lose that block while a cold start keeps it. Whether an actor loses it depends on whether its spawn finalized before the drain (spawn is resumable and can span frames).
- **Evidence**: `once_per_day_blocked` excludes only `current != Some(package)`; with the block gone, the package can win again inside its 24 hours once it stops being the running one.
- **Impact**: Minor: an Once Per Day package may repeat within 24 game hours for actors that were already running it at load. No data loss.
- **Related**: #7230 (closed), AIP-FLG-001
- **Suggested Fix**: Prune only entries whose actor is no longer live (`world.is_alive` / spawned-in-this-session check) instead of replacing the whole map, or reset before the reload.

### IA-C-03: The widened #7230 drain guard still misses the "re-arms" / "restarts on load" wordings, so it does not "read every wording"

- **Severity**: LOW
- **Dimension**: save Dim 1 (guard coverage) / test-gap
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs` `LOAD_RESET_PHRASES` (about `:1305-1316`); unmatched reasons: `AttractionState` ("a load re-arms"), `RandomConversationState` ("a load re-arms them"), `DeadBodyState` ("a load re-arms the scan"), `JailEscapeWatch` ("a load re-observes"), `ObScriptQuestTimers` ("cadence restarts on load"), `RestSession` ("reset on live load")
- **Changed in**: 48c6ea693 (#7230)
- **Status**: NEW
- **Description**: The phrase list is nine exact lowercase `a load <verb>` strings. At least six other NOT_SAVED rows promise a load-time reset in other spellings and are checked by nothing. `RestSession` happens to be reset by the drain; `AttractionState.pairs` (EntityId triples), `RandomConversationState` (EntityId maps) and `DeadBodyState.discovered` (a `HashSet<EntityId>` pruned only against live `Dead` bodies) are the same keyed-by-outgoing-entities class that `PackageDayBlocks` was, and the drain does not name them. I did not find a concrete wrong outcome: `AttractionState` rebuilds `pairs` each pass, and `DeadBodyState` prunes on its first pass; a recycled EntityId would have to collide.
- **Evidence**: `rg` over `NOT_SAVED_BY_DESIGN` reasons for `re-arm|on load|restarts on` returns the rows above; none appears in `discard_outgoing_session_requests`.
- **Impact**: The guard's stated purpose (the next unbacked promise fails a test) is only partly met; the commit message's "reads every wording" is an overstatement.
- **Related**: #7230, #6574, #6508
- **Suggested Fix**: Match on a stem (`"load"` within a few words of a reset verb including `re-arm`, `re-observe`, `restart`, `reset`) or add the six rows to `RESET_OUTSIDE_THE_DRAIN` with their real mechanism, so a rewording cannot dodge the check.

### IA-C-04: #7225 left the same defect for Near Editor Location Guard (and the other anchor-from-current packages)

- **Severity**: LOW
- **Dimension**: gameplay Dim 5 (SIBLING check)
- **Location**: `byroredux/src/systems/guard.rs:89-90,203-211` (`resolve_anchor(.., home = current)`); `byroredux/src/npc_spawn/ai_package.rs:487-492` (`Guard { anchor_form_id: location_reference, .. }`, no PLDT type carried)
- **Changed in**: not touched by a0e45a578 (#7225)
- **Status**: NEW (sibling of closed #7225; the issue's own Suggested Fix said "and Patrol/Guard/Sandbox as needed")
- **Description**: `GuardBehavior` carries no location type. A Guard package whose PLDT is Near Editor Location (type 3) resolves its anchor to the actor's position the first time `guard_system` sees it, i.e. where an earlier package left the actor, exactly the #7225 pattern; the GECK page for Guard calls the location "where the actor stands while running the guard package". Sandbox and Patrol take their centre from the same kind of "current position" fallback.
- **Evidence**: `resolve_anchor` returns `home` (`current`) when `anchor_form_id` does not resolve; `from_package`'s Guard arm never inspects `location_anchor(package.location)`.
- **Impact**: Unmeasured. A guard handed its post by a package after being moved holds the wrong spot. I did not count vanilla Guard packages with type 3, so frequency is unverified.
- **Related**: #7225 (closed), #5391
- **Suggested Fix**: Carry `near_editor_location` on `GuardBehavior` and anchor on `EditorPlacement` as `travel_home` / the wander home now do; count vanilla Guard/Sandbox/Patrol type-3 packages first to rank it.

## 4. Missing Tests

### Renderer (group A)

- No test covers the TLAS gate in `tlas.rs` (patch mesh with no ready BLAS is skipped and not counted eligible; patch mesh with a ready BLAS takes the patch address, carries `TLAS_SBT_TAG_TERRAIN_PATCH`, and ignores the `bone_offset` branch). The pure planner and `align_size` are tested; the integration of `ready_address` into `build_tlas_instances` is not.
- No test pins the order "patch compute, then patch BLAS record, then `build_tlas`" or that the patch compute's exit barrier names `ACCELERATION_STRUCTURE_BUILD_KHR` (the barrier set is unit-invisible; the existing `build_tlas_is_preceded_by_an_unconditional_as_write_to_as_read_barrier` guard does not cover the patch BLAS record).
- `patch_square_key`, `TerrainPatchPass::blas_requests` (address arithmetic: `global_vertex_offset * size_of::<Vertex>()` and `global_index_offset * 4`) and `reserve_terrain_patch_meshes` have no unit test. A wrong stride there is a CRITICAL-class bug (wrong AS geometry), so a pure-function test of the address maths is worth the few lines. I verified by reading that both offsets are in elements (`mesh.rs:155`, same pattern as `resources.rs:571`).
- No test for the new `TimerPass::TerrainPatchBlas` bracket (every other bracket has a `*_reports_measured_duration` test); the generic row and count pins pass.
- No Rust-side pin that `TLAS_SBT_TAG_TERRAIN_PATCH` is the only non-zero SBT offset (a future second tag would collide silently); the shader side is pinned by `both_tiers_share_the_cover_trace_and_it_skips_terrain_patches`.

### Plugin activation (group B)

- **`read_plugin_header`.** No test covers a header over 1 MiB, a truncated TES4 (shorter than 24 bytes, or data_size larger than the file), or the Oblivion 20-byte variant. The reader is hand-rolled over `EsmReader`, and the 24-byte prefix over-reads 4 bytes on Oblivion.
- **`find_raw_record`.**
  - The only fixture is a flat Tes5+ `WEAP` group. There is no Oblivion variant, no nested CELL/WRLD (REFR) case, no compressed record, and no malformed-GRUP case (`total` less than the header, a child GRUP larger than its parent).
  - These are the untrusted-input paths that matter.
- **`FoldProvenance`.**
  - No test asserts `injected()` after an override (IA-B-01).
  - There is no deletion-first case. Edge: a non-owner plugin's first sight of a form is a `DELE`. The injected set skips deletes (`!deletes`), so a later override lists the slot owner as definer even if the owner never carried the form.
- **`plugin_activation`.** `activate_plugin_args` is covered only for "no `--esm`". Real-file argv rewriting (hoist plus paired flags) is exercised by the smoke gate and ignored real-data tests only. A test over generated tiny plugins (like the gate's python) would run in the default suite.
- **Non-ASCII case-folding.** There is no test for non-ASCII plugin names across `activate`, `pairs_with` and the load-order key (IA-B-03).
- **Hoist wording.** No test covers a transitive hoist's report text (see Observations).

### Gameplay fixes (group C)

- #7233: only `rerun_tree_leaf` / `record_tree_leaf` are tested directly. Nothing drives `ambient_ai_package_system` over a tree package, so the `tree` flag (`template.procedures` non-empty), the `record_tree_leaf` call on a reselect, and the clearing on a switch to a flat package are unpinned.
- #7230: only `BowState` and `CastState` are seeded in the drain test. `LevelNotices` (the one user-visible bug), `StanceTimers`, `MineClock` and `PackageDayBlocks` are covered only by the name-presence guard, not by behaviour.
- #7224: no test for an `EvaluatePackage` request against a no-Done Must Complete package (see IA-C-01).

## Observations (not findings)

### Renderer (group A)

Verified OK (so the next reader does not redo them):
- **Build vs refit.** `TERRAIN_PATCH_BLAS_FLAGS` = `PREFER_FAST_BUILD | ALLOW_UPDATE` is used for the size query, the BUILD and the UPDATE alike (`blas_terrain_patch.rs:38-42`, `:151`, `:372`), so VUID-03667 (UPDATE needs ALLOW_UPDATE on the source build) holds. A refit only happens when `content` matches the last build including both device addresses; counts are enforced before planning (`:238-250`), so UPDATE geometry counts, vertex format, `max_vertex`, index type and `first_vertex` are identical to the build.
- **Scratch.** Regions are `align_size(need, scratch_align)` multiples, the base address is rounded with `align_scratch_address`, and the buffer is sized `total + scratch_alignment_padding(align)`, the same contract as the skinned path. `need` is the build or update size from the BUILD-mode size query, per op. A grown scratch retires the old buffer through `pending_destroy_scratch` (#1782 pattern) and is replaced before the old one retires.
- **Barriers.** Compute exit barrier: SHADER_WRITE to SHADER_READ at AS_BUILD (correct for AS-build geometry input, audit-renderer Dim 4 checklist). Before the batch: AS_WRITE to AS_READ|AS_WRITE at AS_BUILD. After the batch: AS_WRITE to AS_READ at AS_BUILD|FRAGMENT|COMPUTE, covering the TLAS build and every ray query. The unconditional frame-scope barrier before `build_tlas` is unchanged. Input usage flags (`ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | SHADER_DEVICE_ADDRESS`) are on the global buffers when RT is on (`mesh/geometry_ssbo.rs:341`, `:551`), and `accel` being `Some` is the same gate.
- **In-flight reads and destroy order.** Frames serialize on the all-slots fence wait (`sync.rs`). The patch BLAS are session-lived and destroyed at the end of `AccelerationManager::destroy` after `drain_pending_destroys` (which drains the grown-scratch queue) and after the TLAS, under the existing `device_wait_idle`. A failed `create_acceleration_structure` frees its buffer.
- **TLAS.** A deferred or failed patch is `continue`d before `eligible_instances += 1`, so `rt.integrity` (emitted == eligible) stays clean (the doc reports the same). The BUILD-vs-UPDATE decision keys on the address list, so a patch entering or leaving the TLAS forces a BUILD. The instance custom index still comes from the shared instance map. SBT offset: the field is a 24-bit low half of `Packed24_8` (`new(tag, flags)`), unused by ray queries, read only by the new `gcRootCovered`.
- **Geometry registry moves.** Patch BLAS addresses are re-derived each frame from the registry (`blas_requests`), per the audit-safety note ("re-resolved at record time, never cached"). A pool rebuild changes `vertex_address`, which forces a BUILD; a BLAS is self-contained so a retired vertex buffer does not dangle it.
- **Constants and shaders.** `TLAS_SBT_TAG_TERRAIN_PATCH` is mirrored in `shader_constants_data.rs` and the generated header and the sync test passes; the committed `.spv` match a fresh compile exactly. `gcRootCovered` uses `gl_RayFlagsNoOpaqueEXT` and confirms every candidate except tagged patches, which reproduces the old Opaque trace for everything else (the ray flag overrides the BLAS `OPAQUE` geometry flag). `rayQueryGetIntersectionInstanceShaderBindingTableRecordOffsetEXT` is the correct GLSL accessor and nothing else reads that field.
- **Telemetry.** `QUERIES_PER_FRAME` 70 to 72 is consistent with the const-assert, the doc table, bit 35 of the `u64` mask (no collision with the geometry-phase bits 25-29), `BENCH_GPU_KEYS` / `bench_gpu_samples` (`byroredux/src/main.rs`), `metrics.rs`, `telemetry.rs` and `SkinCoverageStats`. The new bracket's timestamp stage is AS_BUILD (same as `BlasRefit` end).
- **`terrain.patches rt off`.** Changes only `in_tlas` on the patch draws; the BLAS refit keeps running, which is what the §8.3 A/B expects.

Unverified or judgment calls (not findings):
1. **A patch at the 600-refit limit that loses the 2-build-per-frame budget leaves the TLAS** until built, though a refit would still be valid. Each patch hits the limit at its own frame (it was built then), so collisions need a simultaneous reassignment; I did not see a clumping scenario in code. A refit-while-waiting fallback for the limit case would avoid the dropout. Not measured.
2. **GPU/VRAM cost of the patch BLAS is not budgeted.** Up to 48 AS plus a scratch buffer stay resident; they count in `total_blas_bytes` but not `static_blas_bytes`, so `blas_budget_for_heap` does not account for them, and `memory-budget.md` has no row. The actual size is only visible at runtime (`terrain.patches` `blas: ... bytes=`); I could not measure it. By the same logic as skinned BLAS this is probably acceptable, but it is unbilled.
3. **Groundcover cost of `NoOpaque`.** The cover trace now runs the candidate loop for every triangle along the span instead of resolving as opaque in hardware. The commit docs measure the patch premium but not the `groundcover_scatter` / `groundcover_models` brackets before and after. Compare `gpu_groundcover_scatter` and `gpu_groundcover_models` across the change in a runtime audit.
4. **Entry-creation failure is retried and warned every frame** (`blas_terrain_patch.rs:284-288`): an OOM during sizing gives up to 48 warns per frame with no backoff.
5. **A recorded-but-unsubmitted frame** leaves `entry.content` claiming a build that never ran (the commit-after-submit latch discipline used elsewhere, #3991, is not applied here). The effect is a refit over stale geometry next frame, which is still valid Vulkan and self-corrects on the next square change; it only degrades BVH quality briefly.
6. **#7221's title** ("one multi-geometry BLAS") no longer matches the implementation (one BLAS per patch mesh, batched into one build call); terrain-displacement.md §7.2 explains why. Retitle before closing.
7. **Cosmetic**: `groundcover_scatter.comp` has a mis-indented comment line (`// gcRootCovered ...`) after the include refactor; `TLAS_SBT_TAG_TERRAIN_PATCH` sits inside the `TERRAIN_PATCH_*` run of `shader_constants.glsl`, unrelated to patch geometry.
8. The terrain-tile upload (and its dirty-flag consume) now happens earlier in the frame than before, which if a later step returns `Err` before `queue_submit` consumes the dirty flag with an unsubmitted copy. That window existed before (record-time consume) and is not widened in kind.

### Plugin activation (group B)

- **Hoist report wording.** The line reads "now loads ahead of X, which declares it as a master". For a transitively hoisted master (profile `A(dep C), C(dep B), B`, where B is hoisted "ahead of A") A does not itself declare B. `Hoist::ahead_of` documents this correctly ("directly or through another hoisted master"). The same wording feeds the `--esm must load last` error.
- **TES4 size cap.** `read_plugin_header` refuses a TES4 record over 1 MiB and marks the plugin unreadable. The largest header among the 181 installed plugins is 612 KB (FO76) and 107 KB for non-FO76, so there is headroom. A merged plugin with a very large `ONAM` list would be disabled with a clear message rather than loaded.
- **`Engine.esm` fallback.** `synth_child.rs:59,830` hashes the fallback `"Engine.esm"` and now gets the `engine.esm` identity. The old mixed-case hash is gone. It applies only to quest-reference placements whose form ID has no owning plugin, so negligible.
- **Console `plugins.conflicts <id> fields` is synchronous.** It reads the whole CELL+WRLD (REFR-type) or type group of every carrier plugin from disk, and `diff_fields::unmatched` is O(a x b) per repeated tag. It runs on whatever thread drains console commands. This is a user-invoked debug command, so large-group hitches are acceptable. `--plugin-report` is the headless alternative.
- **Oblivion archive pairing.** The `<stem><space|-|_>…` prefix rule can pair `Mod Extended.bsa` to `Mod.esp` ahead of `Mod Extended.esp`. `pair_archives` documents first-plugin-wins, and the UESP rule it implements has the same shape. No source for a stricter rule exists.
- **Archive identity on case-sensitive filesystems.** `archive_identity` lowercases the canonical path, so two files differing only in case collapse to one identity on Linux. This is consistent with LMC-ID-009 (case-insensitive archive names) and is not a practical issue.
- **Unicode folding scope.** `plugin_key` folds Unicode, while in-archive asset paths and BSA hashes use ASCII folding elsewhere. LMC-ID-009's "every host filesystem" has no stated Unicode policy.
- **Not verified.** The commit's performance figures (0.02 s of 0.48 s provenance; 9,807 forms overridden on the ten-plugin Skyrim SE set) and the lmc-activation.sh live legs (they need a built engine and game data, and I was told not to launch it) were not re-run. The ignored real-data tests were not run (OOM risk).

### Gameplay fixes (group C)

- #7233 narrows the issue's trigger: a leaf whose conditions flip while its procedure runs is still never swapped when that procedure has no Done state (Sandbox, Wander, Eat, Sleep: `tree_leaf_done` is false for them). Only a done Travel/Find/UseWeapon leaf or a no-leaf install re-walks. This is a deliberate reading of the CK ("a swap mid-run is not described") and is documented; flagging that the common conditioned-leaf case from the original report is not addressed.
- `rerun_tree_leaf`'s adoption branch records the pass's `chosen` leaf, not the leaf spawn actually installed; the doc says "adopted as it stands". They differ only if conditions flip between spawn and the first tick on a tree that does not need an actor.
- A tree Travel leaf with Near Editor Location and no radius uses the default radius (`radius_of(location)`), where the flat Travel arm uses radius 0 (the `CGBladesWaitToMove` rule). Pre-existing divergence, now more visible because both set `near_editor_location`.
- `emit_item_transfers` still accepts `count == 0` rows from any producer; #7231 fixed the one caller. A single guard in the emitter would cover siblings.
- `crates/core/src/form_id.rs` `PluginId::from_filename` now hashes the case-folded basename (285633f27, LMC slice). Mixed-case production uses I found are lowercase-keyed through `plugin_basename_lc`; not audited further here.

## Not verified

- Cross-frame GPU synchronisation for the patch BLAS set is argued from the barrier chain and the all-slots fence wait; no `BYRO_VALIDATION=1` or RenderDoc run (the commit's docs claim a clean sync-validation run). Patch BLAS VRAM is not measured.
- The `lmc-activation.sh` live legs and the plugin crate's ignored real-data tests were not run; IA-B-04 is by code reading.
- How often vanilla Guard / Sandbox / Patrol packages use Near Editor Location (IA-C-04) was not counted.
