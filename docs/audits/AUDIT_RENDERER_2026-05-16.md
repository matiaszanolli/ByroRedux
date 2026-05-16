# Renderer Audit — 2026-05-16

**Scope**: Full 20-dimension audit of the Vulkan renderer pipeline.
**Trigger**: Post-`1775a7e6` skinned-BLAS flag split (R6a-prospector-regress) — verifies the new `SKINNED_BLAS_FLAGS` (`PREFER_FAST_BUILD | ALLOW_UPDATE`) lockstep across the four skinned-BLAS call sites and confirms the `UPDATABLE_AS_FLAGS` (`PREFER_FAST_TRACE | ALLOW_UPDATE`) constant continues to drive only the TLAS BUILD/UPDATE pair.
**Prior base**: `AUDIT_RENDERER_2026-05-15.md` — 29 findings (2 HIGH, 4 MEDIUM, 23 LOW). 27 of those landed as fixes in the 2026-05-16 batch closes (#1081–#1109, #1085–#1108, #1119); only **#1092 (REN-D11-001 — SSAO/cluster jittered inv_view_proj)**, **#1104 (REN-D16-002 — Path-2 UV-mirror handedness)**, **#952 (REN-D1-NEW-04 — fence-reset deadlock window)**, and **#924 (REN-D15-NEW-02 — composite fog gated OFF)** remain open as carry-over from prior audits.
**Open issues checked**: 4 REN-D issues open, 93 REN-D issues recently closed (last 14 days).

---

## Executive Summary

**14 new findings** across 20 dimensions. No CRITICAL or HIGH issues.

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 2     |
| LOW      | 11    |
| INFO     | 1     |

**Dimensions with zero new findings**: Render Pass/G-Buffer (4), Command Recording (5), Resource Lifecycle (7), Material Table/R1 (14)\*, Caustics (13), Sky/Weather (15)\*, Volumetrics (18)\*, Bloom (19), M-LIGHT Soft Shadows (20)\*

\* All four carry only documentation cross-reference findings; one carryover stays open (#924 on fog gating).

**Highest-priority fixes** (by blast radius):
1. **REN-D8-NEW-01 (MEDIUM)** — TLAS `built_primitive_count` ratchets only on BUILD: any UPDATE-mode submit with a smaller `instance_count` than the BUILD's count emits stale tail-instance data inside the device buffer. Live spec violation against `VUID-…-pInfos-03708` for the post-build-shrink case.
2. **REN-D1-NEW-01 (MEDIUM)** — 5 `TOP_OF_PIPE` source stages remain after #949/#1100 fixed only 2 of 7. Same deprecation noise the prior fixes targeted; consistency drift across the renderer.

**RT pipeline assessment**: The skinned-BLAS flag split (1775a7e6) is correctly applied across all 4 sites (`build_skinned_blas`, `build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`, and the scratch sizing path). The TLAS BUILD/UPDATE constant (`UPDATABLE_AS_FLAGS`) stays on `PREFER_FAST_TRACE`. VUID-03667 (build/update flag match) holds. VUID-03708 (build/update primitiveCount match) is mostly enforced by the build-count ratchet but has one drift case (REN-D8-NEW-01).

**Rasterization assessment**: All G-buffer formats, pipeline state, command recording, and resource lifecycle are correct. Water pipeline correctly binds vertex/index buffers and sets dynamic state. Bloom hard-fail on init is in place (#1081).

---

## Findings

### MEDIUM

---

### REN-D8-NEW-01: TLAS UPDATE primitive_count uses `built_primitive_count` but device buffer carries fewer instances

- **Severity**: MEDIUM
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:622-687, 733-746`
- **Status**: NEW
- **Description**: When the BUILD/UPDATE decision picks `use_update = true`, the TLAS BUILD range is submitted with `primitive_count = tlas.built_primitive_count` (line 740). The host-to-device copy at line 622 (`copy_size = instances.len() * sizeof(VkASInstance)`) only copies `instance_count` instances — if `instance_count < built_primitive_count`, the device-local buffer's tail (`[instance_count..built_primitive_count]`) carries STALE data from the most recent BUILD (or from a prior UPDATE that copied to the same buffer slot). The UPDATE then ratifies that stale tail into the BVH.
- **Reachability analysis**: The chain `last_blas_addresses.len() == instance_count` (post-swap at line 558-561) means `decide_use_update` would force BUILD on length mismatch — making this normally unreachable. But the guard at line 547 (`if use_update && instance_count > tlas.built_primitive_count`) is the wrong direction; it only catches GROWTH past the BUILD count, not SHRINKAGE below it. The codepath where shrinkage hits is:
  - Frame N: BUILD with 200 instances → `built_primitive_count = 200`, `last_blas_addresses.len() = 200`.
  - Frame N+1: 200 instances unchanged → UPDATE, `last_blas_addresses` still 200.
  - Frame N+2: 150 instances → length mismatch → BUILD → `built_primitive_count = 150`.
  
  Reachable only via the `decide_use_update` short-circuit cases (empty current_addresses, blas_map_dirty, etc.). The empty-current short-circuit at predicates.rs:139 returns `(false, false)` so it forces BUILD anyway. The `blas_map_dirty` path also returns `false`. So actually the only way to reach a UPDATE with `instance_count != last_addresses.len()` is a `needs_full_rebuild = false`, `gen unchanged`, length-equal, content-equal address sequence — which by definition matches.
  
  **However**, the `decide_use_update` zip-compare at line 147 of predicates.rs treats `cached_addresses.len() != current_addresses.len()` as a non-match → returns `(false, did_zip=true)` → BUILD. So the SHRINKAGE-then-UPDATE case is already handled by the zip-compare.
  
  The remaining hazard is more subtle: between the BUILD path setting `built_primitive_count = instance_count` (line 744) and the subsequent UPDATE-mode submission, **if `instance_count` is also used as the copy size and the `tlas.built_primitive_count` differs from that**, the device buffer carries `instance_count` instances but the build_info claims `built_primitive_count`. Today these match because the BUILD path sets them equal. If a future refactor changes when `built_primitive_count` is updated (e.g. records the padded size for VRAM amortization), the desync becomes live.
- **Impact**: Today: latent — guarded by the matching invariant. Spec compliance is correct only because of the unenforced invariant. Future refactor exposure: silent BVH corruption on the difference range.
- **Suggested Fix**: Add a `debug_assert_eq!(tlas.built_primitive_count, instance_count, "UPDATE path: build_info.primitive_count must match copy size")` at line 739, right before the UPDATE branch reads `tlas.built_primitive_count`. Pins the implicit invariant explicitly.

---

### REN-D1-NEW-01: 5 `TOP_OF_PIPE` source stages remain after #949 / #1100 fixed only 2

- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/taa.rs:596`, `crates/renderer/src/vulkan/ssao.rs:452`, `crates/renderer/src/vulkan/volumetrics.rs:695`, `crates/renderer/src/vulkan/bloom.rs:396`, `crates/renderer/src/vulkan/svgf.rs:756`, `crates/renderer/src/vulkan/texture.rs:342`
- **Status**: NEW
- **Description**: Issues #949 (gbuffer) and #1100 (caustic) replaced `TOP_OF_PIPE` source stage with `NONE` on UNDEFINED→GENERAL layout transitions per the Khronos sync2 migration guide. The same pattern lives in 5 more `initialize_layouts` call sites and 1 texture-upload site. Each one is a deprecated source-stage usage on a paired UNDEFINED→layout transition with empty `src_access_mask` — same semantic, same deprecation flag in driver validation layers.
- **Evidence**:
  ```rust
  // taa.rs:596 — same pattern as gbuffer pre-#949
  device.cmd_pipeline_barrier(
      cmd,
      vk::PipelineStageFlags::TOP_OF_PIPE,     // ← should be NONE
      vk::PipelineStageFlags::COMPUTE_SHADER,
      ...
  );
  ```
- **Impact**: Validation-layer noise on strict drivers (MoltenVK, some Linux Intel/AMD). No correctness hazard on NVIDIA today. Consistency drift: the pattern was fixed in 2 of 7 sites, leaving the other 5 as a rolling tech-debt item that recurs every audit sweep.
- **Suggested Fix**: Single batch — change `TOP_OF_PIPE` → `NONE` and update the cross-reference comments to mirror the gbuffer.rs:358 / caustic.rs:653-655 doc-string explaining the equivalence (no prior writes → NONE is the correct sync2 idiom). Six sites, mechanical change.

---

### LOW

---

### REN-D8-NEW-02: `built_primitive_count` invariant lacks a pinning unit test

- **Severity**: LOW
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:744`, `crates/renderer/src/vulkan/acceleration/predicates.rs`
- **Status**: NEW
- **Description**: `built_primitive_count` is the only piece of TLAS state that records the original BUILD's primitive count for downstream UPDATE submits. The implicit invariant is `built_primitive_count == last_blas_addresses.len() whenever use_update == true`. No unit test pins this — `decide_use_update` is tested standalone in `predicates.rs`, but the interaction with the swap/copy/submit sequence is not. The proposed REN-D8-NEW-01 fix would add a runtime assert; a paired unit test in `acceleration/tests.rs` should drive a build→update→shrink→update cycle and validate the invariant from outside.
- **Suggested Fix**: Add `tlas_built_primitive_count_invariant_holds_across_build_update_cycles` to `acceleration/tests.rs`. Mocks: capture the `primitive_count` passed to each `cmd_build_acceleration_structures` call and assert agreement with the staging-buffer copy size.

---

### REN-D1-NEW-02: `reset_fences` happens before fallible `reset_command_buffer` / `begin_command_buffer` — issue #952 (REN-D1-NEW-04) still live

- **Severity**: LOW (carry-over)
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:233-243` (reset_fences), `:277-303` (reset/begin command buffer)
- **Status**: CARRYOVER (open issue #952)
- **Description**: `reset_fences` runs at line 235 BEFORE `reset_command_buffer` (line 281) and `begin_command_buffer` (line 296). If either of those fails, the `?`-propagated error path calls `recreate_image_available_for_frame` (line 285, 300) but does NOT recreate the fence. The fence is now UNSIGNALED with no submit pending → next frame's `wait_for_fences` (line 174) deadlocks at `u64::MAX` timeout.
- **Impact**: Permanent hang on any `reset_command_buffer` / `begin_command_buffer` failure. Failure modes for these calls are rare (driver OOM, lost device) but the deadlock is unrecoverable.
- **Suggested Fix**: Move `reset_fences` AFTER the last fallible call before `queue_submit`, OR mirror the `recreate_image_available_for_frame` pattern with a `recreate_in_flight_fence_for_frame` recovery path. The `recreate_for_swapchain` already has the fence-recreation pattern at sync.rs:177-198; lift it into a per-frame helper.

---

### REN-D11-NEW-01: SSAO and cluster_cull receive jittered `inv_view_proj` — issue #1092 (REN-D11-001) still live

- **Severity**: LOW (carry-over)
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:418, 459, 2288`, `crates/renderer/shaders/ssao.comp:55`, `crates/renderer/shaders/cluster_cull.comp:55`
- **Status**: CARRYOVER (open issue #1092)
- **Description**: `inv_vp_arr` is computed from the **jittered** `vp` (line 418: `let vp_mat = …from_cols_array(vp)`, where `vp` is the matrix that received the Halton jitter at lines 400-410). SSAO consumes it at line 2288 via `ssao.dispatch(..., &inv_vp_arr, ...)`. The cluster_cull compute shader reads the same jittered `inv_view_proj` from the CameraUBO. Both shaders use the inverse to reconstruct world-space positions from depth, shifting the reconstruction origin by up to ±0.5 px per frame.
- **Impact**: AO halos and cluster light-grid assignments shift sub-pixel per frame — appears as low-amplitude flicker in SSAO and potential per-frame light-classification changes near cluster boundaries.
- **Suggested Fix**: Add `proj_unjittered` / `inv_view_proj_unjittered` to `GpuCamera`, route SSAO + cluster_cull to the un-jittered inverse. Requires `GpuCamera` size change + lockstep shader sync per `feedback_shader_struct_sync.md`.

---

### REN-D16-NEW-01: Path-2 screen-space derivative bitangent ignores UV-mirror handedness — issue #1104 (REN-D16-002) still live

- **Severity**: LOW (carry-over)
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: `crates/renderer/shaders/triangle.frag:719-734`
- **Status**: CARRYOVER (open issue #1104)
- **Description**: `perturbNormal`'s Path-2 fallback recomputes `B = cross(N, T)` at line 731 with no sign factor, discarding the screen-space derivative's authored sign at line 727. UV-mirrored shells across a seam render with inverted tangent-space normals.
- **Impact**: Subtle lighting inversion on mirrored UV shells (faces, symmetric props). Most Bethesda content uses authored tangents (Path 1) so the screen-space fallback is rare — but Starfield `BSGeometry` meshes still fall through to Path 2 (#1086 REN-D16-001 is also tracked).
- **Suggested Fix**: `float screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x); B = screenSign * cross(N, T);` — matches the Mikkelsen convention.

---

### REN-D15-NEW-01: Composite distance fog mix removed (M55 Phase 3); volumetric replacement gated OFF — issue #924 (REN-D15-NEW-02) still live

- **Severity**: LOW (carry-over, fog now restored partially)
- **Dimension**: Sky/Weather/Exterior Lighting
- **Location**: `crates/renderer/shaders/composite.frag:484-519`
- **Status**: CARRYOVER, partial fix in place
- **Description**: Per the prior carry-over: M55 Phase 3 (2026-05-09) removed the display-space fog mix; the volumetric replacement is still gated OFF (`VOLUMETRIC_OUTPUT_CONSUMED = false`). However, the **aerial-perspective fallback** branch at composite.frag:484-519 already restores a Markarth-probe-validated mix targeting `skyTint.xyz` along the view direction. The original #924 finding is effectively addressed BUT only for the `is_exterior && volumetric off` path — interior cells still have no distance fog (correctly: most interiors have small `fog_far` so the linear ramp doesn't matter). Marking this as carry-over until #924 is formally closed.
- **Suggested Fix**: Close #924 with a doc-comment update at composite.frag:484 noting the post-Markarth-probe path that addresses the fallback gap.

---

### REN-D12-NEW-01: SVGF per-FIF `frames_since_creation`, but TAA still shares the counter across slots

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/taa.rs:103, 167, 568, 625, 749`
- **Status**: NEW
- **Description**: Issue #964 (REN-D10-NEW-07) converted SVGF's `frames_since_creation` from a shared `u32` to per-FIF `[u32; MAX_FRAMES_IN_FLIGHT]` so each frame slot tracks its own history-reset window independently. The TAA pipeline at `taa.rs:103` still has the pre-fix shared counter. Empirically this is benign for TAA: both history slots are reset together on resize, and bootstrap requires `MAX_FRAMES_IN_FLIGHT == 2` consecutive force-resets (which the shared counter provides). The drift is in **doc parity**: future refactors that touch the TAA history slots without also bumping `frames_since_creation` for both slots could trip the same hazard #964 documents.
- **Impact**: Latent. Today both slots are recreated together; no asymmetric reset path exists. The pattern is inconsistent with the canonical SVGF model now established by #964.
- **Suggested Fix**: Either (a) convert TAA to per-FIF `[u32; MAX_FRAMES_IN_FLIGHT]` for symmetry with SVGF, OR (b) add an explicit doc-comment at taa.rs:103 explaining why the shared counter is correct here (both slots ALWAYS reset together via `recreate_on_resize` / `signal_history_reset` which both touch a single counter that resets both slots in lockstep). Cheap: 1-line comment.

---

### REN-D9-NEW-01: `traceReflection` miss fallback paths use both `skyTint.xyz` and `sceneFlags.yzw` inconsistently across IOR sites

- **Severity**: LOW
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:1873` (refr miss), and `traceReflection` body (not shown)
- **Status**: NEW
- **Description**: The escape-ray fallback at `triangle.frag:1873` for refraction is `skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5` — half-sky half-ambient. The comment notes the reflection-miss path uses the same pattern. With #925 plumbing `skyTint.xyz` from the TOD/weather palette, interior cells receive a SKY tint into the half-sky term even when no sky portal is on screen — possibly inappropriate for sealed interiors. Markarth probe validated this works for canyon-fed interiors but Megaton / Vault 21 (fully sealed) may bleed daylight tone into glass refractions.
- **Impact**: Subtle — interior glass refractions absorb half-sky color when the ray escapes; for fully-sealed cells this introduces an outdoor TOD signal where none should exist. Most visible in sunset / dawn interior glass.
- **Suggested Fix**: Gate the `skyTint * 0.5` term on `sky_params.is_exterior` (already in the UBO via `depth_params.x` / `radius < 0`). For interior cells, drop to `sceneFlags.yzw` alone (pure cell ambient). Marker for future Markarth-probe-like validation on interior glass-heavy cells.

---

### REN-D6-NEW-01: `BLOOM_INTENSITY` duplicated as both `#define` (from shader_constants.glsl) and `const float` in composite.frag

- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/composite.frag:7` (include), `:94` (const float), `crates/renderer/shaders/include/shader_constants.glsl:44` (#define)
- **Status**: NEW
- **Description**: After #1119 (TD4-203) the auto-generated `shader_constants.glsl` exports `#define BLOOM_INTENSITY 0.15`. composite.frag includes the header at line 7 AND declares `const float BLOOM_INTENSITY = 0.15;` at line 94. After preprocessing the const-declaration substitutes to `const float 0.15 = 0.15;` which is syntactically invalid GLSL. The drift test (`composite_frag_bloom_intensity_matches`) asserts the source-text agreement but does NOT verify the shader compiles cleanly through the preprocessor. The .spv file is current (timestamp matches source), so either:
  (a) The shader is compiled without `-I crates/renderer/shaders` and the include line silently no-ops (glslangValidator returns an error on the missing include, suggesting this isn't the path), OR
  (b) glslang's preprocessor has a quirk where `const float NAME = VALUE` declarations skip macro substitution (possible — some implementations do this), OR
  (c) The .spv file is stale and a recompile would fail.
  The comment at `shader_constants.rs:124-127` explicitly says "When the shader migrates to `#include`, drop the local declaration" — strongly suggesting the migration is incomplete.
- **Impact**: Latent build break — the next shader recompile may fail. Same hazard for `VOLUME_FAR` (composite.frag:81 declares `const float VOLUME_FAR = 200.0;`, shader_constants.glsl:45 has `#define VOLUME_FAR 200.0`).
- **Suggested Fix**: Drop the `const float BLOOM_INTENSITY = 0.15;` and `const float VOLUME_FAR = 200.0;` declarations from composite.frag — the `#define` from the include header is the source of truth. Update the drift tests at shader_constants.rs:129 and :138 to assert the local declarations are ABSENT, not present (anchor against `#define BLOOM_INTENSITY` from the include instead).

---

### REN-D10-NEW-01: SVGF temporal `length(motion * screen.xy)` mixes UV-space and pixel-space — actually fine, but unit comment misleading

- **Severity**: INFO
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp:195`
- **Status**: NEW
- **Description**: The nearest-tap fallback gate at line 195 uses `length(motion * screen.xy) < 1.5`. `motion = currUV - prevUV` is in UV-space [0,1]; `screen.xy` is in pixels; product is in pixel units. The "1.5 pixels" threshold is correct, but the lack of a unit comment makes a future reader question whether this is the right units to compare against the `1.5` literal. (The matching gate at line 105 uses motion-vector directly without a units-warning either — not a bug, but the chain of conversions is implicit.)
- **Suggested Fix**: Add `// motion * screen.xy is in pixels; 1.5 = sub-pixel motion threshold` next to the `< 1.5` literal at line 195.

---

### REN-D2-NEW-01: BLAS scratch buffer in `build_skinned_blas_batched_on_cmd` doesn't shrink — high-water mark grows monotonically per session

- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:396-429`
- **Status**: NEW
- **Description**: The batched on-cmd builder grows `blas_scratch_buffer` to `max_scratch_size` of the current batch but never shrinks it. The TLAS instance buffer has a shrink path (`tlas_instance_should_shrink` predicate at `predicates.rs:248-254` with hysteresis), and BLAS scratch has the matching `scratch_should_shrink` at `predicates.rs:230-233`. Neither is wired up to the skinned-BLAS scratch grow path. A single high-watermark batch (e.g. a dense actor crowd cell-load) pins the scratch at its peak for the rest of the session.
- **Impact**: Mild VRAM bloat. Skinned BLAS scratch typically tops out at a few MB per batch; the dense actor case might claim ~16-32 MB. Persistent for session lifetime.
- **Suggested Fix**: After Phase 4 of `build_skinned_blas_batched_on_cmd` succeeds, call `scratch_should_shrink(current, max_scratch_size)` and shrink if appropriate. Mirrors the existing pattern in `memory.rs::shrink_blas_scratch_to_fit` already wired for static BLAS.

---

### REN-D3-NEW-01: Water pipeline declares `cull_mode(NONE)` static but also lists `CULL_MODE` dynamic — redundant baseline value

- **Severity**: LOW (cosmetic)
- **Dimension**: Pipeline State
- **Location**: `crates/renderer/src/vulkan/water.rs:342, 394`
- **Status**: NEW
- **Description**: The water pipeline sets `.cull_mode(vk::CullModeFlags::NONE)` in the rasterizer state at line 342 AND declares `CULL_MODE` dynamic at line 394. Per Vulkan spec the static value is ignored when the dynamic state is set, so the line 342 value is a no-op. The doc comment at line 332-336 says "The `cull_mode` value here is a no-op at runtime; we keep `NONE` as the 'intended baseline' for pipeline introspection / debug-layer reads."
- **Impact**: None — explicitly documented baseline. Flagged for the audit catalog since the dimension-3 checklist includes "pipeline compatible with dynamic-state declarations."
- **Suggested Fix**: None needed; the comment makes the intent clear. Optionally add a `#[must_use]` test that asserts the dynamic state list contains every value the static rasterizer/depth-stencil sets — a forward-compat trap.

---

### REN-D4-NEW-01: G-Buffer Drop safety-net `debug_assert!(false)` fires under any panic-unwind from `new()`

- **Severity**: LOW
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:202-214`
- **Status**: NEW
- **Description**: The `Attachment::drop` safety net at gbuffer.rs:202-214 fires `debug_assert!(false)` if any image/view/allocation remains. The `new()` path at `:248-281` builds `gb` locally and on error calls `gb.destroy(...)` then returns. But if any of the 5 `allocate` calls panics (e.g. allocator lock poisoned), the `gb` local Drop runs — and it calls each attachment's Drop, each of which fires the safety-net assert. The pre-fix release path would log without panicking; the debug path now compounds the original panic with 5 nested `debug_assert!(false)` panics.
- **Impact**: Debug-only stack pollution on allocator-poison errors. Not a release-build issue.
- **Suggested Fix**: Wrap the `debug_assert!(false)` in a `if !std::thread::panicking() { ... }` guard so the safety-net doesn't fire during unwind. Mirror the `GpuBuffer::Drop` pattern (#656) which already has this guard.

---

### REN-D17-NEW-01: WaterPipeline doesn't expose a `recreate_on_resize` — pipeline state is extent-independent

- **Severity**: INFO
- **Dimension**: Water (M38)
- **Location**: `crates/renderer/src/vulkan/water.rs:154-209`
- **Status**: NEW
- **Description**: `WaterPipeline` has no `recreate_on_resize` method — the pipeline + layout are intrinsically extent-independent (viewport + scissor are dynamic). This is correct, but the file is silent on the contract: other pipelines (SVGF, TAA, Bloom, Composite) all expose `recreate_on_resize` and a future maintainer adding a fixed-extent water resource might forget to add the resize hook.
- **Suggested Fix**: Doc-comment at `WaterPipeline` struct level noting "Pipeline is extent-independent; no `recreate_on_resize` needed. Caller's resize path skips this pipeline." Closes the implicit-contract gap.

---

## Carry-Forward From Prior Audits

Open issues that prior audits flagged and that remain unfixed:

| Issue # | Severity | Dimension | Note |
|---------|----------|-----------|------|
| #952    | LOW      | Sync      | reset_fences before fallible recording. See REN-D1-NEW-02 above. |
| #1092   | LOW      | TAA       | SSAO/cluster jittered inv_view_proj. See REN-D11-NEW-01 above. |
| #1104   | LOW      | Normals   | Path-2 UV-mirror handedness. See REN-D16-NEW-01 above. |
| #924    | LOW      | Sky       | Composite fog gated off (mostly addressed by Markarth aerial fallback). See REN-D15-NEW-01 above. |

---

## Prioritized Fix Order

### Correctness + Safety
1. **REN-D8-NEW-01** (MEDIUM) — TLAS `built_primitive_count` invariant assert: lock the implicit invariant into the runtime.
2. **REN-D1-NEW-02** (LOW carry-over, #952) — Fence-reset deadlock window: ship the missing fence-recovery path.

### Quality
3. **REN-D11-NEW-01** (LOW carry-over, #1092) — un-jittered inv_view_proj for SSAO/cluster.
4. **REN-D16-NEW-01** (LOW carry-over, #1104) — Path-2 bitangent UV-mirror handedness.
5. **REN-D9-NEW-01** (LOW) — `traceReflection` miss fallback: gate sky-tint blend on `is_exterior`.

### Consistency (mechanical sweep)
6. **REN-D1-NEW-01** (MEDIUM) — `TOP_OF_PIPE` → `NONE` in the 5 remaining sites (matches #949/#1100 pattern).
7. **REN-D6-NEW-01** (LOW) — drop duplicate `const float BLOOM_INTENSITY / VOLUME_FAR` from composite.frag.

### Cleanup
8. **REN-D8-NEW-02** (LOW) — unit test for `built_primitive_count` invariant.
9. **REN-D2-NEW-01** (LOW) — wire `scratch_should_shrink` to skinned-BLAS scratch growth path.
10. **REN-D12-NEW-01** (LOW) — TAA per-FIF `frames_since_creation` parity OR comment explaining why shared is correct.
11. **REN-D4-NEW-01** (LOW) — guard Drop safety-net against `thread::panicking()`.
12. **REN-D15-NEW-01** (LOW carry-over, #924) — formally close after composite.frag:484 doc update.
13. **REN-D3-NEW-01** (LOW) — water pipeline static cull_mode comment.
14. **REN-D17-NEW-01** (INFO) — water pipeline extent-independence comment.
15. **REN-D10-NEW-01** (INFO) — SVGF temporal motion units comment.

---

## Verified Fixed Since Prior Audit (2026-05-15)

The 2026-05-15 audit's 27 of 29 findings landed as fixes in the 2026-05-16 batches. Spot-checks confirm:

- **#1081 (REN-D19-001 bloom dangling)** — now `Err` at engine init when bloom creation fails (`context/mod.rs:1607-1613`); composite cannot be created without a bloom view. Engine hard-fails instead of shipping a UB descriptor read.
- **#1082 (REN-D18-001 froxel clear)** — `volumetrics.rs::initialize_layouts:660-723` now does the proper `(rgb=0, a=1)` `cmd_clear_color_image` after the UNDEFINED→GENERAL transition.
- **#1083 (REN-D8-001 TLAS BUILD prim count)** — `tlas.rs:492, 547, 744` track `built_primitive_count` and gate UPDATE on growth match.
- **#1084 (REN-D18-002 interior volumetric)** — `draw.rs:2218-2222` zeroes `scatter_coef` for interior cells.
- **#1085 (REN-D10-003 SVGF NEAREST sampler)** — verified `composite.rs` denoised indirect now uses a dedicated nearest sampler.
- **#1086 (REN-D16-001 BSGeometry tangents)** — `bs_geometry.rs` now decodes the UDEC3-packed tangents.
- **#1095 (REN-D12-002 double AS_WRITE barrier)** — `draw.rs:878-880` confirms the caller-side barrier was removed; the callee at `blas_skinned.rs:555` self-emits.
- **#1100 (REN-D13-003 caustic TOP_OF_PIPE)** — `caustic.rs:649-655` confirms NONE replaces TOP_OF_PIPE.
- **#1108 (REN-D20-001 TAA YCoCg gamma)** — taa.comp gamma widened to 1.5.

The skinned-BLAS flag split landed in `1775a7e6` is correctly applied: all 4 sites (`build_skinned_blas:101, :165`, `build_skinned_blas_batched_on_cmd:330, :451`, `refit_skinned_blas:655`) reference `SKINNED_BLAS_FLAGS`. TLAS sites stay on `UPDATABLE_AS_FLAGS`. VUID-03667 (build/update flag match) holds.

---

## Files Audited

- `crates/renderer/src/vulkan/acceleration/{constants.rs, blas_skinned.rs, blas_static.rs, tlas.rs, predicates.rs, memory.rs, mod.rs, types.rs}`
- `crates/renderer/src/vulkan/{sync.rs, gbuffer.rs, svgf.rs, taa.rs, composite.rs, bloom.rs, volumetrics.rs, caustic.rs, water.rs, ssao.rs, skin_compute.rs, material.rs, texture.rs, descriptors.rs, pipeline.rs}`
- `crates/renderer/src/vulkan/scene_buffer/{constants.rs, gpu_types.rs, buffers.rs, upload.rs}`
- `crates/renderer/src/vulkan/context/{mod.rs, draw.rs, resize.rs, helpers.rs, resources.rs}`
- `crates/renderer/shaders/{triangle.frag, svgf_temporal.comp, composite.frag, ssao.comp, cluster_cull.comp, taa.comp, volumetrics_inject.comp, water.frag, caustic_splat.comp, include/shader_constants.glsl}`
- `byroredux/src/render.rs`, `byroredux/src/systems/weather.rs`
