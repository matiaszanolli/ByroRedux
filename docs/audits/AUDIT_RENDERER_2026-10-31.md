**HEAD**: `670f81917` · **Baseline**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` (HEAD `fa6be4a6b`; 136 commits since, 64 files under the renderer-adjacent paths, of which 45 under `crates/renderer`) · **Audited**: Dim 12 (delta traced), Dims 7/8 (composite + water delta read) · **Unchanged since baseline (skimmed)**: Dims 1, 2, 3, 4, 5, 6, 9, 10, 11 (no semantic change; see per-dimension notes)

# Renderer Audit — 2026-10-31 (delta, suite run)

## Executive Summary

| Severity | NEW | Existing | Findings |
|---|---:|---:|---|
| CRITICAL | 0 | 0 | — |
| HIGH | 0 | 1 | REN-D5-01 (still open as #6506) |
| MEDIUM | 0 | 1 | REN-D2-01 (still open as #6493) |
| LOW | 1 | 4 | NEW: REN-D12-01. Existing: REN-D1-01 (#6494), REN-D8-01 (#6495), REN-D8-02 (#6496), REN-D8-03 (#6497) |

- **The renderer barely moved.** Of the 136 commits since the baseline, ~90 are M43 gameplay (`byroredux/src/{crafting,smithing,lockpick,...}`), and the rest under the renderer are the 2026-10-28 fix wave (#6358, #6415, #6418, #6419, #6420, #6455, #6412, #6414) plus docs and a bench-capture flag. No commit touches `acceleration/`, `shaders/include/`, any ray-query, SVGF, TAA, bloom, skinning, lighting or FSR file semantically.
- **Guards**: renderer lib **1503 passed, 0 failed, 1 ignored** (the device-only `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`; +2 over the baseline's 1501, from #6358's `every_named_bracket_has_exactly_one_pass_row` and #6415's `the_shaft_lane_packs_a_clamped_finite_transmittance`). `scripts/check-shader-artifacts.sh`: 38 shaders + the early-test variant match glslang byte for byte (including the rebuilt `composite.frag.spv`), so shader-side guards are trustworthy. Bin-crate renderer guards (30 selected: static-BLAS recovery, pool reserve, bench GPU keys, bone-palette overflow, skin-dispatch rollback, `every_exterior_spawner_inserts_a_boundary_material`, FSR default, `mat_set`, bench capture, terrain soften mapping, texture-swap flush, `render_one_frame` size pin): **30 passed, 0 failed**.
- **All seven 2026-10-27 findings are still present**, none of the fixes landed (all filed: #6506 for REN-D5-01, #6493, #6494, #6495, #6496, #6497). Each was re-confirmed against current source below. Nothing is re-reported as new.
- **#6358's 1190-line `gpu_timers.rs` rewrite is behaviour-preserving.** All 25 old `cmd_*_start/_end` method bodies were extracted from `fa6be4a6b` and compared with the 25 `TimerPass::row` entries: slot, active bit and both `PipelineStageFlags` match for every pass. `cmd_end` still ORs the same bit; `QUERIES_PER_FRAME` is still 60.
- **The #6415 / #6418 / #6419 / #6420 fixes are complete.** Constants are single-sourced in core (`WATER_CLEAN_IOR` 1.33 = `WaterMaterial::default().ior`, test-pinned; the three shore constants keep their old literal values 0.6 / 0.5 / 0.1), `sky_aperture_count.y` was a reserved zero lane and the `uvec4` layout is unchanged, and the packer clamps and treats a non-finite reading as clear sky.

## RT Pipeline Assessment

- **AS / SSBO indexing**: no change. `build_and_upload_instances.rs` gained only the `TimerPass` migration and the `camera_cloud_sun_transmittance` input. The final-grow → batch-clamp → upload order is unchanged. The two order pins are still definition-order checks (REN-D1-01, #6494).
- **Ray queries / ReSTIR**: no shader or `restir.rs` change. The moved-surface overflow (REN-D2-01, #6493) is unchanged: `upload_moved_surface_ids` still warns on every frame once the 64-entry cap is exceeded.
- **Denoiser / composite**: the only delta is the underwater shaft term, now `shaftColor * sun_dir.w * 0.12 * cameraCloudSunT * underwaterGodRays(...)`. Order is still linear HDR, shafts before precipitation, bloom downstream. `composite_frag_spv_matches_recompiled_branch_count` passes against the rebuilt SPIR-V.

## GPU-Struct & Memory Assessment

- No layout change. `GpuTerrainTile` is still 224 B (the delta is a stale-size comment fix, #6412); `CompositeParams.sky_aperture_count` keeps its `uvec4` type; `every_shader_struct_is_classified` and `no_file_states_a_stale_gpu_material_size` pass.
- **Teardown gap still open**: `SceneBuffers::destroy` (`scene_buffer/descriptors.rs`) frees `selected_ray_probe_buffers` and then goes straight to the terrain staging buffers; `moved_surface_buffers` (allocated in `buffers.rs`, bound at binding 25) is still never destroyed. That is REN-D5-01 = #6506 (HIGH): every clean exit takes the #665 allocator leak-guard branch. No new allocation entered the window (diff grep for `create_host_visible|create_device_local|create_host_readback|GpuImage::|clamp_sampler(` is clean).
- #6111 adds `flush_pending_loose_textures(ctx)` to `step_texture_swaps` (bin crate). It reuses the streaming `flush_pending_uploads` path at the frame boundary and is a no-op when nothing is queued. No lifecycle issue.

## Findings

### REN-D12-01: A comment in `dispatch_skin_and_cluster.rs` still names the deleted `cmd_blas_refit_end` timer method
- **Severity**: LOW
- **Dimension**: Debug/Telemetry (doc-rot)
- **Location**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (the comment above the unconditional AS-write → AS-read barrier before the TLAS build: "it avoids disturbing the `cmd_blas_refit_end` GPU-timer bracket")
- **Status**: NEW
- **Description**: #6358 (`0288f468d`) replaced the 25 `cmd_<pass>_start/_end` method pairs with `cmd_start` / `cmd_end(TimerPass::…)`. Its grep for stale names covered docs and tests but not this prose reference. The sentence now cites a method that does not exist; the bracket it refers to is `cmd_end(TimerPass::BlasRefit, …)` in `skinned_blas_refit.rs`.
- **Evidence**: `grep -rE "cmd_[a-z_]+_(start|end)\b" crates byroredux docs/engine .claude/commands` (excluding `cmd_start`/`cmd_end`, the geometry-phase and fragment-invocation methods) returns exactly this one hit outside historical `docs/audits/` files.
- **Impact**: None at runtime. A reader chasing the #1194 bracket by method name finds nothing.
- **Suggested Fix**: Reword to "the `TimerPass::BlasRefit` GPU-timer bracket".

### Existing / still present (re-verified against current source, not re-filed)

- **REN-D5-01 (HIGH) = #6506** (dup of `AUDIT_SAFETY_2026-10-27` SAFE-2026-10-27-01): `SceneBuffers::destroy` has no loop for `moved_surface_buffers`. Coverage-scan suggestion is #6521.
- **REN-D2-01 (MEDIUM) = #6493**: `upload_moved_surface_ids` warns unconditionally every frame past 64 movers; the cap counts skinned parts, not actors. Related perf item: #6485.
- **REN-D1-01 (LOW) = #6494**: `instance_buffers_grow_before_upload_and_rebind_the_caustic_set` and `the_clamp_runs_after_the_grow_and_before_the_indirect_upload` still compare whole-file offsets against needles that now live in the later-defined helper `upload_instance_and_draw_buffers`.
- **REN-D8-01 (LOW) = #6495**: `underwaterGodRays` still evaluates `underwaterFocusNoise(entry / 70.0 …)` and `entry / 37.0` on a render-origin-relative `entry`.
- **REN-D8-02 (LOW) = #6496**: `underwaterSunVisibility` still reads `texture(depthTex, uv).r` through the LINEAR `hdr_sampler`.
- **REN-D8-03 (LOW) = #6497**: `water.vert` still declares `inColor` as "unused on water — kept for VAO compat".
- Carried and unchanged: #6341, #6316, #6315, #5365, #6143, #6371, #6106, #6248, #6486 (god-ray per-pixel `inv_view_proj` inversion), #6526 (god-ray inline literals), #6527/#6524/#6520 (geometry-pool tier fallout).

### Checked, no finding

- **Dim 12 / #6358**: all 25 passes equivalent (see Executive Summary). The new `every_named_bracket_has_exactly_one_pass_row` reads `production_text`, requires one row per `Q_*_START`, and asserts the row names its own END slot and bit; it is not vacuous (≥ 20 brackets parsed, then `rows.len() == starts.len()`). `groundcover_models.rs`'s `model_timer_encloses_all_phases_and_stats_copy` was rewritten to read the `GroundcoverModels` row and still asserts `COMPUTE_SHADER` / `BOTTOM_OF_PIPE` / `BIT_GROUNDCOVER_MODELS`. All call sites: 25 start/end pairs, plus `GroundcoverBench` and `GroundcoverModels` each at two sites (bake + steady state) and `SkinPalette` at three lazy-start sites that share one `skin_palette_timer_started` latch.
- **Dim 8 / #6420**: `WaterBedField::sample_baked` scales `uphill` by the point's distance from the border in lattice steps (0 on the border, 1 from one step in). `water.vert` gates on `uphillLen > WATER_SHORE_MIN_UPHILL_LEN` and normalises, so a faded vector keeps its heading; the vertex-equality test `the_physics_bed_field_agrees_with_the_baked_shore_direction` pins mesh and physics together. Water content is `/audit-exterior`'s.
- **Dim 7 / #6415**: under water in an interior that carries a portal sky, `params.sun_dir` is the outdoor palette's sun, so applying the camera column's cloud transmittance is consistent with the froxel sun (`portal_sun_radiance` is the outdoor sun). Interiors without a portal sky have `sun_dir.w` from the cell's own key. The caustic path's `caustic_sun_transmittance(is_exterior, …)` special case (#6245) is about the XCLL key and is not the same question; not a finding.
- **Bench capture (`f7194719f`)**: stride ≥ 3, a 3-frame tail guard, a 60-frame deadline with `bridge.cancel()` on timeout, and the final `--screenshot` waits on `capture_in_flight`. It shares the CLI owner slot with the final screenshot only. No renderer-side change.
- **`volumetrics.rs` `strongest_radiant_luma`** (`8985a2e99`): a log-only field read from the same sorted candidate scratch as the existing `strongest_centroid_m`, so both name the highest-`gi_priority_score` candidate (not necessarily the brightest, and not necessarily one that was appended when `append_count` is 0). Telemetry only.
- **Dims 1, 2, 3, 5, 6, 9, 10, 11**: no source change that alters behaviour. `material_translate.rs`'s `SPAWNER_ROOTS` drops `npc_spawn.rs` because #6357 turned it into a directory; the `npc_spawn` directory root still covers the split files.

## Prioritized Fix Order

1. **REN-D5-01 (#6506)**: one destroy loop plus the field-coverage scan (#6521). It still red-lines every debug shutdown.
2. **REN-D2-01 (#6493)**: warn once per overflow episode, add telemetry, census the cap.
3. **REN-D1-01 (#6494)**: re-anchor the two order pins on the call site before the next edit to `build_and_upload_instances`.
4. **REN-D8-02 / -01 / -03 (#6496, #6495, #6497)**: one water/composite commit.
5. **REN-D12-01**: one-line comment reword, fold into the next `gpu_timers` touch.

## Needs-RenderDoc

1. #6249: on-device A/B (`r.shadowfilter on/off`) plus a `BYRO_VALIDATION=1` pass over the sampled mesh-ID input.
2. #6250: `render.debug restir_history` with an NPC stepping out from behind a fence, and a teleport.
3. REN-D8-01: underwater camera crossing a 4096 grid line.
4. #6415: confirm on screen that a clouded-over exterior dims the underwater shafts (the commit says not confirmed on screen).
5. #6343: still needs a fault-injection hook to prove the before and after VUID.
6. Carried: FSR layouts (reasoned only), the untested FP32 FSR permutation, egui `drop_without_applying_deltas` discard sites.

## Stale skill premises

For the next `/audit-sync`:

- **Dim 12**: the skill's `gpu_timers` paragraph is current (TimerPass rows, #6358). Add that bracket call sites are `cmd_start(TimerPass::X, …)` / `cmd_end(TimerPass::X, …)`, so a grep for `cmd_<pass>_start` finds nothing and prose references to the old names are doc-rot (REN-D12-01).
- **Dim 8 / Dim 7**: `composite.frag`'s `CompositeParams.sky_aperture_count` is `x` = aperture count, `y` = camera-column cloud transmittance as `f32` bits (#6415), `zw` reserved. The skill names this under Dim 2; add it to the Dim 3 "semantic lane" list.
- **Dim 3**: `TERRAIN_LOD_SOFTEN_ONSET_FRACTION` (0.5) and `TERRAIN_LOD_SOFTEN_REACH_SHRINK` (0.35, bin crate `app_frame.rs`) are the named halves of the `lod_soften_end` mapping; the skill text names only the latter.
- **Dim 8**: `WATER_CLEAN_IOR`, `WATER_SHORE_RIPPLE_TURN_SHARE`, `WATER_SWELL_SHORE_TOWARD_RAMP`, `WATER_SHORE_MIN_UPHILL_LEN` are core-owned and in `SHADER_DEFINES`; `WaterBedField::sample_baked` is the one shore-direction rule (#6420). The skill already mentions the first three-of-four by issue number only.
- **Dim 1 / Dim 5** (carried from 10-27, still unfixed): the two #4199/#4726 pins are definition-order checks; "every `GpuBuffer` field of `SceneBuffers` must appear in `SceneBuffers::destroy`".
- **Delta scoping**: run the `First step:` path lists through `bash -c` (zsh does not word-split unquoted lists); this run again used `git log fa6be4a6b..HEAD` with the baseline's recorded HEAD rather than `--since=<date>`, because the baseline was cut at 20:22 on 2026-10-27 and a date cut misses the same-day fix commits that landed after it.
