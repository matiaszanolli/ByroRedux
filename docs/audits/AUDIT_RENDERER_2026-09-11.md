# Renderer Audit — 2026-09-11 (full sweep, all 23 dimensions)

**Scope**: `/audit-renderer`, all 23 dimensions, `--depth deep` (default). Each
dimension ran as an independent agent against live HEAD, cross-checked against
`docs/audits/AUDIT_RENDERER_2026-09-06.md` (5 days prior), `AUDIT_RENDERER_2026-09-05.md`,
`AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md`, `AUDIT_RENDERER_2026-09-04.md`, and the
56 open issues in `gh issue list --repo matiaszanolli/ByroRedux`.
Reference docs (`docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md`)
were read in full before dispatch and used as the ground truth for GPU-struct
layouts, descriptor bindings, and the per-frame submission order — this report
does not restate them.

## Executive Summary

**7 new findings** — **0 CRITICAL, 0 HIGH, 2 MEDIUM, 5 LOW**.

This is an unusually clean sweep. The prior 2026-09-06 audit (4573 lines, dozens
of findings across all severities including one CRITICAL) was followed by five
days of fix commits (`3c16c42e`, `4ad39f16`, `1ac8fb74`, `28c87cb5`, `1b15cf99`,
`8144ea58`, `803d2d0e`, `043dbbb9`, `ea6d2437`, `0025d822`, `208a68a5`, `17949a9b`,
`1efc5251`, `a65dbffe`, and others) that closed essentially every open finding
from that report, including the sole CRITICAL (`REN-2026-09-06-D9-01`, a skinned
BLAS built from unwritten `bind_inverses` memory) and every HIGH (water
coverage-blending unwritten G-buffer attachments, off-frustum flag-assembly
premise rot, `taa_failed`/`svgf_failed` dead latch arms, GLSL↔Rust struct mirror
gaps, BSDF/GGX shader-side hazards, the FSR/BLAS-budget upscaler exclusion, the
Starfield light-flag premise). Every dimension agent independently re-verified
each fix against live source (not merely trusted the commit message) before
declining to re-file it.

What is left after that sweep is exactly what a mature codebase produces at the
margin: **doc-rot** (stale byte-size comments that outlived a struct shrink,
three times over for the same `GpuMaterial` figure — see D3-01/D7-01), **coverage
gaps in defense-in-depth tooling** rather than live bugs (a dead telemetry
accessor in D5-01, a missing reflection-validation call in D11-01), and **one
real, still-unfixed ordering gap** in the FSR/BLAS-budget resize path (D23-01)
that mirrors a bug class the project has already fixed twice elsewhere. Two
dimensions (D20) surfaced not new code defects but stale/partially-stale
*existing* GitHub issues that should be closed or narrowed rather than re-filed
as-is.

**Pipeline areas affected**: GPU-struct documentation (Material Table, GPU-Struct
Layout), GPU memory telemetry (Memory/Lifecycle), pipeline reflection coverage
(Pipeline/RenderPass), and the FSR/BLAS-budget resize path (FSR/Presentation).
No finding touches AS correctness, ray-query safety, SSBO indexing, sync/barriers,
denoiser correctness, skinning, camera precision, TAA, caustics, water rendering,
volumetrics/bloom, Disney BSDF, sky/weather, tangent-space, or light-animation
translation — all 15 of those dimensions returned clean.

## RT Pipeline Assessment

**Clean.** BLAS/TLAS build/refit/eviction/deferred-destroy (D1), SSBO indexing
and ray-query safety including ReSTIR-DI spatial reuse and the thin-glass gate
(D2), and denoiser stability including SVGF firefly rejection and TAA history
management (D8, D13) all independently re-verified against live source with no
new findings. The `integrity_snapshot()` → `rt.integrity` telemetry chain — the
subject of two prior false-negative sweeps caused by grepping the wrong symbol
— was re-confirmed live and wired correctly. The one CRITICAL finding carried
over from the prior sweep (`D9-01`, a first-sight skinned BLAS built from
never-written `bind_inverses` memory) is confirmed fixed by `043dbbb9`, with the
whole-frame skip landing before any slot is created so the entity retries
cleanly the following frame.

## GPU-Struct & Memory Assessment

Layout pins (`GpuInstance` 160 B, `GpuCamera` 368 B, `GpuMaterial` 428 B/107
fields, the 5+1 `GpuInstance` shader mirrors, `DBG_BITS` 35 entries exhausting
all 32 bits, capacity constants) are all internally consistent and correctly
enforced by their pinning tests — this audit independently re-derived every
size and field count rather than trusting a quoted number, per the skill's
No-Guessing discipline, and found the *tests* correct in every case. What is
**not** correct is a set of narrative doc comments describing `GpuMaterial`'s
growth history: `#3909` (2026-09-07) shrank the struct 432→428 B and correctly
updated the pinning tests, but at least 16 separate comment/doc sites across
`material.rs`, `constants.rs`, `shader_contract_tests.rs`,
`material_translate.rs`, `static_meshes.rs`, two SKILL.md files, and five
`docs/engine/*.md` files still say 432 B or cite a test name
(`gpu_material_size_is_432_bytes`) that no longer exists (D3-01, D7-01). This is
the fourth time this exact struct's size comment has drifted after a shrink.

Memory lifecycle (deferred-destroy queues, BLAS/TLAS scratch shrink call-site
placement, the resumable geometry-SSBO rebuild, teardown ordering) is clean.
The one gap is observability, not correctness: `TextureRegistry::pending_destroy_count`
has had zero callers since #732 despite its doc comment promising a regression
test and shutdown telemetry that don't exist — the same blind spot the BLAS-side
sibling accessors had until `#3999` fixed them (D5-01).

## Findings

Findings are grouped by severity, then by dimension number. IDs use the
`REN-2026-09-11-D<N>-NN` scheme. No CRITICAL or HIGH findings this sweep.

---

# MEDIUM

### REN-2026-09-11-D11-01: `water.rs` and `presentation.rs` are the only two pipelines that build a private descriptor-set layout with zero reflection validation against the shaders that declare it
- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/water.rs` (`WaterPipeline::new`, the `water_caustic_set_layout` construction); `crates/renderer/src/vulkan/presentation.rs` (`PresentationPipeline`'s `descriptor_set_layout` construction); `crates/renderer/src/vulkan/reflect.rs` (`validate_set_layout`)
- **Status**: NEW
- **Description**: Every other pipeline that owns a descriptor-set layout (`bloom.rs`, `caustic.rs`, `composite.rs`, `compute.rs`, `groundcover.rs`, `skin_compute.rs`, `ssao.rs`, `svgf.rs`, `taa.rs`, `volumetrics/init.rs`, plus the two shared set-0/set-1 layouts) calls `reflect::validate_set_layout` at construction to catch drift between the hand-written Rust `DescriptorSetLayoutBinding` list and the SPIR-V that actually declares those bindings. `water.rs`'s set 2 (water-caustic accumulator + `GpuWaterParams[]`) and `presentation.rs`'s set 0 (upscaled-scene sampler + image-health counter) build their layouts with only a prose comment asserting the binding matches — no test would catch drift.
- **Evidence**: `grep -rln validate_set_layout crates/renderer/src/vulkan/*.rs` omits `water.rs` and `presentation.rs`, though both call `create_descriptor_set_layout` directly. Both layouts are currently correct by hand-inspection against `water.vert`/`water.frag` and `presentation.frag` respectively.
- **Impact**: Defense-in-depth gap, same class as the already-tracked `D16-01` GLSL↔Rust struct-mirror gap. A future binding edit to either shader that isn't mirrored in the Rust layout fails only at `vkCreateDescriptorSetLayout`/`vkUpdateDescriptorSets` time (or silently misbehaves with validation off), invisible to `cargo test`. Not a live bug today.
- **Related**: `REN-2026-09-06-D11-01`/#3977 (the fragment-output-interface sibling of this exact gap class — that finding's own suggested fix named this generalization, which was never implemented).
- **Suggested Fix**: Add a `reflect::validate_set_layout` call at the end of each pipeline's layout construction, mirroring the pattern `scene_buffer/buffers.rs` already uses for set 1. Pure test-time reflection check; no Vulkan behavior change.

### REN-2026-09-11-D23-01: the resize path's BLAS-budget recompute reads the upscaler's SDK memory figure before the upscaler is recreated for the new extent, leaving the reservation's SDK term one resize cycle stale with no later call to correct it
- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation (Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_bloom_and_volumetrics`'s `recompute_blas_budget` call; `recreate_screen_passes`'s ordering relative to `recreate_taa_and_presentation`)
- **Status**: NEW
- **Description**: `#3988` correctly widened `recompute_blas_budget`'s signature to include the FSR SDK memory footprint and fixed both *construction-time* call sites (`init.rs`) with an explicit two-phase pattern (call with `0` before the upscaler exists, call again with the real figure right after). The *resize* path only got the first half: `recreate_bloom_and_volumetrics` reads `self.frame_upscaler.sdk_memory_bytes()` from the pre-resize upscaler object while pairing it with the already-updated new `frame_extents`, and `recreate_taa_and_presentation` (which actually calls `frame_upscaler.recreate(...)`, refreshing `sdk_memory_bytes` for the new extent) runs *after* that budget recompute. There are only 3 call sites for `recompute_blas_budget` in the crate (`init.rs` ×2, `resize.rs` ×1) — nothing re-derives the budget after the upscaler catches up, so the mismatch persists until the *next* resize (repeating the same one-cycle lag against the next extent). `set_upscaler_mode` reuses this same resize path, so runtime upscaler switches are affected identically.
- **Evidence**:
  ```rust
  // resize.rs — recreate_bloom_and_volumetrics
  let extents = self.frame_extents;               // already the NEW extent
  let sdk_bytes = self.frame_upscaler.as_ref().map_or(0, |u| u.sdk_memory_bytes()); // still the OLD upscaler
  accel.recompute_blas_budget(extents, volumetrics_config, sdk_bytes);
  // ... later in the same recreate_screen_passes sequence ...
  // recreate_taa_and_presentation:
  upscaler.recreate(..., self.frame_extents)?;   // NOW rebuilds sdk_memory_bytes for the new extent — too late
  ```
  `grep -rn recompute_blas_budget crates/renderer/src` confirms exactly 3 call sites, none after step 4.
- **Impact**: Budget-accuracy only — same failure class as the now-fixed `#3839`/`#3988` (BLAS eviction starting too late on small-VRAM cards under-reserves for whatever the FSR SDK context's actual footprint is at the new resolution). Not reproduced on the 12 GB dev card; flagged per this project's speculative-Vulkan-fix policy as a code-shape finding needing verification on small-VRAM hardware, not a measured failure.
- **Related**: `#3839`, `#3988` (fixed the construction-time half of this exact bug class).
- **Suggested Fix**: Move the `recompute_blas_budget` call in the resize path to after `recreate_taa_and_presentation`, or add a second call there once the upscaler reflects the new extent — mirroring `init.rs`'s two-phase shape.

---

# LOW

### REN-2026-09-11-D3-01 / REN-2026-09-11-D7-01: `GpuMaterial`'s 432→428 B shrink (#3909) was correctly applied to every layout-pin test but left the OLD size and a nonexistent test name in 16+ doc/comment sites
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout / Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs` (struct top-doc + 4 more inline sites), `crates/renderer/src/vulkan/material_tests.rs`, `crates/renderer/src/vulkan/scene_buffer/constants.rs`, `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`, `byroredux/src/material_translate.rs`, `byroredux/src/render/static_meshes.rs`, `.claude/commands/audit-safety/SKILL.md`, `.claude/commands/_audit-common.md`, and five `docs/engine/*.md` files
- **Status**: NEW (two independent dimension agents found the same class of drift from different entry points; merged here)
- **Description**: `a65dbffe` (Fix #3909, 2026-09-07) removed `GpuMaterial.texture_index`, correctly shrinking the struct from 432 B to 428 B and correctly renaming/updating the pinning test (`gpu_material_size_is_428_bytes`) and one nearby "Shader Struct Sync" paragraph. It did not reach the struct's *own* top-of-file doc comment (`material.rs`, ~4 lines above the corrected paragraph) or the 15+ other files that cite the old figure — several of which (`material.rs:1047/1321/1363/1380`, `material_translate.rs`) narrate the struct's entire growth history and simply stop one step short of the real total. `material.rs` now contradicts itself within 30 lines, and `material_tests.rs`'s own doc comment sits two lines above the assertion it contradicts. This is the third or fourth recurrence of this exact doc-rot class for this exact struct (`#3846`, `#3414`/`#3240`).
- **Evidence**: `rg -n "fn gpu_material_size_is_432_bytes"` → zero hits anywhere in the tree; only `gpu_material_size_is_428_bytes` exists and asserts `size_of::<GpuMaterial>() == 428`. Re-derived field count independently: 107 `pub` fields × 4 B = 428 B, matching the live test.
- **Impact**: Documentation only — `hash_gpu_material_fields`, `DrawCommand::material_hash`, and every offset/size pin are internally consistent at 428 B; no pixel or dedup-key behavior is affected. Cost is to the next reader/contributor, and to the next (eighth) size change, which will very likely repeat this at the same sites absent a structural fix.
- **Related**: #3909, #3846, #3414/#3240 (prior recurrences of this exact class), #1321/#1522/#1624/#3869/#4042 (the parallel `classify_pbr` doc-rot recurrence, same shape different symbol).
- **Suggested Fix**: Mechanical find/replace "432 B"/`432`/`gpu_material_size_is_432_bytes` → "428 B"/`428`/`gpu_material_size_is_428_bytes` at all sites listed. Given this is the third+ sweep for this one struct, consider a `collect_stale_gpu_material_size_claims` source-scanning test (mirroring `#4042`'s `collect_live_classify_pbr_claims` for the `classify_pbr` recurrence) that fails the build if a `4\d\d B` figure near "GpuMaterial" doesn't match `size_of::<GpuMaterial>()`.

### REN-2026-09-11-D5-01: `TextureRegistry::pending_destroy_count` has no caller anywhere in the workspace — the same dead-accessor shape already fixed for the BLAS deferred-destroy queue two sweeps ago, unfixed in this sibling subsystem
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/texture_registry/release.rs` (`pending_destroy_count`, `drain_pending_destroys`, `tick_deferred_destroy`)
- **Status**: NEW
- **Description**: The doc comment on `pending_destroy_count` claims it is "surfaced for the regression test and shutdown telemetry" — neither exists. `grep -rn "\.pending_destroy_count("` across the workspace returns zero hits; `ScratchTelemetry` (the struct `ctx.scratch` reads) has no texture-registry pending-destroy field, and `fill_scratch_telemetry` never calls this accessor. This is the identical shape `REN-2026-09-06-D5-04` found and fixed (`#3999`) for `AccelerationManager`'s three deferred-destroy accessors — that fix did not extend to this sibling subsystem.
- **Evidence**: The deferred-destroy bookkeeping itself (`drop_texture`/`drop_textures` → `tick_deferred_destroy` → `drain_pending_destroys` at shutdown) is sound; only the telemetry accessor is unread.
- **Impact**: No leak or correctness risk — pure observability gap. If `tick_deferred_destroy` ever stalled (e.g. a mis-set `current_frame_id`), there is no console/log surface to notice the texture-side deferred-destroy queue growing unbounded before it eventually shows up as VRAM pressure.
- **Related**: `#3999`/`REN-2026-09-06-D5-04` (the BLAS-side fix this subsystem was never brought in line with).
- **Suggested Fix**: Add a `textures_pending_destroy` row to `ScratchTelemetry`, populated from this accessor in `fill_scratch_telemetry` alongside the existing BLAS rows, plus a shutdown-drain test in `texture_registry_tests.rs` mirroring the BLAS-side coverage — or delete the accessor and correct its doc comment. Given `#3999` already established the pattern, extending it is the smaller diff.

### REN-2026-09-11-D20-01: existing issue #4022 (`depth.stats`'s "armed, run again" dead end on a non-D32_SFLOAT device) is fully fixed at HEAD — recommend closing, not re-filing
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/core/src/ecs/resources/mod.rs` (`DepthCaptureBridge::unsupported_reason`), `byroredux/src/commands/depth.rs`
- **Status**: Existing: #4022 — RESOLVED at HEAD by `17949a9b` (Fix #4003, 2026-09-07, filed independently of #4022 the day after it was raised)
- **Description**: `depth.stats` now checks `bridge.unsupported_reason()` before arming a capture and returns a specific "depth capture unsupported: device selected {format}, not D32_SFLOAT" message instead of ever reaching the generic "armed, run again" line on an unsupported device.
- **Impact**: None remaining — cannot be reproduced against current `main`.
- **Suggested Fix**: Close #4022 with a reference to `17949a9b`/#4003. No code change needed.

### REN-2026-09-11-D20-02: existing issue #4021 (depth-capture ordering-test hazard scan) is partially fixed — the false-rationale half is resolved, the window-anchor and hazard-string-list half is unchanged
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`capture_ordering_tests::record_copy_runs_immediately_after_the_depth_history_copy`)
- **Status**: Existing: #4021 — partially resolved at HEAD by `1efc5251` (Fix #4032)
- **Description**: Of #4021's three sub-issues: (1) the test's assertion message falsely attributed the layout precondition to the history copy's own barriers — **fixed**, now correctly attributes it to the render pass's `final_layout`. (2) the scan window still anchors on the conditional history-copy call site rather than render-pass-end — **still open**. (3) the hazard-string list is still missing `memory_barrier(`, `image_barrier_*`, `cmd_pipeline_barrier2`, and `cmd_clear_depth_stencil_image` — **still open**, and `draw.rs` calls `memory_barrier(...)` inside the unscanned window.
- **Impact**: Documentation/test-coverage only; the underlying invariant holds at HEAD.
- **Suggested Fix**: Update #4021's body to drop the now-fixed rationale complaint and keep only the window-anchor and hazard-list suggestions. Do not re-file as a new issue.

---

## Prioritized Fix Order

1. **D23-01** (MEDIUM) — fix the resize-path BLAS-budget ordering gap; this is a real, reproducible-by-code-inspection ordering bug in a class the project has already twice paid down elsewhere, and the fix is a small, low-risk reorder.
2. **D11-01** (MEDIUM) — add `validate_set_layout` calls to `water.rs`/`presentation.rs`; pure test-time coverage, no behavior change, closes the last two gaps in an otherwise-complete pattern.
3. **D3-01/D7-01** (LOW) — mechanical doc-rot fix across ~16 sites, ideally paired with a structural guard test so an eighth size change can't repeat this a fourth time.
4. **D5-01** (LOW) — wire the dead texture-registry telemetry accessor into `ScratchTelemetry`, mirroring the already-shipped BLAS-side pattern.
5. **D20-01/D20-02** (LOW, issue-tracker hygiene) — close #4022, narrow #4021's body. No code change.

## Needs-RenderDoc

None. Every finding in this sweep is either a documentation/comment defect, a
missing test-time reflection/telemetry guard, or a code-shape ordering bug
verifiable by static inspection (D23-01) — none required GPU-side capture
verification, and no dimension agent proposed a speculative barrier/pipeline
change.

## Verified-Clean Dimensions (no findings)

D1 (Acceleration Structures), D2 (SSBO/Ray Queries), D4 (Sync/Barriers), D6
(NIFAL Material), D8 (Denoiser/Composite), D9 (GPU Skinning/BLAS Refit), D10
(Camera-Relative Precision), D12 (Command Buffer Recording), D13 (TAA), D14
(Caustic Splat), D15 (Water), D16 (Volumetrics/Bloom), D17 (Disney BSDF/Soft
Shadows), D18 (Sky/Weather), D19 (Tangent-Space), D21 (Cornell Harness), D22
(Light Animation). Each dimension independently re-verified every checklist
item and every candidate finding carried over from the 2026-09-06 report
against live source before concluding clean — see the per-dimension detail
that was collected during this sweep for the specific commits (`3c16c42e`,
`4ad39f16`, `1ac8fb74`, `28c87cb5`, `1b15cf99`, `8144ea58`, `803d2d0e`,
`043dbbb9`, `ea6d2437`, `0025d822`, `208a68a5`, `17949a9b`, `4847a6a7`,
`31ebdd7c`, `0742209b`, `1a2d1675`, `fd3e66f0`) that closed each prior finding.

## Pre-Existing Open Issues Noted (not re-verified/re-filed, dedup only)

- `#2764` (D12 scope) — `order_dependent_glass` unnecessarily fragments opaque
  MultiLayerParallax batches.
- `#3572` (D13 scope) — TAA resolves only pre-composite direct HDR; sky/GI/
  volumetrics/caustics/bloom bypass the resolve.
- `#3902` (D7 scope) — secondary-ray `rayHitAlbedo` drops decal/tint/inner-layer/
  dark/detail composition; only its alpha-coverage half was fixed by `#3986`.
- `#3985` (D18 scope) — `cloud_scroll_vectors` zero-vector sentinel + fallback
  ignores authored wind direction.
- `#4023` (D21 scope) — four `glass_*` `GpuMaterial` scalars have no `mat.set`
  console arm.
- `#4041`, `#4044` (D6 scope) — SKILL.md caller-list drift; greyscale-LUT
  hand-copied at two particle spawn sites.
