**HEAD**: `3bcf6c8e8` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` (HEAD `00f580e09`) · **Audited**: Dim 7 (deep, the suite's streaming emphasis), Dims 1 and 3 (delta read plus a fresh look at the changed code), Dims 2, 4 and 5 (delta read plus guard runs). This covers 81 commits in `00f580e09..3bcf6c8e8`. · **Unchanged since baseline (skimmed)**: Dim 6 (zero commits in its paths), Dim 8 (zero commits in its paths)

# Performance Audit — 2026-10-09

**Command**: `/audit-performance`, `--depth deep`. This is one leg of `/audit-suite --preset streaming-deep`, run solo (no sub-agents). The suite asked for priority on `byroredux/src/streaming/`, `byroredux/src/npc_spawn/` and `byroredux/src/cell_loader/`.

**Mode**: static analysis plus unit-test, dhat, shader-parity and harness-provenance runs. Per the suite rules, no engine, GPU process or benchmark was launched. **Every cost statement below is either derived from code or quoted from a dated earlier measurement. None was measured in this run.**

## Executive Summary

| Severity | NEW | Already tracked (verified still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 4 (#5401, #5288, #5289, #5291) |
| LOW | 3 | 7 (#5447, #5448, #5449, #5450, #5451, #5339; #5365 is unlabelled) |

**No guard has eroded.** Every guard test that was run passed (see "Guard verification").

**Status of the baseline's findings.** All six were published (#5401, #5447–#5451) and all six are still open. The code at each site is unchanged, or has only moved; see the "Already tracked" table. One item the baseline listed as tracked is now resolved:

- **#5355** (`articulation_joints` never pruned) was closed by `26a1277bc`.
- `clamp_explosive_velocities` now runs a liveness `retain` at the top of every substep.

**The streaming area is in good shape.** `byroredux/src/streaming/{mod,pre_parse,telemetry}.rs` has no commits since the baseline. The 12 in-area commits are mostly correctness fixes with negligible cost, and three of them are performance fixes:

- **#5418** (`9bbe9304e`): the unload detach pass now takes one `Parent` guard for the whole sweep and tests membership by binary search, instead of one lock round-trip per victim.
- **#5376** (`4e58f241d`): when a force-greet fails to open, it now consumes its directive. Before, it re-ran the whole selection stack every frame for as long as the NPC carried the directive.
- **#5358** (`479414ffe`): with Skyrim `BODT` masks now decoded, the #3411 covered-bits gate actually fires on Skyrim. It skips 80 redundant same-slot addon meshes across 75 ARMOs.

**New in this run: three LOW findings.**

- **PERF-D7-2026-10-09-01**: the per-frame cell-climate override never caches its failure result. It logs `log::warn!` and re-resolves on every exterior frame.
- **PERF-D1-2026-10-09-01**: #5391 added an "In-Cell" Eat/Sleep idle state that never records itself. The actor re-resolves its anchor every frame for the whole package window. The walk leg also pays the per-actor `world.get` lock round-trips that #5418 just removed from unload.
- **PERF-D3-2026-10-09-01**: #5275's fix re-armed only the warning. The ~8–33 MB staging-arena allocation attempt is still retried every frame under persistent host-visible memory pressure.

**Observed vs ROADMAP.** Nothing was measured, so this report claims no bench delta.

- The Bench-of-record is still `a37fcba3c`, now **536 commits** behind HEAD. ROADMAP line 32 records 481 at the 2026-10-09 close (R6a-stale-26).
- `scripts/check-bench-harness-provenance.sh a37fcba3c` still reports **DIVERGED** with the same two commits (`a4ede5fa0`, `8fdd9ae56`). This is tracked as #5451.
- Frame-path changes since the baseline that the next refresh must cover:
  - #5369 (`2a7223121`): deeper direct-light EMA plus history mode 3. Constants only; no ray added.
  - #5211 (`e6e888f68`): SVGF finite guards.
  - #5482 (`3bcf6c8e8`): presentation contrast toe, 3 `pow` per output pixel.
  - #5381 (`46dfdcc6c`): removes an unauthored parallax-occlusion (POM) ray-march from every Starfield surface. This is a GPU win, but unmeasured.
  - #5277 (`9bb6b7dce`): Starfield CDB `AlphaBlend` materials move onto the blended path.

## Hot Path Analysis

### Per-pass GPU cost

There are no new timings. The newest in-repo per-pass numbers are still `AUDIT_PERFORMANCE_2026-09-26b.md`: HEAD `b7491072f`, MedTekResearch01, renderer-stepped/pan, Native AA 1280×720, **RT quality tier pinned to 0**. The baseline report quotes them in full. Only the deltas are listed here.

| Bracket (`GpuTimerSnapshot`) | Last measured (09-26b, tier 0) | Code change since the 10-08 baseline |
|---|---:|---|
| `main_render_ms` (inclusive) | 63.789 ms (60.4–67.0) | #5369: EMA cap 64→256 and floor 0.025→0.008, plus a mode-3 branch. No new trace. The commit names "raise parked K (`direct_shadow_samples`)" as the next step; that would be a per-fragment ray-count increase in this pass. #5401 (the transmission self-skip loop) is still open. |
| `svgf_ms` | 0.476–0.488 | #5211: two `isnan`/`isinf` guards per pixel |
| `presentation_ms` | not in 09-26b | #5482: `gradeContrast`, 3 `pow` + `mix` per output pixel; exposure applied before the grade |
| every other bracket | see baseline | none |

The bracket inventory is unchanged: 28 `_ms` fields (23 named + 5 main-render sub-phases) in 56 query slots (`gpu_timers.rs:131`). No pass was added or removed.

### Per-frame CPU (derived from code)

| Item | Cost shape | Change since baseline |
|---|---|---|
| `retry_cinematic_readoption` (`app_step.rs:87`) | one resource read when nothing is pending. While pending: a fresh player-subtree `HashSet` per call (#5379). | Pending is transient (only after a convoy route ends with no cell under it). Not filed. |
| `apply_cell_climate_override` (`app_step.rs:99`) | normal frame: two map lookups. **Failure arms: a `log::warn!` + re-resolve every frame.** | Pre-existing (`a919fcd70`, 2026-08-13). Newly reported: **D7-01**. |
| `eat_sleep_system` (`systems/eat_sleep.rs:76`) | per actor that is not seated: 2–6 separate `world.get` / `try_resource` round-trips. In-Cell idle re-resolves every frame. | **#5391 new shape (D1-01)**. The furniture-gather retry is #5449. |
| Restir rig key (`build_and_upload_instances.rs:711-764`) | 11 floats folded per visible light, plus one more `rigid_instance_set_changed` call | new (#5369), negligible |
| Dynamic-RGBA staging growth (`dynamic_rgba.rs:146-173`) | **under persistent failure: one host-visible allocation attempt per frame** | the residual #5275 left behind (**D3-01**) |
| `forcegreet_system` | a failed open now consumes the directive (#5376); #5392 adds one `player_can_act` + one resource read, only when directives exist | down |
| Dialogue voice (`dialogue_voice.rs:198`) | event-driven. `VoiceSoundCache` is a `Vec` LRU with a linear key scan (~O(100) entries at the 64 MiB budget), and decode still runs on the main thread. | new (#5382 / #5410). Event-driven, not filed (the baseline already marked voice decode "checked"). |

## Findings

### Eroded guards

None.

### New issues

### PERF-D7-2026-10-09-01: A failed cell-climate override logs a warning and re-resolves on every exterior frame
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/app_step.rs:92-106`: the per-frame call, deliberately outside the `grid_changed` guard. The comment there says it "Costs one map lookup and an `Option<u32>` compare on every other frame".
  - `byroredux/src/scene/world_setup.rs:472-514` (`apply_cell_climate_override`).
  - `byroredux/src/env_translate.rs:642-658` (`resolve_cell_climate`'s warn) and `:683-714` (`resolve_default_weather`).
- **Status**: NEW. The code has existed since `a919fcd70` (2026-08-13, #2451) and is reported here for the first time.
  - Related but distinct: #5463 (open) covers what TES5/FO4 `XCCM` means; it mentions the warning but not that it fires every frame. #5424 (closed) covers the WTHS stand-in. #3679 (closed) fixed the region-ambient sibling of this call with a per-grid cache.
- **Description**: `step_streaming` calls `apply_cell_climate_override` on every frame in exteriors. It does no caching of its own. Two failure arms re-run each frame:
  1. **The cell's `XCCM` is not a parsed `CLMT`.** `resolve_cell_climate` emits `log::warn!` and returns the worldspace climate. That equals `applied_climate`, so the caller returns `false`, and the warning fires again on the next frame.
  2. **The override climate resolves but has no default weather.** The caller warns and returns `false` without recording anything. The comment says this is "so a later fix (or a different cell) is still re-evaluated". So every frame re-runs `resolve_default_weather` and warns again.
     - Since #5424, that re-run includes `default_weather_by_edid`: an O(all WTHR records) EDID string scan whenever the climate carries WTHS rows.
- **Trigger**:
  - A modded Skyrim or FO4 exterior that authors `XCCM`. On those games it is a `REGN` reference (#5463), so it never matches a `CLMT`.
  - A missing master.
  - A `CLMT` whose weather list points at WTHR records the load order never supplied.
  - Vanilla: none. #5463's census found all 214 + 35 + 19 Skyrim/DLC `XCCM` cells are interiors.
- **Evidence**:
  - `world_setup.rs:486-493`: resolve first, then compare with `applied_climate`.
  - `:499-514`: warn plus `return false` with no state recorded.
  - `env_translate.rs:653`: an unconditional `log::warn!`.
- **Impact**: one formatted warning per frame (60–144 lines per second) written through the logger, for as long as the player stands in that cell. Arm 2 also repeats the resolve work. Failure path only; the sky itself stays correct. No quantitative guard exists for this site.
- **Related**: #5463, #5424, #3679 (the cache shape to copy), #5339 (the same "failure retried every frame" class in `water.rs`).
- **Suggested Fix**:
  - Cache the decision per `(worldspace_key, player_grid)`, as `applied_region_ambient` does, so the resolve and any warning run once per grid change.
  - Optionally record a failed override per override FormID so it warns once per session.
  - Correct the `app_step.rs:96-98` cost comment.

### PERF-D1-2026-10-09-01: An "In-Cell" Eat/Sleep idle re-resolves every frame, and the walk leg pays per-actor lock round-trips
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**:
  - `byroredux/src/systems/eat_sleep.rs:126-139`: no `EatSleepState` is recorded when `resolve_anchor` returns `None`.
  - `:199-238`: `resolve_anchor` and `cell_is_resident`. Each call allocates a `to_ascii_lowercase` `String` and does a std `HashMap<String, _>` get.
  - `:140-189`: the walk leg, which per actor per frame does `world.get::<EatSleepState>`, `::<GlobalTransform>`, `::<WalkSpeed>` and `::<Transform>`, plus a `PhysicsWorld` `try_resource` and a `query_mut::<Transform>`.
  - Registration: `byroredux/src/boot/schedule/post_update.rs:129` (`add_exclusive`, every frame).
- **Status**: NEW. Arrived with `42aab4c09` (#5391, 2026-10-09). Related: #5449 (open), which covers the same system's arrived-but-unseatable furniture-gather retry. That is a different trigger with the same fix shape.
- **Description**: #5391 gives every Eat/Sleep actor an authored `PLDT` anchor. For an `InCell` package, `resolve_anchor` returns `None` unless the package's cell is the resident interior.
  - In exteriors, In-Cell anchors are never resolved at all (v0): `CurrentCellContext` is removed there (`transition.rs:446`).
  - `None` makes the loop `continue` without recording anything, so the module doc's "resolved again next tick" is literally every frame. That lasts for the whole package time window.
  - Each idle frame pays `world.get::<EatSleepState>` plus `world.get::<GlobalTransform>` plus a resource probe. In a non-matching interior it also allocates the lowercase cell-key `String` and does a SipHash map lookup.
  - Separately, the walk leg reads four components through four individual `world.get` calls. Each one is a full read-lock acquire/release with lock-tracker bookkeeping. #5418 just removed that exact per-entity pattern from the unload detach pass. The other locomotion systems (travel, wander, forcegreet after #5371) gather in query passes instead.
- **Evidence**: `eat_sleep.rs:131` (`let Some(destination) = resolve_anchor(world, npc, location) else { continue; };`) and `:222-238`. The commit's own census says 418 of 706 FNV Eat references use NearEditor, InCell or NearCurrent; it does not give the InCell share.
- **Impact**: a few lock round-trips plus at most one small `String` allocation per affected actor per frame. Bounded by the number of Eat/Sleep actors, unbounded in time. Small. No quantitative guard exists for this site.
- **Related**: #5449, #5418, #3353 (the minute-gate cost model the ambient package system follows), #2033 (the persistent-scratch pattern).
- **Suggested Fix**:
  - Record an "idle until the resident cell changes" state, keyed on the `CurrentCellContext` identity or a load generation, instead of returning `None`.
  - Gather `(EatSleepState, GlobalTransform, WalkSpeed, Transform.rotation)` for all actors in one pass per storage, as `travel.rs` does.
  - Compute the lowercase cell key once per frame. This can share #5449's retry-cadence fix.

### PERF-D3-2026-10-09-01: #5275 fixed the warning but not the allocation; staging-arena growth is still retried every frame under persistent host-visible pressure
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**:
  - `crates/renderer/src/texture_registry/dynamic_rgba.rs:146-173`: the growth attempt. It returns `Ok` on failure and leaves every update dirty.
  - `:296-301`: #5275's re-arm site.
- **Status**: NEW. This is the residual half of the closed #5275.
- **Description**: #5275's Impact listed two per-frame costs under a persistent BAR or host-visible failure: "one `log::warn!` per frame … **plus one host-visible allocation attempt of about 8.3 MB (1080p) or 33 MB (4K) per frame**". `7e43a1550` moved the warning's re-arm. Its commit message says the warning flag is what caused the per-frame allocation attempt. The code says otherwise:
  - `staging_skip_logged` gates only `note_staging_skip`'s log line.
  - Whenever dirty bytes exceed the slot's arena, `GpuBuffer::create_host_visible` is called again on every frame. When no existing block fits, that means gpu-allocator tries a fresh `vkAllocateMemory`, which then fails.
- **Evidence**: `dynamic_rgba.rs:146-149` (the size check), `:164-172` (`Err(e) => { note_staging_skip(..); return Ok(()); }`). No backoff state exists anywhere in `DynamicRgbaUploads`.
- **Impact**: one failing driver-side allocation per frame for the whole memory-pressure episode, which is exactly when VRAM and BAR headroom matter most. Failure path only; the frame still renders and the overlay keeps its last uploaded image. Unmeasured.
- **Related**: #5275 (closed), #4889 (the degrade-not-die contract this keeps).
- **Suggested Fix**: back off growth retries during an episode. For example, retry every N frames, or only after the dirty byte total shrinks or a free event fires. Reuse the episode flag as the backoff key. A unit test can pin "skip → skip → skip calls the allocator once per backoff window".

### Already tracked (verified still present; not re-filed)

| Issue | Title (short) | State in code at `3bcf6c8e8` |
|---|---|---|
| #5401 | #5249's transmission self-skip trace, not governed by the ray tier (MEDIUM) | unchanged: `MAX_TRANSMISSION_SELF_SKIPS 8u` (`shader_constants.glsl:38`), call at `triangle.frag:3863` |
| #5288 | Door transition opens the archive set twice (MEDIUM) | `loading_screen.rs`: zero commits |
| #5289 | LOD-water recenter rebuilds the full mesh at every crossing (MEDIUM) | `water.rs`: zero commits; still reached from `app_step.rs:136` (`recenter_lod_water`) |
| #5291 | Starfield CDB `MaterialIndex` built on the main thread, ~2 s / ~470 MB (MEDIUM) | **Grew.** #5277 (`9bb6b7dce`) adds `effect_blend: HashMap<u32, Option<String>>` (`crates/sfmaterial/src/index.rs:152`), one owned `String` per EffectSettings object for a small closed enum (`"AlphaBlend"`, `"None"`, …), plus an `emissive` tuple map (`:155`). `resolve` clones the `String` per lookup (`:381-384`). An interned enum or `&'static str` would remove the per-object allocation. `memory-budget.md` still has no row for the index. Add this to #5291. |
| #5447 | `story_change_location_system` builds the CLOC key every frame | `story_events.rs:29-61` unchanged; #5372 touched only `raise_hello_story_event` (event path) |
| #5448 | `BYRO_M42_DEBUG` `getenv` every frame | moved to `ai_package.rs:902`; `last_evaluated` Vec at `:874` |
| #5449 | `eat_sleep` whole-world furniture gather retried every frame | `seat_at_marker` at `eat_sleep.rs:244-283`, fresh `actors` Vec at `:77`; see D1-01 for the new sibling |
| #5450 | Script-killed corpse scan covers every placement on every game | `references/mod.rs:344-357`: still no `game` gate |
| #5451 | Bench harness edited twice since the record | provenance: DIVERGED, same 2 commits |
| #5339 | Failed distant-water rebuild retries a full scan every frame | `water.rs`: zero commits |
| #5365 | Top-of-frame all-slots fence wait | open, not re-measured |
| ~~#5355~~ | `articulation_joints` never pruned | **closed** by `26a1277bc` (verified: liveness `retain` per substep) |

## Guard verification

| Guard | Result |
|---|---|
| Renderer lib, filters `acceleration static_blas_recovery scene_buffer skin_compute dispatch_skin gpu_timers material_tests hash_ bone_world has_any_view_of_path early_fragment light_history group_state tlas_scratch restir_history caustic light_rig` | **458 passed**. Includes #5369's `light_rig_geometry_key_ignores_intensity_but_not_geometry` and `flicker_only_changes_keep_the_deep_ema_mode`. |
| Bin (toolchain 1.96.0), filters `draw_sort_key static_blas_recovery skin_dispatch_ran bench_gpu_keys atw_bracket sort_key streaming load_order::parallel eat_sleep forcegreet story_events detach_victims retention lod_bands cinematic climate default_weather reference_state` | **210 passed, 4 ignored** (manual benches and the real-order digest). Includes #5418's `detach_victims_takes_one_parent_guard_for_the_whole_sweep`, #5387's `same_level_leftover_generation_quads_are_pruned`, and #5379's `purge_despawns_pending_readoption_entities`. |
| NIF dhat (`--features dhat-heap`: `heap_allocation_bounds`, `_geometry`, `_import`) | 7 + 1 + 1 passed. #5389's `decline_unbounded_packed_indices` allocates nothing. |
| `scripts/check-shader-artifacts.sh` | 36 shaders + the opaque early-test variant are byte-identical (glslang 11:16.4.0) |
| `scripts/check-bench-harness-provenance.sh a37fcba3c` | **DIVERGED**, 2 commits (#5451) |
| `.claude/commands/_audit-validate.sh` | OK, all path references valid (symbol advisories only) |

Symbol and constant checks (grep and read). All held:

- **Dim 7**:
  - Constants: `PRE_PARSE_RAYON_MIN` 8 (`pre_parse.rs:340`), `STREAM_PARSE_INPUT_BYTES` 64 MiB (`:398`), `STAGED_BYTE_CAP` 256 MiB (`texture_prefetch.rs:27`), `STREAMING_APPLY_BUDGET` 16 ms, `BOUNDARY_LOD_RECONCILE_SAFETY_CEILING` 500 ms, `MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME` 2 (`app_step.rs:33-54`).
  - Batched teardown: `World::despawn_batch` → `PackedStorage::remove_entities_erased` plus its tests; `UnloadPhaseTimings`.
  - Worker and import path: `AppearanceProviders`, `decode_precombine_csg`, `reintern_imported_meshes`, `worker_batch_duplicate_skips`, `mesh_declared_size`.
  - Unload keeps its one-whole-world-scan-per-batch shape (#3690).
- **Dim 1**: `drain_dirty_into_preserves_storage_capacity`. The animation scratches, billboard `last_cam`, `SceneEffectSoftCache` and `SkinSlotPool.free_list` all sit in files with zero commits.
- **Dim 2**: `DRAW_SORT_PARALLEL_THRESHOLD` 3000 (`render/mod.rs:933`); `byroredux/src/render/` has zero commits.
- **Dim 3**: `acceleration/`, `context/resources.rs`, `mesh/` and `deferred_destroy.rs` have zero commits, so their constants are unchanged.
- **Dim 4**: `MAX_INSTANCES` 0x40000, `INITIAL_INSTANCE_CAPACITY` 0x10000, `MAX_INDIRECT_DRAWS` = `MAX_INSTANCES`, `MAX_MATERIALS` 16384 (`scene_buffer/constants.rs:151,167,257,286`). `classify_pbr_keyword` appears only at `material_translate.rs:664`.
- **Dim 5**: `ENABLE_LEGACY_WRS` 0 (`shader_constants_data.rs:2046`); `QUERIES_PER_FRAME` 56.
- **Dim 6**: `plan_palette_dispatch`, `skinned_blas_refit_limit`, `SKINNED_BLAS_REFIT_THRESHOLD`/`JITTER`, `skinned_blas_stays_fx_hashed`, `pose_dirty_crosses_the_crate_boundary_without_siphash`, `palette_dispatch_uses_the_dirty_plan_and_rearms_late_bind_inverses`, `a_reused_plan_scratch_keeps_capacity_and_drops_the_previous_plan`.
- **Dim 8**: 28 `_ms` brackets; `bench_gpu_keys_match_the_reported_bracket_order`, `RENDER_ORIGIN_SNAP`, `origin_corrected_prev_view_proj`.

### Checked and not filed (quantify first)

- **DIAL override fold (#5375, `168b81322`)**: `fold_info_into_topic` (`crates/plugin/src/esm/records/index.rs:98`) is O(override INFOs × topic INFOs).
  - For each override INFO it does two linear `position` scans over the topic's `Vec<InfoRecord>` (~300-byte records) and a `Vec::insert` shift.
  - FNV GREETING is the largest case: 5,300 master INFOs and 11–129 per story DLC. That suggests roughly tens of milliseconds per load order. The cost is boot-time only, and it sits in the load-order fold, which the skill says is the right place for order-dependent work.
  - A per-topic FormID→position map would make it linear. File only if the ESM-parse timing shows it.
- **LOD quad prune (#5387)**: `prune_same_level_overlaps` (`lod_bands.rs:453`) builds a std `HashMap` per level and makes O(levels × authored) passes. `objects_for` / `terrain_for` clone a `HashSet` into a `Vec`. All of this is recomputed on every `reconcile_lod_rings` call (crossings, plus idle frames while pending), although the input is static per worldspace. At FO3 sizes (366 object / 2,231 terrain quads) that is microseconds.
- **#5277 blended-path shift**: `"AlphaBlend"` CDB materials now set `has_alpha` and can take the sorted alpha-over / glass path, which gives up early-Z. Any Starfield batch-count or `main_blended_ms` movement after `9bb6b7dce` is a correctness change. Compare captures from the same camera pose.
- **Cinematic re-adoption (#5379 / #5384)**: the player-subtree `HashSet` walk runs only while convoy entities are pending, and the release walk only on terminal arrival.
- **Unload detach (#5418)**: it copies and re-sorts a victim list that `drain_cell_victims` has already sorted and deduped. pdqsort is linear on sorted input, so this is negligible.
- **Dialogue voice (#5382 / #5383 / #5393 / #5395 / #5410)**: event-driven. `voice_owner` clones `LoadedPluginSet.masters` per spoken line. The `VoiceSoundCache` `Vec` LRU scan is O(entries), about 100 entries at the 64 MiB budget. The follow pass reuses the change-gated `sync_emitter_positions`.

## Prioritized Fix Order

1. **#5451 and the bench-of-record refresh (R6a-stale-26)**. This is still the top measurement item: 536 commits, plus #5369 / #5381 / #5277 / #5482 frame-path changes since the baseline. Nothing in Dim 5 can be confirmed or cleared until it is done.
2. **#5401**: a pinned-tier `main_render_ms` A/B across `34c3adf14`. Run it before any "raise parked K" follow-up to #5369, since both add rays to the same pass.
3. **PERF-D7-2026-10-09-01**: quick win. Cache per grid (the #3679 shape), and warn once per override.
4. **PERF-D1-2026-10-09-01 together with #5449**: one retry-cadence plus single-pass-gather change in `eat_sleep.rs` covers both.
5. **PERF-D3-2026-10-09-01**: add a growth backoff on the episode flag.
6. Earlier quick wins still open: #5448 (read the env var once), #5447 (early-out when there is no SM tree), #5450 (gate on game).
7. Tracked MEDIUM items #5289, #5288 and #5291 (now including the #5277 index growth).

## Stale skill premises

- **Dim 7 First step** still lists the pathspec `byroredux/src/streaming.rs`, which was deleted in #5092. Tracked as #5473 (TD4-2026-10-08-01, open). Delta-scoping this run used `byroredux/src/streaming` (the directory).
- **Dim 7 Paths** still omit several files that carried findings:
  - from the baseline: `byroredux/src/loading_screen.rs`, `byroredux/src/cell_loader/water.rs`, `byroredux/src/cell_loader/reference_state.rs`, `byroredux/src/cell_loader/legacy_lod_index.rs`;
  - from this run: `byroredux/src/cell_loader/lod_bands.rs`, plus the per-frame streaming-step callees `byroredux/src/scene/world_setup.rs` (`apply_cell_climate_override`, `apply_cell_region_ambient`) and `byroredux/src/env_translate.rs`;
  - `crates/plugin/src/esm/records/index.rs` (the load-order fold, `merge_from`), which the ESM-parse bullet describes but Paths do not list.
- **Dim 3 Paths** list only `crates/sfmaterial/src/reader.rs` (under Dim 7). The Phase-2 `crates/sfmaterial/src/index.rs` holds #5291's memory and grew this run. `byroredux/src/asset_provider/material/cdb.rs` and `loose_mat.rs` are still missing as well (noted by the baseline).
- **Dim 1 Paths** still predate `eat_sleep`, `forcegreet`, `story_events`, `npc_dialogue` and `dialogue_voice` (baseline note). All three per-frame LOW items in the open set (#5447–#5449) and D1-01 live there.
- **Dim 3 Guard / #5275**: the dynamic-RGBA staging contract should say that *retry cadence* under failure is part of the contract, not only the warning. See D3-01.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-09.md`
