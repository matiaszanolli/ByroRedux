# 4585: REN-D3-2026-09-21-02: `MeterParams` ↔ `exposure_meter.comp` `Params` is missing from the SPIR-V UBO block-size table

State: OPEN  Labels: ['bug', 'renderer', 'low', 'shaders', 'test-gap']

**Severity**: LOW (test gap; the two layouts match today) · **Dimension**: GPU-Struct
**Location**: `crates/renderer/src/vulkan/reflect.rs` `every_remaining_uniform_block_size_matches_its_host_struct` (~:746); `crates/renderer/src/vulkan/exposure_meter.rs` `MeterParams` (~:36) and `meter_params_are_two_vec4s` (~:367); `crates/renderer/shaders/exposure_meter.comp` `uniform Params` (binding 2)
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`every_remaining_uniform_block_size_matches_its_host_struct` pins each host UBO mirror's size against the block size in the committed `.spv`. It covers `CompositeParams`, `TaaParams`, `CausticParams`, `SvgfTemporalParams`, `SSAOParams` and the two bloom `Params` blocks.

The new exposure-meter UBO is not in the table. That is `MeterParams` on the host and `layout(set = 0, binding = 2) uniform Params` in `exposure_meter.comp`. `meter_params_are_two_vec4s` asserts only the Rust size (32 B). So a GLSL-side change to `Params`, or a stale `exposure_meter.comp.spv`, would not fail any test.

The audit checked the two blocks field by field, and they match at HEAD.

## Evidence

- `grep -n "exposure_meter\|MeterParams" crates/renderer/src/vulkan/reflect.rs` finds no match.
- `exposure_meter.rs` has `fn meter_params_are_two_vec4s() { assert_eq!(std::mem::size_of::<MeterParams>(), 32); }`.

## Impact

`cargo test` would not detect future drift between the host struct and the shader block. It would surface only as wrong exposure, or through CI's parity job if the `.spv` is stale. No runtime effect today.

## Related

- #1493 and #2464 (closed): earlier UBOs added to the reflection table for the same reason.
- REN-D3-2026-09-21-01 (#4584): the other Stage-1 GPU-contract pin gap.

## Suggested Fix

Add `("exposure_meter.comp", include_bytes!("../../shaders/exposure_meter.comp.spv"), "Params", size_of::<MeterParams>())` to the table. Consider a guard that fails when a new host UBO mirror is added without a table row.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D3-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: other UBO mirrors outside the table (e.g. `SkyCubeParams`, which has its own field-lockstep test in `sky_cube.rs`) checked for block-size coverage
- [ ] **TESTS**: the new table row fails when `exposure_meter.comp`'s `Params` block changes size


---

# 4586: REN-D4-2026-09-21-01: `shader-pipeline.md` and `memory-budget.md` did not move with the Stage-1 exposure meter — no meter step between bloom and TAA, presentation still "exposure + ACES"

State: OPEN  Labels: ['documentation', 'renderer', 'low', 'doc-rot']

**Severity**: LOW (reference-doc drift) · **Dimension**: Pipeline/RenderPass
**Location**:
- `docs/engine/shader-pipeline.md`: Per-Frame Submission Order steps 17-20 (~:199-233); the compute-shader table and the `presentation.frag` row (~:33-50); the FSR-tail note (~:241-245).
- `docs/engine/memory-budget.md`: § FSR 3.1 Upscaler (~:341-343).
- Recording site: `crates/renderer/src/vulkan/context/post_passes.rs` `record_exposure_meter_pass` (~:1030), called from `record_post_passes` (~:294).

**Status**: NEW (Stage-1 commits `c5663fe39` / `d54382415`)
**Verified against**: HEAD `f97775ca8`

## Description

The Stage-1 exposure pipeline added a per-frame compute pass, `record_exposure_meter_pass` (`exposure_meter.comp`). It runs between bloom and TAA/upscale and writes the per-FIF 1×1 exposure texel that FSR and presentation read. Presentation now applies `tonemap(graded * exposure)`, with an ACES|AgX switch.

Neither reference doc moved:
- **`shader-pipeline.md`**:
  - The "authoritative" submission order goes 17 bloom → 18 `taa.comp`, with no meter step.
  - Step 20 still reads "exposure + ACES tone-map".
  - `exposure_meter.comp` is missing from the compute-shader table.
  - The `presentation.frag` row still says "applies ACES tone-mapping".
- **`memory-budget.md`**: the FSR section still names a single exposure producer (`exposure.rs`). The meter's 2 per-FIF slots, 2 UBOs and pipeline are not ledgered. They are tiny, but the ledger claims to be complete.

This is the third renderer audit in a row where the docs did not move with a post-pass change, despite the doc-moves-with-pass rule added after 2026-09-20.

## Evidence

- `grep -n -i "exposure" docs/engine/shader-pipeline.md` finds only step 20 ("exposure + ACES tone-map"), the `presentation.frag` table row, and the FSR-tail note naming `exposure.rs`. Nothing names the meter.
- `grep -n -i "exposure" docs/engine/memory-budget.md` finds one mention, `exposure.rs` in the FSR section header.
- `record_post_passes` calls `record_exposure_meter_pass` after `record_bloom_pass` and before `record_taa_pass` / `record_upscale_pass`. `taa_resolves_the_post_bloom_scene_tap` pins that order.

## Impact

Anyone reasoning about barriers or VRAM from the reference docs misses a per-frame compute dispatch, its two stage-wide barriers, and its resources. The performance audit's GPU-timer finding on this pass had to cite the gap. No runtime effect.

## Related

- #4308 (open): the same submission-order list is also missing the ground-cover passes. Sweep both in one edit.
- #4525 and #4526 (closed): the previous round of this doc drift (2026-09-20).
- PERF-D8-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`): the exposure meter has no GPU-timer bracket. That report defers the doc gaps to this finding.
- REN-D7-2026-09-21-01 (#4587): the same frame-flow drift in `docs/engine/renderer.md`.

## Suggested Fix

In `shader-pipeline.md`:
- Add a numbered exposure-meter step between bloom and TAA (dispatch + barriers, fixed vs auto mode).
- Reword step 20 and the `presentation.frag` row to `tonemap(graded * exposureTex)` with the ACES|AgX switch.
- Add `exposure_meter.comp` to the compute table.

In `memory-budget.md`, ledger the meter's slots, UBOs and pipeline.

To stop a fourth round, consider a source-shape test that every `record_*_pass` called from `record_post_passes` is named in `shader-pipeline.md`.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: the `docs/engine/renderer.md` frame flow (REN-D7-2026-09-21-01 (#4587)) and #4308's ground-cover steps updated in the same sweep
- [ ] **TESTS**: optional doc-coverage pin (`record_post_passes` callees ↔ `shader-pipeline.md` step names)


---

# 4587: REN-D7-2026-09-21-01: `docs/engine/renderer.md` still describes retired renderer states — composite ACES, volumetrics ×0.0, pre-#3572 TAA/bloom order, `rebind_hdr_views`/`fall_back_to_raw_hdr`, "exposure + ACES" presentation

State: OPEN  Labels: ['documentation', 'renderer', 'low', 'doc-rot']

**Severity**: LOW (reference-doc drift) · **Dimension**: Denoiser/Composite
**Location**: `docs/engine/renderer.md`:
- the pipeline bullet "Composite pass … with ACES tone mapping" (~:42-46);
- the file-tree entry "swapchain_extent, rebind_hdr_views" (~:216);
- frame-flow steps 17-23 (~:323-343);
- TAA section, step 3 (~:508-510).

**Status**: NEW. These sites were missed by the sweeps for #3573, #3608 and #4524.
**Verified against**: HEAD `f97775ca8`

## Description

`renderer.md` still describes several renderer states that no longer exist:
1. **Composite tone-maps.** The pipeline bullet says composite reassembles "… + sky, with ACES tone mapping". Since the FSR split, composite emits linear HDR and tone mapping lives in presentation (#4202).
2. **Volumetrics gated off.** Frame-flow step 17 says "the output is multiplied by 0.0 in composite until Phase 2 lands". `VOLUMETRIC_OUTPUT_CONSUMED` is `true` (`vulkan/volumetrics.rs`). #3573 fixed the § Volumetric section and the pipeline bullet, but not this step.
3. **Pre-#3572 order.** Frame-flow steps 18-21 run TAA (18) before bloom (20) and composite (21), and composite computes "`direct + indirect * albedo + caustic + bloom`". The live order is composite → bloom (`bloom_apply.comp` adds in place, #2796) → exposure meter → TAA → upscale (#3572). #3608 fixed the § Bloom section and the pipeline bullet, but not the frame flow.
4. **Retired rebind/fallback.** TAA § step 3 says `CompositePipeline::rebind_hdr_views()` swaps composite's input to the TAA output, and that composite falls back via `fall_back_to_raw_hdr`. The file tree also lists `rebind_hdr_views`. Both functions are gone. #4524 swept six code-comment sites, not this doc.
5. **Presentation.** Step 23 reads "exposure + ACES tone map". Presentation now applies `tonemap(graded * exposureTex)` with an ACES|AgX switch and a meter-produced exposure texel (Stage 1).

## Evidence

- `grep -rn "fn rebind_hdr_views\|fn fall_back_to_raw_hdr" crates/renderer/src` finds no match.
- `crates/renderer/src/vulkan/volumetrics.rs` has `pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;`.
- `taa_resolves_the_post_bloom_scene_tap` (`crates/renderer/src/vulkan/context/post_passes.rs`) pins the live frame-tail order: composite → bloom → exposure meter → TAA → upscale.

## Impact

The renderer overview doc presents the wrong frame graph: tone-map location, TAA position, volumetrics status, and a fallback mechanism that no longer exists. Contributors and auditors who rely on it reason about the wrong pipeline. No runtime effect.

## Related

- #3573, #3608 and #4524 (closed): earlier sweeps of these retired states. Their named sites are fixed; these `renderer.md` sites were outside their scope.
- #4202 (closed): moved ACES out of composite.
- REN-D4-2026-09-21-01 (#4586): the same frame-flow drift in `shader-pipeline.md` / `memory-budget.md`.

## Suggested Fix

In `renderer.md`:
- Rewrite frame-flow steps 17-23 to the live order: composite → bloom → exposure meter → TAA → upscale → presentation.
- Fix the composite bullet: linear HDR, no tone map.
- Drop the ×0.0 volumetrics note.
- Replace TAA step 3 and the file-tree mention with the current resolve wiring.

A `renderer.md` ↔ `record_post_passes` order pin, of the kind #3573 proposed for the volumetrics claim, would prevent another round.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D7-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `shader-pipeline.md` (REN-D4-2026-09-21-01 (#4586)) and the audit-renderer skill's frame-order line updated in the same sweep
- [ ] **TESTS**: optional doc-order pin against `record_post_passes`


---

# 4588: REN-D8-2026-09-21-01: the glass-caustic occlusion gate's NDC-space depth slop grows ~d²/near — occluders within 23 % of the receiver distance pass at 300 BU

State: OPEN  Labels: ['bug', 'renderer', 'low', 'shaders']

**Severity**: LOW (gate precision; caustic energy lands on occluders) · **Dimension**: Caustics
**Location**: `crates/renderer/shaders/caustic_splat.comp` (~:612-631): `float depthSlop = 1.0e-4 + ndc.z * 5.0e-3; if (depthIsInFront(receiverZ, ndc.z) && abs(receiverZ - ndc.z) > depthSlop) continue;`
**Status**: NEW (introduced by `0418ac768`, the #4545 glass sibling)
**Verified against**: HEAD `f97775ca8`

## Description

`0418ac768` gates glass-caustic deposits on whether the landing pixel is visible. It compares the receiver hit's NDC depth with the opaque depth buffer at the landing pixel, using a slop of `1e-4 + ndc.z * 5e-3` in raw NDC. The renderer uses conventional, not reversed, depth (`BYRO_REVERSED_Z 0`). With conventional depth, NDC z ≈ 1 − near/d, so a fixed NDC slop becomes a view-space tolerance that grows roughly as d²/near. An occluder at view distance d_o in front of a receiver at d is rejected only when (d − d_o)/d_o > slop · d / near.

The comment claims "real occlusion events are orders of magnitude larger in encoded depth". That holds only near the camera.

## Evidence

How far in front of the receiver an occluder must be before the gate rejects it, as a fraction of d (computed from the shader's constants):

| Content | near | d | Occluders missed within |
|---|---|---|---|
| BU content (`NEAR_PLANE_BU_SCALE` = 5) | 5 | 50 BU | ~4 % of d |
| | 5 | 100 BU | ~9 % of d |
| | 5 | 300 BU | ~23 % of d |
| Cornell box | 0.1 | 10 units | ~34 % of d |

On the Cornell box the tolerance is already 34 % at 10 units. The commit's Cornell check could therefore only show that the gate does not over-reject, not that it catches occluders.

## Impact

The motivating case, "a bottle standing between the camera and its own table pool", is not rejected beyond about 50-100 BU. Past that, the bottle's front face receives the caustic deposit meant for the table. Visual only, glass caustics.

## Related

- #4545 (closed): the water-side twin, a camera-visibility ray. REN-D8-2026-09-21-02 (#4589) covers that twin's mask.
- `0418ac768`: "Gate glass caustic deposits on landing-pixel visibility (#4545 sibling)".

## Suggested Fix

Linearize both depths with `depthLinearize(z, nearPlane, farPlane)` (`include/depth_convention.glsl`). Then compare view-space distances with a relative tolerance: reject when the linear depth at the pixel is less than `d_hit * (1 − ε)`, with ε of a few percent plus the ±8 px jitter allowance. Add a CPU mirror test at 50, 300 and 1000 BU.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D8-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: other raw-NDC depth comparisons with a fixed slop checked for the same d²/near growth
- [ ] **SIBLING**: `caustic_splat.comp.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a mirror test that an occluder 5 % in front of the receiver is rejected at 50, 300 and 1000 BU


---

# 4589: REN-D8-2026-09-21-02: the #4545 water-caustic visibility ray traces mask `0xFF` with opaque traversal — effect cards and whole alpha-card foliage quads hide caustics the camera can see

State: OPEN  Labels: ['bug', 'renderer', 'low', 'water', 'shaders']

**Severity**: LOW (water caustics over-occluded) · **Dimension**: Water
**Location**: `crates/renderer/shaders/water.frag` (~:1385-1413): the `occlRq` camera-visibility ray, `rayQueryInitializeEXT(occlRq, topLevelAS, gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF, …)`
**Status**: NEW (introduced by `1375abf53`, the #4545 fix)
**Verified against**: HEAD `f97775ca8`

## Description

The #4545 fix gates each water-caustic deposit on a terminate-on-first-hit ray cast from the riverbed hit back toward the camera. The ray uses cull mask `0xFF` and `gl_RayFlagsOpaqueEXT`:
- The mask includes `VISIBILITY_LAYER_EFFECT` (32) and `VISIBILITY_LAYER_GLASS` (16).
- The opaque flag skips any alpha test.

As a result:
- **Effect cards occlude.** Effect-layer meshes (spray, mist, splash cards) between the bed and the camera count as occluders.
- **Foliage occludes as whole quads.** Alpha-tested foliage cards (reeds, lily pads, overhanging leaves) block with their full quad, not just their visible texels.
- **Glass occludes.** The compute twin `caustic_splat.comp` deliberately lets caustics show through glass, since its depth test reads opaque-only depth.

In each case the camera sees the bed through or around the occluder, but the caustic pattern vanishes.

**Publisher's validation note (fix scope).** The report proposes `VISIBILITY_MASK_SOLID`, but that fix removes only the EFFECT layer:
- SOLID is 31 = ARCHITECTURE | STATIC_PROP | DYNAMIC_ACTOR | FOLIAGE | GLASS, so glass still occludes.
- Foliage stays whole-quad under `gl_RayFlagsOpaqueEXT`.
- Since `f97775ca8`, FO3/FNV/Oblivion blended FX cards are no longer in the EFFECT layer at all (REN-D1-2026-09-21-01 (#4576)), so no mask choice excludes them.

## Evidence

- `water.frag`: `rayQueryInitializeEXT(occlRq, topLevelAS, gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT, 0xFF, floorWorld + toCamera * 1.0, 0.0, toCamera, max(cameraDist - 2.0, 0.0));`
- `shader_constants.glsl`: `VISIBILITY_LAYER_FOLIAGE 8u`, `VISIBILITY_LAYER_GLASS 16u`, `VISIBILITY_LAYER_EFFECT 32u`, `VISIBILITY_MASK_ALL_OPAQUE 15u`, `VISIBILITY_MASK_SOLID 31u`.

## Impact

Caustics disappear under spray and mist cards, around reeds and foliage cards, and behind glass, even where the bed itself is visible. Visual only, water caustics.

## Related

- #4545 (closed): the fix this refines.
- REN-D8-2026-09-21-01 (#4588): the depth gate on the glass-caustic twin.
- REN-D1-2026-09-21-01 (#4576): legacy blended FX cards now sit in opaque buckets, so this ray's mask cannot exclude them until that is fixed.

## Suggested Fix

Trace `VISIBILITY_MASK_ALL_OPAQUE` (which excludes glass and effect) and make foliage coverage-aware. There are two ways: drop `gl_RayFlagsOpaqueEXT` and run a candidate loop with `rayHitHasCoverage`, as `shadow_transport.glsl` does; or accept whole-quad foliage as a documented approximation. Pin the mask choice with a source-shape test.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D8-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: `caustic_splat.comp`'s glass gate and any other camera-visibility query use the same occluder policy
- [ ] **SIBLING**: `water.frag.spv` recompiled; `scripts/check-shader-artifacts.sh` green
- [ ] **TESTS**: a source-shape pin on the visibility ray's mask and flags


---

# 4590: REN-D11-2026-09-21-02: auto-exposure adapts each per-FIF slot from its own two-frame-old value with a one-frame alpha — the effective time constant is 2τ, not the documented τ

State: OPEN  Labels: ['bug', 'renderer', 'low']

**Severity**: LOW (opt-in `--auto-exposure`; adaptation speed off by 2×) · **Dimension**: FSR/Presentation
**Location**:
- `crates/renderer/shaders/exposure_meter.comp`: the header doc (~:14-21) and `imageLoad(dstExposure)` + `mix` (~:92-94).
- Host alpha: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (~:1070-1073), `adaptation_alpha(frame_delta_seconds, self.exposure_adaptation_seconds)`.
- `crates/renderer/src/vulkan/exposure.rs` `adaptation_alpha`.

**Status**: NEW (introduced by `c5663fe39` / `d54382415`)
**Verified against**: HEAD `f97775ca8`

## Description

In auto mode the meter blends `adapted = mix(previous, target, alpha)`. Here `previous = imageLoad(dstExposure)` is this frame-in-flight slot's own texel. With `MAX_FRAMES_IN_FLIGHT = 2`, that texel was written two frames earlier. The host computes `alpha = 1 − exp(−dt/τ)` from a single frame's `dt`, where τ = `exposure_adaptation_seconds` (default 0.2 s).

So the two slots run as two independent adaptation chains. Each advances by a one-frame step once every two frames. The effective time constant is 2τ (0.2 s documented, ~0.4 s actual). Each chain updates at half the frame rate, and presented frames alternate between the two chains.

The shader header says "adaptation simply lags MAX_FRAMES_IN_FLIGHT frames, which is negligible". That is wrong: this is a rate change, not a lag.

## Evidence

- `exposure_meter.comp`: `float previous = imageLoad(dstExposure, ivec2(0)).r; float adapted = mix(previous, target, clamp(params.mode.w, 0.0, 1.0));`
- Host: `mode.w = adaptation_alpha(frame_delta_seconds, self.exposure_adaptation_seconds)`, with `exposure_adaptation_seconds: 0.2` (`context/init.rs`) and `adaptation_seconds: 0.2` (`byroredux/src/components.rs`).
- `crates/renderer/src/vulkan/sync.rs`: `MAX_FRAMES_IN_FLIGHT: usize = 2`.

## Impact

With `--auto-exposure`, eye adaptation runs at half the tuned and documented speed. Because presented frames alternate between two chains, transitions show a subtle frame-to-frame exposure alternation. Fixed mode, the default, is unaffected.

## Related

- SAFE-D7-2026-09-21-01 (`docs/audits/AUDIT_SAFETY_2026-09-21.md`): the meter's averaging-divisor bug, a separate defect. That report defers this adaptation-history issue to this finding.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this as a cross-frame indexing issue whose barriers trace clean.
- REN-D11-2026-09-21-03 (#4591): the same pass lacks the raw-debug gate.

## Suggested Fix

Pick one of two fixes:
- **Host-side alpha.** Compute alpha over the slot's real interval: `adaptation_alpha(dt_since_this_slot_was_written, τ)`, about `MAX_FRAMES_IN_FLIGHT × dt`.
- **Shader-side read.** Read the other slot's most recent exposure (the previous frame's) as `previous`. This adds a cross-FIF read and the barrier it requires.

Either way, correct the shader header. Add a test that simulates N frames and checks that the effective time constant equals τ.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: FSR's per-FIF exposure-texel consumption re-checked under the chosen fix (FSR and presentation read the same slot)
- [ ] **TESTS**: a host-side simulation pinning the effective time constant to `exposure_adaptation_seconds`


---

# 4591: REN-D11-2026-09-21-03: `record_exposure_meter_pass` lacks the `render_debug_requires_raw_output` gate that bloom, TAA and the upscale carry — auto-exposure adapts to debug imagery

State: OPEN  Labels: ['bug', 'renderer', 'low']

**Severity**: LOW (opt-in auto mode; debug-view side effect) · **Dimension**: FSR/Presentation
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs` `record_exposure_meter_pass` (~:1030). The gated siblings are `record_taa_pass` (~:864), `record_bloom_pass` (~:1110) and `record_upscale_pass` (~:1199, ~:1245).
**Status**: NEW (introduced by `d54382415`, the Stage-1 wiring)
**Verified against**: HEAD `f97775ca8`

## Description

Every other post pass after composite checks `render_debug_requires_raw_output(flags, mode)` and skips its look transform when a raw correctness view is active. That covers TAA, bloom, the upscaler (which is forced to the native path) and presentation (`presentation.frag` returns early, before exposure). `record_exposure_meter_pass` has no such gate. It returns early only when the meter failed or is missing.

In auto mode the meter therefore meters the debug image (false-colour views, raw AO, facing ratio, …) and adapts the persistent per-FIF exposure toward it. Nothing is visible while the view is up, because presentation bypasses exposure for raw views. When the user leaves the view, though, the frame starts from a debug-driven exposure and visibly re-adapts, at the 2τ rate of REN-D11-2026-09-21-02 (#4590).

## Evidence

- The `record_exposure_meter_pass` body is `if self.exposure_meter_failed { return; }` followed by `meter.dispatch(...)`. It never checks `render_debug_requires_raw_output`.
- `record_taa_pass`, `record_bloom_pass` and `record_upscale_pass` each call `crate::shader_constants::render_debug_requires_raw_output(...)`.

## Impact

With `--auto-exposure`, turning off any raw debug view produces an exposure "pop", then re-adaptation from the wrong starting value. Debugging sessions that alternate views see exposure drift unrelated to the scene. Fixed mode, the default, is unaffected: the meter just writes the fixed value.

## Related

- #4513 (closed): the same missing-debug-gate class (Halton jitter applied under a raw-output view).
- REN-D11-2026-09-21-02 (#4590): the adaptation rate of the same pass.

## Suggested Fix

In `record_exposure_meter_pass`, skip the dispatch when auto mode is on and `render_debug_requires_raw_output(flags, mode)` is true. The slot then holds its last scene-driven value. Fixed mode can keep writing the constant. Extend the post-pass debug-gate source-shape pins to cover the meter.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: every `record_*_pass` in `record_post_passes` audited for the raw-output gate (TAA, bloom, upscale, presentation, meter)
- [ ] **TESTS**: a source-shape pin that `record_exposure_meter_pass` consults `render_debug_requires_raw_output`


---

# 4592: SAFE-D1-2026-09-21-01: FSR dispatch-failure recovery blit declares `scene_color` `oldLayout = GENERAL`; the image is in `SHADER_READ_ONLY_OPTIMAL`

State: OPEN  Labels: ['bug', 'renderer', 'high', 'vulkan', 'sync', 'safety']

**Severity**: HIGH (Vulkan spec violation) · **Dimension**: 1 — FFI call-site contract / 5 — Vulkan spec
**Location**: `crates/renderer/src/vulkan/frame_upscaler.rs`
- `FrameUpscaler::record`, the `if let Err(error) = dispatch` recovery arm: `record_native_blit(…, GENERAL, GENERAL)` (~:629-636)
- `record_native_blit` before-barrier `.old_layout(source_layout)` (~:729-736) and after-barrier `.new_layout(restore_layout)` (~:800-807)

**Status**: NEW (introduced by `ba4c0efcf`, #3572, 2026-09-18)
**Verified against**: HEAD `f97775ca8`

## Description

- #3572 (`ba4c0efcf`) inserted a `source_layout` parameter ahead of `output_layout` in `record_native_blit` and updated its three call sites. At the FSR dispatch-failure recovery call, the existing `vk::ImageLayout::GENERAL` argument, which had been the *output* layout, stayed in place and became `source_layout`. A second `GENERAL` was appended as the new `output_layout`. The call now passes `GENERAL, GENERAL`.
- In FSR mode, `scene_color` is the composite scene image, in `SHADER_READ_ONLY_OPTIMAL`:
  - `record_upscale_pass` (`context/post_passes.rs`) hands the upscaler the TAA output (`GENERAL`) only when `self.post.taa` exists. TAA is built only for `UpscalerMode::Taa` (`context/init.rs`); `set_upscaler_mode` builds or destroys it.
  - `record_fsr_barriers_before` checks `debug_assert_eq!(inputs.scene_color_layout, SHADER_READ_ONLY_OPTIMAL)` (#4538), and its `fsr_input_read_barrier` keeps `scene_color` at `old == new == SHADER_READ_ONLY_OPTIMAL`.
- So the recovery blit records `oldLayout = GENERAL → TRANSFER_SRC_OPTIMAL` on an image that is in `SHADER_READ_ONLY_OPTIMAL`. `restore_layout` is keyed off `source_layout`, so the after-barrier also leaves the image in `GENERAL`, not the layout it arrived in.

## Evidence

```rust
// FrameUpscaler::record: dispatch-failure arm (runs after record_fsr_barriers_before)
self.record_fsr_depth_restore(device, cmd, inputs.depth);
self.record_native_blit(
    device, cmd, frame, inputs.scene_color,
    vk::ImageLayout::GENERAL,   // source_layout: scene_color is SHADER_READ_ONLY_OPTIMAL here
    vk::ImageLayout::GENERAL,   // output_layout: correct (the FSR barriers moved the output to GENERAL)
);
// record_native_blit
.old_layout(source_layout)                       // GENERAL
.new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
```

- `git show ba4c0efcf -- crates/renderer/src/vulkan/frame_upscaler.rs` shows the recovery-call hunk keeping the old `GENERAL` line and adding `+ vk::ImageLayout::GENERAL`. The same commit changes the scene barrier from `.old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)` to `.old_layout(source_layout)`.
- The other two call sites are correct: the bridge path passes `inputs.scene_color_layout`, and the params-absent path passes `SHADER_READ_ONLY_OPTIMAL, SHADER_READ_ONLY_OPTIMAL`.

## Impact

- This path runs on every real FSR dispatch error, and deterministically under the fault injector `BYRO_FSR_FORCE_DISPATCH_FAIL=1` (`crates/fsr3-sys/src/lib.rs`). #3572's sync-validation matrix (Cornell TAA, TAA + raw view, FSR default, FNV Prospector) never exercised the injector.
- The failure frame triggers VUID-VkImageMemoryBarrier-oldLayout-01197. A wrong `oldLayout` is undefined behaviour: on drivers where `SHADER_READ_ONLY_OPTIMAL` and `GENERAL` differ in compression state, that frame's blit reads undefined contents.
- The leftover `GENERAL` layout is harmless after that frame. The composite colour attachment has `initial_layout = UNDEFINED` (`composite.rs`), and bloom and the exposure meter run before the upscale.
- This still needs confirmation from a validation-layer run (`BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1`). The audit did not run one, and no CI lane can: SAFE-D5-2026-09-21-01 (#4596).

## Related

- #4538 (closed) is the inverse, hypothetical case: the FSR barrier hard-codes `SHADER_READ_ONLY_OPTIMAL` while `GENERAL` is representable. Its fix added only the `debug_assert` and did not touch this call.
- #2140, #2145, #2519 and #3632 (closed) cover other parts of this recovery path. #3572 (closed) is the change that introduced this mistake.
- SAFE-D4-2026-09-21-02 (#4600): `record_native_blit`'s `# Safety` still says `scene_color` must be in `SHADER_READ_ONLY_OPTIMAL`, which is part of why the wrong argument reads as plausible.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

- At the recovery call, pass `inputs.scene_color_layout` (`SHADER_READ_ONLY_OPTIMAL` in FSR mode) as `source_layout`. Keep `GENERAL` only for `output_layout`.
- Extend `fsr_scene_color_barrier_asserts_the_layout_it_hard_codes`, or add a sibling source pin, so the recovery call's source argument must be `inputs.scene_color_layout` rather than a literal.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D1-2026-09-21-01)

## Completeness Checks
- [ ] **UNSAFE**: the recovery call's `// SAFETY:` and `record_native_blit`'s `# Safety` state the `source_layout` contract (SAFE-D4-2026-09-21-02 (#4600))
- [ ] **SIBLING**: all three `record_native_blit` call sites re-checked (bridge, params-absent, dispatch-failure)
- [ ] **TESTS**: a source pin fails if the recovery call passes a literal source layout; a `BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1` run is clean on the failure frame


---

# 4593: SAFE-D2-2026-09-21-01: Three staging-pool release sites still record `allocation.size()` as capacity — the #4512 `vkCmdCopyBuffer` overrun class, unfixed on the mesh/terrain paths

State: OPEN  Labels: ['bug', 'renderer', 'high', 'vulkan', 'memory', 'safety']

**Severity**: HIGH (Vulkan spec violation; #4512 was filed HIGH) · **Dimension**: 2 — Memory corruption / UB / 5 — Vulkan spec
**Location**:
- `crates/renderer/src/vulkan/buffer.rs`: `GpuBuffer::create_device_local_buffer`, pooled-release arm (~:1599-1605)
- `crates/renderer/src/vulkan/buffer.rs`: `GpuBuffer::copy_bytes_range` (~:1758-1764)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `upload_terrain_tiles`, previous-slot release (~:1033-1040)

**Status**: NEW. These are unfixed siblings of closed #4512: its fix was incomplete, and this is not a regression.
**Verified against**: HEAD `f97775ca8`

## Description

- `80813026a` (the fix for #4512) traced the live `+8 B` `vkCmdCopyBufferToImage` overrun to pooled staging being released at `allocation.size()`. That value is the driver-rounded footprint, which sits above the `VkBuffer` create size. `StagingPool::acquire`'s best fit (`e.capacity >= size`) trusts the recorded capacity as the buffer's usable size, so it can hand a later request a `VkBuffer` smaller than the request.
- That commit fixed `record_dds_upload` and `create_device_local_buffers_batched`. It also rewrote `StagingGuard::release_to`'s contract: "`capacity` must be the buffer's *requested* size … never the allocation footprint."
- Three callers still pass the footprint:
  - `create_device_local_buffer` and `copy_bytes_range` both feed `MeshRegistry::geometry_staging_pool` (`crates/renderer/src/mesh.rs`). The first serves the global vertex/index SSBO build (two calls in `crates/renderer/src/mesh/geometry_ssbo.rs`); the second serves the #3298 chunked rebuild (two calls in the same file).
  - The per-cell batched upload (`create_device_local_buffers_batched`, called from `mesh.rs`) draws from the same pool, so it can be handed an inflated entry left by either of the other two.
  - The terrain-tile ring releases the previous frame slot's guard at `allocation.size()`.
- `Vertex` is 104 B, which is 8 mod 16. A `GEOMETRY_REBUILD_CHUNK_BYTES` (64 MiB) vertex chunk is 645,277 vertices = 67,108,808 B, also 8 mod 16. So odd-sized vertex uploads leave pool entries whose recorded capacity is 8 B above their `VkBuffer`.

## Evidence

```rust
// buffer.rs: create_device_local_buffer (pooled arm) and copy_bytes_range
let capacity = staging.allocation.as_ref().map(|a| a.size()).unwrap_or(size);
staging.release_to(pool, capacity);
// scene_buffer/upload.rs: upload_terrain_tiles
let capacity = previous.allocation.as_ref().map(|allocation| allocation.size()).unwrap_or(byte_size);
previous.release_to(&mut self.terrain_tile_staging_pool, capacity);
```

- The guard `pooled_staging_releases_the_requested_size_not_the_allocation_footprint` (`buffer.rs`, `staging_release_capacity_tests`) cannot see any of the three. It forbids only the exact spelling `.map(|allocation| allocation.size())` and scans only `buffer.rs`. The two `buffer.rs` sites spell it `.map(|a| a.size())`.
- `release_to`'s `debug_assert!(capacity <= alloc.size())` is always satisfied by the footprint itself.
- #4512 was closed with its SIBLING completeness box unchecked.
- Publish-time check: the other two `release_to` callers, `vulkan/texture.rs` and `texture_registry/upload.rs`, already release at the requested `image_size` via `record_dds_upload`.

## Impact

- Suppose a later batched or chunked request falls inside the slack, between the `VkBuffer` size and the footprint. It is handed the smaller buffer, and its `vkCmdCopyBuffer` region then exceeds `srcBuffer`'s size (VUID-vkCmdCopyBuffer-srcOffset-00113 / size family). That is the #4512 defect on the mesh path.
- The CPU write stays in bounds, because the mapped slice is allocation-sized. The GPU reads slack bytes of the same allocation. The result is a validation error and spec-level undefined behaviour, not a crash.
- The collision window is narrow, so this is rarer than the DDS case, where sizes cluster. The terrain ring uses 160 B `GpuTerrainTile`s, always a multiple of 16, so it is exposed only on drivers that round buffer requirements above 16 B.

## Related

- #4512 (closed): the texture-path fix this completes.
- #4187 (closed): the same `StagingPool` on the terrain ring.
- #1921 / #1954 (closed): an earlier change in the other direction, which made texture-flush releases record the allocation size so the 128 MB budget ledger would not under-count. #4512 then settled on the requested size for correctness.
- SAFE-D3-2026-09-21-01 (#4599): the same `StagingGuard` / `StagingPool` free paths.
- `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md` and `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cite this finding without re-reporting it.

## Suggested Fix

- At all three sites, release at the size each guard was acquired with. The terrain site needs care: its fallback `unwrap_or(byte_size)` is the *current* frame's size, not the size `previous` was acquired with, so a bare substitution would be wrong there.
- Better: have `StagingGuard` record its acquired size, and remove the caller-chosen `capacity` argument from `release_to`, so no caller can pick the footprint.
- Widen the pin to every `release_to` caller in the crate (both files, any closure spelling), or make it unnecessary through the API change above.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `release_to` / `StagingPool::release` caller in `crates/renderer` re-checked (the three sites here plus `vulkan/texture.rs` and `texture_registry/upload.rs`)
- [ ] **DROP**: if the `StagingGuard` API changes, `release_to` / `Drop` still free or return each buffer exactly once
- [ ] **TESTS**: a pin or unit test fails if any release records more than the acquired size


---
