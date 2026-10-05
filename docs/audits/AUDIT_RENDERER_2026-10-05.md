**HEAD**: `a2c24b16e` · **Baseline**: `AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`) · **Audited**: Dims 1–12, delta-scoped to `f002763b4..a2c24b16e` (111 commits) · **Unchanged since baseline (skimmed)**: Dim 7 (comment-only delta: `c8ff61169`); Dim 9 core (`skin_compute.rs`, `morph_compute.rs`, `render/skinned.rs`, `skin_slot_pool.rs`, both skin shaders — no commits); Dim 11 FSR core (`crates/fsr3-sys`, `frame_upscaler.rs`, `upscaling.rs`, `presentation.rs` — no commits); Dim 12 harness (`egui_pass.rs`, `render_debug.rs`, `depth_capture.rs`, `cornell/` — no commits)

# Renderer Audit — 2026-10-05 (all 12 dimensions, delta)

This run is part of `/audit-suite --preset comprehensive`. It audits the 111 commits since the 10-03 baseline. The renderer-relevant delta is almost entirely the follow-up wave to that baseline: #5187–#5221 (minus the open #5210 and #5211). There are also exterior/WATAL changes (#5243/#5244 per-cell distant water, #5245/#4929 water transport, #5173/#5177–#5184 sky and ground cover), the #4441 classifier doc correction, two unsafe/byte-view hardening fixes (#5120, #5122), and the precombine/TRNS-radians fix (`f3e1bba62`).

**Method.**
- Every dimension was analysed synchronously in this session; no sub-agents were used. Per-dimension notes are in `/tmp/audit/renderer/dim_1.md` … `dim_12.md`.
- No engine, GPU process or `BYRO_VALIDATION=1` run was launched. Every Vulkan-facing conclusion comes from source and guard-test reading.
- Guards run:
  - renderer lib: 1379 / 0 / 1;
  - bin (rustc 1.96.0): 2648 / 0 / 52;
  - core `--features inspect`: 795 / 0;
  - fsr3-sys: 8 / 0;
  - NIFAL corpus `cross_game_translation_completeness` (`--ignored`): 1 / 0;
  - stale-SPIR-V gate: clean (36 shaders plus the early variant, glslang 11:16.2.0).

## Executive Summary

| Severity | Count | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 2 | D10-01 (#5192 re-opens #4946: the transmission lobes are unshadowed for every light, sun included), D1-01 (#5195 missed the TLAS scratch *shrink* target) |
| LOW | 3 | D1-02 (stale scratch-size comment), D6-01 (#4441's new doc contradicts #5197), D11-01 (#5218 left the meter's S/K = 8.0 hand-typed) |

Also carried: an Existing #5210 addendum (a fifth spawner-guard exemption), and Existing #5211, which is still live.

**The baseline's fix wave landed cleanly.** The baseline's 35 findings map to issues #5187–#5221:

- 33 are CLOSED. Every code fix was read in its diff and checked against HEAD. The doc-only fixes (#5202, #5203, #5205, #5213, #5214, #5217, #5219, #5221) were accepted on their commits.
- #5210 and #5211 are still open.

The two CRITICALs are fixed and pinned at source level with `production_text`:

- D9-01: `#5187` gates the skin-chain residency.
- D4-01: `#5188` closes the caustic latch and drops the RT flag while the geometry globals are dead.

One archaeology trap: commit `0df2f88e0` "Fix #5194" contains only the #5193 `release_entities` signature follow-up. The #5194 swap-on-success code shipped inside `2f02be01b` (#5187). The fix is present and pinned (`forced_rebuild_swap_tests`). Only the commit attribution is wrong.

**The two MEDIUMs are both "a fix applied at one site of two":**

- **D10-01.** #5192 resolved the closed-mesh self-occlusion of the back lobes by leaving them *unshadowed* rather than tracing them with the fragment's own instance skipped. That brings back the exact symptom #4946 fixed: translucency leaking "through walls and terrain shadow". The #4946 guard only pins *where* the lobe is evaluated, so it stays green.
- **D1-01.** #5195 sized the TLAS scratch for `max(build, update)` at the grow site. But `tlas_scratch_peak_bytes`, which drives `shrink_tlas_scratch_to_fit`, still records `build_scratch_size` alone. A shrink then reallocates below the UPDATE requirement. The guard `fresh_build_records_peak_unconditionally_of_scratch_regrow` pins the literal defect.

## RT Pipeline Assessment

**AS correctness (Dim 1/9).**
- **#5195 (partial).** The skinned shared scratch, its shrink peak, the refit assert and the TLAS grow site now use `max(build, update)`. The TLAS shrink target does not (D1-01).
- **#5201.** Both `build_blas_batched` one-time-submit arms now follow #4891 (`may_be_in_flight` → `mem::forget`). The texture.rs scan now covers `blas_static.rs`, and it is non-vacuous: the needle is `format!`-built, and the test lives in another file.
- **#5200.** `non_finite_transform` flows snapshot → `fill_rt_integrity_stats` → `RtIntegrityStats::verdict`. The machine-line consumers (`scripts/rt-lod-sweep.sh`, `commands_tests.rs`) parse by key, so the inserted field is safe.
- **#5194.** No pre-drop. Phase 4's `drop_skinned_blas` defers the old entry through `pending_destroy_blas` before the insert. The provisional rollback on an unsubmitted frame leaves the entity BLAS-less (first-sight retry), not parked.

**SSBO indexing / geometry liveness (#5188).** While `global_vertex_buffer` is `None`, the TLAS block forces `rt_flag = 0` and skips the TLAS descriptor write and `tlas_built_this_frame`. Every reader of set-1 bindings 8/9 gates on `sceneFlags.x`: `triangle.frag`, `water.frag`, and `groundcover_blade.frag` (traced shadow only). The ground-cover scatter, model tier and bench re-resolve the global vertex buffer per frame and bail on null. Old-pair retirement is deferred, so readers in the same frame still see a live buffer.

**Ray-query shading (Dim 2/10).**
- `#5191` adds the geometric-horizon clamp to the front lobes. All four call sites pass the current fragment's `geometricNormal`, and there are no other callers.
- `#5199` guards Path 2's raw and projected tangent.
- `#5192`'s split is internally consistent: the ReSTIR finalize, the legacy-WRS subtraction and the unshadowed `Lo`/`pHat` sums all recombine both halves. Its *convention*, however, is D10-01.

**Denoiser (Dim 7).** No code delta. #5212's `merged_lights` rename is a pure rename, and it is pinned. #5211 (SVGF nearest-tap NaN re-accept) is still live.

**Exposure / FSR (Dim 11).** #5198 single-sources the adaptation constant (0.5 s now actually runs). #5218 single-sources the meter's neutral and luma weights, but not S/K (D11-01). FSR core is unchanged.

## GPU-Struct & Memory Assessment

**GPU structs.** The delta changed no GPU struct. `gpu_types.rs`, `material.rs` and `crates/renderer/shaders/include/bindings.glsl` are untouched. The `scene_buffer` constant/upload/buffer diffs are comment-only (176 B tile; 64 B light: 1023 × 64 + 4112 = 69 584 B ≈ 68 KiB). The size/offset/mirror/UBO pins and `no_file_states_a_stale_gpu_material_size` are green. `GROUNDCOVER_DEFAULT_AFFINITY` was removed from both the Rust source and the generated GLSL (#5177).

**Light identity (#5204).** One tested `sort_lights_by_priority_with_ids` now serves both sort sites, and `frame_light_ids.resize` is hoisted onto every path. There is one behavioural nuance with no impact: the shared helper pins every *contiguous leading directional*, where `collect_lights` used to pin exactly the scene key. See Stale skill premises.

**Lifecycle (Dim 5).**
- `#5207`: samplers are created after pool and set allocation, and the error arm frees the new pool.
- `#5209`: teardown recovers the transfer-fence lock, and the queue lock no longer poisons it.
- `#5193`: `retire_stage` and the degenerate-pose arm both go through `release_entities`.
- `#5206`: replacing artwork releases the old cover.
- `#5122`: `ModelPush` gets `NoUninit` (64 B, explicit pad, SAFETY comment).
- Per-cell LOD water (`rebuild_lod_water_mesh`) allocates the new mesh before it retires the old one. It uploads with `rt_enabled = false`, so no BLAS pairing is owed.
- `#5208` added the volumetrics ledger rows.
- No new GPU resource owner appeared in the delta.

## Findings

### CRITICAL

None.

### HIGH

None.

### MEDIUM

#### REN-D10-2026-10-05-01: #5192 returns the wrap-excess, back-light and translucency lobes to the unshadowed path for every light, including the sun and the interior XCLL directional. This re-opens the "leaking through walls and terrain shadow" defect #4946 fixed.
- **Severity**: MEDIUM
- **Dimension**: Disney BSDF / Soft Shadows
- **Location**:
  - `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: `backSide`, `brdfTransmission`, `out vec3 transmissionRadiance`.
  - `crates/renderer/shaders/triangle.frag`: the ReSTIR finalize (`frameContribution = rad * restirW * visibility + restirSelectedTransmission * restirW`) and the legacy-WRS subtraction.
- **Status**: Regression of #4946. It is a deliberate trade-off in `a4b6a7360` (#5192), and no guard catches it.
- **Description**: #5192 split `shadowableLightRadiance` so that every lobe that is non-zero only where `rawNdotL < 0` leaves through `transmissionRadiance`:
  - the soft-light wrap excess;
  - the Skyrim back-light lobe (`bethesdaBackFactor`);
  - the FO4 `MAT_FLAG_TRANSLUCENCY` SSS lobe.

  Every consumer multiplies traced visibility into the *reflection* half only. The transmission half is never occluded, for any light.

  Before #4946, the same SSS lobe was added from unshadowed radiance and leaked "through walls and terrain shadow". The comment beside the lobe still says so. #4946 moved it into the shadowed function to stop exactly that. #5018 and #5192 then found that tracing it from the light-side origin self-occludes on closed meshes. Of the two fixes #5192's own issue offered, the commit chose "leave it unshadowed" over "trace with the fragment's own instance skipped".

  The commit's rationale is that "most reference-content local lights are not shadow casters". That contradicts the engine's own visibility policy: `VisibilityMask::for_legacy_local_light()` (`crates/core/src/lighting.rs`) returns `FULL` for every legacy light. The rationale also does not cover directional lights at all, and the sun and the interior XCLL key are always traced.
- **Evidence**:
  - The #4946 guard (shader_contract_tests.rs, "translucency lobe must be evaluated in shadowableLightRadiance") pins only the evaluation site.
  - `shadowable_light_radiance_splits_transmission_lobes_from_traced_visibility` pins the unshadowed transmission as intended behaviour.
  - The finalize adds `restirSelectedTransmission * restirW` with no `visibility` factor.
- **Impact**: Back-side transmission from the sun or the XCLL directional reaches surfaces in a real occluder's shadow. This affects:
  - FO4/FO76/Starfield BGSM translucent foliage and thick-translucency skin under a building or in terrain shadow;
  - Skyrim soft-lit (`MAT_FLAG_SOFT_LIGHTING`) and back-lit (`MAT_FLAG_BACK_LIGHTING`) NPCs inside a shadowed interior.

  The effect is visual only, bounded by the lobe magnitudes. The lobes now also feed `pHat` unshadowed, so reservoir selection favours lights behind walls for those materials.
- **Related**: #4946, #5018, #5192 (all closed); REN-D2-2026-10-03-01 / #5191 (front lobes, correctly clamped).
- **Suggested Fix**: Take #5192's documented alternative for the transmission half. Trace it with a candidate-loop query that skips the fragment's own `instanceCustomIndex`, giving up `TerminateOnFirstHit` on those rays only. That keeps the closed-mesh self-occlusion fix and restores real-occluder shadowing. At minimum, shadow the transmission half of directional lights. Gate the change on a before/after capture (see Needs-RenderDoc).

#### REN-D1-2026-10-05-01: #5195 fixed the TLAS scratch *grow* site but not the *shrink* target. `tlas_scratch_peak_bytes` still records `build_scratch_size`, so `shrink_tlas_scratch_to_fit` can reallocate the per-slot scratch below the driver's `updateScratchSize`, and later UPDATE-mode TLAS builds run on it.
- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/tlas.rs`, `ensure_tlas_state`: `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;` and the comment above it ("refit/update reuse the existing scratch on the spec guarantee `BUILD ≥ UPDATE`").
  - `crates/renderer/src/vulkan/acceleration/memory.rs`, `shrink_tlas_scratch_to_fit` (`target = peak + scratch_alignment_padding`).
  - Field doc in `crates/renderer/src/vulkan/acceleration/mod.rs`.
  - Pin in `crates/renderer/src/vulkan/acceleration/tests/scratch_tests.rs`.
- **Status**: NEW. This is an incomplete fix of #5195, which is closed; the same class as baseline D9-03.
- **Description**: #5195 established that the spec does not bound `updateScratchSize` by `buildScratchSize` (VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-12259). It fixed:
  - the skinned path;
  - `BlasEntry::scratch_requirement` (the skinned shrink peak);
  - the TLAS *allocation* (`build_scratch_size.max(update_scratch_size) + padding`).

  It missed the TLAS peak, which is the shrink target. The shrink runs every frame from `draw.rs` on the next slot. Once its hysteresis (`tlas_scratch_should_shrink`) fires after a shrink-triggered rebuild, the slot's scratch is reallocated at BUILD size plus padding. Subsequent UPDATE builds (`decide_use_update`) reuse that buffer. The regrow check lives only inside `need_new_tlas`, so nothing re-grows it before the next fresh build.
- **Evidence**:
  - `fresh_build_records_peak_unconditionally_of_scratch_regrow` searches for the literal `self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;`, so the guard pins the defect.
  - The `shrink_tlas_scratch_to_fit` doc still says "the recorded peak is the unpadded `build_scratch_size`".
- **Impact**: On a driver whose `updateScratchSize > buildScratchSize`, every TLAS refit after a shrink writes past the scratch allocation, a GPU out-of-bounds write. It is reachable only on such a driver and only after the shrink path fires. That matches baseline D9-03's MEDIUM: it is unmeasured on the 4070 Ti and RADV.
- **Related**: #5195, #2915, #682.
- **Suggested Fix**: Record `sizes.build_scratch_size.max(sizes.update_scratch_size)` as the peak. Fix the comment and the `mod.rs`/`memory.rs` docs, and update the test needle to the max form.

### LOW

#### REN-D1-2026-10-05-02: The first-sight batch comment in `skinned_blas_refit.rs` still says the helper sizes the shared scratch from `build_scratch_size`
- **Severity**: LOW
- **Dimension**: AS Correctness (doc)
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`, the comment above `accel.build_skinned_blas_batched_on_cmd(` ("The helper queries every entity's `build_scratch_size`, grows `blas_scratch_buffer` ONCE to the max demand of the batch").
- **Status**: NEW (residue of #5195)
- **Description / Fix**: Since #5195 the helper grows to `max(build, update)` across the batch. Reword the comment. It is the deletion-inviting kind of text #5195 removed elsewhere.

#### REN-D6-2026-10-05-01: #4441's new boundary doc says Starfield material-reference stubs reach the keyword classifier, but since #5197 every stub whose `.mat` hits the CDB is stamped `NO_SIGNAL_NEUTRAL` first
- **Severity**: LOW
- **Dimension**: NIFAL Material (doc)
- **Location**: `byroredux/src/material_translate.rs` (the boundary contract: "BGEM and the Starfield material-reference stubs do not … the classifier arm is a live path for them"); `crates/core/src/ecs/components/material.rs` (`resolve_pbr` inline comment); versus `byroredux/src/asset_provider/material/merge.rs` `apply_cdb_material`.
- **Status**: NEW
- **Description**: `c2b67d81e` (#4441, 10-04) landed one day after `978d25c19` (#5197). It describes the stubs as unconditionally unclassified. `apply_cdb_material` stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` metalness and roughness whenever the overrides are `None` on a CDB hit. Only CDB misses, and runs without the CDB loaded, still reach `classify_pbr_keyword`.

  The text exists to stop a reader deleting the classifier arm. Read as written, it also invites "restoring" the classifier for CDB hits, which would re-open #5197 (for example, `iris_iron_color.dds` turning into a 0.9-metal eye).
- **Suggested Fix**: Qualify it as "Starfield stubs whose `.mat` misses the CDB (or with no CDB loaded)", and name `NO_SIGNAL_NEUTRAL` as the CDB-hit outcome. Fold this into #5210's NIFAL doc pass if that lands first.

#### REN-D11-2026-10-05-01: `exposure_meter.comp` still hand-types the S/K = 8.0 meter calibration, which Rust derives from `SENSOR_SENSITIVITY_S` / `LIGHT_METER_CALIBRATION_K`
- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/exposure_meter.comp` (`float ev100 = log2(max(avg_luminance, 1.0e-6) * 8.0);`); `crates/renderer/src/vulkan/exposure.rs` (`ev100_from_average_luminance`, `EXPOSURE_CONSTANT` doc).
- **Status**: NEW. This is an incomplete fix of #5218, which is closed.
- **Description**: `fc73e0a66`'s message names three hand-typed meter values: the 1.2 neutral, "the 8.0 S/K constant" and the Rec.709 weights. It single-sources only the neutral and the weights. S and K live in `exposure.rs`, not in `shader_constants_data.rs`, so the generated header cannot carry them, and no test ties the shader's literal to their ratio.

  The `EXPOSURE_CONSTANT` doc still claims the meter, the chroma compress and the host "cannot disagree". Retuning K (ISO 2720 allows 12.5 to 14) would move `auto_exposure` and the #5158 envelope tests but not the GPU meter.
- **Suggested Fix**: Move S and K, or their ratio, into `shader_constants_data.rs` and read it in the shader. Alternatively, add a source pin in `exposure_meter.rs` that the shader's factor equals `SENSOR_SENSITIVITY_S / LIGHT_METER_CALIBRATION_K`.

#### Existing #5210: addendum
- `d1ec1c57f` added a fifth spawner-guard exemption: the LOD-water `plane.entity` mesh swap in `rebuild_lod_water_mesh` (#5243). `docs/engine/nifal.md` §3 still says "exactly four deliberate exemptions", so the doc and the guard diverge further. Fold this into #5210's reconciliation.

#### Existing #5211: still live
- `svgf_temporal.comp`'s sub-pixel nearest-tap fallback still loads `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb` with no `isnan`/`isinf` test. The bilinear loop's check is not repeated. There is no delta on this file.

## Prioritized Fix Order

1. **D1-01.** A one-line correctness fix: peak = `max(build, update)`. Update the test needle in the same commit.
2. **D10-01.** Shadow the transmission half with an own-instance-skipping trace, at least for directional lights. Accept it only against the capture listed below, because it trades ray cost (no `TerminateOnFirstHit`) for correctness.
3. **D11-01.** Single-source S/K, or pin it.
4. **Doc batch.** D1-02, D6-01, the #5210 addendum (with #5210 itself).
5. **#5211** (open, carried).

## Needs-RenderDoc / live validation

None of these was run: no engine or GPU process was launched in this audit.

- **D10-01.** An FO4 exterior with BGSM translucent foliage, or an NPC with thick-translucency skin, standing in a building's or terrain's shadow with the sun behind the surface. Also a Skyrim soft-lit/back-lit NPC in a shadowed interior with an XCLL directional. Compare HEAD against an own-instance-skip trace of the transmission half. Expect HEAD to show the transmission glow inside the occluder's shadow.
- **D1-01.** Dump `updateScratchSize` against `buildScratchSize` for the TLAS on the 4070 Ti and on RADV, using the same query as baseline D9-03. Then force a TLAS shrink (load a dense exterior, then a small interior) under `BYRO_VALIDATION=1` with GPU-assisted validation.
- **#5188.** Fault-inject a `build_geometry_ssbo` failure on the reclaim arm under `BYRO_VALIDATION=1`. Expect no descriptor-validity VUIDs on the scene and caustic sets, and `rt_flag = 0` for the window.
- **#5187 / #5194.** A GPU-assisted-validation exterior crossing that streams in NPCs. Also fault-inject a forced-rebuild failure and confirm the actor stays in the TLAS (`rt.integrity` PASS).
- **#5215.** The sync-validation run with an active combustion emitter (occupancy bindings 24/25) is still owed (carried from the baseline as c705c310d).
- **Carried from the baseline:**
  - #5057 early-test A/B; #5062 binding-16 RAW;
  - `GpuLight` 64 B ArrayStride on device;
  - `p6-loading-model.sh` under validation (now also covering #5193/#5206);
  - #4890 rapid resize, #5072, #4889, #4885;
  - the `r.upscaler` switch, `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, resize under FSR, the default FSR Quality validation run;
  - the FP32 SDK permutation, which is untested.

## Stale skill premises (for the next `/audit-renderer` sync)

**Dim 1**
- The scratch-sizing bullet lists "the skinned shared scratch, its shrink peak and refit assert, the TLAS per-frame scratch". Add **the TLAS shrink peak (`tlas_scratch_peak_bytes`)** as a site that must carry `max(build, update)`. Today it does not (D1-01).
- `fresh_build_records_peak_unconditionally_of_scratch_regrow` pins the build-only literal. Flag it as a guard that encodes a defect until D1-01 lands.

**Dim 9**
- Commit `0df2f88e0` ("Fix #5194") holds no #5194 code; the code is in `2f02be01b`. Any regression trace of #5194 should start from `forced_rebuild_swap_tests`, not that commit.

**Dim 2 / Dim 10**
- The checklist states #5192's convention as settled: "transmission lobes are kept out of traced visibility". Record it as an open trade-off against #4946 (D10-01). The #4946 guard checks only the evaluation site, not visibility.
- "The leading directional prefix (the scene key at slot 0) is pinned" now means *every contiguous leading directional*. `sort_lights_by_priority_with_ids` uses `take_while(type > 1.5)`, whereas the pre-#5204 `collect_lights` pinned exactly the pushed scene key.

**Dim 6**
- Add #5197's `NO_SIGNAL_NEUTRAL` stamp on CDB hits to the CDB bullet. The skill and the #4441 doc both still describe Starfield stubs as unconditional classifier inputs.
- The spawner-guard exemption list in the Guard line already includes the LOD-water re-insert. nifal.md §3 does not (#5210).

**Dim 11**
- "#5218 single-sources the meter constants" is only partly true: S/K is still hand-typed (D11-01).

**Process**
- `_audit-common.md` Dedup step 1 says `--limit 400`. The suite cache `/tmp/audit/issues.json` held 97 open issues, all of which were consulted.

## Guard posture

| Dim | Guards confirmed (exist, not `#[ignore]`d, ran green) | Blind spot found |
|---|---|---|
| 1 | `acceleration` filter, TLAS barrier pin, static-BLAS recovery (lib + bin), new `tlas_non_finite_integrity_tests`, #4891 scan now incl. `blas_static.rs` | `fresh_build_records_peak_…` pins D1-01's defect |
| 2 | `shader_contract` incl. new #5191/#5192 pins, depth family, glass/IOR, `light_history` | #4946 pin checks evaluation site only (D10-01) |
| 3 | size/offset/mirror/UBO/light-header, `no_file_states_a_stale_gpu_material_size`, new sort test | — |
| 4 | post-pass, egui, depth capture, FIF contract (now names `combustion_occupancy_buffers`), blend split, reflection, new `geometry_dead_invalidation_tests`, `merged_lights_tests` | — |
| 5 | geometry compaction/rebuild, skin-slot drain, allow-lists, new `teardown_path_recovers_poisoned_locks_beyond_the_allocator_family`, `retire_stage_routing_tests`, `artwork_replacement_release_tests` | the teardown lock scan's needle is the phrase "lock poisoned"; a bare `.lock().unwrap()` would pass |
| 6 | spawner guard, core PBR idempotence/glass overlay, corpus NIFAL (`--ignored`, 1/0) | — |
| 7 | TAA/SVGF/bloom/jitter/aperture | no SVGF NaN pin (#5211 open) |
| 8 | water/volumetrics/caustic incl. the rewritten #4784 guards (#5216) | — |
| 9 | push-constant size, stride, `palette`, morph weak-ref, new `skin_residency_gate_tests` + `forced_rebuild_swap_tests`, bin overflow/rollback | — |
| 10 | BSDF/light source-shape pins, bin light-policy tests, overflow warn | see Dim 2 |
| 11 | lib `exposure|tonemap|upscal|post_passes`, fsr3-sys 8/0, bin FSR default, new `exposure_tuning_defaults_match_the_renderer_constants` | no pin on the shader's S/K (D11-01) |
| 12 | `gpu_timers`, bracket coverage, debug-mode guards, bin bench keys, `mat_set_tests`, new `prepare_unlatches_the_stats_when_nothing_is_placed` | — |

## Process notes

- **Dedup.**
  - Open issues came from the suite cache (`/tmp/audit/issues.json`, 97 open).
  - Closed-issue keyword searches: "TLAS scratch update", "updateScratchSize", "tlas_scratch_peak_bytes", "translucency shadow", "transmission lobes shadow", "back-light unshadowed", "exposure meter 8.0", "LIGHT_METER_CALIBRATION_K", "NO_SIGNAL_NEUTRAL classifier stubs".
  - Only the regression/residue links above matched.
- **Baseline closure check.**
  - All baseline issues #5187–#5209 and #5212–#5221 are CLOSED (states checked with `gh issue view`). Code fixes were read in their diffs; the doc-only fixes were accepted on their commits.
  - #5189's fix is in `e44152d45`; it touches `cell_loader/spawn.rs`, outside the Dim 10 Paths.
  - #5213/#5214 are in `77696ec75` (docs).
- **Dropped candidates.**
  - The LOD water rebuild's `drop_mesh` without `drop_blas`: water meshes upload with `rt_enabled = false`.
  - Ground-cover readers of the global vertex buffer during #5188's dead window: they re-resolve per frame and bail on null.
  - #5187 eviction churn for skipped skinned entities: they are excluded from the TLAS by `in_tlas`, and the dead-geometry case predates #5187.
  - The chroma-compress "~0.74 stops" prose: correct against `EXPOSURE_METER_NEUTRAL` 1.2.
  - `f3e1bba62`'s test comment "6.1 ≈ −π/6" (6.1 − 2π ≈ −10.5°): content code owned by `/audit-fo4`, not a renderer finding.
- **Out-of-scope pointers.**
  - `rebuild_lod_water_mesh` now does a synchronous one-time-submit mesh upload on each grid crossing, which marks the geometry SSBO dirty (`/audit-performance`).
  - The flowing-water weather-transport damping in `render/water.rs` is canonical-kind policy (`/audit-exterior`).
