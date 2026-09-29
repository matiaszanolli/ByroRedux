**HEAD**: `9fcfdc3fc` · **Baseline**: `AUDIT_RENDERER_2026-09-27.md` (HEAD `7e9da5dcc`) · **Audited**: Dims 1–12, delta-scoped to `7e9da5dcc..9fcfdc3fc` (every dimension had commits on its Paths) · **Unchanged since baseline (skimmed)**: none

# Renderer Audit — 2026-09-29 (all 12 dimensions, delta)

`/audit-renderer` run as part of `/audit-suite --preset comprehensive`. There were 40 commits in the window. Most of the renderer delta is the follow-up wave to the 09-27 audit: `efc059f3a`, `a37fcba3c`, `b978bb5a1`, `4ec1e48c5`, `21319618c`. It closed 38 of the 42 issues that audit filed (#4940–#4981). Four remain OPEN:

- #4940: its fix has landed, but the issue waits on the A/B.
- #4956, #4957, #4958 and #4981: doc and test gaps.

The window also added the player body (`a070baaad`), mid-life gear import (`0182fc5e8`), the egui texture fixes from the concurrency audit (`7039ebbbc`), and the auto-exposure default (`546e7fbc7`).

This run analysed every dimension itself, in sequence; no sub-agents were used. It could not launch the engine or any GPU process, so this report contains **no `BYRO_VALIDATION=1` run and no capture**. Every Vulkan-facing conclusion below comes from reading the source and the guard tests. Anything that needs a device is listed under *Needs-RenderDoc*.

## Executive Summary

| Severity | Count | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | REN-D10-2026-09-29-01 (#4946's fix makes the translucency lobe self-shadowing; the Skyrim/FO4 back-light lobe always was) |
| MEDIUM | 0 | — |
| LOW | 6 | D2-01 (ReSTIR ratio EMA is whole-reservoir), D3-01 (`dof_params.w` semantic change undocumented), D4-01 (reactive attachment doc omits TAA), D5-01 (no egui ledger row, new CPU mirror), D5-02 (player gear imports never released), D11-01 (startup FSR→TAA fallback persisted to `settings.toml`) |

**The fix wave is sound.** Each fix was checked against its issue's premise:

- **#4940 (ReSTIR normalisation).** The fresh weight is scaled after the stream and before any reuse combine, and the weights are consistent with Bitterli Alg. 4. The issue stays open for the transport-oracle A/B, as intended.
- **#4942 (ratio EMA).** Verified, with one residual: D2-01.
- **#4943 (instance-set change in the scene-static signal).** Verified.
- **#4944 (TAA reads the reactive mask).** Descriptor, pool, layout and every construction site checked.
- **#4945 (`NoOpaque` flag on the glass-portal query).** Verified.
- **#4948 (TLAS membership in `decide_use_update`).** Verified; its tests are no longer vacuous.
- **#4969 (non-sentinel LRU stamps).** Verified.
- **#4976 (native-blit source layout).** Verified at every call site.
- **#4978 (probe entity map).** Verified.
- **Regressions of any closed issue:** none found.

**The one HIGH was introduced by a fix.** #4946 moved the BGSM translucency lobe into `shadowableLightRadiance` so that it is shadowed, as the 09-27 audit recommended. But the ReSTIR visibility ray starts on the *viewer's* side of the surface, and the back lobes are non-zero only when the light is on the *other* side. So the ray hits the fragment's own triangle, and the lobe the fix was meant to shadow correctly is now always black. The Skyrim/FO4 back-light lobe has had the same self-occlusion since it moved into that function. No `cargo test` guard can see this: the new pin asserts where the lobe lives, not that its visibility can be non-zero.

**Guard posture: green.**

- Renderer lib: 1296 passed, 0 failed, 1 ignored. The ignored test is device-only (`gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`).
- Bin crate: 2522 passed, 48 ignored. Every ignore is data- or bench-gated.
- Core `--features inspect`: 779 passed.
- `byroredux-fsr3-sys`: 8 passed.
- The opt-in real-data NIFAL corpus test `cross_game_translation_completeness` is **green again**. It was red at the baseline; #4965 fixed it.
- `scripts/check-shader-artifacts.sh`: 36 shaders plus the early-test variant are byte-identical to a fresh compile.
- Every named `Guard:` test exists, ran, and is not `#[ignore]`d.

**Performance.** No FPS or ms figures appear in this report. ROADMAP's Bench-of-record is the stepped-camera refresh at `a37fcba3c`. It names one open regression, **R6a-regress-22** (FO4 frame time doubled inside `4c9a5b36..99933f87b`), which lies outside this audit's correctness scope. Two renderer commits in this window fall inside that range: `21319618c` and `4ec1e48c5`.

## RT Pipeline Assessment

- **Acceleration structures (Dim 1): healthy.**
  - `decide_use_update` now owns the membership rule (#4948). The early-BUILD short-circuits are unchanged.
  - `last_entity_ids` is refreshed only on BUILD, through the same `tlas_instance_entity_ids` iterator.
  - The new tests exercise equal-address tie-breaks under three permutations, so REN-D1-2026-09-27-01 is resolved.
  - The `instance_custom_index` == SSBO-slot contract is untouched: no diff to `begin_frame_recording` or `instance_map_cap`.
- **SSBO indexing and ray queries (Dim 2): clean.** The indexing chain, the coordinate-space split and the depth convention have no delta.
  - The #4940 scale sits between the fresh stream and the temporal combine, which `fresh_enumeration_is_weighted_by_candidate_count` pins.
  - The #4942 ratio estimator is correct in expectation, but it is aggregated over the whole reservoir (D2-01).
  - #4950 (sky along `-V`), #4951 (IGN wrapped to 64 frames) and #4973 (IOR floored at vacuum) are verified.
- **Ray-query safety:** one self-intersection by construction (REN-D10-2026-09-29-01, above). Every other ray family already orients its origin with `offsetRayOriginForDirection`; the direct-light visibility ray is the exception.
- **Denoiser and resolve (Dim 7): healthy.**
  - #4943 feeds instance-set changes into `caustic_scene_static`. SVGF reads that signal directly. ReSTIR reads it one build late through `dof_params.w`; the lag is documented on `scene_static_last_build`.
  - #4944 closes the TAA-mode water ghost.
  - #4966 resets TAA/FSR/volumetrics history when a raw-output view is entered or left. The legacy `render_debug_flags` are set only at init, so no runtime path can cross the raw-output boundary without the reset.
- **Volumetrics, caustics and water (Dim 8): healthy.**
  - #4945's candidate loop can now iterate. It is still the only candidate-type loop in the shader tree.
  - #4971 (froxel budget) and #4968 (nuclear dimmer vs grid reach) are verified.
  - #4979 makes water and ground-cover blades recede under raw views they do not own.

## GPU-Struct & Memory Assessment

- **Layout (Dim 3).** No `#[repr(C)]` struct or GLSL mirror changed layout in the window; the `Reservoir` edit is comments only (still 32 B). All size, offset and mirror guards pass.
  - #4954 (Fx light gate over count + lights + remap), #4955 (generated render-layer / fog-shape ids, no literals left) and #4952 (light SSBO doc pin) are verified.
  - One *semantic* lane change has not reached its docs: `dof_params.w` (D3-01).
- **Memory (Dim 5).**
  - `teardown.rs`, `allocator.rs`, `image.rs`, `device.rs` and `deferred_destroy.rs` are untouched.
  - #4960 now tells the two compaction branches apart by allocation, and the fallback warns once.
  - The #4961/#4962/#4963 ledger rows are pinned against the constants.
  - #4969 removes the 0-sentinel rebase leak.
  - Two new resource owners: the egui CPU mirror (no ledger row, D5-01), and the player's imported gear, which has no release path (D5-02).
- **Skinning (Dim 9).** Every SkinSlot/MorphSlot stamp goes through `skin_lru_stamp`, which never returns 0. A first-person-hidden player body is skipped by both palette build and static collection.
- **Sync (Dim 4).**
  - #4986 makes every egui upload a full one. Its immediate destroy of the old image is covered by rider 14 of the all-slots fence wait.
  - #4959 pins the final instance-SSBO grow's real window against five pre-grow recorders and `draw_frame` ordering.

## Findings

### HIGH

#### REN-D10-2026-09-29-01: #4946 made the translucency lobe self-shadowing — the back-lobe visibility ray starts on the viewer's side and hits the fragment's own triangle, so `MAT_FLAG_TRANSLUCENCY` (and the back-light lobe) contribute ~0 for every traced light
- **Severity**: HIGH. This is the special-rule floor "Ray query self-intersection (wrong tMin/origin bias)". The impact is visual, and limited to materials that carry the translucency or back-light flags.
- **Dimension**: Soft Shadows / Disney BSDF
- **Location**:
  - `crates/renderer/shaders/include/lighting.glsl`, `shadowableLightRadiance`: the translucency block (`backDotL = max(-rawNdotL, 0.0)`, added by `efc059f3a`) and `bethesdaBackFactor(mat, rawNdotL)`.
  - `crates/renderer/shaders/triangle.frag`, ReSTIR finalize: `vec3 shadowOffsetNormal = dot(geometricNormal, V) < 0.0 ? -geometricNormal : geometricNormal; vec3 rayOrigin = offsetRayOrigin(fragWorldPos, shadowOffsetNormal);`. The compiled-out legacy-WRS arm has the same shape.
- **Status**: NEW. The #4946 fix (`efc059f3a`, CLOSED #4946) introduced it for translucency. The back-light sibling predates the window. Searched "translucency", "back lighting shadow": no open or closed issue covers self-occlusion.
- **Description**: Both back lobes are non-zero only for N·L < 0, where the light is on the far side of the surface from the viewer. The finalize offsets the ray origin toward the viewer and traces toward the light, so the ray crosses the fragment's own triangle a few hundred ulps out (`offsetRayOrigin`'s integer offset). `traceShadowTransmittanceDetailed` uses tMin 0.0 and `VISIBILITY_MASK_ALL_OPAQUE`, which includes this instance: `shadow_mask_for_instance` keeps translucent materials in their opaque layer bucket. It commits that hit and returns `vec3(0.0)` in both cases:
  - a non-alpha-sensitive surface returns at once;
  - an alpha-tested one is `covered` at essentially the fragment's own UV, since the fragment already passed its alpha test.

  Visibility is therefore 0 in exactly the configuration the lobe models. Before #4946 the term was added unshadowed (the leak the baseline filed). Now it is shadowed, including by its own surface. The ray helper that orients the origin by direction, `offsetRayOriginForDirection` (`ray_origin.glsl`), is already used by the glass, GI and reflection rays, but not here.
- **Evidence**:
  ```glsl
  // lighting.glsl, inside shadowableLightRadiance (post-#4946)
  float backDotL = max(-rawNdotL, 0.0);            // non-zero only when the light is behind
  ...
  // triangle.frag, ReSTIR finalize
  vec3 shadowOffsetNormal = dot(geometricNormal, V) < 0.0 ? -geometricNormal : geometricNormal;
  vec3 rayOrigin = offsetRayOrigin(fragWorldPos, shadowOffsetNormal);   // viewer side
  // shadow_transport.glsl
  if (!alphaSensitive && !nearEmitter) return vec3(0.0);
  if (covered && !sourceShell) return vec3(0.0);
  ```
  `translucency_lobe_is_shadowed_with_the_other_direct_lobes` pins where the lobe lives, not that its visibility can be non-zero.
- **Impact**:
  - Thin-sheet transmission (leaves, paper, fabric) and thick-object SSS on BGSM v≥8 translucency content render no back-lit contribution from any cell light or the sun inside `shadowFade`. Past the fade the ray is skipped and the lobe reappears unshadowed, so it switches on with distance.
  - Skyrim SLSF2 / FO4 back-light materials have had the same zeroed back-light lobe since that lobe moved into this function.
  - Secondary effect: the ReSTIR pHat includes the back lobe, so a light behind the surface is selected in proportion to a contribution its ray always zeroes. The estimate stays unbiased, but the pixel's one reservoir sample is spent on a guaranteed-dark candidate, which adds noise on the lit side.
- **Related**: #4946, #3574 (the gate that made the lobe reachable), #1147 Phase 2b, `offsetRayOriginForDirection`.
- **Suggested Fix**: Orient the visibility-ray origin toward the light: `offsetRayOriginForDirection(fragWorldPos, geometricNormal, rayDir)`.
  - For N·L > 0 this is identical to today.
  - For the back lobes it starts on the light's side of a thin sheet.
  - Thick objects would still self-occlude through their far wall; whether their SSS should skip the own instance is a separate decision.
  - Gate the change on a before/after capture of a flagged material lit from behind. This is not a speculative barrier change, but its visual effect cannot be seen from `cargo test`.

### MEDIUM

None.

### LOW

#### REN-D2-2026-09-29-01: #4942's ratio accumulator is a single whole-reservoir ratio — where lights of different visibility share a cluster, flicker is still partly low-passed, and the fixture covers only one light
- **Severity**: LOW
- **Dimension**: Ray Queries / Light Animation
- **Location**: `crates/renderer/shaders/triangle.frag`, ReSTIR finalize: `ratioFrame = … frameContribution / max(restirUnshadowedSum, vec3(1e-6)) …`, `accum = mix(prevAccum, ratioFrame, alpha)`, `Lo += restirUnshadowedSum * accum`. Test: `direct_history_accumulates_a_shadow_ratio_not_radiance` (`crates/renderer/src/vulkan/restir.rs`).
- **Status**: NEW (residual of CLOSED #4942).
- **Description**: The accumulated quantity is one ratio, Σrad·V / Σrad, for all of the fragment's streamed lights. Heitz 2018 applies the ratio per light. When one light's intensity animates, the true ratio changes with it, and the EMA'd ratio lags.
- **Evidence**: Light A is visible and flickers at 12 Hz with intensity 1 ± 0.5. Light B has equal intensity and is fully occluded.
  - Truth: A(t) ∈ [0.5, 1.5].
  - With a parked camera (α = 0.025) the EMA ratio settles near mean(A/(A+1)) ≈ 0.48, so the output is (A+1)·0.48 ∈ [0.72, 1.20]: about 48 % of the authored amplitude. Before the fix it was about 2 %.
  - A light switched off next to an occluded one still leaves a lit ratio for the EMA time constant.

  The test mirrors one light under constant shadow, and the shader comment ("intensity animation … reaches the pixel undelayed") holds only when all streamed lights share a visibility.
- **Impact**: Torch flicker still lags in mixed-visibility clusters, though far less than before #4942. This is visual only.
- **Related**: #4942, #4940, REN-D7-2026-09-27-01.
- **Suggested Fix**: Soften the comment to state the limit and add a two-light, mixed-visibility fixture. A per-light ratio would need per-light history; weigh it against the 32-B reservoir.

#### REN-D3-2026-09-29-01: `GpuCamera.dof_params.w` now carries a 0/1/2 history mode (#4942) but every description of the lane still says "camera_static (1.0 = parked)"
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**:
  - Writer: `restir_history_mode` in `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`.
  - Readers: `triangle.frag` `dofParams.w > 1.5` (ReSTIR EMA) and `> 0.5` (GI seed).
  - Stale text:
    - the `GpuCamera::dof_params` rustdoc in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`, which also says it is "Written in `vulkan/context/draw.rs`";
    - the five GLSL `CameraUBO` mirror comments (`include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp`);
    - `docs/engine/shader-pipeline.md`, GpuCamera row at offset 304;
    - the triangle.frag GI-seed comment "(dofParams.w = camera_static)".
- **Status**: NEW. OPEN #4870 is a GPU-struct doc-drift bundle but does not name this lane.
- **Description**: Layout tests cannot see a semantic change to an existing lane; this is the skill's semantic-lane rule. The rustdoc itself requires all five mirrors to carry the same comment, so all seven sites now describe a binary flag. A future reader keyed on `== 1.0` would silently miss the scene-static parked state.
- **Impact**: None today. It sets a trap for the next reader of the lane.
- **Suggested Fix**: Rewrite the eight texts (rustdoc, five mirror comments, the shader-pipeline.md row, the triangle.frag GI-seed comment) to "w = history mode: 0 moving, 1 parked, 2 parked + scene-static (`restir_history_mode`)". Correct the writer's location in the rustdoc.

#### REN-D4-2026-09-29-01: shader-pipeline.md still describes the reactive attachment as FSR-only; TAA samples it since #4944
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (doc)
- **Location**: `docs/engine/shader-pipeline.md` §G-Buffer Layout ("read by SVGF, TAA, SSAO, composite, and (the two FSR mask attachments) `frame_upscaler`'s FSR 3.1 SDK dispatch"; the Reactive row says "FSR 3.1 reactive mask"). The input prose in the `crates/renderer/shaders/taa.comp` header lists only the scene, motion vectors and mesh IDs.
- **Status**: NEW. Not in OPEN #4958 / #4871, which cover frame order.
- **Description**: `taa.comp` binding 9 (`uReactive`) reads attachment 6 under `--upscaler taa`. It bypasses history at 1.0 and raises α below that. The docs attribute the attachment to FSR alone.
- **Impact**: Documentation only. A future change to the reactive writers could miss TAA as a consumer.
- **Suggested Fix**: Name TAA as a reader of attachment 6 in the G-buffer table and in the `taa.comp` header.

#### REN-D5-2026-09-29-01: memory-budget.md has no egui entry, and #4986 added a host-RAM mirror of every egui image
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `EguiPass::image_mirrors` (`crates/renderer/src/vulkan/egui_pass.rs`); `docs/engine/memory-budget.md`.
- **Status**: NEW.
- **Description**: `promote_partial_deltas` keeps a CPU `Color32` copy of each egui-managed image, mainly the font atlas, which grows with glyph coverage. The copy lives as long as the texture. Every partial atlas delta becomes a full-image re-upload. Grepping memory-budget.md for "egui" finds nothing: the GPU atlas has no row either. The skill rule is that every resource owner added since the baseline gets a ledger row.
- **Impact**: This is ledger completeness; the memory is typically a few MiB of host RAM, equal to the atlas.
- **Suggested Fix**: Add one row: atlas GPU image, an equal-sized CPU mirror, and a full re-upload on atlas growth, citing #4986.

#### REN-D5-2026-09-29-02: the player's mid-life gear imports are never released — each distinct item ever equipped keeps its imported mesh and texture references resident until shutdown
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `GearImportLoader::step` (`byroredux/src/npc_spawn/loot_appearance.rs`). Its doc says: "the player has no `CellRoot` — their gear outlives cells exactly like the body it hangs from".
- **Status**: NEW. The owner overlaps with `/audit-gameplay`.
- **Description**:
  - Equipping an item the actor did not spawn wearing imports its worn NIF under the player body root.
  - Unequipping hides that root, and re-equipping reveals it.
  - The only path that despawns an `NpcEquipmentPart` root is cell teardown (`stamp_cell_root_range`), and the player never gets one. A failed player import likewise leaves its hidden partial range in place.
- **Impact**: For every distinct armour or clothing piece equipped in a session, the geometry (global and per-mesh buffers, plus its BLAS once drawn) and its texture refcounts stay resident. Dropping, selling or destroying the item frees nothing. The growth is bounded by the item catalogue, happens per user action rather than per frame, and was not measured.
- **Suggested Fix**: When the imported item's form leaves the inventory, release its root through the same entity-despawn and GPU-handle release path that cell teardown uses. Alternatively, cap hidden gear roots with an LRU.

#### REN-D11-2026-09-29-01: a startup FSR→TAA fallback is persisted to `settings.toml`, so one failed FSR context permanently turns later no-flag launches into TAA
- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - `byroredux/src/app_events.rs` `resumed`: `crate::record_active_upscaler(&self.world, ctx.renderer_config.upscaler, false)`.
  - `record_active_upscaler` (`byroredux/src/main.rs`): `if changed || released { settings_io::save(...) }`.
  - The FSR→TAA promotion in `VulkanContext::new` (`crates/renderer/src/vulkan/context/init.rs`, the #2480 arm).
- **Status**: NEW. Residual of CLOSED #4974/#4975.
- **Description**: When FSR context creation fails at boot, the renderer promotes to TAA, and `resumed` records that mode with `chosen = false`. `record_active_upscaler` saves whenever the registry value *changed*, whatever the value of `chosen`. With no CLI pin (the default launch), the fallback is written as `render.upscaler = "taa"`. `active_upscaler_setting_tests` codifies this: `record_active_upscaler(&world, UpscalerMode::Taa, false)` leaves the file holding `taa`. Even without that save, the registry now holds `taa`, so the next unrelated settings save would persist it; that is the #4974 mechanism.
- **Impact**: One transient FSR init failure (a driver hiccup, a device swap, or the unexercised FP32 permutation) replaces the engine's default render path for every later launch without `--upscaler`. #4947's `source: settings.toml` log line makes this visible, which is why the severity is LOW rather than the MEDIUM the 09-27 silent-override finding got. It still re-opens the 09-27 harness hazard of a persisted value changing which path "default" exercises.
- **Suggested Fix**: Treat a non-`chosen` startup fallback like a CLI override: `pin_stored(UPSCALER_SETTING_ID)` before updating the in-memory registry, so neither this call nor a later unrelated save writes the fallback, and release the pin only on a `chosen` switch. Add the inverse fixture: a fallback with nothing pinned leaves the file untouched.

## Prioritized Fix Order

1. **REN-D10-2026-09-29-01.** Orient the direct-light visibility origin toward the light, and confirm with a capture of a translucent or back-lit material lit from behind. This correctness fix restores a whole feature.
2. **REN-D11-2026-09-29-01.** Stop persisting a boot fallback; this protects the default render path and every harness that relies on it.
3. **REN-D5-2026-09-29-02.** Give imported player gear a release path. This is a lifecycle fix and can be coordinated with `/audit-gameplay`.
4. **REN-D2-2026-09-29-01.** Add the two-light fixture and correct the comment. Pair it with the #4940 A/B, which already needs a multi-light capture.
5. Documentation and ledger: **REN-D3-2026-09-29-01**, **REN-D4-2026-09-29-01**, **REN-D5-2026-09-29-01**.

## Needs-RenderDoc / live validation

- **REN-D10-2026-09-29-01**: capture a BGSM translucency (or Skyrim back-light) surface with a light behind it, before and after the origin change; compare the ReSTIR `direct_only` view and the `restir_light` selection view.
- **#4944** (TAA binding 9) and **#4986** (egui full-upload path): take one `BYRO_VALIDATION=1` run under `--upscaler taa` with the pause menu open while new glyphs appear. This run could not launch the engine.
- **#4940**: run the transport-oracle A/B (Cornell multi-light plus one multi-light interior) before closing. The issue stays OPEN by design.
- Carried from 09-27, still not live-covered: resize, `r.upscaler` preset switch, exterior grid streaming, water cells, and an animating Scaleform HUD.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 2 / Dim 10, back lobes.** "translucency and back-light lobes live inside it (#4946)" is true, but the checklist should add that the direct-light visibility-ray origin is viewer-oriented (`shadowOffsetNormal`), which zeroes both back lobes (REN-D10-2026-09-29-01). A future sync should also record whatever origin rule replaces it.
- **Dim 2, ReSTIR bullet.** "the normalisation fix for #4940 awaits its transport-oracle A/B": the fix *landed* in `a37fcba3c` (`restirWSum *= restirM` after the fresh stream). Only the A/B is outstanding. The skill could name the pin `fresh_enumeration_is_weighted_by_candidate_count`.
- **Dim 3, semantic-lane bullet.** Add `GpuCamera.dof_params.w` as a history mode: 0 moving, 1 parked, 2 parked + scene-static (`restir_history_mode`; ReSTIR reads `> 1.5`, GI seed `> 0.5`).
- **Dim 4 / Dim 11 First step.** "run a bench cell under `BYRO_VALIDATION=1`" cannot be done inside `/audit-suite`, where engine launches are prohibited. Say what to do instead: list the items under Needs-RenderDoc.
- **Dim 5.** Add `EguiPass::image_mirrors` (host RAM) and the player gear-import lifecycle to the owner list.
- **Dim 6 Guard line.** `cross_game_translation_completeness` is `#[ignore]` (data-gated) in `crates/nif/tests/translation_completeness.rs`. Its command is `cargo test -p byroredux-nif --test translation_completeness -- --ignored cross_game_translation_completeness` (~3 s). It is green at this HEAD.
- **Dim 7.** The ReSTIR parked-history key is `dof_params.w > 1.5`, read one build late through `scene_static_last_build`, and it now includes instance-set changes (#4943).
- **Housekeeping.** #4868 (`timestampValidBits`) is still OPEN although production reads through `snapshot_from_bits_with_valid_bits` (fixed in `0e0d35b96`). The 09-27 list of open-but-fixed issues (#4778, #4783, #4785, #4862–#4867, #4872, #4880, #4864) is still open in `/tmp/audit/issues.json`; this run re-verified only #4868.

## Guard posture

| Dim | Guards run | Result |
|---|---|---|
| 1 | `acceleration` + TLAS barrier pin + static-BLAS recovery (lib), bin `static_blas_recovery_runs_between_frames_not_in_the_render_driver` | pass |
| 2 | `shader_contract`, `triangle_frag_keeps_glass_identity_and_ior_across_rt_lods`, `light_history::tests`, ray budget, depth family | pass |
| 3 | size/offset/mirror/UBO/light-header guards, `gpu_material_size_claims`, new light-doc and hash pins | pass |
| 4 | post-pass, egui dependency, depth capture, FIF contract, blend split, descriptor reflection | pass |
| 5 | geometry compaction/rebuild, skin-slot drain, new ledger-row pins | pass |
| 6 | bin spawner guard, core `resolve_pbr_is_idempotent` / `glass_behavior_preserves_authored_map_overlay`, corpus `cross_game_translation_completeness` (ignored; run explicitly) | pass (corpus green again) |
| 7 | TAA/SVGF/bloom/jitter/aperture guards + new #4943/#4944/#4966 pins | pass |
| 8 | water/volumetrics/caustic guards + new #4945/#4968/#4971 pins | pass |
| 9 | push-constant size, stride, `palette`, morph weak-ref, bin palette-overflow + rollback, new #4969 pins | pass |
| 10 | `shader_contract` BSDF/light pins, bin light-policy tests, overflow warn | pass (none can see D10-01) |
| 11 | lib `exposure|tonemap|upscal|post_passes`, `byroredux-fsr3-sys` 8/0, bin `renderer_config_defaults_to_fsr_quality`, new blit-layout pin | pass |
| 12 | `gpu_timers` (incl. new uniqueness test), post-pass bracket coverage, debug-mode guards, probe map, bin bench keys, `mat_set_tests` | pass |

Totals: renderer lib 1296 passed / 0 failed / 1 ignored (device-only); bin 2522 / 0 / 48 (data- and bench-gated); core `--features inspect` 779 / 0; fsr3-sys 8 / 0. Stale-SPIR-V gate: clean.

**Vacuity check.** The new source-scan tests in the window read `source_scan::production_text` or scan files other than their own. The exceptions scan GLSL, or scan `upload.rs` whole with *negative* needles only. None is satisfied by its own literals.

## Process notes

- **Dedup.** Open issues come from the pre-fetched `/tmp/audit/issues.json` (163 open). Closed issues were checked with `gh issue list --state closed --search` for "translucency", "back lighting shadow" and the #4940–#4981 range, whose states were read one by one.
- **Legacy sibling of #4941, probed and dropped.** The case is a disabled `NiSpecularProperty` combined with a metal-keyword texture. The code path is real: `classify_pbr_keyword`'s metal arms run before specular is consulted. But the sample found no vanilla instance:
  - FNV: 60 `*metal*` meshes, 243 shapes, 130 keyword-metal, 0 with zero specular.
  - Oblivion: 120 iron/steel/metal/gold/silver/dwarven/chain meshes, 377 shapes, 233 keyword-metal, 0 with zero specular.
  - Method: `listbsa` + `extract_nif` + `material_dump` examples into the session scratchpad.
- **No cross-dimension duplicates.** The ReSTIR ratio residual is filed once, under Dim 2. Dim 7 and Dim 10 reference it.
- **Per-dimension scratch files** (`/tmp/audit/renderer/dim_1.md` … `dim_12.md`) hold the full verified-OK lists. They were reconciled against this report: 7 findings in the scratch files, 7 here.
