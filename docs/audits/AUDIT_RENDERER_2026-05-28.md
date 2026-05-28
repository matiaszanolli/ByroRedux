# Renderer Audit — 2026-05-28

## Scope and methodology note

This audit is a **partial sweep** of the 20-dimension `/audit-renderer` checklist. The full parallel-agent fanout (max 3 concurrent) hit transient API rate-limit / per-agent tool-use caps and failed to produce any output across two attempts. The user chose to fall back to a focused self-audit of the 5 highest-risk dimensions; the remaining 15 dimensions are NOT covered by this report and should be re-run when the agent fanout is reliable again.

**Dimensions covered**: 1 (Vulkan Sync), 8 (Acceleration Structures), 12 (GPU Skinning + BLAS Refit), 14 (Material Table R1), 17 (Water M38)

**Dimensions deferred**: 2 (GPU Memory), 3 (Pipeline State), 4 (Render Pass + G-Buffer), 5 (Command Recording), 6 (Shader Correctness), 7 (Resource Lifecycle), 9 (RT Ray Queries), 10 (Denoiser + Composite), 11 (TAA), 13 (Caustic), 15 (Sky / Weather), 16 (Tangent-Space + Normal Maps), 18 (Volumetrics), 19 (Bloom), 20 (Soft Shadows), 21 (Disney BSDF)

Selection rationale: Dim 12 was prioritized because today's #1284 commit (a3c2836a) reshaped `SkinSlotPool` + `SKIN_MAX_SLOTS` + added `DebugStats::skin_pool_*` telemetry; Dim 8 because it's adjacent to Dim 12 in the barrier chain; Dim 17 because Phases A-E of #1210 landed across 2026-05-22/23 and hadn't been re-audited post-completion; Dim 14 because of historically frequent regression risk; Dim 1 because semaphore/barrier defects sit in the CRITICAL severity tier.

## Executive Summary

| Dimension | Findings | CRITICAL | HIGH | MEDIUM | LOW |
|-----------|---------:|---------:|-----:|-------:|----:|
| 1 — Vulkan Sync | 0 | 0 | 0 | 0 | 0 |
| 8 — Acceleration Structures | 0 | 0 | 0 | 0 | 0 |
| 12 — GPU Skinning + BLAS Refit | 1 | 0 | 0 | 0 | 1 |
| 14 — Material Table (R1) | 2 | 0 | 0 | 1 | 1 |
| 17 — Water (M38) | 0 | 0 | 0 | 0 | 0 |
| **Total (5 dims audited)** | **3** | **0** | **0** | **1** | **2** |

**Headline**: zero critical / high / safety findings across the audited surface. One medium-severity drift risk on the `MAT_FLAG_*` lockstep contract. Two low-severity doc / spec rot items.

## RT Pipeline Assessment (Dim 8 + 12)

The ray-tracing pipeline is in unusually good shape post-Session-35 split:

- **AS flag composition** is hoisted to three module-level constants (`STATIC_BLAS_FLAGS`, `SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS`) with deliberate per-tier choices (rigid + TLAS prefer FAST_TRACE; skinned prefers FAST_BUILD per R6a-prospector-regress). `built_flags` recorded on every BlasEntry guards VUID-03667.
- **TLAS-scratch shrink** (#1226 / cf3d8ec6) is alive — the `tlas_scratch_should_shrink` predicate is wired separately from the BLAS-calibrated `scratch_should_shrink`, fixing the dead-code finding from a prior audit.
- **`missing_blas` counter is split by cause** (#1228) — `skinned + rigid + ssbo_evicted` rather than one conflated number. Throttled to 1 log/sec via atomic stamp.
- **Empty-TLAS-at-init** is handled cleanly: camera UBO uploads with `rt_flag = 0` on first frame each FIF slot; `patch_camera_rt_flag(1.0)` (#1227 / 6dd40d3f) flips it in-place after `write_tlas` succeeds and before the render pass begins. The 1-2-frame TLAS-disabled flash on cell entry is fixed.
- **Skinning barrier chain** correctly sequences COMPUTE → AS_BUILD → AS_BUILD → FRAGMENT/COMPUTE through 3 explicit `memory_barrier` calls in `draw.rs`. `pose_dirty` gates both dispatch and refit with the same predicate (#1195 + #1196 paired gate) — no chance of split decisions.
- **Today's #1284 changes verified**: `MAX_TOTAL_BONES` 32 768 → 196 608 cleanly cascades through the SSBO sizing, staging buffer, slot-pool cap, and (via the new const expression) `SKIN_MAX_SLOTS`. Pinned by `bone_palette_overflow_tests.rs` parameterised on `MAX_TOTAL_BONES`. No invariant violations introduced.

## Rasterization Assessment (Dim 1 + 17)

- **Sync surface** has explicit recovery paths on every fallible Vulkan call, semaphore signal-pending windows are documented with VUID citations, and the cross-slot fence-wait invariant is pinned in code comments against future "perf optimization" regressions. Per-image vs per-frame `render_finished` semaphore was bounced and re-bounced; current per-image position resolves VUID-vkQueueSubmit-pSignalSemaphores-00067 across both FIFO and MAILBOX (548c1b69, post-Khronos issue 2007 spec clarification).
- **`reset_fences` placement** (#952 / REN-D1-NEW-04) moved from frame-start to immediately-before-submit, closing a deadlock window of ~2200 lines where any `?` error would leave the fence UNSIGNALED with no pending submit.
- **Water M38 / caustic synthesis Phases A-E** are fully landed: `sun_direction` in CameraUBO (Phase A+B, 8a1a06b4), `WaterCausticAccum` per-FIF image (Phase C, 5f1a9158 / #1255), live synthesis in `water.frag:511-596` (Phase D, 19dfc79c / #1256), composite reads + adds to direct lighting (Phase E, c87ca9db / #1257), `INSTANCE_FLAG_CAUSTIC_SOURCE` macro lift (#1234). Pre-clear / post-render barrier ordering is correct (pre-render-pass clear; post-render-pass barrier sequences fragment-shader writes to composite reads).

## Findings (grouped by severity)

### MEDIUM

#### M14-1: `MAT_FLAG_*` bits 5-9 bypass the generated-header lockstep

- **Dimension**: Material Table (R1)
- **Location**:
  - Rust source: `crates/renderer/src/vulkan/material.rs:448-476` (`material_flag::PBR_BSDF` through `TRANSLUCENCY_MIX_ALBEDO`, bits 5-9)
  - Shader source: `crates/renderer/shaders/triangle.frag:183-187` (hand-written `#define MAT_FLAG_*`)
- **Issue**: The `#1190 / TD4-NEW-01` invariant says `MAT_FLAG_*` bits in `shader_constants_data.rs` emit into the auto-generated `shader_constants.glsl` consumed by `triangle.frag`. Bits 0-4 (`VERTEX_COLOR_EMISSIVE`, `EFFECT_*`) flow through the generated header correctly. Bits 5-9 (the PBR / SSS / model-space-normals suite) live ONLY in `material.rs::material_flag` (Rust) and the shader's hand-written `#define` block — there's NO test asserting the two sets match.
- **Risk**: Drift risk. A future flag-bit reassignment (e.g., dropping a deprecated `TRANSLUCENCY_MIX_ALBEDO` and shifting subsequent bits down) could land Rust-side without touching `triangle.frag`, silently mis-routing material flags. Today the values happen to match.
- **Suggested Fix**: Move bits 5-9 from `material.rs::material_flag` into `shader_constants_data.rs` alongside bits 0-4. Add them to the `shader_constants.rs::tests::generated_header_contains_all_defines` table. Remove the hand-written `#define`s from `triangle.frag:183-187`. Keep the `material_flag::PBR_BSDF` etc. as `pub const = MAT_FLAG_PBR_BSDF` aliases for the Rust-side ergonomic name.
- **Dedup**: NEW.

### LOW

#### S12-1: Stale doc reference to `BONE_PALETTE_OVERFLOW_WARNED`

- **Dimension**: GPU Skinning
- **Location**: `crates/core/src/ecs/resources.rs:646`
- **Issue**: `SkinSlotPool` docstring says "See `BONE_PALETTE_OVERFLOW_WARNED` in `byroredux::render::skinned`" but that `Once`-gated warn no longer exists in `byroredux/src/render/skinned.rs` (only `SKIN_DROPOUT_DUMPED` does). The warn now fires from `SkinSlotPool::allocate` directly via the `overflow_warned` bool field (and as of today, the `overflow_attempt_count` counter feeds `DebugStats::skin_pool_*`).
- **Risk**: Doc rot. No correctness impact.
- **Suggested Fix**: Update the doc reference to "See `Self::overflow_warned` (one-shot warn) and `Self::overflow_attempt_count` (cumulative spill telemetry surfaced via `DebugStats`); see #1284 for the cap-sizing feedback loop."
- **Dedup**: NEW (introduced today by #1284's instrumentation patch; the rename was unintentional).

#### M14-2: Audit-spec size pin (260 B) is stale — `GpuMaterial` is now 300 B

- **Dimension**: Material Table (R1) — audit-spec drift
- **Location**: `/audit-renderer` skill checklist for Dim 14 vs `crates/renderer/src/vulkan/material.rs:1156-1158`
- **Issue**: Audit checklist text says "`GpuMaterial` is exactly **260 bytes**". Actual size is **300 B** per documented growth: 260 → 268 (#1248 IOR) → 284 → 296 (#1249 sheen/sheen_tint/subsurface) → 300 (#1250 anisotropic). The test name and function name are deliberately left as "260" per the in-code comment so a future rename happens in lockstep with the next size shift.
- **Risk**: None on the runtime side — the size pin asserts the correct current value (300). Audit-spec checklist drift means future audit runs against this dimension might be confused by the "260" reference.
- **Suggested Fix**: Update the `/audit-renderer` skill's Dim 14 checklist to say "`GpuMaterial` is exactly 300 bytes after the Disney BSDF lobe additions (#1248 ior, #1249 sheen/subsurface, #1250 anisotropic)." Optionally rename the test to `gpu_material_size_pin`.
- **Dedup**: NEW (audit-spec rot, not a code regression).

## Prioritized Fix Order

1. **M14-1** (drift-risk, medium). Move `MAT_FLAG_*` bits 5-9 into `shader_constants_data.rs` + remove hand-written `#define`s. ~1 file change Rust-side, 1 file change shader-side, 1 new pin test. Low risk, mechanical.
2. **S12-1** (doc rot, low). One-line doc fix in `resources.rs:646`. Trivial.
3. **M14-2** (audit-spec rot, low). Update the `/audit-renderer` skill text under `.claude/commands/audit-renderer.md`. Trivial.

No correctness fixes required.

## What's NOT in this report

The following dimensions were NOT audited and have unknown status as of 2026-05-28:

- **Dim 2** (GPU Memory) — allocator lifecycle, scratch buffer reuse, SSBO growth policy
- **Dim 3** (Pipeline State) — vertex input layout, push constant ranges, dynamic state
- **Dim 4** (Render Pass + G-Buffer) — attachment ops, mesh-ID encoding, layout transitions
- **Dim 5** (Command Recording) — including #1258 / #1259 / #1260 freshness checks
- **Dim 6** (Shader Correctness) — SPIR-V vs GLSL diffs, struct layouts
- **Dim 7** (Resource Lifecycle) — reverse-order teardown of all 54+ VulkanContext fields
- **Dim 9** (RT Ray Queries) — shadow / reflection / GI / window-portal / glass-passthrough rays
- **Dim 10** (Denoiser + Composite) — SVGF temporal accumulation, composite formula
- **Dim 11** (TAA) — Halton sequence, YCoCg neighborhood clamp, disocclusion
- **Dim 13** (Caustic Splat #321 — glass / MultiLayerParallax sibling of Dim 17)
- **Dim 15** (Sky / Weather / Exterior Lighting) — CLMT TNAM, WTHR NAM0, cloud parallax
- **Dim 16** (Tangent-Space + Normal Maps) — Bethesda authored vs FO4 inline vs synthesized
- **Dim 18** (Volumetrics M55) — froxel grid sizing, HG phase, VOLUMETRIC_OUTPUT_CONSUMED gate
- **Dim 19** (Bloom M58) — 5-mip down + 4-mip up, additive HDR blend, bloom-before-ACES order
- **Dim 20** (M-LIGHT v1 Soft Shadows) — sun angular radius, stochastic cone sample, TAA absorption
- **Dim 21** (Disney BSDF Gating #1248-#1252) — `MAT_FLAG_PBR_BSDF` 3-gate-site count, anisotropic GGX degeneracy

A re-run of `/audit-renderer` (or `/audit-renderer --focus 2,3,4,5,6,7,9,10,11,13,15,16,18,19,20,21`) is required to close these dimensions.

## References

- Dimension-level outputs:
  - `/tmp/audit/renderer/dim_1.md`
  - `/tmp/audit/renderer/dim_8.md`
  - `/tmp/audit/renderer/dim_12.md`
  - `/tmp/audit/renderer/dim_14.md`
  - `/tmp/audit/renderer/dim_17.md`
- Dedup baseline: `/tmp/audit/renderer/issues.json` (200 issues, all CLOSED — no open dedup targets)
- Recent contextual commits:
  - `a3c2836a` (today, 2026-05-28) — Fix #1284 triple-bump SkinSlotPool cap + descriptor pin + spill telemetry
  - `cf3d8ec6` — Fix #1226 TLAS-scratch shrink (was dead code)
  - `b2fd533f` — Fix #1145 `built_flags` recording + refit assert
  - `548c1b69` — Fix VUID-vkQueueSubmit-pSignalSemaphores-00067 (per-image render_finished)
  - `6dd40d3f` — Fix #1227 post-TLAS rt_flag patch
  - `c87ca9db` (Phase E of #1210) — Fix #1257 composite samples water caustic accumulator
