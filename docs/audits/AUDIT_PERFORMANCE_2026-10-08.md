**HEAD**: `00f580e09` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dims 1, 2, 3, 4, 5, 7 (116 commits in `a2c24b16e..00f580e09`; Dims 2, 3 and 4 changed only slightly and got a delta read plus guard runs) · **Unchanged since baseline (skimmed)**: Dim 6 (comment-only and source-scan commits), Dim 8 (zero commits in its paths)

# Performance Audit — 2026-10-08

**Command**: `/audit-performance`, `--depth deep`, one leg of `/audit-suite --preset comprehensive`, solo (no sub-agents). **Mode**: static analysis plus unit-test, dhat, shader-parity and harness-provenance runs. Per the suite rules no engine, GPU process or benchmark was launched. **Every cost statement below is either derived from code or quoted from a dated earlier measurement; none was measured this run.**

## Executive Summary

| Severity | NEW | Already tracked (verified still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 3 (#5288, #5289, #5291) |
| LOW | 5 | 2 (#5355, #5339); plus #5365 (unlabelled, the fence-wait throughput half) |

No guard eroded. The baseline's two findings that were not yet published are resolved:

- **PERF-D1-2026-10-05-01 (Talk-arm rebuild every frame)** is fixed by #5293 / `9d4a951ad`. The bound set is cached on the `SceneActorBindings` and `QuestStageState` revisions and the arm now probes only the bound entities. The hit path still clones the (tens-of-entities) `FxHashSet` each frame. That residual is not filed.
- **PERF-D8-2026-10-05-01 (no `ctx.scratch` row for `layout_order`) does not reproduce.** The row `groundcover_collect_scratch.layout_order` has been at `app_events.rs:892-895` since `226f2dd4c` (2026-10-02), three days before the baseline's HEAD. The baseline finding was a false positive and should not be published.

The three baseline MEDIUM findings (#5288 door-transition archive reopen, #5289 LOD-water recenter rebuild, #5291 Starfield CDB index) are still open and the code is unchanged.

**New this run**, all from code that landed in the delta:

- **PERF-D5-2026-10-08-01 (MEDIUM)**: #5249 adds a closest-hit ray-query loop (up to 8 hops) to the ReSTIR finalize of every transmission-lobe fragment. It sits in the dominant GPU hotspot, the ray tier does not govern it, and the commit carries no timing evidence.
- Five LOW findings: three per-frame CPU waste sites in the new M42 / Story Manager systems, a first-cell-load scan that runs on every game, and a bench-harness edit that changes a measured scene without a re-bench.

**Observed vs ROADMAP.** Nothing was measured, so no bench delta is reported. The Bench-of-record is still `a37fcba3c` and is now **455 commits behind HEAD** (ROADMAP **R6a-stale-25** says 390 at the 2026-10-06 close). Frame-path changes since that close are not in its list: #5249 (transmission trace), #2764 (indirect group key), #5368 (shader rebuild) and #5250. `scripts/check-bench-harness-provenance.sh a37fcba3c` returns **DIVERGED**, now citing **two** commits (`8fdd9ae56` and `a4ede5fa0`; see PERF-D8-2026-10-08-01). The R6a-regress-22 residual stands as recorded (+0.7 ms, fence +2.6 ms from the old pose).

## Hot Path Analysis

### Per-pass GPU cost

No new timings. The most recent in-repo per-pass numbers are `AUDIT_PERFORMANCE_2026-09-26b.md` (HEAD `b7491072f`, MedTekResearch01, renderer-stepped/pan, Native AA 1280×720, **RT quality tier pinned to 0**, 118 completed readbacks per run). They are quoted, not re-measured:

| Bracket (`GpuTimerSnapshot` field) | 2026-09-26b MedTek, tier 0 | What changed since in code |
|---|---:|---|
| `main_render_ms` (inclusive) | 63.789 ms (reported summary 60.4–67.0) | **#5249 adds a per-fragment trace (finding D5-01)**; #5057 early-Z kinds, #5192 lobe split (both before the baseline) |
| of which opaque/alpha-tested (`main_opaque_ms`) | 60.380 ms | same |
| of which blended (`main_blended_ms`) | 3.016 ms | none |
| main pass with main rays disabled (ablation) | 36.369 ms inclusive | the rays are about 27 ms of the pass |
| `skin_blas_refit_ms` | 0.434–0.439 | none |
| `volumetrics_ms` | 0.879–1.328 | none |
| `svgf_ms` | 0.476–0.488 | none |
| `cluster_cull_ms` | 0.178–0.181 | none |
| `sky_cube_ms` | 0.201–0.210 | none |
| `tlas_build_ms` | 0.030 | #5250 changes only the recorded scratch peak |
| `skin_dispatch_ms` / `skin_palette_ms` | 0.007–0.013 / 0.013 | none |

The bracket inventory is unchanged: 28 `_ms` fields on `GpuTimerSnapshot` (23 named + 5 main-render sub-phases) in 56 query slots, matching the skill table. No pass was added. Parent/child brackets must not be summed.

The main geometry pass is the measured hotspot, so any added per-fragment ray work there is the highest-leverage place to look. That is why D5-01 is MEDIUM.

### Per-frame CPU (derived from code)

| Item | Cost shape | Change since baseline |
|---|---|---|
| Talk-filter bound set (`interaction.rs:1312`) | one `FxHashSet` clone on a cache hit | down: was a full alias-table rebuild plus four entity sets (#5293) |
| `story_change_location_system` (`story_events.rs`) | 1–2 String allocs + 2–4 SipHash lookups, every frame, every game | **new** (D1-01) |
| `ambient_ai_package_system` env probe (`ai_package.rs:814`) | one `getenv` scan per frame | **new** (D1-02) |
| `eat_sleep_system` (`eat_sleep.rs`) | idle: nothing. Arrived-but-unseatable actor: one full furniture gather + a fresh Vec, per actor, per frame | **new** (D1-03) |
| `forcegreet_system`, `npc_dialogue` goodbye check, `story_manager_dispatch` | resource reads and empty collects when idle | new, no finding |
| Audio `sync_emitter_positions` (#3086) | O(active sounds), change-gated on `last_position` | new, no finding |
| `retry_cinematic_readoption` (#3817) | one resource read when the pending list is empty | new, no finding |

## Findings

### Eroded guards

None. See "Guard verification".

### New issues

### PERF-D5-2026-10-08-01: #5249's transmission-lobe shadow trace adds an unmeasured, ray-tier-ungoverned closest-hit loop to the dominant GPU pass
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**:
  - `crates/renderer/shaders/triangle.frag:3833-3867` (the finalize; the same source builds `triangle_early.frag.spv`)
  - `crates/renderer/shaders/include/shadow_transport.glsl:179-225` (`traceShadowTransmittanceSkippingInstance`)
  - `crates/renderer/shaders/include/lighting.glsl:494-515` (`traceLightTransmittanceSkippingInstance`)
  - `crates/renderer/shaders/include/shader_constants.glsl:38` (`MAX_TRANSMISSION_SELF_SKIPS` 8u)
- **Status**: NEW. It arrived with `34c3adf14` (#5249, 2026-10-07), a closed correctness fix. It is not listed in ROADMAP R6a-stale-25's frame-path changes (the list predates it).
- **Description**: the transmission half of the selected light's radiance (wrap excess, back-light, translucency) used to be unshadowed (#5192). #5249 gave it its own visibility trace. For each fragment whose `restirSelectedTransmission` is non-zero and `shadowFade > 0.01`, the finalize now runs `traceLightTransmittanceSkippingInstance`: a loop of up to 8 `rayQueryInitializeEXT` + full `rayQueryProceedEXT` traversals.
  - Each hop needs the nearest committed hit, so it cannot use `TerminateOnFirstHit`.
  - A hit on the receiver's own instance is stepped past. The first foreign hit hands the remaining leg to the shared alpha/glass-aware transport, which is a further traversal.
  - Eligibility is material-scoped. The lobes are non-zero only for materials with `MAT_FLAG_SOFT_LIGHTING`, `MAT_FLAG_BACK_LIGHTING` or `MAT_FLAG_TRANSLUCENCY` (wrap-lit skin, hair, foliage, FO4 v≥8 translucency), and only when the selected light is behind the shading plane (`rawNdotL < 0`).
  - That includes Skyrim and FO4 NPC bodies.
- **Evidence**:
  - `triangle.frag:3854-3867` gates the trace only on `dot(restirSelectedTransmission, restirSelectedTransmission) > 1e-12 && shadowFade > 0.01`.
  - The loop bound is the constant `MAX_TRANSMISSION_SELF_SKIPS`.
  - `GpuRayBudget` (`scene_buffer/ray_budget.rs:12-30`) has `direct_shadow_samples`, `ray_count`, `quality_tier` and others; none of them is read by this trace. The existing shadow rays are bounded by `clamp(rayBudget.directShadowSamples, 1u, MAX_DIRECT_SHADOW_SAMPLES)` (`triangle.frag:3771`).
  - The commit message records "live-verified on FNV GSProspectorSaloonInterior, 120 frames, RT tier 1, zero validation errors" and nothing about `main_render_ms`.
- **Impact** (derived; unmeasured): on every eligible fragment of every frame, at least one extra traversal on top of the K ≥ 1 existing shadow rays, and up to 8 plus a shared-transport leg when the ray starts inside a closed own-instance body, which is exactly the case the trace was written for. At tier 0, `directShadowSamples` clamps to the minimum, so the relative increase is largest where the adaptive budget is trying to shed cost. In the last measured dense scene the main pass is 60–67 ms with about 27 ms attributable to rays, so this is the pass where a regression is least affordable. The size depends on the on-screen share of those materials, which no in-repo capture quantifies.
- **Related**: #5192 (the lobe split), #5249 (closed), #4946 (the wall-bleed it re-closes), `docs/audits/AUDIT_PERFORMANCE_2026-09-26b.md` (hotspot attribution), ROADMAP R6a-stale-25.
- **Suggested Fix**:
  1. Capture `main_render_ms` A/B with `--rt-test-ray-quality-tier` pinned (same pose, same upscaler) on a Skyrim NPC interior (WhiterunBanneredMare) and FO4 MedTek, before and after `34c3adf14`.
  2. If material, reuse ray 0's result. It already traced the same origin and direction (`selectedRayOrigin/Direction/TMax`) through `traceLightTransmittanceDetailed`, which reports a committed instance. The skip loop adds information only when that first hit is the receiver's own instance. Verify the exact `committedInstance` semantics in `traceShadowTransmittanceDetailed` before relying on it.
  3. Cap the hop budget below 8 for the common case, and/or gate the trace on `rayBudget.quality_tier`.
  - This is a shader change with no unit-test coverage of cost. State confidence and keep a revert plan (see the skill's speculative-Vulkan caveat).

### PERF-D1-2026-10-08-01: `story_change_location_system` re-derives the CLOC key with String allocations and SipHash every frame, on every game
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/story_events.rs:29-92` (`story_change_location_system`, `resolve_current_lctn`, `location_hash`); registration `byroredux/src/boot/schedule/update.rs:70-72,408`; cursor install `crates/scripting/src/story_manager.rs:347-359`
- **Status**: NEW. Arrived with `3ba9f1d5c` and `17fed2565` (#5366 Phases 1–2).
- **Description**: the system runs ungated in `Stage::Update` every frame once a player entity exists. Each call:
  - clones the `Arc<EsmIndex>`;
  - on an interior cell, does `cell.cell_editor_id.to_ascii_lowercase()` (a String) and a `HashMap<String, _>::get`;
  - when the cell resolves an LCTN, does `format!("{:08X}", lctn)` (a String);
  - on an exterior cell, does `format!("grid:{:?}", exterior.grid)` (a String) plus two nested std-`HashMap` lookups;
  - SipHashes the result.
  The answer changes only on a cell transition. `install_story_manager` inserts `StoryLocationCursor` unconditionally, so FO3, FNV and Oblivion, which author no SM records, pay the same cost. `emit_change_location_on_key_change` then bails at the cursor compare, after the key has already been built.
- **Evidence**: `story_events.rs:40-55` (no early-out before the key derivation); `asset_provider/script.rs:582` (the install is not game-gated); `story_manager.rs:351`.
- **Impact**: 1–2 small heap allocations and 2–4 SipHash operations per frame for the life of the session. Small and permanent. No quantitative guard exists for this site.
- **Related**: #5366, #5293 (the same "answer changes on a generation, recomputed per frame" shape).
- **Suggested Fix**: return early when the `SmTree` has no nodes. Otherwise cache the derived key on a cell-context generation, or compare cheap inputs (cell identity / grid tuple) before building any String.

### PERF-D1-2026-10-08-02: `ambient_ai_package_system` probes the environment every frame, ahead of the minute gate that exists to make idle frames free
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:814-816` (per frame), `:892-897` (per due actor); the per-frame `last_evaluated` Vec at `:786-796`
- **Status**: NEW. Added by `00f580e09` (2026-10-08).
- **Description**: the system's own comment says nothing per-actor may be paid before the minute gate, so about 119 of every 120 frames fall straight through. The new `[m42-tick]` diagnostic sits before the `due.is_empty()` early-out. It calls `std::env::var_os("BYRO_M42_DEBUG")` on every frame that has at least one NPC with an `AmbientPackageRuntime`. That is a global environment read lock plus a linear `environ` scan. The variable is undocumented and nothing else reads it. The `[m42-eval]` probe at `:892` is per due actor per game minute, which is fine.
  The same function builds a fresh `Vec<(EntityId, Option<u16>)>` over every ambient NPC each frame (`:786`), an O(NPCs) allocation that a persistent scratch would remove. That part predates the baseline (#3353) and is noted here only because the new probe sits beside it.
- **Evidence**: `ai_package.rs:786-816`.
- **Impact**: one `getenv` scan per frame plus an O(NPCs) Vec fill and free per frame. Small. No quantitative guard exists.
- **Related**: #3353 (the minute-gate design), #2033 (the persistent-scratch pattern `sandbox` uses).
- **Suggested Fix**: read the variable once into a static (`OnceLock<bool>`) or drop the probes. Optionally move `last_evaluated` into a closure-held scratch like `make_animation_system`'s.

### PERF-D1-2026-10-08-03: `eat_sleep_system` retries a whole-world furniture gather every frame, per actor, whenever an arrived actor cannot be seated
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/eat_sleep.rs:164-223` (`seat_at_marker`), `:53-60` (fresh `actors` Vec); `byroredux/src/systems/sandbox.rs:161-181` (`collect_marker_seats`); registration `byroredux/src/boot/schedule/post_update.rs:129`
- **Status**: NEW. Arrived with `00f580e09` (M42 Eat/Sleep).
- **Description**: the module docs say seating is one-shot because `Seated` skips the actor on later ticks. That holds only on success. An arrived actor that is not seated (no matching marker in the cell, or every marker reserved, or `pick_nearest_seat` finds none within the radius) returns from `seat_at_marker` without recording anything and tries again next frame.
  - Each retry calls `collect_marker_seats`, which walks every `Furniture` entity in the world (not just within the radius) and composes a world transform per accepted marker, into a fresh `Vec`.
  - Sleep with no sleep markers does the gather twice (sleep predicate, then the sit fallback).
  - The cost is per arrived-unseated actor, so it multiplies. `sandbox_seat_system` handles the same situation with a persistent `SandboxScratch` (#2033) and one gather per frame behind an `any_unseated` gate (#3354).
  - The no-clip case is cheap (`SandboxSitClip` resource read, then return).
- **Evidence**: `eat_sleep.rs:169-183` (the gather runs before `pick_nearest_seat`), `:204-206` (the `None` seat returns silently).
- **Impact**: in the failure case, N arrived-unseated actors × F furniture × M markers per frame. Bounded by the number of Eat/Sleep actors in that state and uncommon in unmodified content, but unbounded in time. Unmeasured; no quantitative guard exists.
- **Related**: #3354, #2033, `systems/sandbox.rs` (the pattern to mirror).
- **Suggested Fix**: share one gather per frame across all Eat/Sleep actors using a persistent scratch, and record a per-actor "no seat" state that retries on a coarse cadence (for example once per game minute, like the package system).

### PERF-D7-2026-10-08-01: #5248's script-killed corpse set scans every placed reference in the load order on the main thread at the first reference apply, for all games
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/reference_state.rs:554-575` (`script_killed_corpse_forms_for_load_order`), `:493-523` (`script_killed_corpse_forms`); `byroredux/src/cell_loader/references/mod.rs:344-357` (the lazy first-call build)
- **Status**: NEW. Arrived with `d4e8c31be` (#5248, 2026-10-07).
- **Description**: the set is built lazily the first time `load_references_budgeted` runs. It iterates every interior cell, every exterior grid cell and every worldspace-persistent cell in the whole `EsmIndex`. Per placement:
  - pass 1: a `linked_refs` check plus a std-HashMap base-script lookup when links exist;
  - pass 2: a `killed.contains` and an `index.npcs.get(&base_form_id)` SipHash lookup, for every placement including statics.
  Only FO3 and FNV can ever match, because `script_is_vanilla` accepts only the twelve names in `VANILLA_KILL_SCRIPT_PLUGINS`. On Oblivion, Skyrim, FO4, FO76 and Starfield the result is always empty, but the scan still covers every placement. Before #5248 the work was proportional to one cell's refs.
- **Evidence**: no `index.game` gate at `references/mod.rs:344-357`; the scan sits on the main thread (`world.insert_resource`) in the first apply.
- **Impact**: a one-time O(total placed references in the load order) pass of hash lookups at first-cell load. The count is large on the big masters, but the pass is one-shot and boot is unbudgeted by design, so LOW. It adds directly to time-to-first-frame on the five games where it cannot match. Unmeasured; no quantitative guard exists.
- **Related**: #5248, #5223, #5304.
- **Suggested Fix**: gate on `record_index.game` (FO3/FNV only), and/or compute it on a worker overlapped with the ESM parse. Alternatively fold the recognizer into the existing ESM index walk so no second pass over placements is needed.

### PERF-D8-2026-10-08-01: the bench harness pair was edited twice since the record, and the second edit changes a measured scene's inputs without a re-bench
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost (bench discipline)
- **Location**: `scripts/fsr-bench-matrix.sh:131-146` (Prospector scene args), `scripts/fsr_bench_report.py`
- **Status**: NEW as a finding. The need for a re-bench is already tracked in ROADMAP R6a-stale-25, but no GitHub issue exists for either harness edit.
- **Description**: `scripts/check-bench-harness-provenance.sh a37fcba3c` now reports **DIVERGED** with two commits:
  - `8fdd9ae56` (#5128, 2026-09-30) appends `camera_pos`/`camera_forward` columns. Measurement-neutral by inspection (the baseline audit made the same call).
  - `a4ede5fa0` (#5257, 2026-10-06) adds `--bsa "Update.bsa"` and `--textures-bsa "Update.bsa"` to the `prospector` scene's argument list. That changes the archives the measured scene mounts: "overrides 36 entries" plus 2 textures per the commit message. The commit states entity count and `p0` source count are unchanged, but no frame-time control exists.
  Per the skill, an edit to either file that was not itself re-benched is a finding. The script's own comment asks that the change "land byte-identically across the next same-machine control".
- **Evidence**: provenance script output (quoted above); `git show a4ede5fa0 -- scripts/fsr-bench-matrix.sh`.
- **Impact**: any future comparison of a Prospector frame time against the 2026-09-28 record is not apples-to-apples until a same-machine control runs both sides. The record is already 455 commits behind HEAD, so this compounds an existing gap rather than creating a new one.
- **Related**: ROADMAP R6a-stale-25, R6a-regress-22, #5128, #5257.
- **Suggested Fix**: fold into the R6a-stale-25 refresh: rebuild `a37fcba3c` in a worktree with the *new* harness, bench both sides in one session, and record the harness commit alongside the new record.

### Already tracked (verified still present; not re-filed)

| Issue | Title (short) | State in code at `00f580e09` |
|---|---|---|
| #5288 | Door transition opens the archive set twice (LSCR cover + `step_cell_transition`) | unchanged; `loading_screen.rs` changed only for the turntable clock (#5306) |
| #5289 | LOD-water recenter rebuilds the full mesh with blocking uploads each crossing | unchanged; `streaming/mod.rs:576-607` → `cell_loader/water.rs` `rebuild_lod_water_mesh` (`:1202`, `distant_water_cells` at `:1214`) |
| #5291 | Starfield CDB `MaterialIndex` built lazily on the main thread (~2 s, ~470 MB by `224a19372`'s own figure) | unchanged; `cdb.rs` gained warn arms only (#5319); `memory-budget.md` "Starfield Component Database" still has no row for the built index |
| #5365 | Top-of-frame all-slots fence wait (throughput half of #4606) | open, not re-measured |
| #5355 | `articulation_joints` never pruned; `clamp_explosive_velocities` walks it every substep | still true: only `ragdoll.rs:529` pushes, `recovery.rs:400` walks |
| #5339 | Failed distant-water rebuild retries a full-worldspace scan every frame | not re-read; `water.rs` unchanged since the baseline |

## Guard verification

| Guard | Result |
|---|---|
| Renderer lib, filters `acceleration static_blas_recovery scene_buffer skin_compute dispatch_skin gpu_timers material_tests hash_ bone_world has_any_view_of_path early_fragment light_history` | 403 passed |
| Renderer lib, extra filters `group_state bone_world_slot instance_hash indirect_hash light_hash material_hash tlas_scratch` | 37 passed (includes the #2764 pair `opaque_glass_flag_does_not_split_groups` / `blended_glass_flag_still_splits_groups`) |
| Bin (toolchain 1.96.0), filters `draw_sort_key static_blas_recovery skin_dispatch_ran bench_gpu_keys atw_bracket sort_key interaction streaming:: load_order::parallel groundcover_hasher light eat_sleep forcegreet story_events` | 235 passed, 3 ignored (manual benches and the real-order digest) |
| Core (`--features inspect`), filters `skin_slot_pool drain_dirty_into pose_dirty` | 29 passed |
| NIF dhat `heap_allocation_bounds` (`harness = false`) | 7 tests passed; `_geometry` and `_import` passed |
| `scripts/check-shader-artifacts.sh` | 36 shaders + opaque early-test variant match glslang 11:16.4.0 (parity proven against 16.2.0 and 16.4.0) |
| `scripts/check-bench-harness-provenance.sh a37fcba3c` | **DIVERGED**, 2 commits (see PERF-D8-2026-10-08-01) |
| `.claude/commands/_audit-validate.sh` | OK, all path references valid (symbol advisories only) |

Symbol and constant checks (grep and read), all held:

- **Dim 1**:
  - `drain_dirty_into_preserves_storage_capacity`; `take_dirty` only in tests;
  - animation `entities_scratch`/`playback_scratch`; billboard `last_cam`;
  - `SceneEffectSoftCache`; `SkinSlotPool.free_list`.
- **Dim 2**:
  - `DRAW_SORT_PARALLEL_THRESHOLD` = 3000; `opaque_depth_bucket`, `blend_pipeline_slot`, `pack_depth_state`; `build_instance_map`;
  - `needs_two_sided_blend_split` (`frame_params.rs:1888`);
  - baseline TSVs: `bench_draws_batches` ≤ `bench_draws_raster_cmds` in all five files (FNV 500/1284, FO3 511/1285, FO4 700/3618, Oblivion 18/20, Skyrim 643/1786). Only FO4 (3618) is above the 3000 threshold.
- **Dim 3**: `MIN/MAX_BLAS_BUDGET_BYTES` 256 MB / 1 GB, `BATCH_EVICTION_CHECK_INTERVAL` 64, `MIN_TLAS_INSTANCE_RESERVE` = `WORKING_SET_FLOOR` = 8192, `DEFAULT_STAGING_BUDGET_BYTES` 128 MiB, `DEFAULT_COUNTDOWN` = `MAX_FRAMES_IN_FLIGHT`.
- **Dim 4**:
  - `MAX_INSTANCES` 0x40000, `INITIAL_INSTANCE_CAPACITY` 0x10000, `MAX_MATERIALS` 16384, `MAX_INDIRECT_DRAWS` = `MAX_INSTANCES`;
  - `classify_pbr_keyword` only at `material_translate.rs:664` (spawn-time boundary, not a render path);
  - `memory-budget.md` roll-up ~1.95 GB at 1080p / ~4.28 GB at 4K native.
- **Dim 5**: `ENABLE_LEGACY_WRS` 0; only precomputed `invViewProj` fields for view-projection (the two shader-side `inverse()` calls are mat3, pre-existing, not the view-projection one); depth-history copy gate; `save_pipeline_cache_if_grown`; `QUERIES_PER_FRAME` 56.
- **Dim 6**: `plan_palette_dispatch`, `skinned_blas_refit_limit`, `SKINNED_BLAS_REFIT_THRESHOLD` 600 / `JITTER` 60, `skinned_blas_stays_fx_hashed`, `pose_dirty_crosses_the_crate_boundary_without_siphash`.
- **Dim 7**: `PRE_PARSE_RAYON_MIN` 8, `STREAM_PARSE_INPUT_BYTES` 64 MiB, `STAGED_BYTE_CAP` 256 MiB; `AppearanceProviders` sharing (#5061) unaffected by the `loot_appearance` delta.
- **Dim 8**: 28 `_ms` brackets, `BENCH_GPU_KEYS` 22 entries in reported order, `RENDER_ORIGIN_SNAP` = `EXTERIOR_CELL_UNITS`, `render/camera.rs` untouched.

### Checked and not filed (quantify first)

- **Loose `.mat` probe (#4277)**: every Starfield `.mat` merge now first calls `provider.extract_from_archives(&path)`. Vanilla ships zero loose `.mat`, so it is a guaranteed NotFound probe (path normalisation plus a per-archive name lookup). It runs once per unique material per NIF import, not per frame.
- **Voice playback (#5367 V)**: `dialogue_voice::play_line_voice` extracts and decodes on the main thread when a line is spoken. It is event-driven and uses the shared `SoundArchiveProvider`, so it does not repeat the #5061 pattern. Measure before filing if a hitch is reported on conversation open.
- **Legacy LOD index (#5222)**: `LegacyLodQuadIndex::scan` is FO3/FNV-only and runs once per streaming state. It allocates two Strings per archive name over the whole mesh + texture name tables. Cold path.
- **`generic_greeting_record`**: an O(DIAL records) scan per activation or force-greet, not per frame.
- **CELL walker dedup (#5309)**: `CellSubrecordFields` adds no per-subrecord allocation and is pinned equivalent by `walker_equivalence` tests.

## Prioritized Fix Order

1. **PERF-D8-2026-10-08-01 and the bench-of-record refresh (R6a-stale-25)**: the top *measurement* item. Nothing else here can be confirmed or cleared until a same-machine control exists with the current harness.
2. **PERF-D5-2026-10-08-01**: after the refresh, one pinned-tier A/B of `main_render_ms` across `34c3adf14` settles it. Any fix is a shader change, so measure first.
3. **PERF-D1-2026-10-08-02**: quick win (read the env var once or drop the probes).
4. **PERF-D1-2026-10-08-01**: early-out when the `SmTree` is empty; cache the key on a cell-context generation.
5. **PERF-D7-2026-10-08-01**: gate the corpse-set build on the game.
6. **PERF-D1-2026-10-08-03**: share one gather per frame and add a retry cadence for unseatable actors.
7. Tracked MEDIUM items #5289 (cache the per-cell water projection), #5288 (App-owned provider slot, #2039's design) and #5291 (build CDB indices off-thread).

## Stale skill premises

- **Dim 7 First step** lists the pathspec `byroredux/src/streaming.rs`. That file no longer exists. #5092 split it into `byroredux/src/streaming/{mod,pre_parse,telemetry}.rs`, so a `git log` over the old pathspec silently misses the new files. The pathspec sits inside one backtick span, so `_audit-validate.sh` does not see it.
- **Dim 7 Paths** still omit `byroredux/src/loading_screen.rs`, `byroredux/src/cell_loader/water.rs` (both carried the baseline's findings), `byroredux/src/cell_loader/reference_state.rs` and `byroredux/src/cell_loader/legacy_lod_index.rs` (this run).
- **Dim 3 Paths** still omit `byroredux/src/asset_provider/material/cdb.rs` and `loose_mat.rs`; `docs/engine/memory-budget.md` has no row for the Phase-2 index (#5291).
- **Dim 1 Paths** list the systems directory by name but predate `eat_sleep`, `forcegreet`, `story_events`, `npc_dialogue` and `dialogue_voice`, and the scripting-side `story_manager_dispatch_system`. The new per-frame M42 and Story Manager systems are exactly the ambient per-frame code the dimension targets.
- **Dim 5 Guard** should record that `scripts/check-shader-artifacts.sh` is the only thing that catches an uncompilable `triangle.frag` with stale committed `.spv` files. Between `34c3adf14` and `ebf1d0492` (#5368) the source did not compile (undefined `MAX_TRANSMISSION_SELF_SKIPS`) and the local gate was blocked by the glslang 16.2.0-only pin until `5c81a2f90`. Any GPU timing captured in that window is invalid.
- **Bench discipline** should say whether an *input-changing* harness edit (`a4ede5fa0`) is treated differently from an *output-column-only* one (`8fdd9ae56`). The provenance script reports both as DIVERGED.
- **Previous-report correction**: PERF-D8-2026-10-05-01 was a false positive (see Executive Summary); the baseline "Checked and still true" list otherwise holds: 28-bracket / 56-query inventory, `DRAW_SORT_PARALLEL_THRESHOLD` 3000, the streaming constants, the `needs_two_sided_blend_split` predicate, early-test eligibility 0..=16, `GpuLight` at 64 B.
- **Phase 4**: the skill says `rm -rf /tmp/audit/performance`. Under `/audit-suite` the scratch files (`/tmp/audit/performance/dim_1.md` … `dim_8.md`) are left for the orchestrator's reconciliation.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-08.md`
