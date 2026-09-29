**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-09-26b.md` (HEAD `b7491072f`) · **Audited**: Dims 1–8 (every dimension had commits in `b7491072f..9fcfdc3fc`: 73 commits in total) · **Unchanged since baseline (skimmed)**: none. Dims 6 and 8 had small deltas (3 and 6 commits) and got guard checks plus a delta read.

# Performance Audit — 2026-09-29

**Command**: `/audit-performance`, `--depth deep`, one leg of `/audit-suite --preset comprehensive`. **Mode**: static analysis plus unit-test and guard runs only. Per the suite constraints no engine, GPU process or benchmark was launched. **Every cost figure below is either derived from code or quoted from an earlier dated measurement; none was measured this run.**

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 (one is an eroded guard) |
| LOW | 5 |

- **Most important**: the new P4 NPC "Talk" arm of the per-frame interaction candidate scan (`ab31cfefe`, two commits before HEAD) walks **every placement root** each frame. Each one costs two `World::get` lock acquisitions, and each living actor adds a quest-alias walk with two Vec allocations. That is the per-candidate lock-churn pattern #3475 removed, back at a larger scale (PERF-D1-2026-09-29-01).
- **Eroded guard**: the #4796 dhat pin `bulk_array_allocates_exactly_one_output_buffer` failed in about half of the default-parallel runs this session. That is the command CI's `nif-heap-allocation-bounds` job runs. It passes 6/6 with `--test-threads=1` (PERF-D7-2026-09-29-01).
- Everything else is LOW: an allocation-heavy texture-residency probe in the new prefetch planner, a CPU-only field inflating `GpuLight` by 25%, the opaque early-test pipeline admitting only `material_kind == 0`, a third private archive set opened by the new mid-life gear loader, and a stale `pre_parse_cell` doc.

**Observed vs ROADMAP.** Nothing was measured, so no bench delta is reported. The Bench-of-record (LIVE, HEAD `a37fcba3c`, renderer-stepped, `scripts/fsr-bench-matrix.sh 3 300`) and its open regression **R6a-regress-22** (FO4 frame time doubled inside `4c9a5b36..99933f87b`; fence-bound; batch merge rate roughly halved at flat draw counts) stand as recorded. `scripts/check-bench-harness-provenance.sh a37fcba3c` returned: *"harness byte-stable since this record — a re-run is a valid apples-to-apples comparison against it."*

**Static attribution for R6a-regress-22.** No new finding; the bisect ROADMAP prescribes remains the next step.
- `186234944` added the early-test eligibility axis as `draw_sort_key` opaque slot 6, ahead of mesh. That splits each (render layer, two-sided) run into a late run and an early run, which can explain part of the **indirect-call** rise (Dugout 12c → 43c). It cannot explain the **batch** rise at flat draw counts (Prospector 54b → 257b at 928 → 904 draws): a mesh splits into two batches only if its instances disagree on eligibility.
- The same commit's per-frame 29.49 MB reservoir clear (`ReservoirBuffers::begin_frame`) costs about 0.06 ms of DRAM bandwidth at 1280×720, far below the +14 ms Dugout fence wait.
- That clear's first barrier (`restir.rs:150-158`, `FRAGMENT_SHADER|TRANSFER` → `TRANSFER|FRAGMENT_SHADER`) is a stage-wide dependency on all prior fragment work, the previous frame's composite, presentation and UI included. Whether that costs frame overlap needs a RenderDoc/Nsight capture (speculative-Vulkan caveat, low confidence).

## Hot Path Analysis

No new timings. The latest in-repo per-pass numbers are MedTekResearch01 from `AUDIT_PERFORMANCE_2026-09-26b.md`: renderer-stepped/pan, Native AA 1280×720, **RT quality tier pinned to 0**, HEAD `b7491072f`. Quoted for context only, not as a delta.

| Metric (MedTek, tier 0) | ms |
|---|---:|
| GPU main render (Native AA, median) | 66.214 |
| — of which opaque/alpha-tested (readback mean) | 60.380 |
| GPU skinned BLAS refit | 0.434–0.439 |
| GPU volumetrics | 0.879–1.328 |
| GPU SVGF | 0.476–0.488 |
| GPU sky cube | 0.201–0.210 |
| GPU TLAS build/refit | 0.030 |
| CPU build_render_data | 4.15–4.35 |
| CPU scheduler | 9.27–10.09 |

Since that measurement, the delta changes the CPU side in these ways (derived from code, not measured):
- `build_render_data` now overlaps palette assembly with camera extraction, and static meshes with lights, fog volumes and fog height, via nested `rayon::join` on the idle global pool (`de808add3`).
- `upload_lights` adds a ≤1023-entry `FxHashMap` remap per frame (`LightHistory`, `186234944`).
- The new interaction arm (finding -01) adds O(placement roots) lock traffic to the `Stage::Update` exclusive head.

GPU side: 36 shaders plus the opaque early-test variant are byte-reproducible from GLSL (`scripts/check-shader-artifacts.sh`). `ENABLE_LEGACY_WRS` = 0. No pass was added. The timer inventory holds 28 brackets (23 named + 5 main-render sub-phases) in 56 query slots, matching the skill.

## Findings

### Eroded guards

### PERF-D7-2026-09-29-01: The #4796 dhat pin fails in roughly half of default-parallel runs, which is how CI runs it
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/tests/heap_allocation_bounds.rs:77-97` (the assertion at `:93`); `.github/workflows/ci.yml:295-299`
- **Status**: NEW. The test came with `e2f99ad55`, just before the baseline. No issue matches "dhat" or "heap_allocation_bounds flaky" in open or closed issues.
- **Description**: `bulk_array_allocates_exactly_one_output_buffer` asserts `after.total_blocks - before.total_blocks == 1` around `read_u32_array`. `DHAT_LOCK` serializes the tests' own profilers, but dhat counts every allocation in the process. The libtest harness keeps allocating on other threads (test-thread spawn, output capture, result bookkeeping) while this profiler is live. The six sibling tests assert loose upper bounds and absorb that noise; this test asserts exact equality.
- **Evidence**: `cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds` failed in 5 of at least 9 parallel runs this session. The captured failure reports `left: 3, right: 1`. With `-- --test-threads=1` it passed 6/6, and it passes when run alone. The `_geometry` and `_import` binaries each passed 6/6.
- **Impact**: the CI lane guarding NIF parse-allocation hygiene (#832 / #833 / #831 / #408 / #4796) goes red on unrelated commits. That teaches maintainers to re-run or ignore it, which would hide a real return of the scratch copy. It also makes "propose a dhat bound" unreliable for this binary.
- **Related**: #4796 (closed; its fix is intact, only the pin is flaky), #1763, #4617.
- **Suggested Fix**: run the dhat binaries single-threaded (`-- --test-threads=1` in `ci.yml` and in the file's doc command), or make `heap_allocation_bounds` a `harness = false` sequential main. Keep the exact-count assertion; it is the only one that can see a scratch copy.

### New issues

### PERF-D1-2026-09-29-01: The NPC "Talk" candidate arm scans every placement root each frame with two `World::get` locks each, plus a quest-alias walk and two Vec allocations per living actor
- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:1258-1291` (the `populate_candidates` "talkable" block) and `:1338-1344` (the `PlacementContentWithheld` retain). Called every frame via `interaction_system` → `select_interaction_target` → `collect_candidates` (`:806`, `:872`, `:1192`). Also `crates/scripting/src/scene/quest_alias.rs:916-941` (`running_quests_binding_entity`).
- **Status**: Regression of #3475. The issue is still open, but its fix landed (`PERFORMANCE_FIX_STATUS_2026-09-26.md`: "Interaction selection snapshots each storage in separate scopes"). `ab31cfefe` (2026-09-29) brings the per-candidate lock re-acquisition back in a new arm. The O(n·m) `withheld` retain came with `f87490826` (2026-09-28).
- **Description**: the arm iterates `world.query::<SceneAliasCandidate>()`. That component is stamped on every placement root (`stamp_quest_reference`, `cell_loader/references/mod.rs:746`; `exterior.rs:333`), not only on actors. For each root, while the query guard is held, it calls `world.get::<Dead>` and then `world.get::<ActorValues>`. Each call is a TypeId map lookup, a `TrackedRead` (always on in release, per #3475) and an RwLock read. For each living actor it then calls `running_quests_binding_entity`, which:
  - takes 3 resource read locks,
  - walks every alias of every quest in `SceneQuestAliasRegistry` through `SceneActorBindings::resolve`,
  - collects, sorts and re-collects two `Vec`s,
  
  all only to test `.is_empty()`. Separately, `candidates.retain(|e, _| !withheld.contains(e))` is O(candidates × withheld).
- **Evidence**: see Location; `World::get` is `crates/core/src/ecs/world.rs:362-380`.
- **Impact**: O(placement roots) lock acquisitions plus O(actors × quest aliases) work and 2 heap allocations per actor, every frame, on the single-threaded head of `Stage::Update`. The answer changes only when bindings, quest running state, `Dead` or `ActorValues` membership change. Dense cells have tens of thousands of entities (MedTek: 39 560); the placement-root share was not measured. On the target hardware a CPU bottleneck is a bug. No quantitative guard exists for this site (per-frame ECS paths have no dhat coverage).
- **Related**: #3475, #3059, #2149, #3265 (same pattern fixed three times before); CONC audit (the nested `world.get` under a held query guard adds lock-order edges).
- **Suggested Fix**:
  - Drive the arm from the `ActorValues` query (actors only), and snapshot `Dead` once in its own scope, as the #3475 fix does.
  - Replace the per-entity `running_quests_binding_entity` call with a per-frame set of bound entities: one pass over the running quests' aliases, or a cache keyed on the `SceneActorBindings` dirty generation (`mark_scene_actor_bindings_dirty` already exists).
  - Make `withheld` an `FxHashSet`.

### PERF-D3-2026-09-29-01: `TextureRegistry::has_any_view_of_path` builds up to 16 keyed-path Strings per probe, and the prefetch planner calls it for every fresh texture on the main thread
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure (texture residency) / Streaming
- **Location**: `crates/renderer/src/texture_registry/lookup.rs:67-91`; key builders `texture_registry/mod.rs:1220-1241`; caller `byroredux/src/streaming_helpers.rs:720-725` (`prefetch_import_textures` → `TextureProvider::texture_resident`)
- **Status**: NEW (`a3632909a`)
- **Description**: the probe tries 4 clamp modes × {2D, cube} × {sRGB, linear}. Each combination re-runs `normalize_path` (an allocation), formats the clamp digit (another allocation), pushes suffixes, and does a SipHash `path_map` lookup. `any()` stops only on a hit, and the planner exists to find textures that are **not** resident. So the common case runs all 16 combinations, about 48 small allocations per fresh texture, on the main thread inside the budgeted apply slices the prefetch was built to relieve.
- **Impact**: main-thread allocator traffic proportional to unique textures × fresh models per crossing. Small per call, unmeasured. No quantitative guard exists for this site.
- **Suggested Fix**: normalize once and rewrite only the suffix in a reused buffer, or keep a secondary `FxHashSet` of normalized resident base paths, maintained beside `path_map`.

### PERF-D4-2026-09-29-01: `GpuLight.history_id` is CPU-only identity shipped in the GPU struct (+16 B per light) and mirrored in four shaders that never read it
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:362`; mirrors in `shaders/include/bindings.glsl:293`, `cluster_cull.comp:43`, `caustic_splat.comp:56`, `volumetrics_inject.comp:125`; producers `byroredux/src/render/lights.rs:183,244`
- **Status**: NEW (`186234944`)
- **Description**: `LightHistory::remap` runs on the CPU and uploads its result as the header's `previous_to_current` table. No shader reads `history_id`: a grep of `crates/renderer/shaders` finds only the four struct declarations. The field still grows the light stride from 64 to 80 B, so each light upload carries up to 16 KiB of unused bytes at the 1023-light cap. The upload repeats every frame whenever any light animates, because the dirty gate hashes the whole light bytes. It also adds a fifth copy to the Shader-Struct-Sync lockstep set.
- **Impact**: small upload waste and maintenance surface. Shader fetch cost is essentially unchanged: member-wise SSBO loads skip the unused vec4.
- **Suggested Fix**: carry the identities in a CPU-side array parallel to `gpu_lights`, moved through the priority sort in the `light_sort_scratch` tuple. Then drop the field from `GpuLight` and the four GLSL mirrors.

### PERF-D5-2026-09-29-01: Early-fragment-test eligibility admits only `material_kind == 0`, although lighting-shader kinds 1–16 have no discard or depth-write path
- **Severity**: LOW (an optimization gap in a landed feature; benefit unmeasured)
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/types.rs:374-384` (`DrawCommand::allows_early_fragment_tests`); kind source `crates/nif/src/import/material/dedicated_shader.rs:488` (`material_kind = shader_type`); discard sites `crates/renderer/shaders/triangle.frag:463,477,1208,1249,1290`
- **Status**: NEW
- **Description**: the certificate's doc asks for other kinds to be reviewed against every discard in `triangle.frag`. That review is mechanical:
  - `triangle.frag` never writes `gl_FragDepth`.
  - Its discards are the alpha test (already excluded by `alpha_threshold == 0.0`), the blend-only transparent-texel cull (already excluded by `!alpha_blend`), and the `MATERIAL_KIND_EFFECT_SHADER` (101) and `MATERIAL_KIND_FIRE_REFRACTION` (103) blocks.
  - `BSLightingShaderProperty` kinds 1–16 (env map, glow, parallax variants, face/skin/hair tint, eye env) only change shading.
  
  Every such draw is still routed to the late-test pipeline.
- **Impact**: on Skyrim and FO4 interiors, where env-mapped, glow and skinned-actor surfaces are a large share of opaque pixels, the early-Z saving is not realized for those draws. The opaque interval is about 95% of MedTek's main pass (26b report).
- **Suggested Fix**: replace `== 0` with an explicit allow-list of reviewed kinds, pinned by a source-scan test that fails when a new `discard` or `gl_FragDepth` appears under a `materialKind` branch. A/B it with `BYRO_PROFILE=1` `opaque_fragment_invocations` and `main_opaque_ms` at a pinned tier, and repeat the Kendall visual gate.

### PERF-D7-2026-09-29-02: `GearImportLoader` opens a third private archive set on the main thread at the first mid-life equip, and imports worn NIFs synchronously, bypassing `NifImportRegistry`
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/npc_spawn/loot_appearance.rs:549-600` (`GearImportLoader::step`)
- **Status**: NEW (`0182fc5e8`)
- **Description**: the loader mirrors `LootAppearanceLoader` by lazily calling `build_texture_provider` and `build_material_provider`. The first equip of an item the wearer never spawned wearing therefore re-opens every mesh and texture archive (headers and file tables) on the main thread during gameplay. The loader then keeps a third resident copy of those tables and a third BGSM cache. Each import extracts, parses and imports on the main thread (one NIF per frame) with no `NifImportRegistry` lookup, so the same armor on a second wearer is parsed again.
- **Impact**: a one-time first-equip hitch (unmeasured; scales with archive count) plus duplicated archive-index memory for the session. Event-driven, not per frame.
- **Suggested Fix**: share one provider set across the corpse and gear loaders (a World resource or the streaming state's `Arc<TextureProvider>`), and route the import through `NifImportRegistry`.

### PERF-D7-2026-09-29-03: `pre_parse_cell`'s doc still describes a serial coordinator extract, the exact regression shape #3659 / `67de801f8` removed
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming.rs:1700-1703`. It is contradicted by `:1870-1880` and by `pre_parse_one`.
- **Status**: NEW. `7a5cfa7d5` fixed the body comment but not the item doc.
- **Description / Impact**: the doc says the coordinator "extracts bytes serially through the archive provider while `parse_nif_pipeline` parses". The code admits against `STREAM_PARSE_INPUT_BYTES` and extracts inside each pool task. A maintainer following the doc could restore the serial extract. Doc only.
- **Suggested Fix**: reword to "admits each input against the decoded-input budget; each pool task extracts and parses its own input".

## Guard verification (all held unless listed above)

| Guard | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux draw_sort_key` | 27 passed, 2 ignored (manual benches) |
| Renderer lib, filters `acceleration static_blas_recovery scene_buffer skin_compute dispatch_skin gpu_timers material_tests hash_ bone_world` | 384 passed |
| Bin, filters `static_blas_recovery skin_dispatch_ran bench_gpu_keys atw_bracket sort_key interaction` | 69 passed, 2 ignored |
| Bin `streaming::tests` + `load_order::parallel_tests` | 36 passed, 1 ignored (real-order digest, not run) |
| NIF dhat `heap_allocation_bounds_geometry` / `_import` | 6/6 each |
| NIF dhat `heap_allocation_bounds` | **flaky in parallel**, 6/6 serial (PERF-D7-2026-09-29-01) |
| `scripts/check-shader-artifacts.sh` | 36 shaders + early variant reproducible |
| `scripts/check-bench-harness-provenance.sh a37fcba3c` | byte-stable since the record |

Symbol checks also held (grep and read):
- Dim 1: `drain_dirty_into` used by the per-frame systems (`take_dirty` only in tests), animation/billboard scratches, `SceneEffectSoftCache`, the debug-UI snapshot gate.
- Dims 3/4: `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`, instance-grow hash invalidation, `hash_light_upload` covers the remap, `hash_gpu_material_fields` single Fx write.
- Dim 5: the depth-history copy gated on `has_effect_soft_material`, `save_pipeline_cache_if_grown` on variant creation.
- Dim 7: #3813 order-dependent work kept on the calling thread, `StringsTableGuard` inside the walk.
- Dim 8: `NESTED_GPU_BRACKETS` is complete for the metrics map, and camera origin handling is unchanged.

## Prioritized Fix Order

1. **PERF-D7-2026-09-29-01**: one line in `ci.yml` plus the test doc. It restores trust in the NIF allocation gate.
2. **PERF-D1-2026-09-29-01**: restores the #3475 posture. Actor-driven iteration, a snapshotted `Dead`, a per-frame bound-entity set. Removes O(placement roots) lock traffic per frame.
3. **PERF-D3-2026-09-29-01** and **PERF-D7-2026-09-29-03**: small local cleanups on the streaming path.
4. **PERF-D4-2026-09-29-01**: ABI change (`GpuLight` plus 4 shader mirrors plus layout pins). Batch it with the next deliberate `GpuLight` edit.
5. **PERF-D5-2026-09-29-01**: needs a GPU A/B and the Kendall visual gate. Measure before landing.
6. **PERF-D7-2026-09-29-02**: provider sharing across the corpse and gear loaders is architectural. Pair it with any future loader consolidation.

The R6a-regress-22 bisect (ROADMAP) stays the top **measurement** item. None of the findings above is claimed to explain it.

## Stale skill premises

- Dim 7 says FO4 precombines are "CSG-decoded on the worker (`decode_precombine_csg`, `merge_precombine_materials`)". At HEAD the decode is on the worker, but `merge_precombine_materials` runs on the main thread in `finish_partial_import` (`cell_loader/partial.rs:81`). So does `merge_external_materials` for ordinary partial imports (`partial.rs:106-116`). This is pre-existing and not a regression, but the checklist line should say so.
- Phase 4 says `rm -rf /tmp/audit/performance`. The scratch files were kept for the `/audit-suite` orchestrator's reconciliation.
- The remaining premises checked out: the 28-bracket / 56-query inventory, `DRAW_SORT_PARALLEL_THRESHOLD` = 3000, `STREAM_PARSE_INPUT_BYTES` / `PRE_PARSE_RAYON_MIN` / `STAGED_BYTE_CAP` values, and the `needs_two_sided_blend_split` predicate.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-09-29.md`
