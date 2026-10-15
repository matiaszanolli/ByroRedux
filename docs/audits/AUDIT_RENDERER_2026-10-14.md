**HEAD**: `adbd468e2` · **Baseline**: `AUDIT_RENDERER_2026-10-12.md` (HEAD `5d31fd637`) · **Audited**: Dims 1–12 (delta `5d31fd637..adbd468e2`, 328 commits; 16 under `crates/renderer`) plus guard runs · **Unchanged since baseline (skimmed)**: Dim 9 (only the #5593 binding-helper swap in `skin_compute.rs` and one comment re-point), Dim 11 (only #5593 in `presentation.rs`/`exposure_meter.rs` and the `RENDER_DEBUG_MODE_MAX` bump), Dim 10 (only #5593 in `ClusterCullPipeline` and #5370's `camera_static`)

# Renderer Audit — 2026-10-14 (delta)

The window is dominated by the ground-cover grass rework, the #5593 descriptor-layout/`try_or_cleanup!` refactor, and two `triangle.frag` ReSTIR/GI changes. The #5593 refactor was checked binding-by-binding across about 15 pipelines and is behaviour-preserving. One real defect landed: **`c2647d468` makes `triangle.frag` read a `writeonly` SSBO** (HIGH). The #5370 `camera_static` fix has a tolerance problem that three dimensions found from opposite directions (MEDIUM). Everything else is LOW doc rot or missing pins.

**Method**
- Static review only. Twelve dimension agents ran read-only; none launched an engine or Vulkan process.
- Per dimension: `git log`/`git diff 5d31fd637..HEAD` over its Paths, the delta read in full, then the owning guards on the default toolchain (renderer lib) and rustc 1.96 (bin crate).
- Dedup against `/tmp/audit/renderer/issues.json` (200 most recent, all states), open-issue keyword searches and closed-issue searches.
- I re-checked the HIGH finding's premise myself: `bindings.glsl` declares binding 16 `writeonly` and `triangle.frag` loads `reservoirsCurr[pixelIdx].histLenAndDepth`. Dim 7 independently confirmed the `NonReadable` decoration plus `OpLoad` in the committed SPIR-V.

**Guards run** (all pass, none `#[ignore]`d unless noted)
- Dim 1: `acceleration` 146; `build_tlas_is_preceded_by_an_unconditional_as_write_to_as_read_barrier`; `static_blas_recovery_*` 3; bin `static_blas_recovery_runs_between_frames_not_in_the_render_driver`.
- Dim 2: `shader_contract` 130; `light_history` 8; `restir` 19; `shader_constants` 66; `camera` 33; `view_proj_static` 2.
- Dim 3: wide filter `scene_buffer material shader_constants gpu_` 340 passed / 1 ignored (GPU-only); bin `groundcover` 66; all 36 shaders recompiled with plain `-V` byte-identical to committed `.spv`.
- Dim 4: 20 named renderer guards plus 83 filtered; bin 44.
- Dim 5: all named guards plus `memory_budget_ledgers_the_sky_and_ground_cover_owners`; 38 reflection tests.
- Dim 6: bin `asset_provider` 241 / 11 ignored (game-data gated); `material_translate` 77; `cross_game_translation_completeness --ignored` passes on all 8 games; core `inspect` 2.
- Dim 7–12: `taa`/`svgf`/`bloom` guards; `volumetrics caustic water fog` 203; bin `fog` 77; `exposure` 14, `tonemap` 16, `upscal` 30, `post_passes` 17, `fsr3-sys` 8; `gpu_timers` 17, `render_debug` 10, `bench_gpu_key` 2, `mat_set_tests` 10; Dim 10 bin `render::lights` + `systems::light_anim` 51.
- `scripts/check-shader-artifacts.sh`: clean.
- Not run (needs a GPU): `cornell_parked_penumbra_converges_at_one_shadow_ray`; the Cornell forced-low-budget gate (open #5761).

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | REN-H-01 |
| MEDIUM | 1 | REN-M-01 |
| LOW | 11 | REN-L-01 … REN-L-11 |

Dimension reports are in `/tmp/audit/renderer/dim_N.md` (removed by cleanup; the finding IDs below keep the source dimension).

## RT Pipeline Assessment

- **AS (Dim 1):** no change under `acceleration/`, `resources.rs`, `begin_frame_recording.rs`, `build_and_upload_instances.rs` or `dispatch_skin_and_cluster.rs`. The TLAS consumers' bindings survived the #5593 refactor unchanged. `step_static_blas_restore` still runs after `step_streaming`.
- **Ray-query shading (Dim 2):** the GI hemisphere move to `ditheredSequence2` (`0b56f1133`) is sound; the salt `MAX_LIGHTS` is one past every light index and the mask quality was measured. The shadow-transport reorder is a pure move. **The ReSTIR reservoir ownership change is not sound (REN-H-01).**
- **Denoiser (Dim 7):** SVGF/TAA/composite/bloom inputs are unaffected by the reservoir change (the reservoir is zero-filled every frame in `begin_frame`). `camera_static` classification is the open question (REN-M-01).
- **Barriers/layouts:** no barrier, render-pass or layout edit in the window. Nothing was sync-validated live (see Needs-RenderDoc).

## GPU-Struct & Memory Assessment

- `GpuMaterial`, `GpuInstance`, `GpuCamera`, `GpuLight`, `GpuTerrainTile` and `bindings.glsl` are untouched. All size and offset pins pass. Every new constant is single-sourced in `SHADER_DEFINES` and the generated header matches byte for byte.
- #5593's shared `try_or_cleanup!` is identical to the twelve local copies it replaced, and `create_per_frame_uniforms` frees partial buffers on failure. No new `?`/`unwrap` in the added production lines. `teardown.rs`, `init.rs` and `resize.rs` are unchanged, and the three load-bearing teardown orderings hold.
- Ground cover: the ledger rows (134,831,492 B and 17,418,288 B) match the code and are fixed-size, so not billing them in `screen_scaled_reservation_bytes` is correct. The prose around them is stale (REN-L-09, REN-L-10).

## Findings

### REN-H-01: `triangle.frag` loads from `reservoirsCurr`, which `bindings.glsl` declares `writeonly` — undefined read, and the "nearer fragment keeps the claim" RMW has no memory dependency
- **Severity**: HIGH
- **Dimension**: Ray Queries (ReSTIR reservoir ownership) / Sync
- **Location**: `triangle.frag` ReSTIR finalize tail (`claimed = unpackHalf2x16(floatBitsToUint(reservoirsCurr[pixelIdx].histLenAndDepth))`, then the conditional `reservoirsCurr[pixelIdx] = rc`, under `if (stableTemporalSurface)`), from `c2647d468`; declaration `ReservoirCurrBuffer` in `include/bindings.glsl` (set 1 binding 16)
- **Status**: NEW. Violates the contract of closed #5062 (its `begin_frame` barrier is intact). No open issue matches `reservoirsCurr` or reservoir ownership.
- **Description**: #5062 made binding 16 `writeonly` so sync validation would not classify it as a fragment `SHADER_STORAGE_READ` interface. `c2647d468` adds a load from it. glslang accepts it, but the SPIR-V decorates the block member and variable `NonReadable` and still emits an `OpLoad` through them; reading a `NonReadable` object is undefined. Independently of the qualifier, the claim is a non-atomic load-then-store: no pipeline barrier between one draw's store and the next draw's load, the binding is not `coherent`, and overlapping invocations on the same pixel can interleave between load and store, so "nearer wins" is only probabilistic.
- **Evidence**:
  - `bindings.glsl`: `layout(std430, set = 1, binding = 16) writeonly buffer ReservoirCurrBuffer { Reservoir reservoirsCurr[]; };`, with a comment saying `triangle.frag` "only ever stores this frame's reservoirs".
  - `spirv-dis triangle.frag.spv`: `OpMemberDecorate %ReservoirCurrBuffer 0 NonReadable`, `OpDecorate %__12 NonReadable`, and an `OpAccessChain`/`OpLoad` at the `claimed` read. `triangle_early.frag.spv` carries the same decorations. The baseline SPIR-V has only the store.
  - Both `.spv` files are byte-identical to a fresh `glslangValidator -V` compile of HEAD (this is not a stale-artifact artefact).
  - Guard gap: no `shader_contract`/`restir` test mentions `claimed`, `reservoirsCurr` or the access mode of binding 16.
- **Impact**: the commit measured a real win on the dev NVIDIA card (FO4 Third Rail: 26% of lit pixels no longer lose history each frame; decal floor stains stop speckling), so visible behaviour is currently good. Risk is portability: another vendor or driver revision may compile the `NonReadable` load to garbage or a stale line, flipping the claim test at random and re-creating the history loss this commit removed, or leaving a farther hidden surface owning the slot. `BYRO_VALIDATION=1` is probably blind to it because of the `NonReadable` decoration, so the validation lane cannot clear it.
- **Related**: #5062; CONC-D2-2026-09-29-01 (predicted exactly this RAW); `AUDIT_RENDERER_2026-09-27` "Reservoir ownership under overdraw"; REN-L-04.
- **Suggested Fix**: do not just delete `writeonly` (that reintroduces the #5062 validation red and leaves the race). Resolve ownership with a real atomic: a per-pixel `uint` claim word resolved by `atomicMin` on a (packedDist, order) key, in a correctly qualified binding cleared by the same `begin_frame` fill, then write the full reservoir only for the winner; or resolve ownership from depth after the geometry pass. Confirm with a RenderDoc capture on an overdraw-heavy interior and a `BYRO_VALIDATION=1` run, update the `bindings.glsl` comment, and add a source-shape pin for the chosen access mode.

### REN-M-01: `view_proj_is_static` (#5370) tolerance depends on the 4096-snapped render origin, so it is too tight for some standing poses and too loose for near geometry
- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite (consumers: ReSTIR history mode, SVGF `params.w`, caustic parked EMA)
- **Location**: `view_proj_is_static`, `VIEW_PROJ_STATIC_REL_EPS`, `VIEW_PROJ_STATIC_ABS_EPS`, test `view_proj_static_tests` in `context/assemble_camera_and_lights.rs` (`b909a6928`); `snap_render_origin` (`scene_buffer/constants.rs`)
- **Status**: NEW (the original #5370 is CLOSED; this is a residual of its repro and a bound-derivation error, not a regression). Merges REN-D7-2026-10-14-01, REN-D2-2026-10-14-02 and REN-D4-2026-10-14-01.
- **Description**: the test allows `1e-6 + 1e-5 * max(|a|, |b|)` per matrix entry. The view-projection is render-origin-relative and the origin is snapped to a 4096-unit grid, so translation entries scale with the camera's offset from an arbitrary grid plane, not with pixel motion. Two consequences:
  - **Too tight (Dim 7):** the measured wobble (±1.6e-3 on Y) clears the flag only if `1e-5 * |entry| >= 1.6e-3`, i.e. eye height above the snapped plane of at least 160 units for a level camera. A numeric model of the shipped predicate gave 0% parked at h=128 level and 100% at h=364 level; with ±20° pitch, 63–84% of poses were parked for h=60–364. The speckle #5370 reported can therefore persist depending on cell position and view direction.
  - **Too loose (Dim 2, Dim 4):** the docblock derives the sub-pixel bound at the origin's clip `w` (~2120). A surface at `w` = 10 sees the allowed translation delta (~0.045 u) as about 2.9 px per frame, so slow camera creep still reports parked.
- **Evidence**: the Dim 7 figures are from its model, not a live capture; the 160-unit threshold is analytic. `view_proj_static_tests` pin one matrix (the Institute capture, `vp[13]=879`) at one eye height, so they cannot see either direction. Dim 7 judged the too-loose side as slight indirect-term softening only: motion vectors still use the true previous matrix, so there is no mis-registration or disocclusion ghosting.
- **Impact**: where the flag stays false for a standing player, ReSTIR keeps its moving cap (16 / floor 0.1), SVGF keeps its alpha floor and the caustic EMA never engages. Behaviour varies with position and heading, which is hard to bisect. Where it is too loose, slow drift engages the deep ReSTIR tail (cap 1024 / floor 0.002) and progressive SVGF. Not measured live.
- **Related**: #5370, #4942, #5369, REN-L-01.
- **Suggested Fix**: test in an origin-independent space: the reprojection error of a few probe points at near and mid depth (max NDC delta below a fraction of a pixel), or compare camera position (world units) and rotation block separately. Extend `view_proj_static_tests` with the wobble at eye heights 60 and 128 above the origin plane (must clear) and a 0.05-unit step 2000 units from the origin (must not clear). The `restir_history` debug view (red = history rejected) is the quickest on-device check.

### REN-L-01: with #5370 live, SVGF progressive accumulation engages in every standing session, but its "scene unchanged" key ignores emissive/material/ambient changes
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `caustic_scene_key` / `caustic_history_valid` in `build_and_upload_instances.rs`; `svgf_temporal.comp` (`floorC = params.w > 0.5 ? 0.0 : params.x`, history age cap 255) — REN-D7-2026-10-14-02
- **Status**: NEW
- **Description**: before #5370, `camera_static` was almost never true for a standing player; now a parked player with an unchanged light rig gets `alpha = 1/(histAge+1)` down to 1/256. The key covers rigid instance motion, instance-set changes, `pose_dirty` and the every-float light rig, but not animated emissives, quest-state material swaps or ambient/DALC/fog/sky colour.
- **Impact**: seconds of lag in indirect response to animated emissives (neon, TV, lava) while the player stands still. Not measured.
- **Suggested Fix**: only after a capture confirms it, fold a per-frame emissive/material generation counter into `caustic_scene_key`. Do not widen the key speculatively.

### REN-L-02: `restir_history` and earlier raw debug views are painted late in `main()`, so window-portal and thick-glass fragments overwrite them
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `triangle.frag` window-portal return (~l.2017) and thick-glass IOR return (~l.2701) versus the `viewRestirHistory` / `viewRestirLight` blocks — REN-D12-2026-10-14-01
- **Status**: NEW (the leak predates the new mode; `restir_history` inherits it)
- **Description**: nothing between l.1880 and l.4057 tests `debugMode`, so those early returns write lit blended colour into raw correctness views. `material_lobe` and `rt_lod` were already hoisted above them for this reason.
- **Impact**: correctness views show lit colour on portal/glass fragments, which misleads a reader of the history view.
- **Suggested Fix**: hoist the raw-view branch above those returns, as was done for `material_lobe`/`rt_lod`.

### REN-L-03: the new ownership-claim and decal-substrate paths are unguarded; `substrate` restates `TEMPORAL_NORMAL_COS` as a bare `0.906`
- **Severity**: LOW
- **Dimension**: Ray Queries (test coverage / magic numbers)
- **Location**: `triangle.frag` `substrateValid` block (`0.906`, `max(8.0, …)`, 0.5% depth term, `max(abs(dot(geomN, V)), 0.1)`) and the `if (stableTemporalSurface)` write — REN-D2-2026-10-14-03
- **Status**: NEW
- **Description**: `TEMPORAL_NORMAL_COS` is function-local to the temporal-reuse `if`, so the substrate block re-types its value (three `0.906` sites, plus `SPATIAL_NORMAL_COS`). No `shader_contract`/`shader_constants`/`restir` test mentions `substrate`, `claimed` or `reservoirsCurr`. #5617 closed the same class for the history cap and floor.
- **Suggested Fix**: hoist the cosines and tolerances into `shader_constants_data.rs`, and pin that blended/glass fragments never reach the `reservoirsCurr` store and that the substrate path reads `reservoirsPrev`, not `reservoirsCurr`. Do this together with REN-H-01's fix.

### REN-L-04: `viewRestirHistory` block sits under the comment describing `viewRestirLight`
- **Severity**: LOW (comment rot)
- **Dimension**: Debug/Telemetry
- **Location**: `triangle.frag`, `c2647d468` — REN-D12-2026-10-14-02
- **Status**: NEW
- **Suggested Fix**: give the history block its own comment.

### REN-L-05: about ten refactored pass layouts have no device-free `validate_set_layout` test
- **Severity**: LOW (test gap)
- **Dimension**: Sync/Pipeline
- **Location**: the pass layouts rebuilt by #5593; `validate_ssao_layout_matches` in `reflect.rs` checks a hand-copied layout, not the production array — REN-D4-2026-10-14-02
- **Status**: NEW
- **Impact**: a future binding drift in those passes is caught only at device init, not by `cargo test`.
- **Suggested Fix**: make each layout builder return a testable array that the constructor and the reflect test share.

### REN-L-06: volumetrics init — host-mapped `moment_buffer` / `occupancy_buffer` locals are outside `partial` when their fill/flush fails
- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `vulkan/volumetrics/init.rs`, the moment/occupancy buffer setup — REN-D8-2026-10-14-01 (also noted by Dims 4 and 5)
- **Status**: NEW (pre-existing; unchanged by #5593)
- **Description**: not a leak: `GpuBuffer::Drop` (#656) frees the buffers with a warning. The real effect is a debug-build `debug_assert!` that panics and masks the `Err`, on a path that is effectively unreachable.
- **Suggested Fix**: push each buffer into `partial` before filling it.

### REN-L-07: fog-colour decode (`6eb1f378a`) left stale "authored/linear fog colour" wording; the commit carries no tracking issue
- **Severity**: LOW (doc rot)
- **Dimension**: Camera/Sky docs
- **Location**: `GpuCamera.fog` docs (`bindings.glsl` and `cluster_cull.comp` comments, `docs/engine/shader-pipeline.md` row, the `pack_fog_tint` docs), `docs/engine/procedural-volumetric-fog.md:97` — REN-D3-2026-10-14-01 and REN-D8-2026-10-14-02
- **Status**: NEW
- **Description**: fog colour reaching the renderer is now sRGB-decoded linear radiance; the docs still say "fog color (RGB 0-1)" / "authored". No shader reads `camera.fog`; the live consumers are the composite `fog_color` and the volumetric `fog_tint`, fed by one decode at the `render/mod.rs` boundary. This matches the project's one fog-colour exception to "no sRGB decode", and `CellLightingRes` keeps the authored value. The commit also bundles an unrelated `.claude/issues/5155/ISSUE.md`.
- **Suggested Fix**: update the wording; state the decode convention once in `shader-pipeline.md`.

### REN-L-08: `GROUNDCOVER_BLADE_FADE_START` is a second independent 7000.0 with no `FADE_START < DRAW_DISTANCE` assertion
- **Severity**: LOW
- **Dimension**: GPU-Struct / constants
- **Location**: `shader_constants_data.rs` — REN-D3-2026-10-14-02
- **Status**: NEW
- **Description**: both fades use `smoothstep(START, GROUNDCOVER_DRAW_DISTANCE, d)`, which is undefined when START is not below the end. Fine today (7000 < 8000), but a latent footgun on the next distance retune.
- **Suggested Fix**: add a const assert, or derive START from the draw distance.

### REN-L-09: `docs/engine/memory-budget.md` says ground-cover rows total "About 19.8 MB"; they sum to about 154.6 MB
- **Severity**: LOW (doc rot)
- **Dimension**: Memory/Lifecycle
- **Location**: the Sky and Ground Cover table sentence — REN-D5-2026-10-14-01
- **Status**: NEW
- **Description**: the rough-budget row on the same page already says ~155 MB. The ledger test pins each row, not this sentence.
- **Suggested Fix**: fix the sentence, or compute it from the pinned rows.

### REN-L-10: `groundcover.rs` module doc still says 64 MiB arena / 256 chunks / 3000-unit draw distance
- **Severity**: LOW (doc rot)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` module doc — REN-D5-2026-10-14-02
- **Status**: NEW (same rot class as #4497)
- **Description**: `b731b01f5` made it 512 chunks, a 128 MiB arena and an 8000-unit reach.

### REN-L-11: doc blocks displaced onto the wrong item by recent edits (`step_cell_transition`, `merge_bgsm_arm`)
- **Severity**: LOW (doc rot)
- **Dimension**: Tech debt / docs
- **Location**: `byroredux/src/app_step.rs` — commit `16c9bd0b2` inserted `step_player_move_request`, `step_created_references` and `step_actor_graphs` between `step_cell_transition`'s doc block (including the #5288 caching note) and its `fn`; `byroredux/src/asset_provider/material/merge.rs` — after #5735 (`a6cef30a5`) the `merge_bgsm_arm` doc sits on `struct BgsmTextureSlotState` and the old `set_*` comments are orphaned at the top of `merge_bgsm_arm` — REN-D1-2026-10-14-01 and REN-D6-2026-10-14-01
- **Status**: NEW; same defect class as closed #5692 and open #5704
- **Impact**: documentation only. Dim 6 confirmed the #5735 refactor is behaviour-preserving (the loop body moved verbatim into three helpers in the same call order).

## Prioritized Fix Order

1. **Correctness:** REN-H-01 (replace the non-atomic claim read with a defined, ordered mechanism), with REN-L-03's pins in the same change. Then REN-M-01 (origin-independent `view_proj_is_static`), which also resolves the too-loose and too-tight reports.
2. **Safety/robustness:** REN-L-06 (own the buffers in `partial`), REN-L-05 (device-free layout tests), REN-L-08 (const assert).
3. **Quality/doc:** REN-L-02, REN-L-04, REN-L-07, REN-L-09, REN-L-10, REN-L-11, then REN-L-01 only after a capture.

## Needs-RenderDoc

1. **Binding 16 claim stability (REN-H-01):** capture an alpha-tested, overdraw-heavy interior (FO4 GoodneighborTheThirdRail is the commit's repro) and confirm the claim yields a stable owner frame to frame. Run once under `BYRO_VALIDATION=1`, expecting the validator to be blind to the load, and compare the `restir_history` fraction between NVIDIA and a non-NVIDIA device if one is available.
2. **`camera_static` (REN-M-01):** with the player standing, check at eye heights near a 4096 multiple whether the `restir_history` view stays red, and at 2000+ u from the origin whether a ~2 u/s camera creep flips to the deep tail and softens SVGF.
3. **Animated-emissive lag (REN-L-01):** stand in front of a neon/TV and watch indirect response.
4. **Decal substrate:** grazing and back-lit translucent decals in the `restir_history` view.
5. **Ground-cover blade shadow at the far end of its range** (`groundcover_blade.frag` is a second directional-light composition site that skips `shadowableLightRadiance`; whether it skips the `shadowFade` mix is unverified; owned by `/audit-exterior`).
6. **#5593's clean `BYRO_VALIDATION=1` claim** (FNV interior and exterior), and the sync of the new ground-cover selection buffer and wedge residency. Not re-run by this audit.
7. **Fog colour on exteriors (`6eb1f378a`):** its measurement is interior-only (FNV Wrangler). The beyond-grid `authoredLuma*4` clamp is tighter and the terrain-to-sky seam may shift; the decoded chromaticity also tints light scatter, and WATR underwater fog stays raw.

Carried: the FP32 FSR permutation is untested; no test pins the egui `drop_without_applying_deltas` discard sites; TLAS UPDATE after a shifted `instance_custom_index` is validation-only.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 1:** the `First step` git log lists only three of the dimension's paths; `app_step.rs` is a Dim 1 path only for `step_static_blas_restore` (the M47.2 growth belongs to the gameplay/scripting audits).
- **Dim 2:**
  - The GI hemisphere also uses `ditheredSequence2` (`gi_hemisphere_samples_are_dithered_low_discrepancy`); only ReSTIR stream selection and the GI glass/Fresnel event rand still use `hash2_pixel_frame`.
  - The ReSTIR bullet needs the reservoir ownership rule and decal substrate (`substrateValid`, `restir_history` mode 16), and its `writeonly` premise for binding 16 is false in the shader at HEAD.
  - Add `assemble_camera_and_lights.rs` (`view_proj_is_static`, `camera_static`, `restir_history_mode`) to Paths, and `view_proj_static_tests` to Guards.
- **Dim 3:** add `GROUNDCOVER_*` constants as single-sourced, and the fog-colour decode convention. No other premise stale.
- **Dim 4:** add `passes_build_layouts_writes_and_teardown_through_the_shared_helpers` and `view_proj_static_tests` to Guard. Layouts are now built with `descriptors::layout_binding`/`compute_binding` (hand-written `DescriptorSetLayoutBinding::default()` is source-scan banned apart from the bindless texture layout and a reflect fixture). `camera_static` is now `view_proj_is_static`, not an exact compare. `present_modal_pages` runs ahead of the debug-UI snapshot in `app_frame.rs`.
- **Dim 5:** Guard omits `memory_budget_ledgers_the_sky_and_ground_cover_owners`; the First-step path list is incomplete; add the `try_or_cleanup!`/`create_per_frame_uniforms` idiom and the ground-cover sizing (512 chunks, 128 MiB arena, 8000-unit reach).
- **Dim 6:** scalar and flag forwarding now lives in `apply_bgsm_scalars`, `apply_bgsm_texture_slots` and `apply_bgsm_alpha_and_addressing`, not in `merge_bgsm_arm` itself. `MERGE_BGSM_ARM_LINE_PIN` is now 360.
- **Dim 7:** Paths and Guards omit `view_proj_is_static`/`camera_static`; the skill never defines camera-static (now a relative per-entry tolerance). The raw-view list includes mode 16. The direct-EMA description should mention nearer-claim ownership and the 1024-frame / 0.002-floor deep tail.
- **Dim 8:** state the fog-decode convention; `light_rig_geometry_key` and the caustic key asymmetry are unchanged.
- **Dim 9:** `bone_palette_overflow_tests` lives in `render/bone_palette_overflow_tests.rs`; `VERTEX_STRIDE_FLOATS` is defined in `shader_constants.rs`.
- **Dim 10:** scope the "only `disneyDiffuseSplit` call site" wording to `triangle.frag` (`groundcover_blade.frag` is a second directional composition site); add the substrate branch; add `shadow_transport.glsl` and `caustic.rs` `light_rig_geometry_key` to Paths; add the rig-key tests and the #5593 scan test to Guards.
- **Dim 12:** "16 `USER_MODES`" is now 17 (`restir_history` added; `RENDER_DEBUG_MODE_MAX` is 16). Bench test names are `bench_gpu_keys_match_the_reported_bracket_order` and `every_bench_gpu_key_is_printed_on_the_bench_line`.

## Process notes

**Dedup.** Each dimension agent searched open and closed issues by keyword. Related existing items: closed #5062, #5370, #5617, #5692, #4497; open #5704, #5761 (the Cornell forced-low-budget hardware gate, recorded red; Cornell sources are unchanged in this window and the gate was not run), #5365, #5594.

**Dropped candidates.**
- GI and shadow-ray samples sharing the same R2 `n`: only a constant per-pixel rotation offset differs, no estimator multiplies them, no bias.
- `ignMask` precision at salt 1023: measured, uniformity and neighbour anti-correlation unchanged.
- Decal substrate bleeding a wall's shadow onto a parallel blended card within 8 units: documented, intended trade-off in the commit.
- Zero-filled reservoir matching surface ID 0: pre-existing; the depth gate rejects it beyond ~2 BU.
- A duplicate parse-diagnostic warn on the dormant >16-deep BGSM template-chain path: log-only, unreachable with vanilla content.
- `step_created_references` not being guarded on the loading cover: wasted work, not a leak.
