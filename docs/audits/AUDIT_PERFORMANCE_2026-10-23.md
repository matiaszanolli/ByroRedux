**HEAD**: `1d88bfe85` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-21.md` (HEAD `4a3e89d56`, 137 commits since) · **Audited**: Dims 1, 3, 4, 5, 7, 8 (delta read plus guard lanes) · **Unchanged since baseline (skimmed)**: Dim 2 (no batching or sort change), Dim 6 (one `byte_view` refactor in its paths)

# Performance Audit — 2026-10-23

**Command**: `/audit-performance`, eight dimension sub-agents (3 concurrent). Scratch: `/tmp/audit/performance/`.

**Mode**: static analysis, filtered guard-test lanes, and a few throwaway probes. No engine, GPU process or bench was launched. **No GPU frame time is claimed.** Every figure is labelled *measured* (probe or guard run) or *derived* (code plus a cited input).

## Executive Summary

| Severity | NEW | Existing / tracked (still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 1 (#6248) |
| LOW | 5 | 2 (#6313, #6315) |

- **No guard eroded.** Every `Guard:` symbol across the eight dimensions resolves and its lane passes (renderer lib `acceleration` 146, `texture_registry` 64, filtered `gpu_timers` 19, bin `draw_sort_key` 27 / `sort_key` 34, streaming 28 + 129, plugin 23, core 7 + 29, nif dhat 7+1+1, and the rest).
- **Both baseline MEDIUMs from 10-21 are resolved or reduced.** PERF-D4-01 (SVGF scene key churning every frame) is fixed by `68ecd0819` (#6247, closed). PERF-D3-01 (terrain height maps re-decoded on a cache hit) is fixed by `aa10b9bd6` (#6314). Baseline PERF-D1-01 (`trap_contact_system`) is fixed by `9290248d6` (#6312, closed). PERF-D1-02 (#6313) is partly fixed by `c10c48cca`.
- **One new MEDIUM, introduced by #6092 (`635330d08`)**: the LOD starvation re-arm can cycle (PERF-D3-02). It is derived from a code trace and is not reproduced.
- **PERF-D4-02 (#6315) got about 2× worse as a side effect of the #6247 fix** (`svgf_material_hash` now copies and masks 23 lanes per material). Measured: 98 µs @ 4,000 materials, 194 µs @ 8,000, against 57/113 µs in the baseline. The fix #6315 proposes (reuse the table's stored hashes) is no longer valid, because those hashes include the animated lanes.
- **Observed-vs-ROADMAP delta: none can be stated.** No bench was run. The live record is `d9a201c71` and the harness is byte-stable against it (see Dim 8). R6a-regress-23 (HEAD reads 1–9% slower than the control on every scene; `gpu_main` +0.5–1.2 ms on Cornell, Whiterun and MedTek is unattributed) is still open and a static pass cannot attribute it.
- The 137-commit delta is mostly cost-neutral refactor: #6271 exterior split, #6144 NIF dispatcher split (parse parity **measured**, below), #6160/#6188 `SubReader` reads, #6270 `byte_view`, #6272–#6274.

## Hot Path Analysis

### Per-pass GPU cost
Not measured (no engine). Pass inventory deltas since the baseline:

| Pass | Change | Timer | Status |
|---|---|---|---|
| Sky-cube bake | `cloud_shadow` bracket added (`994620123`, #6316): `QUERIES_PER_FRAME` **60**, **30** brackets (25 named + 5 sub-phases); `BENCH_GPU_KEYS` has 24 | `cloud_shadow_ms` ⊂ `sky_cube_ms` | author's measurement 0.033–0.034 ms inside ~0.22 ms (not re-measured here); an interior gate saves at most ~0.03 ms |
| Main pass, terrain | parallax march 6–20 steps within 20 m (#6197) | `main_opaque_ms` | author measured −0.04 ms mean `main_render`; not reproduced; `GpuTerrainTile` 224 B |
| Ground-cover blades | per-fragment root-colour fetches unchanged | `groundcover_blade_draw_ms` | #6248 still present; `groundcover_blade.frag` byte-unchanged |
| Ground-cover models | wind lean: two 2-octave analytic fbm per accepted foliage plant, no texture fetch (`1ca5032ae`) | `groundcover_models_ms` | derived, not benched |
| SVGF + shadow filter | #6247 moved sun/sky/fog out of the discrete key into `svgf_continuous_inputs_jumped` | `svgf_ms` ⊃ `shadow_filter_ms` | #5959 still open |

RT quality tier: not applicable, since no frame was rendered. Any later A/B must pin `--rt-test-ray-quality-tier`.

### Per-frame CPU (measured unless marked derived)

| Site | Cost | Label |
|---|---|---|
| `svgf_material_hash` loop (#6315) | 98 µs @ 4,000 materials, 194 µs @ 8,000, 24 µs @ 1,000 (pre-#6247: 50 / 99 µs) | measured, shape probe, hot cache |
| `obscript_object_system::collect_due` set rebuild (PERF-D1-03) | 1.5 µs @ 200, 6.6 µs @ 1,000, 36.6 µs @ 5,000 entities | measured primitive probe; the stub count itself is derived |
| `archetype_idle_system`, FO4 (PERF-D1-04) | a few µs to ~30 µs for tens to hundreds of actors | derived |
| `story_change_location_system`, Skyrim/FO4 (PERF-D1-05) | a few hundred ns, 2 `String` allocs per interior frame | derived |
| `encumbrance_system` remaining perk recompose (#6313) | a few µs | derived |
| `trap_contact_system`, live-actor map | tens of µs worst case in a trap-dense dungeon; grows with FO4 corpses since #6255 | derived |
| NIF parse across the #6144 dispatcher split (FNV `Fallout - Meshes.bsa`, release, `nif_stats`) | parent `4cd4bacbd` 3.66–3.67 s vs HEAD 3.65–3.66 s; 104-row type histograms byte-identical | measured |
| `EsmIndex::game_setting_float` linear scan (#6220) | ~0.6–2 µs per call at 1.9k–10k entries | measured, synthetic keys; dismissed |
| `persistent_refs_for_tile` (#6076) | ~25 µs per tile | derived |
| Geometry pool worst case | 4 M × 104 B + 16 M × 4 B = 480 MB (`mesh.rs:30-35`); the PERF-D3-02 cycle rebuilds this | derived |

### Harness provenance
`scripts/check-bench-harness-provenance.sh` (full history, non-shallow) against the live record: *"`d9a201c71`: harness byte-stable since this record — a re-run is a valid apples-to-apples comparison against it."* (exit 0). Older records read DIVERGED: `a3a7a43b0` with **2** edits (`fc83608d3`, `994620123`) and `a37fcba3c` with **4**. `994620123` has no recorded disposition (PERF-D8-01).

## Findings

### Eroded guards

None.

### New issues

### PERF-D3-02: #6092's starvation re-arm reopens on every pool compaction, so a partially-uploaded LOD quad can cycle decode, partial upload, unload, compaction, reopen, re-arm
- **Severity**: MEDIUM (could be HIGH if it reproduces, because each cycle includes an O(pool) geometry rebuild)
- **Dimension**: GPU Memory Pressure
- **Location**: `byroredux/src/app_step.rs:137-147` (the re-arm edge); `byroredux/src/cell_loader/lod_support.rs:419` (`lod_should_rearm_after_starvation` = `starved && admission_open`); `byroredux/src/cell_loader/{object_lod,placement_lod}.rs` (partial upload, then `unload_*_lod_block`); `crates/renderer/src/mesh/geometry_ssbo.rs:268` (`scene_geometry_admission_closed = false` inside `plan_geometry_compaction`); `crates/renderer/src/mesh.rs` (`drop_mesh` sets `geometry_has_holes`).
- **Status**: NEW (introduced by `635330d08`, #6092/#6093, both closed). Related and open: #6106, a different gap.
- **Description**: since #6092 a multi-sub-mesh LOD quad (object `.bto` quad, placement cell) whose k-th sub-mesh trips the hard pool cap is abandoned whole, and `unload_*_lod_block` drops the k−1 sub-meshes that already uploaded. Those drops set `geometry_has_holes`, so the next geometry rebuild runs `plan_geometry_compaction`, which reopens admission **unconditionally** (verified by reading `geometry_ssbo.rs:265-270`). The re-arm test only looks at the closed-to-open edge and never checks that the pool gained headroom. After compaction the pool is back to its pre-attempt layout, and the closest-first order is deterministic, so the same quad is the first candidate again and fails at the same sub-mesh.
- **Evidence**: code trace above (the reopen was re-read by the orchestrator). The existing tests (`a_capacity_refusal_is_held_back_not_cached_as_a_miss`, `a_held_back_quad_is_retried_after_admission_reopens`, `starvation_rearms_only_once_admission_reopens`) exercise pure classifiers with a fake admission flag. None models "unwinding my own partial upload reopens admission".
- **Impact**: while the pool is saturated and the nearest unresident quad is multi-sub-mesh, each cycle costs an archive read plus decode, partial GPU-pool appends, a chunked O(pool) geometry rebuild (typical ~208 MB, cap 480 MB), and possibly a `geometry_rebuild_needs_idle` device-idle wait at low VRAM headroom. Per-cycle cost is bounded by `LodWorkBudget`, but the cycle can recur every few frames. This is the "re-decode and re-reject" regression #6054 was written to prevent, moved onto a compaction-driven path. Terrain LOD is single-mesh and unaffected. **Confidence: medium** (not reproduced). Runtime tell: a repeating `Geometry pool admission reopened — retrying the held-back LOD quads` log line with `lod_starved` non-zero on the bench line and no cell unload between repeats.
- **Related**: #6054, #6092, #6093, #6106.
- **Suggested Fix**: add hysteresis (record live vertex/index counts when the Starved arm fires and re-arm only once usage has dropped by at least the held-back quad's footprint), or pre-check the whole quad against remaining headroom before uploading any sub-mesh. Add a test that drives a fake registry through partial upload, unload and compaction and asserts no re-arm.

### PERF-D1-03: `obscript_object_system::collect_due` rebuilds std `HashSet`s over every stub entity each frame and probes a SipHash `malformed` set per block per instance
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/scripting/src/obscript_objects.rs:290-299` (`loaded`, the new `not_loaded`, `died`), `:337` (`malformed.contains` inside the instance × block loop), `:99-106`.
- **Status**: NEW. `not_loaded` arrived with `335994b4c` (#6220).
- **Description**: runs every frame on FNV/FO3/Oblivion once any `ObScriptInstance` exists. The `not_loaded` set collects every `UnloadedReferenceStub` and `RemoteSceneActorStub` into a `Vec` and then a SipHash `HashSet`, only so each GameMode instance can ask `contains(entity)`. `malformed` is probed per block per instance although it essentially never matches.
- **Evidence**: measured primitive probe (see the CPU table). Stub counts per worldspace are unmeasured; the engine logs `N logical-only actor identities`. No quantitative guard exists for this site.
- **Impact**: marginal, under ~10 µs per frame for a few hundred stubs.
- **Suggested Fix**: replace `not_loaded` with per-instance `world.has::<UnloadedReferenceStub>(entity)`, build `loaded`/`died` only when their storages are non-empty, and move `malformed`/`scripts` to Fx collections.

### PERF-D1-04: `archetype_idle_system` (FO4) collects every animated actor into a fresh `Vec` each frame, and #6292 added a per-frame `retain` under a resource write lock
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/archetype_idle.rs:55-110` (the `actors` Vec at `:68-77`, `seen.unmatched.retain` at `:72`, per-actor probes at `:81-87`).
- **Status**: NEW (the system predates the baseline; `15bf18ab6` #6292 added the `retain`).
- **Description**: once an actor settles the system does nothing for it, yet every frame it collects all `AnimationTarget` entities, takes the `ArchetypeIdleSeen` write lock and does three sparse probes plus a resource-lock probe per actor. The answer can only change when `AnimArchetypeOverrides` changes, an `AnimationTarget` is added, or an actor loses its player/marker.
- **Evidence**: code read; cost derived (tens to hundreds of actors in FO4 settlements). No quantitative guard exists.
- **Impact**: negligible CPU; it is the pattern the dimension names.
- **Suggested Fix**: key the pass on `AnimationTarget`'s structural generation (`TRACK_CHANGES`) plus the overrides revision, or hold a persistent scratch `Vec` and `clear()`+`extend`.

### PERF-D1-05: `story_change_location_system` re-derives `PlayerCellKind` and `PlayerLocation` from string-keyed maps every frame on Creation titles
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/story_events.rs:29-46`, `:59-71`, `:94`, `:117-132` (interior `cell_editor_id.to_ascii_lowercase()` at `:122-125`).
- **Status**: NEW (`6f6a9eb3d` added the publish; the #5447 guard covers only the CLOC key).
- **Description**: on Skyrim/FO4 (non-empty `LocationTable` and `SmTree`) the system calls `resolve_current_lctn` twice per frame, each allocating a lowercase `String` in an interior, plus several string-hash lookups. The result changes only on a cell transition, which already replaces `CurrentCellContext`/`CurrentExteriorContext` wholesale.
- **Evidence**: code read; derived cost a few hundred ns per frame.
- **Impact**: negligible; a per-frame allocation that a change key removes.
- **Related**: #5447, #5372 (keep the "resolve before the marker guard" lock order if refactored).
- **Suggested Fix**: cache `(cell identity → location, kind)` keyed on the identity hash `location_key` already computes, and share one resolve between both uses.

### PERF-D8-01: `994620123`'s harness edit has no recorded disposition in the provenance script
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `scripts/check-bench-harness-provenance.sh:26-30` (the `DISPOSITION` table); the edit is `scripts/fsr-bench-matrix.sh:214,321`.
- **Status**: NEW
- **Description**: the commit appends the `gpu_cloud_shadow` column. Its siblings `8fdd9ae56`, `a4ede5fa0` and `fc83608d3` each got a one-line disposition (#5451). This one did not, so every record older than it prints an unannotated DIVERGED.
- **Evidence**: a no-argument run lists `994620123` under `a3a7a43b0` (2 edits) and `a37fcba3c` (4) with nothing beneath it. The diff is a pure column append.
- **Impact**: tooling/doc rot only; the live record is byte-stable.
- **Suggested Fix**: add `[994620123]="by inspection: appends the gpu_cloud_shadow column only (#6316; a bracket nested inside gpu_sky_cube); no run arguments, gates or arithmetic changed; d9a201c71 was controlled under it (docs/audits/CONTROL_stepped-camera_a3a7a43b0_under-994620123.tsv)"`.

### PERF-D8-02: `bench_gpu_keys_match_the_reported_bracket_order` pins 10 of 24 key positions
- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/main.rs:423-444`, binding the order in `byroredux/src/app_events.rs:1299-1366,1425-1470`.
- **Status**: NEW (related: #3629)
- **Description**: beyond the length (24) the test asserts indices 4 and 11–18. Indices 0–3, 5–10 and 19–23 (including `shadow_filter` and `cloud_shadow`, the two newest nested children) are unpinned. `every_bench_gpu_key_is_printed_on_the_bench_line` checks presence only. Values come from three hand-maintained parallel lists of 24, each edited by hand on every append.
- **Evidence**: the current order was verified correct by reading all three lists side by side. A transposition would still pass the suite and silently mislabel a bench column.
- **Impact**: latent; the harness extractors would trust a mislabelled column.
- **Suggested Fix**: assert every index, or build the ms/active arrays from one `[(key, ms, active); N]` table so key, value and flag cannot drift.

### Existing / still present

- **#6248 (PERF-D5-01, MEDIUM)**: every grass-blade fragment recomputes the blade's constant root ground colour, with up to ~18 bindless fetches. `groundcover_blade.frag` is byte-unchanged since the baseline; the wind commit changed only the blade vertex wind function (line refs drifted to `groundcover_blade.vert:410-424`). The fix is still deferred pending a `groundcover_blade_draw_ms` A/B.
- **#6313 (PERF-D1-06, LOW, partly fixed)**: `encumbrance_system` no longer clones `Inventory` and `head_look_system` caches its head bone. Still every frame: the player's perk list snapshot into a `Vec`, a `due` list cloning `PerkEntryPointEntry`, and the `applied` Vec. This runs even if no held perk touches `GET_MAX_CARRY_WEIGHT`. The commit message says the rest needs a change signal on `Inventory`/`ActorValues`/perks that does not exist yet. Keep at LOW or close with that note.
- **#6315 (PERF-D4-02, LOW)**: now measured at 98 µs @ 4,000 and 194 µs @ 8,000 materials. **Its proposed fix is invalid after #6247** (the stored hashes include the animated lanes, so folding them would re-key history every frame). Better fix: share one `hash_material_slice` result between the SVGF key and `upload_materials`, and cache the folded key while that slice hash is unchanged. Edit the #6315 body.
- **#5959**: still open (the shadow-filter bracket cost and the unattributed `gpu_main` delta, with R6a-regress-23).
- **#6316 (PERF-D5-02)**: the bracket now exists and shows 0.033 ms inside ~0.22 ms. The remaining cost-motivated work is at most ~0.03 ms, so the issue could close on the numbers.
- **Carried forward unchanged (code untouched in the window)**: #6114, #6116, #6097, #6106, #5764, #5365.

### Checked, no finding (changed code)

- **Dim 1**: `head_look_system`, `obscript_hit_system`, `obscript_stage_result_system`, `take_requests` (#6150), the #5155 character probe, package weekday/date math, `ResetInventory`, trap motion samples (#6236), ragdoll changes (#6255).
- **Dim 3**: #5664 shadow-count memo (bounded at 512, cold path), #5665 CDB panic memoised inside the `OnceLock` initialiser, #6330 loose tier, #6270, #6274, #6273 (tests only). The BLAS budget, admission, scratch-floor, deferred-destroy and staging constants all match the skill text, and no new screen-sized allocation landed.
- **Dim 7**: #5715 stale-payload salvage is queued and finished one import per budget unit, ahead of the active apply. #6092/#6093 cost at most one wasted quad decode per ring per reopen (see PERF-D3-02 for the hole). #6094 forced subdivision raises the finest-band count by a **derived** ~30–50 level-4 quads, unmeasured. All the per-crossing latches, batched teardown, `SharedParseCache`, the created-refs gate and `PRE_PARSE_RAYON_MIN`/`STREAM_PARSE_INPUT_BYTES` guards are intact.
- **Dim 8**: the camera-relative origin is still one `look_at_mat4` plus `rebase_model_matrix` inside the single draw loop, and history is preserved across grid crossings. The 18 `cpu_ms` buckets, the scratch-telemetry coverage tests and the cloud-shadow bracket plumbing in every consumer are correct.
- **Dim 2**: the 12-tuple `draw_sort_key`, `DRAW_SORT_PARALLEL_THRESHOLD` = 3000 and the indirect group key are unchanged. In the 10-07 baseline TSVs only FO4 Institute (3,618 raster commands) crosses the threshold, and batches never exceed raster commands.
- **Dim 6**: only `dedf6c0d6` (`NoUninit` on the skin push-constant structs, casts through `byte_view`) touched its paths. All four guard tests are green and the landed-fix checklist holds.

## Prioritized Fix Order

1. **PERF-D3-02** — re-arm hysteresis or a whole-quad headroom pre-check, with a test for the partial-unwind path. First confirm with the runtime tell (a repeating "admission reopened" log line) on a saturated-pool FO4/Skyrim `--grid` run.
2. **#6315** — correct the issue body, then share one material-slice hash between the SVGF key and the upload gate (~100–200 µs at 4–8 k materials).
3. **#6248** — hoist the blade-root colour out of the fragment shader, after a `groundcover_blade_draw_ms` A/B.
4. Quick wins, each only a few µs: PERF-D1-03 (`has::<…>` instead of per-frame sets), D1-04 (persistent scratch or a generation key), D1-05 (cache by cell identity), #6313 remainder (needs a change signal first).
5. Tooling: PERF-D8-01 (one disposition line) and PERF-D8-02 (pin every index, or one table).

## Stale skill premises (`.claude/commands/audit-performance/SKILL.md`)

- **Preamble**: `a3a7a43b0` now reads DIVERGED with **2** edits (`fc83608d3`, `994620123`), and `a37fcba3c` with **4**. The `994620123` edit has no disposition (PERF-D8-01).
- **Dim 1 Paths** omit the newer Update-stage systems: `systems/{trap_hits,head_look,magic_shader,graph_locomotion,archetype_idle,script_requests,knockdown,flee,revive}.rs`, `byroredux/src/encumbrance.rs`, `systems/story_events.rs` (the location/cell-kind publish) and `crates/scripting/src/{obscript_objects,obscript_results,obscript_state}.rs`.
- **Dim 1 Guard** is missing `trap_contact_system`'s owner cache (`TrapOwnerCache`, #6312, `the_cached_owner_map_follows_script_and_parent_changes`), `encumbrance_system` not cloning `Inventory`, and `HeadLook.head` (`the_head_bone_is_cached_and_revalidated`, #6313). The `story_change_location_system` early-out text holds for the CLOC key only; the `PlayerCellKind`/`PlayerLocation` publish runs before it.
- **Dim 2 Paths and First step** omit `crates/renderer/src/vulkan/context/geometry_pass.rs` (the indirect gather loop and `cmd_set_depth_bias` gating). The test command should read `cargo test -p byroredux --bin byroredux draw_sort_key`, and renderer-side guards need `cargo test -p byroredux-renderer --lib <filter>`. The Guard line omits the `frame_params.rs` test modules (`group_state_tests`, `needs_two_sided_blend_split_tests`, `should_use_indirect_draws_tests`).
- **Dim 3**: the Starved/#6054 sentence is incomplete since #6092/#6093 (held-back verdict, the closed-to-open re-arm, `lod_starved`, `starve_lod`, the `lod-coverage` gate) and should name PERF-D3-02. The archive-handle bullet needs the #5664 shadow-count memo, the CDB bullet needs #5665, and the loose-tier note needs #6330 (Starfield `geometries` root and loose CDBs). Paths omit `asset_provider/material/merge.rs` and `asset_provider/terrain_height.rs`. `GpuTerrainTile` is 224 B (`gpu_terrain_tile_is_224_bytes`), not 208 B.
- **Dim 4**: the `fold_svgf_scene_inputs … unmeasured` sentence is stale. It is measured (98/194 µs), the hash is `svgf_material_hash`, sky and fog left the discrete key via `svgf_continuous_inputs_jumped`, and the stored-hash fix is invalid. Add the #6247 guards `per_frame_animated_material_lanes_do_not_rekey_history` and `continuous_sky_and_fog_reset_history_on_a_jump_not_on_drift`. The `bone_world_slot_needs_copy` tests live in `bone_world_slot_state_tests`. `hash_material_slice` is one bulk `write` over the whole slice.
- **Dim 5 inventory**: the "Main geometry pass" row omits terrain parallax (#6197) and the "Ground-cover model tier" row omits the wind lean (`GROUNDCOVER_MODEL_RECORD_FLAG_SWAYS`). The 60/30 bracket count and the table are correct, and the baseline's 58/29 was true only at its own HEAD.
- **Dim 7**: `cell_loader/exterior.rs` is now a 243-line hub, so add `cell_loader/exterior/{apply_job,persistent,worldspace}.rs` plus the LOD ring files (`terrain_lod.rs`, `terrain_lod_btr.rs`, `object_lod.rs`, `placement_lod.rs`, `tree_lod.rs`, `lod_coverage.rs`, `material_flags.rs`), `references/{import,synth_child}.rs`, `crates/nif/src/blocks/dispatch.rs` (#6144) and `crates/plugin/src/esm/{sub_reader.rs,records/reflection.rs}`. The #5512 salvage sentence needs the #5715 queued shape (`stale_salvage`, `salvage_one_queued_import`, `queued_salvage_finishes_one_import_per_call`, `frame_loop_salvage_is_queued_not_synchronous`). Add the #6092/#6093/#6094 guards. The #6076 "quantify before filing" note can close at ~25 µs per tile. Source-pin tests now scan `exterior::production_source` (the directory).
- **Dim 8**: `bench_gpu_keys_match_the_reported_bracket_order` enforces the length (24) and 10 positions, not the full order (PERF-D8-02).
- **Dim 6**: none.
