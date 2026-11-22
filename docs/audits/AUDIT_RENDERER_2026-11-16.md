# Renderer Audit — 2026-11-16 (all 12 dimensions, delta since 2026-11-09)

Baseline: `docs/audits/AUDIT_RENDERER_2026-11-09.md` (volumetrics-deep). Window: `dbc815c26..HEAD` (`d9d11dc34`), 39 commits on renderer paths. Method: one agent per dimension, delta-first over every listed Path, guard-first with toolchain 1.100.0. No Vulkan process was launched (so no `BYRO_VALIDATION=1` run); `scripts/check-shader-artifacts.sh` passed before and during the audit, and every agent that recompiled shaders found the committed `.spv` byte-identical.

## Executive Summary

**No CRITICAL, HIGH or MEDIUM findings.** 11 LOW findings (after merging two cross-dimension duplicates) plus 1 informational note that is not filed: 4 doc-rot items, 4 test gaps, 1 ledger gap, 1 interpretive-transfer note, 1 design observation with a measurement (REN-D2-02 / REN-D6-02). Dims 1, 9 and 10 are clean. The renderer-lib suite is 1640 passed, 0 failed, 1 ignored (a Vulkan-device test); every named `Guard:` test exists, is not `#[ignore]`d and is not vacuous.

The 39 commits were almost entirely code motion and teardown consolidation (#6371 `destroy_pass_objects`, #6949/#6948 constructor splits, #6960 `invalidate_allocation`, #6964 `fsr_mask_max_attachment`, #6841 `NearestApertures`) plus the regression fixes for the 11-09 report (#6827–#6831, #6836, #6837 — all verified holding). Behavioural changes: M39 `residentLod` / first-level table (`f754eee28`), reversed-Z layer depth bias (`0c118907c`), cloud-drift lanes (`weather_wind.w`, `cloud_origin.w`), and the MultiLayerParallax unmasked env reflection in `triangle.frag` (`d9d11dc34`, the Skyrim ice fix from this session). All were traced and found sound.

Theme worth a decision: the ice fix exposed that the FO3/FNV "normal alpha is the env-reflection fallback" rule still applies to **3,915 vanilla Skyrim kind-1 meshes** (59 % of cubemap kind-1; median normal alpha ≈ 0.18). Whether they are visibly under-reflected is unmeasured (REN-D2-02 / REN-D6-02).

## RT Pipeline Assessment

- **AS (Dim 1): clean.** Only #6837 touched AS behaviour (BLAS reservation now bills the local-grid irradiance image and atlas); checked against the real allocations. Flags, `instance_custom_index` contract, `instance_map_cap`, two-sided cull gate, `tlas_built_this_frame` lifecycle all match.
- **Ray queries (Dim 2): no Floor violation.** `residentLod` use is complete (every explicit LOD on `textures[]` is LOD 0 or `residentLod`), the reversed-Z depth fix and the SH change are fine. The MultiLayerParallax change is correct for its target, an explicit env mask still wins, and `specStrength *= normalAlphaSpecMask` is untouched; no side effects found on glass, eyes, `water.frag` or `ray_hit.glsl`.
- **Denoise/resolve (Dim 7): no code defect.** Two-range composite UBO upload, pipeline-pair error arm, height-fog optical depth (no positive `exp` argument), cloud-drift lanes not folded into the SVGF key, `NearestApertures` all trace clean; `composite_frag_spv_matches_recompiled_branch_count` passes.
- **Media/water (Dim 8): no regression.** REN-D8-06..10 fixes hold with non-vacuous guards.

## GPU-Struct & Memory Assessment

- **Struct layout (Dim 3): no drift.** `GpuInstance`, `GpuCamera`, `GpuLight`, `GpuMaterial`, `GpuTerrainTile`, `CompositeParams` unchanged; new `TextureFirstLevels` binding and `skyDiffuseCoefficients` covered by guards.
- **Memory/teardown (Dim 5):** `teardown`, `init*`, `resize`, `allocator`, `image`, `dds`, `deferred_destroy`, `mesh/*` unchanged; `f754eee28` is transactional and ledgered. Two LOW findings (a ledger gap for the Bink movie overlay, a missing destroy-walk pin).
- **Skinning (Dim 9): clean.**

## Findings

All LOW. Ordered by likely value.

### REN-D2-02 / REN-D6-02: the normal-alpha env-mask fallback is now a per-kind special case, and its remaining population is large
- **Severity**: LOW (design observation + measurement; not a defect claim)
- **Dimension**: Ray Queries / NIFAL Material
- **Location**: `environmentReflectionMask` and `hasExplicitEnvironment` in `crates/renderer/shaders/triangle.frag`; `normal_alpha_spec_applies` in `byroredux/src/material_translate.rs`; guard `normal_alpha_masks_specular_intensity_not_roughness`
- **Status**: NEW (related: open #6816, the FO3/FNV env-mask classifier)
- **Evidence**: the shader decides "is this normal alpha an env mask?" at render time by `materialKind`; only kind 11 is exempt. Measured on vanilla Skyrim SE (22,047 NIFs, 78,146 meshes): kind 1 with cubemap 6,624; of those 3,924 have no env mask + a normal map + no gloss map, and 3,915 of them carry normal alpha (1,445 NIFs). 1×1-mip alpha proxy over those: mean 0.335, median 0.182, p90 1.0; 2,298 below 0.25. Kind 11 with no cubemap and metalness > 0.3 reaches the block with `environmentStrength = 1.0` and an unmasked reflection (population unmeasured). The guard is a whitespace-exact `contains` of the ternary text; it does not pin that other kinds keep the fallback or that kind 11's local specular is still masked.
- **Impact**: latent; possible residual under-reflection on kind 1; brittle guard.
- **Suggested fix**: measure first (see Needs-RenderDoc). If kind-1 shows under-reflection, carry an explicit "env mask source: none / authored / normal alpha" signal from `translate_material` rather than adding shader kind arms. Relax the guard to a token match on `MATERIAL_KIND_MULTI_LAYER_PARALLAX` inside the initializer plus a second assertion that `normalAlphaSpecMask` remains in the fallback arm.

### REN-D6-03: MultiLayerParallax `inner_layer_thickness` is clamped to 0..1 as a weight, but the authored field is a distance (5..500)
- **Severity**: LOW (interpretive-transfer gap; no shipped reference pins the vanilla transfer function)
- **Dimension**: NIFAL Material
- **Location**: the MultiLayerParallax block in `triangle.frag` (`innerWeight = clamp(multiLayerInnerThickness,0,1) * innerSample.a`, `refractUV`); `ray_hit.glsl` (`innerWeight`); pack site in `byroredux/src/render/static_meshes.rs`; `MLP_REFRACTION_UV_PER_UNIT`
- **Status**: NEW (only closed #5961/#619/#343/#430 touch it)
- **Evidence**: `nif.xml` defines the field as "how far from the surface the inner layer appears to be", range 5..500; all 662 vanilla kind-11 meshes author 5..500 (median 22.9), so the clamp is always exactly 1.0 and `innerWeight` equals the inner layer's own alpha; 22.9 and 500 shade identically. Only `refraction_scale` varies the primary path (`0.0659 × viewTS` for ice). nifly and OpenMW parse but never consume the field. `ray_hit.glsl` applies the inner layer without the refraction UV offset the primary path uses (≤ ~0.066 × viewTS at ice values), undocumented.
- **Suggested fix**: do not invent a mapping. Record in the `shader_constants.glsl` comment and `docs/engine/nifal.md` that the lane saturates for vanilla and that the secondary path skips the offset. If a transfer function is ever measured, apply it at the `translate_material` boundary as a resolved weight.

### REN-D2-01 / REN-D6-01: docs still state the old fallback rule
- **Severity**: LOW (doc-rot) — found independently by Dims 2 and 6
- **Location**: rustdoc of `normal_alpha_spec_applies` (`byroredux/src/material_translate.rs`: "normal alpha is also the fallback reflection mask"); doc and in-body comment of `normal_alpha_masks_specular_intensity_not_roughness` (`shader_contract_tests.rs`, the new comment says "materials that do not author their own environment strength" but only kind 11 is exempt, and kind 1 authors a strength too)
- **Suggested fix**: add the kind-11 carve-out to the rustdoc; reword the test comment to "MultiLayerParallax only".

### REN-D4-01: nothing pins that every `cmd_set_depth_bias` site goes through `layer_depth_bias`
- **Severity**: LOW (test gap)
- **Location**: `layer_depth_bias` (`vulkan/pipeline.rs`); two call sites in `context/geometry_pass.rs`
- **Evidence**: INC6-1 (`0c118907c`) fixed a reversed-Z sign flip missed for 8 days; only a core test (`a_layer_bias_pulls_toward_the_camera_under_either_mapping`) pins `map_depth_bias`, no renderer test references `layer_depth_bias`. A new site reading `RenderLayer::depth_bias()` raw compiles and passes everything.
- **Suggested fix**: a source scan (same shape as `the_context_constructor_builds_no_std_hash_map`) asserting `.depth_bias()` appears nowhere in `crates/renderer/src` outside `layer_depth_bias`, or rename the core method to make the raw triple non-obvious.

### REN-D12-01: depth-capture and linear-capture invalidate-before-read have no pin
- **Severity**: LOW (test gap)
- **Location**: `depth_capture_finish_readback` (`context/depth_capture.rs`), the readback block in `context/linear_capture.rs`; only `screenshot_readback_invalidates_before_mapping` (`context/resources.rs`) pins the screenshot site
- **Evidence**: after #6960 each unpinned site is one `invalidate_allocation(...)` line; deleting it compiles and passes all 1640 tests. Stale reads would corrupt depth/linear-radiance samples on non-coherent memory (the M-LIGHT linear-capture gates are the consumer).
- **Suggested fix**: extend the screenshot ordering test into a loop over the three capture files (`production_text` for the two with test modules).

### REN-D5-02: no pin that `TextureRegistry::destroy` drains `first_level_tables`
- **Severity**: LOW (test gap; code correct)
- **Location**: `first_level_tables: Vec<GpuBuffer>` and `destroy` (`texture_registry/mod.rs`); contrast `scene_buffer/destroy_coverage_tests.rs`
- **Evidence**: same shape as #6521 (a missed per-frame buffer family leaks the allocator `Arc`, so shutdown's `Arc::try_unwrap` fails and the device is deliberately leaked).
- **Suggested fix**: a `production_text` scan asserting every `Vec<GpuBuffer>` field `TextureRegistry` declares is drained in `destroy`.

### REN-D11-2026-11-16-01: nothing ties the settings-registry upscaler default to `UpscalerMode::default()`
- **Severity**: LOW (test gap; code correct today)
- **Location**: `register_builtin_settings` default literal `"fsr3/quality"` (`crates/core/src/settings/builtin.rs`) vs `impl Default for UpscalerMode` (`vulkan/upscaling.rs`), consumed by `install_universal_settings` (`byroredux/src/main.rs`)
- **Evidence**: on every launch without `--upscaler`, the registry value wins over `RendererConfig::default()`. `core` cannot depend on `renderer`, so the literal is hand-copied; the only guard (`renderer_config_defaults_to_fsr_quality`) covers the bare CLI path.
- **Suggested fix**: a bin-crate test asserting `parse_upscaler_spec(registry default) == UpscalerMode::default() == parse_renderer_config(&[]).upscaler`.

### REN-D5-01: the Bink movie overlay texture has no memory-budget.md ledger row
- **Severity**: LOW (ledger gap, not a leak)
- **Location**: `VideoPlayer::texture` / `tick` (`byroredux/src/video.rs`, added in `6e64c453b`); `docs/engine/memory-budget.md` ("MenuXml HUD overlay texture", "Dynamic RGBA staging arenas")
- **Evidence**: one `register_rgba` handle is kept for the session, plus a retained host pixel copy and a grow-only staging arena sized by the movie frame (≈ 8.3 + 8.3 + up to 16.6 MB at 1080p); `grep -i "video|movie|bink"` in memory-budget.md returns nothing and the staging-arena consumer list omits the movie player.
- **Suggested fix**: add the row and list the movie player as a consumer; optionally release the handle in `VideoPlayer::finish`.

### REN-D3-01: `build_bindless_descriptor_bindings` rustdoc describes two bindings; it returns three
- **Severity**: LOW (doc-rot) — `texture_registry/mod.rs`; add "binding 2 is the per-slot first-level table read by `residentLod`" (`f754eee28`, #6629).

### REN-D7-01: shader-pipeline.md still says `CompositeParams` ends in the aperture array
- **Severity**: LOW (doc-rot) — `docs/engine/shader-pipeline.md:972-977`; it now ends in `view_proj`, which `composite_upload_ranges` always writes (#6836).

### REN-D8-12: `ripple_amplitude` (#7018) split `authored_wave_height_over_bed`'s rustdoc off its function
- **Severity**: LOW (doc-rot, cross-owner: `/audit-physics`) — `crates/physics/src/water.rs`: `ripple_amplitude` was inserted between the doc block and the public function, so the private helper carries both blocks and the public fn has none. Move the helper above the original doc block.

### REN-D2-03 (informational, not filed): the ice fix shipped inside `d9d11dc34`
The MultiLayerParallax env-mask change, both triangle `.spv` files and the guard test landed in a commit whose subject is cloud simulation, so a subject search for "ice" or "reflection" will not find it (`git log -S"MATERIAL_KIND_MULTI_LAYER_PARALLAX" -- crates/renderer/shaders/triangle.frag` does). Artifacts verified correct. Bisect/blame noise only.

## Prioritized Fix Order

1. **Correctness / measure first**: capture Skyrim kind-1 reflection with and without the normal-alpha mask (REN-D2-02 / D6-02) on one of the named scenes; decide whether to promote the rule to a translate-time signal. Confirm the ice look on screen.
2. **Safety nets (cheap, no behaviour change)**: REN-D4-01, REN-D12-01, REN-D5-02, REN-D11-01 — four small tests.
3. **Interpretive note**: REN-D6-03 (comment + nifal.md only).
4. **Doc/ledger**: REN-D2-01/D6-01, D3-01, D7-01, D8-12, D5-01.

## Needs-RenderDoc

Observations only; none run (no Vulkan process was launched).
- Skyrim `FrozenLake01` ice now reflecting at its authored strength; no kind-11 surface over-reflecting; a kind-11 material with no authored cubemap and metalness > 0.3 (REN-D2-02).
- Kind-1 Skyrim surfaces with a low-alpha normal and no env mask: masked vs unmasked A/B. Candidates (most affected NIFs in the probe): DLC01 castle portal, Dwemer astrolabe lens/hub, DLC01 snow-elf-ruins blast roof.
- Ice primary vs RT-reflected inner layer: is the missing refraction offset in `ray_hit.glsl` visible?
- `0c118907c` reversed-Z layer bias: author states "not yet confirmed on screen"; a decal-heavy interior before/after.
- M39 A11 visual A/B (streamed texture at `first_level > 0` vs fully resident, mirror pose) — still open.
- `BYRO_VALIDATION=1` boot + resize + teardown on the post-#6949 tree (the #6371 validation runs predate it; #6841 has no on-device run); a skinned-NPC cell shutdown (Cornell has no skinned actors); #6835 syncval capture (FRAGMENT→TRANSFER on the local-grid brick buffer).
- `invalidate_allocation`'s hard-coded 256-byte widening could pass the end of an own-block readback allocation (VUID-VkMappedMemoryRange-size-01389) — pre-existing, noted in source; only non-coherent drivers exercise it. First-level table binding 2 is not `UPDATE_AFTER_BIND`; "written before first bind" across a resize is code-reviewed only.
- #6837 adds ≈ 117 MB (1080p) to ≈ 316 MB (4K) to the BLAS reservation; effect on eviction thresholds is unmeasured. FP32 FSR SDK permutation still untested (carried).

## Existing / carried (not re-filed)

#6835 (local-grid brick-buffer fill lacks FRAGMENT→TRANSFER edge; plausible, needs syncval), #5365 (all-slots fence wait throughput), #6371 (hand-written teardown migration; now 19 files migrated, `presentation.rs` and `groundcover_bench.rs` remain among compute passes), #6816 (FO3/FNV env-mask classifier), #6626 / #6629 (M39), #6531 / #6533 (combustion local grid).

## Stale skill premises (for the next `/audit-sync`)

- **First step pathspecs are narrower than the Paths in Dims 1, 4, 5, 7, 12** (and partly 3, 6): run literally they miss real commits (e.g. Dim 4 misses `descriptors.rs`/`reflect.rs`; Dim 7 misses `ssao.rs`, four `context/*.rs`, `sky_apertures.rs`; Dim 1 covers 3 of 9 Paths). Widen each to the full Paths list.
- **Dim 6 Paths** omit `triangle.frag`; add the `environmentReflectionMask` block (also to Dim 10) and name guard `normal_alpha_masks_specular_intensity_not_roughness`. Note kind 11's unmasked env reflection and kind 1's remaining fallback.
- **Dim 3**: `quantize_fade` lives in `render/mod.rs`, not `render/particles.rs`; the "`render_debug` comment says only LOD telemetry" caveat is outdated; add the set 0 binding 2 `TextureFirstLevels` / `residentLod` contract and the `weather_wind.w` / `cloud_origin.w` cloud-drift lanes.
- **Dim 4**: name `destroy_pass_object_sets` / `PassHandleSets` and `migrated_passes_destroy_through_the_shared_helper`; add new guards (`layer_depth_bias`, the composite pipeline-pair and UBO-range pins).
- **Dim 5**: "~17 files still hand-write teardown" is now 2 compute passes; add `video.rs` and `first_level_tables` to Paths; mention the #6948 split and `invalidate_allocation` (+ `debug_assert_flush_range_bounded`).
- **Dim 7**: Paths omit `context/sky_apertures.rs` and `pipeline.rs::fsr_mask_max_attachment`; composite bullet lacks the #6836 two-range upload; the Dim 4 partial-failure pointer is now fixed (#6834).
- **Dim 8**: skill text does not yet mention #6371, #6949, #6964.
- **Dim 10**: `canonical_light_*` attach site is only `synth_child.rs` (`spawn/mesh_instance.rs` merely receives the flags); the `shader_contract` filter is broad — name the nine lighting pins; new CPU lighting consumer `detection/light.rs::light_level_at`.
- **Dim 11**: add `fsr_mask_max_attachment_is_a_red_only_max_accumulation`, `destroy_pass_objects` for `exposure_meter.rs`, and a line that `depth_inverted` comes from `ACTIVE_DEPTH_MAPPING` for both create and dispatch descriptors.
- **Dim 12**: name `buffer::invalidate_allocation` as the shared readback invalidate, add `contact-flat` to the rung list, `has_live_hud_content` as the egui skip-condition home, and `screenshot_readback_invalidates_before_mapping` as a guard.
- `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` still lists the symbols deleted by #6988 (`SkinSlotPool::allocated_count`, `free_list_depth`, `IDENTITY_BONE_SLOT`) as live.
