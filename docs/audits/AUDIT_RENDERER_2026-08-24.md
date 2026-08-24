# Renderer Audit — 2026-08-24

**Scope**: Full comprehensive `/audit-renderer` run, all 23 dimensions, no `--focus` filter.
**Depth**: deep (data-flow tracing + invariant validation, per-dimension worker agents).
**Repo state**: `main`, clean, HEAD `048a8bd8`.
**Method**: 16 parallel worker agents (one per dimension or small dimension pair),
each performing dedup against `gh issue list` (200-issue window) and the prior
two renderer audit reports (`AUDIT_RENDERER_2026-08-20.md`, `AUDIT_RENDERER_2026-08-16.md`),
then live source verification (grep/read + targeted `cargo test`) against every
checklist item in `.claude/commands/audit-renderer/SKILL.md`. No GitHub issues
were created. No Vulkan device / RenderDoc / `BYRO_VALIDATION` run was used —
all barrier/sync verdicts are source-read confidence, flagged as
"needs RenderDoc verification" where a live-device answer is required, per the
project's standing no-speculative-Vulkan-fix policy.

## Executive Summary

**5 NEW findings**: 0 CRITICAL, 1 HIGH, 3 MEDIUM, 1 LOW.

This is a mature, extremely heavily-audited codebase (~90 prior renderer audit
reports). The overwhelming majority of the 23-dimension checklist — BLAS/TLAS
correctness, SSBO indexing, GPU-struct byte-layout lockstep across all 5
`GpuInstance`/5 `GpuCamera` shader mirrors, synchronization/barrier discipline,
GPU memory lifecycle, NIFAL material translation, the material dedup table,
denoiser/composite correctness, GPU skinning + BLAS refit, camera-relative
precision, pipeline/render-pass state, command-buffer recording, TAA, caustics,
water, volumetrics/bloom, Disney BSDF/soft shadows, sky/weather, tangent-space,
debug telemetry, the Cornell harness, light-animation translation, and the FSR
upscaler chain — remains correct and its regression-guard tests all pass.

All 5 new findings landed in code from the last 1-2 days of active development
(the `#3231` GPU morph-target blending feature and the `#2221` animated
material-sink feature, both 2026-08-23, plus a same-week FSR/bloom pipeline
reorder, `5bab2fed`, 2026-08-22) — i.e. this audit's value is concentrated
almost entirely in dimensions that touch genuinely new code, exactly as
expected for a codebase audited this frequently. 21 pre-existing OPEN issues
were re-confirmed still present and unregressed (not re-filed); one OPEN issue
(#2773) was found already fixed in the working tree but not yet closed on
GitHub.

## RT Pipeline Assessment

BLAS/TLAS correctness (Dim 1), SSBO/ray-query safety (Dim 2), and GPU-struct
layout (Dim 3) — the three CRITICAL-tier dimensions — are clean of new
CRITICAL/HIGH-severity structural defects **except** for one: the new
GPU morph-target feature (`#3231`, landed 2026-08-23) introduced a real
unsynchronized host-write/device-read hazard on its weight buffer (D2-01,
below), the first new HIGH finding against this dimension in several audit
cycles. Every previously-established RT invariant — `instance_custom_index`
== SSBO index, deferred BLAS destruction, the AS-build-input vs.
AS-structure-read barrier-flag distinction, the thin-glass/ReSTIR-DI/BC1
punch-through shader gates, and all five `GpuInstance` GLSL mirror sites
(now including morph-target fields, 128→160 B) — is intact and test-pinned.

## GPU-Struct & Memory Assessment

`GpuInstance` (160 B), `GpuCamera` (352 B), and `GpuMaterial` (364 B) are all
byte-correct, offset-pinned, and GLSL-lockstepped at HEAD — a full 21-shader
`glslangValidator` recompile-and-byte-compare confirmed no stale `.spv`
anywhere in the tree (closing the prior REN-D3-01 finding). The one new
GPU-struct-adjacent finding is documentation-only: `GpuMaterial`'s
2026-08-23 growth (348→364 B, `#2221`) was fixed in code and in its GLSL
mirror same-day but missed five other citation sites (D3-01, below) — the
fourth recurrence of this exact doc-drift class on this struct. GPU memory
lifecycle (Dim 5) is fully clean, including a fresh audit of the new
`MorphSlot` resource's LRU/teardown discipline, which correctly generalizes
the already-audited `SkinSlot` pattern.

## Findings

### CRITICAL

None.

### HIGH

#### D2-01: `MorphSlot` weight buffer is written every frame with no per-frame-in-flight double buffering, racing the GPU read from the previous frame's still-in-flight submission

- **Severity**: HIGH
- **Dimension**: SSBO/Indexing (buffer_reference synchronization)
- **Location**: `crates/renderer/src/vulkan/morph_compute.rs` (`MorphSlot::weight_buffer`,
  `MorphSlot::update_weights`), `byroredux/src/render/skinned.rs`
  (`update_morph_weights`), `byroredux/src/app_frame.rs:169`, `crates/renderer/src/vulkan/context/draw.rs:1604-1626`
- **Status**: NEW (feature landed 2026-08-23, `#3231`, one day before this audit)
- **Description**: Every other per-frame-mutated GPU-read resource in this
  codebase (`camera_buffers`, `light_buffers`, `bone_world_staging_buffers`,
  `instance_buffers`, `dalc_buffers`) is double-buffered by `[frame_index]`,
  for exactly the reason `draw_frame`'s own dual-fence wait documents: the GPU
  may still be executing the immediately-previous frame's submission while the
  CPU prepares the next one. `MorphSlot` allocates **one** `weight_buffer` at
  spawn time and never again — not `[frame_index]`-indexed, its GPU address
  cached once as a `buffer_reference` handle. `render::skinned::update_morph_weights`
  overwrites that single buffer's host-visible memory unconditionally every
  frame, called from `app_frame.rs:169` — *before* `ctx.draw_frame()` is
  reached later in the same `render_one_frame` call. `draw_frame`'s
  `wait_for_fences([in_flight[frame], in_flight[prev]], ...)` — the only
  synchronization point that could prove the prior frame's GPU work has
  finished touching this buffer — runs *inside* `draw_frame`, strictly after
  the host write for the current iteration has already landed.
- **Evidence**:
  ```rust
  // byroredux/src/app_frame.rs:165-169 — write happens before draw_frame()
  crate::render::update_morph_weights(&self.world, ctx);
  // draw_frame() called later in the same function (line 474)
  ```
  ```rust
  // morph_compute.rs — ONE buffer, created once at spawn, never re-created:
  pub struct MorphSlot {
      weight_buffer: GpuBuffer,   // no [frame_index] array
      weight_address: vk::DeviceAddress, // cached once
  }
  pub fn update_weights(&mut self, device: &ash::Device, weights: &[f32]) -> Result<()> {
      self.weight_buffer.mapped_slice_mut()?[..bytes.len()].copy_from_slice(bytes);
      self.weight_buffer.flush_if_needed(device)
  }
  ```
- **Impact**: Any entity with a live `AnimatedMorphWeights` component (facial
  expressions, blinking, talking) writes its weight buffer every frame while
  morph animation is active. If the GPU is running behind the CPU by even a
  fraction of a frame (normal under MAILBOX present mode, which this engine
  selects), the shader reads a torn mix of this-frame and previous-frame
  weights via both `triangle.vert` (primary raster) and `skin_vertices.comp`
  (feeds skinned-BLAS geometry hit by RT shadow/reflection/GI rays). Failure
  mode is a transient, self-correcting per-vertex morph glitch, not a crash —
  but it is a genuine unsynchronized host/device access with no execution
  dependency between the write and the prior read, meeting the "Vulkan spec
  violation = at least HIGH" severity floor.
- **Related**: `#3231` (the feature landing this delta); `draw_frame`'s
  documented dual-fence-wait rationale (`#282`); `#2219`
  (`skinned_vertex_address`/`SkinnedVertexRef`, a comparably-shaped buffer
  that does NOT share this hazard because it's written by the GPU compute
  pass itself, not raced by a host write).
- **Suggested Fix**: Give `MorphSlot` two weight buffers mirroring
  `bone_world_staging_buffers[frame_index]`/`bone_world_device_buffers[frame_index]`,
  publishing the address for the current `frame_index` one frame ahead of the
  read that consumes it. Alternatively, since the buffer is tiny and the
  update is host-cheap, move `update_morph_weights` to run *after*
  `draw_frame`'s dual-fence wait (one frame of morph-weight input latency,
  likely imperceptible against animation timescales) — closes the hole
  without a second buffer. Either fix needs a source-level regression-guard
  test; the defect itself is provable from source, but its visible artifact
  needs RenderDoc/runtime confirmation to observe in practice.

### MEDIUM

#### D3-01: `GpuMaterial`'s 348→364 B growth (`#2221`, 2026-08-23) fixed in code and its one GLSL mirror same-day, but missed five documentation citation sites

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout (doc/comment lockstep)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:172-176`
  (cites a dead test name, `gpu_material_size_is_348_bytes`, and understates
  Material SSBO VRAM by ~4.4%); `docs/engine/shader-pipeline.md:283,326,330,395`
  (struct heading, field table's final row — silently omits the two newest
  fields entirely, growth-history prose, capacity table);
  `docs/engine/memory-budget.md:34`; `docs/engine/renderer.md:134,528`;
  `docs/engine/rt-lighting-material-recovery.md:618`
- **Status**: NEW (the fourth recurrence of this exact doc-drift class on
  this struct: `#2222`→`#2308`→`#2415`→`#2483`, now compounding with this
  348→364 growth; `#3240` closed 2026-08-23 fixed only the `bindings.glsl`
  copy of this same comment, the same day the growth landed)
- **Description**: `#2221` added `shader_color_r/g/b`+`shader_float`
  (offsets 348-360) to `GpuMaterial`, growing it 348→364 B. The Rust struct,
  its tests, and the GLSL mirror in `include/bindings.glsl` are all correct
  and lockstepped (`gpu_material_size_is_364_bytes` passes). Five other
  locations were not updated, including `shader-pipeline.md`'s field table —
  the doc `_audit-common.md` names as authoritative for this exact byte
  layout — which has no row past offset 344/348 and so silently drops the
  two newest fields for any reader reconstructing the struct from the doc,
  the identical failure mode `#3201` already documented for `GpuCamera`'s
  missing `render_debug` row.
- **Evidence**: `grep -rn "348" docs/engine/shader-pipeline.md docs/engine/memory-budget.md docs/engine/renderer.md docs/engine/rt-lighting-material-recovery.md` → 7 stale hits, one of which (`renderer.md:134`) also mis-attributes the
  earlier 300→348 growth to `#2219` (actually `1d94eb24`).
- **Impact**: No runtime effect — struct, GLSL mirror, and layout-pin tests
  are correct; VRAM reservation computes from `size_of::<GpuMaterial>()`
  dynamically. Damage is confined to documentation the project's own audit
  guidance designates authoritative.
- **Related**: `#2483`, `#3201` (identical pattern on `GpuCamera`), `#3240`
  (fixed the `bindings.glsl` half of this exact drift same-day),
  `feedback_shader_struct_sync.md`
- **Suggested Fix**: Same mechanical pass as `#3201`'s suggested fix — update
  the five sites to 364 B, add the missing field-table row, rename the dead
  test citation. Given this is the fourth recurrence, consider a doc-glob
  size-literal regression check rather than a fifth manual fix pass next
  growth.

#### D7-01: Animated material sinks (`#2221`) hash raw per-frame float bits into the dedup key with no quantization, unlike the sibling particle color-fade fix (`#1795`) built for exactly this defect class

- **Severity**: MEDIUM
- **Dimension**: Material Table / R1 dedup
- **Location**: `byroredux/src/render/static_meshes.rs` (`collect_static_mesh_draws`,
  ~lines 705-775); `crates/renderer/src/vulkan/material.rs`
  (`hash_gpu_material_fields`)
- **Status**: NEW (introduced by `#2221`, commit `7fbc5baf`, 2026-08-23)
- **Description**: `#2221` correctly wires `AnimatedAlpha`/color/shader
  fields into `DrawCommand` before `material_table.intern_by_hash` (the
  dedup key does see the animated value each frame — no correctness bug).
  What's missing is a cardinality bound: all seven fields hash as raw
  `f32::to_bits()` with no rounding, exactly the shape `quantize_fade`/
  `COLOR_FADE_STEPS` (`#1795`, `render/particles.rs`) was built to prevent
  ("N visually-near-identical continuous values, N `MaterialTable` slots").
  Entities spawned in the same cell-load pass share phase-locked animation
  timing and dedup fine; the gap opens for instances of the same
  animated-material clip attaching on *different* frames — which the
  engine's own actively-developed exterior-streaming architecture produces
  routinely (props spawn as the player approaches, not all at once). The
  codebase already has an established fix pattern for exactly this class
  (`LightFlicker.phase_offset_secs` — "a room full of identical candles
  doesn't flicker in lockstep") that was not extended to material sinks.
- **Evidence**:
  ```rust
  // material.rs — no rounding before hash
  h.write_u32(mat.material_alpha.to_bits());
  h.write_u32(mat.shader_color_r.to_bits());
  h.write_u32(mat.shader_float.to_bits());
  ```
  ```rust
  // particles.rs — the sibling guard this population lacks
  const COLOR_FADE_STEPS: f32 = 32.0;
  fn quantize_fade(t: f32) -> f32 { (t * COLOR_FADE_STEPS).round() / COLOR_FADE_STEPS }
  ```
- **Impact**: No visual corruption or crash — `MaterialTable` clears and
  rebuilds every frame, and the existing `MAX_MATERIALS` overflow-to-id-0 +
  warn-once path bounds worst case. A cell with many independently-phased
  animated-alpha/color props spends one `MaterialTable` slot per instance
  per frame instead of collapsing to one, directly working against the
  `#780` dedup-ratio telemetry this dimension exists to protect. Real-world
  magnitude unmeasured (no engine launch this pass).
- **Related**: `#1795` (the sibling fix this diverges from), `#2221`
  (introduced the gap), the `LightFlicker` phase-offset precedent.
- **Suggested Fix**: Apply `quantize_fade`-style coarse quantization (32-64
  steps) to `material_alpha`, the four color sinks, and `shader_float`/
  `shader_color` before hashing (full precision can still reach the shader
  if quantization is applied only inside the hash function, matching how
  `#1795` left particle size continuous). Alternatively/additionally,
  stagger animation attach phase for streaming-spawned entities sharing a
  clip, mirroring `LightFlicker`. Add a regression test asserting a bounded
  distinct-hash count for a population of phase-jittered animated instances.

#### D23-01: Bloom's relocation onto the FSR color-input path (`5bab2fed`) introduced new barriers around the exact image FSR reads next, unvalidated by RenderDoc or `BYRO_VALIDATION`

- **Severity**: MEDIUM (HYPOTHESIS — needs validation-layer confirmation
  before any code change; no fix proposed on reasoning alone)
- **Dimension**: FSR Upscaler & Presentation Chain
- **Location**: `crates/renderer/src/vulkan/bloom.rs::BloomPipeline::apply_to_scene`
  (`:760-851`), called from `crates/renderer/src/vulkan/context/post_passes.rs::record_bloom_pass`
  (`:880-905`), immediately upstream of `record_upscale_pass` (`:950`)
- **Status**: NEW
- **Description**: Commit `5bab2fed` (2026-08-22, fixing `#2796`) moved
  bloom to run after `record_composite_pass` rather than before it, so its
  pyramid source/write target both changed to `composite.scene_image_views[frame]`
  — the same image `record_upscale_pass` reads as FSR's primary color input
  one call later in the same command buffer. This is new synchronization
  code: a `SHADER_READ_ONLY_OPTIMAL → GENERAL` barrier before the new
  `bloom_apply.comp` dispatch and the reverse restore after it, both
  introduced by this commit. The commit's own message states plainly that
  this sync code is "exercised by no automated test ... not been validated
  with RenderDoc or the Vulkan validation layers in this session." Given the
  severity floor (FSR Quality is the engine default), a wrong layout/barrier
  here would be CRITICAL. The source-level reasoning is careful and
  internally consistent (mirrors `frame_upscaler.rs::record_native_blit`'s
  existing pattern; the restore barrier's `dst_access_mask` covers both
  possible next consumers), but this is source-read confidence, not
  validation-run confidence — the same gap the project's closed `#2139`
  precedent (CHAIN-D2-02) exists to distinguish, and that prior validation
  run predates this commit by a full session.
- **Evidence**: `git show 5bab2fed -- crates/renderer/src/vulkan/context/post_passes.rs crates/renderer/src/vulkan/bloom.rs`;
  `bloom.rs:783-805` (entry barrier), `bloom.rs:827-850` (restore barrier);
  `post_passes.rs:271` (`record_upscale_pass` called immediately after
  `record_bloom_pass`).
- **Impact**: If the barrier reasoning is wrong, FSR's dispatch or the
  native-blit fallback could sample `scene_color` mid-write, or trip a
  `VUID-VkImageMemoryBarrier-oldLayout` validation error every frame under
  the engine's default configuration. If correct, this closes with zero
  further action.
- **Related**: Fixes `#2796` (bloom color-injection correctness — verified
  correct by source read, not in question); closed precedent `#2139`
  (same "asserted but not validation-confirmed" pattern, resolved via
  `BYRO_VALIDATION=1`).
- **Suggested Fix**: Run `BYRO_VALIDATION=1` for a few hundred frames under
  both `--upscaler fsr3` (default) and `--upscaler taa` (exercises the
  native-blit consumer of the same restored layout), interior and exterior
  scenes, grep for `VUID-VkImageMemoryBarrier-oldLayout`/`SYNC-HAZARD-*`
  against `scene_image`. If clean, close as verified with a one-line note in
  `fsr3-upscaler-integration-plan.md`; if not, the fix is a barrier
  correction, not a re-derivation from scratch.

### LOW

#### D11-01: `composite.rs` descriptor-layout comment says "7 bindings", array declares 9

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/composite.rs:702-703`
- **Status**: NEW
- **Description**: The comment above `ds_bindings` reads "7 bindings — HDR,
  indirect, albedo, params UBO, depth, caustic, volumetric (M55 Phase 4)"
  and enumerates exactly 7 items, but the array declares bindings 0-8 (9
  total) — bindings 7 (bloom, `#2796`) and 8 (water-side caustic
  accumulator, `#1257`) were added later without updating the summary.
- **Evidence**: 9 `DescriptorSetLayoutBinding` entries present; each
  individually commented correctly further down; only the summary line is
  stale.
- **Impact**: Documentation drift only — `validate_set_layout()`, called
  immediately after, cross-checks against SPIR-V reflection at pipeline
  creation and fails fast on any real mismatch.
- **Related**: None.
- **Suggested Fix**: Update the comment to "9 bindings" and add bloom +
  water caustic to the list, or drop the itemized list in favor of "see
  per-binding comments below."

## Reconfirmed Existing Findings (not new, not re-filed)

Per the dedup protocol, these OPEN issues were re-verified against current
source this run and found still accurately describing the code (or, in one
case, already fixed but not yet closed). None are counted in the tally above.

| Issue | Dim | Severity | Title | Disposition |
|---|---|---|---|---|
| #2769 | 1 | LOW | Redundant second LRU-stamp pass in `build_tlas` | Still present; issue's own "protects the ssbo-dropped case" justification looks stale against current stamping order — flagged for next audit to re-verify with the issue's own suggested `log::debug!` instrumentation |
| #2774 | 1 | LOW | `shrink_tlas_scratch_to_fit` case-2 reachability | Issue claims case 2 unreachable; re-derivation this pass suggests it **is** reachable via a shrink-request → grow-only-scratch interaction — flagged for re-verification, not a safety issue either way |
| #2773 | 1 | LOW | Stale monolith-era comments citing dead symbols | **Already fixed in the working tree** (via `#2794`/`#2692`) — issue should be closed on GitHub, no code change needed |
| #2483 | 3 | — (doc) | `GpuMaterial`/`GpuInstance` doc comments quote superseded sizes | Still open; scope now includes the 348→364 growth (see NEW finding D3-01 above) |
| #3201 | 3 | — (doc) | `GpuCamera` 336→352 doc drift, 5 sites + dead test name | Still open, confirmed present verbatim at all 5 cited sites |
| #3045 | 9 | LOW | Three per-frame skin collections still `std::collections::Hash*` not `FxHash*` | Unchanged since 2026-08-16 |
| #2762 | 11 | LOW | `gbuffer.rs` header says "Five" targets, table lists seven | Unchanged |
| #2764 | 12 | LOW | `order_dependent_glass` fragments opaque MultiLayerParallax batches | Unchanged; draw-call-count noise only |
| #2766 | 12 | LOW | `indirect_call_count` overcounts on `dispatch_direct` early-return | Unchanged; telemetry-only |
| #2768 | 13 | LOW | `taa.comp` dispatch hardcodes `div_ceil(8)` instead of generated constants | Unchanged |
| #2771 | 13 | LOW | `MAX_FRAMES_IN_FLIGHT >= 2` assert weaker than ping-pong arithmetic requires | Unchanged |
| #3046 | 17/18 | LOW (doc) | Audit-skill checklist text describes a nonexistent `isInteriorFill` special case | Unchanged; live contract is an ordinary shadowed directional at 0.6× — see both Dim 17/18 reports |
| #2811 | 17 | — (doc) | Disney preset values can't be cross-referenced (GLSL-PathTracer repo absent) | Unchanged |
| #3111 | 18 | — | `player_controller_system` reads `WindField` undeclared vs. `weather_system`'s write | Out-of-dimension (ECS/concurrency), noted only because it touches `weather_system` |
| #3176 | 19 | LOW | Degenerate-tangent guard emits zero tangent for `N=±(1,1,1)/√3` | Unchanged |
| #3177 | 19 | LOW | Z-up `synthesize_tangents` never normalizes N; Y-up sibling does | Unchanged |
| #2821 | 20 | LOW | `_active` telemetry flags ignored by 4 readers (gpu_breakdown, skin.coverage, bench: line, ctx.upscaler) | Unchanged |
| #2830 | 23 | — | Dead render-extent clamp in `FrameExtentSet::for_output` | Unchanged |
| #2829 | 23 | — | FSR context leak on non-OK `byro_fsr3_context_destroy` | Unchanged |
| #2519 | 23 | — | Dispatch-failure fallback blits jittered frame with no discontinuity signal | Unchanged |

## Prioritized Fix Order

1. **D2-01** (HIGH) — the `MorphSlot` weight-buffer synchronization gap.
   Genuine unsynchronized host/device access on a feature that shipped one
   day before this audit; fix is a well-understood double-buffer pattern
   the codebase already uses everywhere else for this exact class of
   resource.
2. **D23-01** (MEDIUM, HYPOTHESIS) — run `BYRO_VALIDATION=1` under both
   upscaler modes to confirm or clear the new bloom/FSR barrier round-trip
   before it accumulates more code on top of an unvalidated sync boundary.
3. **D7-01** (MEDIUM) — animated material-sink dedup quantization. Cheap,
   mechanical fix mirroring an existing in-repo pattern (`#1795`); worth
   doing before the actively-developed exterior-streaming work increases
   its real-world hit rate.
4. **D3-01** (MEDIUM) + **#2483** + **#3201** — the recurring `GpuMaterial`/
   `GpuCamera` doc-drift class. Fourth recurrence on `GpuMaterial` alone;
   worth the doc-glob regression-check suggested in D3-01 rather than a
   fifth manual fix.
5. **D11-01** (LOW) + the reconfirmed-existing LOW items — batch into a
   single doc/comment-hygiene pass; none are urgent individually.
6. **#2773** — close on GitHub; the fix already landed in code.

## Needs-RenderDoc

- **D23-01** (this report) — the new bloom-relocation barriers around
  `scene_color` on the FSR input path. Primary candidate for the next
  `BYRO_VALIDATION=1` session.
- Carried forward from `AUDIT_RENDERER_2026-08-20.md`, still unresolved,
  not re-derived this pass:
  - The shader-side `sceneFlags.x < 0.5` early-out for `water.frag`
    (`geometry_pass.rs` documents it as needing RenderDoc/non-RT
    verification).
  - The cross-frame `lighting_volumes[previous]`/`combustion_*[previous]`
    history reads in `VolumetricsPipeline::dispatch` — correct per spec for
    same-queue submissions but not observable from `cargo test`.
- Bloom's per-FIF mip-chain cross-frame WAR discipline (Dim 16) was checked
  for a reintroduction of the pattern `#931` removed and found clean, but
  was not exhaustively re-diffed line-by-line — flagged as
  needs-RenderDoc-for-certainty rather than a finding.

## Coverage

All 23 dimensions received deep, source-verified treatment this run (not a
carry-forward or regression-guard-only skim) — dispatched as 16 parallel
worker agents covering the CRITICAL tier (AS correctness, SSBO/ray-query
safety, GPU-struct layout) individually, and the HIGH/MEDIUM tiers
individually or in closely-related pairs (sync/barriers; GPU memory;
NIFAL+material-table; denoiser; skinning; camera precision; pipeline+command
buffer; TAA+caustics; water+volumetrics/bloom; BSDF+sky/weather;
tangent-space+telemetry; Cornell+light-anim; FSR/presentation).

Every dimension's worker ran `gh`-backed dedup against the 200-issue open/
closed window plus the two most recent full renderer audit reports before
reporting anything as NEW, and re-ran or spot-checked the cited regression
tests (`cargo test -p byroredux-renderer`, `cargo test -p byroredux`) rather
than trusting prior reports' claims at face value. No engine launch,
RenderDoc capture, or `BYRO_VALIDATION` run was performed (none of the 23
dimensions' checklists strictly require one for source-level verification,
and the project's standing policy is to flag rather than guess at
GPU-invisible-to-`cargo-test` behavior) — this is the one systematic gap in
this audit's coverage, concentrated entirely in the "Needs-RenderDoc"
section above.

TALLY: CRITICAL=0 HIGH=1 MEDIUM=3 LOW=1 (5 new findings; 20 pre-existing
OPEN issues reconfirmed unchanged, 1 reconfirmed already-fixed-pending-close)
