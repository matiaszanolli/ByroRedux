# Renderer Audit — 2026-07-05

**Scope**: RT-focused sweep restricted to three dimensions of `/audit-renderer`:

- **Dimension 1** — Acceleration Structures (BLAS/TLAS build, refit, compaction, eviction, scratch alignment) — `crates/renderer/src/vulkan/acceleration/`
- **Dimension 2** — Scene SSBO plumbing + RT ray queries — `crates/renderer/src/vulkan/scene_buffer/` + shaders
- **Dimension 8** — Denoiser / composite / G-buffer — `crates/renderer/src/vulkan/svgf.rs`, `composite.rs`, `gbuffer.rs` + shaders

**Method**: three parallel deep reads (one per dimension), every candidate finding then
re-verified against the live source before inclusion. Two initial candidates were
**disproven** on verification and dropped (documented under "Disproven" below).

---

## Executive Summary

The RT-critical machinery — the AS/SSBO custom-index contract, ray-query geometry,
and the denoiser data flow — **verifies clean**. No CRITICAL or HIGH finding. Every
load-bearing regression guard on the checklist (AS/SSBO 24-bit custom-index contract,
deferred BLAS + scratch destroy, VUID-03667 refit-flag match, `#1790` scratch-serialize
mask, ReSTIR-DI 25° normal-cone spatial reuse, BC1 punch-through gate, SVGF firefly-clamp
hoist, caustic double-count guard, composite tone-map ordering) still holds.

All four NEW findings are **documentation / doc-comment drift** where the code is
authoritative and correct; the docs (one authoritative reference doc, two in-code
docstrings) lag it. Two OPEN issues are corroborated with fresh evidence.

### Findings by severity

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 4 (all NEW) |
| Existing (corroborated) | 2 (#1872, #1874) |

### RT Pipeline Assessment

- **BLAS/TLAS correctness (Dim 1)** — VERIFIED. Geometry format
  (`R32G32B32_SFLOAT` @ offset 0, `UINT32` index, `OPAQUE`, `size_of::<Vertex>()`
  stride) correct at all four build/refit sites. Build-flag constants
  (`STATIC_BLAS_FLAGS`/`SKINNED_BLAS_FLAGS`/`UPDATABLE_AS_FLAGS`) intact.
  `instance_custom_index` is packed from the shared `instance_map` SSBO index with the
  `debug_assert!(ssbo_idx < 1<<24)` mirror of the RP-1 cap — the CRITICAL AS/SSBO
  contract holds. BUILD-vs-UPDATE keys on the `last_blas_addresses` device-address
  sequence with the `built_primitive_count` guard covering VUID-03708 both directions.
  Deferred BLAS destroy (`pending_destroy_blas`), deferred scratch destroy (`#1782`,
  `pending_destroy_scratch`) with the documented `build_skinned_blas_batched_on_cmd`
  immediate exception, TLAS-resize `device_wait_idle` before `free`, and the `#1790`
  `AS_WRITE|AS_READ` scratch-serialize mask all verified present.
- **SSBO + ray queries (Dim 2)** — VERIFIED. Set-1 bindings match between
  `bindings.glsl`, `descriptors.rs`, and the upload writers (b0 lights, b1 camera,
  b2 TLAS, b4 instances, b8 vertex, b9 index, b13 materials, b16/b17 reservoirs).
  Every RT hit-fetch reads `rayQueryGetIntersectionInstanceCustomIndexEXT` (never
  `gl_InstanceID`) then `instances[idx].vertexOffset/indexOffset` +
  `materials[hitInst.materialId]` with a self-consistent
  `indexData[iOff+prim*3+k] → (vOff+i)*25` fetch. Shadow/reflection/GI/glass ray
  origins, biases (tMin 0.05, normal-bias 0.05–0.15), Frisvad refraction basis,
  `GLASS_RAY_BUDGET`, RT gating on `sceneFlags.x > 0.5`, deterministic IGN/hash
  seeding, ReSTIR-DI 25° geometric-normal spatial-reuse cone
  (`SPATIAL_NORMAL_COS=0.906`, 32-B reservoir), and the `#ae285062` BC1 punch-through
  gate all verify.
- **Denoiser stability (Dim 8)** — VERIFIED. SVGF history ping-pong reads
  `prev=(f+1)%2` / writes `f` with a compile-time `MAX_FRAMES_IN_FLIGHT>=2` assert
  (no read==write aliasing); motion vectors match the `triangle.frag`
  `outMotion=(currNDC-prevNDC)*0.5` producer; disocclusion via masked mesh_id + normal
  cone in both the temporal and à-trous passes; firefly clamp hoisted ahead of the
  `hasHistory` branch; composite reassembles `direct + indirect*albedo + caustic`,
  bloom **before** ACES, tone-map **after** reassembly; caustic accumulator sampled
  as `usampler2D`, scaled, and added to **direct** only (no double-count).

### GPU-Struct & Memory Assessment

No layout or lifecycle regression in scope. G-buffer attachment formats verify
against `shader-pipeline.md` (normal `R16G16_SNORM`, motion `R16G16_SFLOAT`, mesh_id
`R32_UINT`, raw-indirect + albedo `B10G11R11_UFLOAT_PACK32`, HDR `R16G16B16A16_SFLOAT`
in `composite.rs`, depth `D32_SFLOAT`), all `COLOR_ATTACHMENT|SAMPLED`, double-buffered
per FIF. The only memory-doc gap is the already-open #1872 (below).

---

## Findings

### REN-2026-07-05-L01: `STATIC_BLAS_FLAGS` comment claims the compaction pass is dead, but it is live
- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs` (`STATIC_BLAS_FLAGS` doc-comment)
- **Status**: NEW
- **Description**: The doc-comment above `STATIC_BLAS_FLAGS` asserts that
  `ALLOW_COMPACTION` is set "even though no caller currently runs the compact pass …
  When the compact pass lands it lights up across all three call sites." That premise
  is stale: `build_blas_batched` (`blas_static.rs`, Phases 3–7) runs a full,
  live compaction pass today.
- **Evidence**: `blas_static.rs::build_blas_batched` creates a
  `QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR` query pool ("Phase 3: Create
  query pool for compacted size readback"), records the size queries after a
  `AS_BUILD → AS_BUILD` WRITE→READ barrier ("Phase 4"), and copies each BLAS with
  `cmd_copy_acceleration_structure(… COMPACT)`. `resources.rs::build_blas_batched`
  drives this on every cell/scene load.
- **Impact**: Documentation only, no runtime effect. A future auditor reading the
  comment could wrongly conclude compaction is unexercised and skip its lifecycle when
  reasoning about the deferred-destroy of uncompacted originals.
- **Related**: Sibling doc-rot to the Dim-8 doc drifts below.
- **Suggested Fix**: Update the comment to state the compaction pass is live in
  `build_blas_batched`; the lockstep-across-call-sites rationale for the shared
  constant remains valid.

### REN-2026-07-05-L02: `shader-pipeline.md` submission order + compute table omit the Session-49 à-trous SVGF pass
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `docs/engine/shader-pipeline.md` (Per-Frame Submission Order block + Compute shader table) — stale; `crates/renderer/src/vulkan/svgf.rs` (`SvgfPipeline::dispatch`) — authoritative
- **Status**: NEW
- **Description**: The doc's authoritative "Per-Frame Submission Order" lists only
  `7 svgf_temporal.comp` and jumps to `14 [Composite render pass]`, and the Compute
  shader table lists only `svgf_temporal.comp`. The live pipeline runs a full
  multi-iteration à-trous spatial chain (`svgf_atrous.comp`) immediately after the
  temporal dispatch, in the same command buffer, and composite samples the à-trous
  final ping-pong slot — not the temporal output.
- **Evidence**: `svgf.rs::dispatch` runs `for k in 0..ATROUS_ITERATIONS { … cmd_dispatch }`
  after the temporal dispatch; `indirect_view(frame)` returns the à-trous final slot.
  `crates/renderer/shaders/svgf_atrous.comp` exists and is compiled. The Session-49
  denoiser overhaul (ReSTIR reservoirs, à-trous) is documented in `memory-budget.md`'s
  ReSTIR section but never propagated to `shader-pipeline.md`'s pass list.
- **Impact**: Documentation only. A reader wiring barriers or reasoning about what
  composite samples would miss the entire spatial filter and its intra-frame
  COMPUTE→COMPUTE barriers.
- **Related**: #1814 (Session-49 overhaul); REN-2026-07-05-L04.
- **Suggested Fix**: Add `svgf_atrous.comp` to the Compute table and insert the à-trous
  chain (with its ping-pong barriers) into the submission-order block after step 7.

### REN-2026-07-05-L03: `shader-pipeline.md` G-buffer row claims reverse-Z depth; the renderer uses standard depth
- **Severity**: LOW
- **Dimension**: Denoiser/Composite (G-buffer)
- **Location**: `docs/engine/shader-pipeline.md` (G-Buffer Layout, Depth row) — stale; `crates/renderer/src/vulkan/pipeline.rs`, `context/draw.rs`, `crates/renderer/shaders/composite.frag` — authoritative
- **Status**: NEW
- **Description**: The G-buffer table states `Depth | D32_SFLOAT | Reverse-Z depth
  (1.0 = camera near, 0.0 = far)`. The renderer uses **standard** depth: near ≈ 0,
  far = 1. This is a self-consistent, correct code convention contradicted only by the
  reference doc.
- **Evidence**:
  - `pipeline.rs` sets `depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)` for both the
    opaque and blend geometry pipelines; viewport `min_depth: 0.0, max_depth: 1.0`.
  - `draw.rs::draw_frame` clears depth to `ClearDepthStencilValue { depth: 1.0, … }`
    (1.0 = far, the standard convention; reverse-Z would clear 0.0 = far with a
    `GREATER` op).
  - `composite.frag` classifies `depth >= 0.9999` as sky/far-plane and comments
    "we use standard depth where 1.0 = far"; the fog/volumetric branches gate on
    `depth < 0.9999`.
- **Impact**: Documentation only today (code is correct). It is a **latent trap** in
  the single authoritative pipeline reference: a depth-pipeline edit that trusted the
  doc and switched to reverse-Z (clear 0.0, `GREATER`) would silently invert composite's
  sky detection and every `depth < 0.9999` fog/volumetric branch, painting near geometry
  as sky. Invisible to `cargo test`.
- **Related**: REN-2026-07-05-L02 (same doc).
- **Suggested Fix**: Correct the Depth row to "standard depth (0.0 = near, 1.0 = far),
  `LESS_OR_EQUAL`, clear = 1.0".

### REN-2026-07-05-L04: `svgf.rs` module docstring is stale on both the indirect-history format and the "temporal only" scope
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/svgf.rs` (module docstring, "Resource layout" + "Phase 3 only" lines)
- **Status**: NEW
- **Description**: Two stale claims in the module header:
  1. It describes `indirect_history[frame]` as **RGBA16F**, but `INDIRECT_HIST_FORMAT`
     is `B10G11R11_UFLOAT_PACK32` (the `#275` 50%-savings change; the constant's own
     comment acknowledges it).
  2. It states "Phase 3 only implements the temporal accumulation pass", but the live
     pipeline now runs the à-trous spatial pass (`svgf_atrous.comp`) — the same
     Session-49 gap as REN-2026-07-05-L02.
- **Evidence**: `svgf.rs` line 11 (`indirect_history … RGBA16F`) vs `INDIRECT_HIST_FORMAT
  = vk::Format::B10G11R11_UFLOAT_PACK32`. `moments_history` correctly stays
  `R16G16B16A16_SFLOAT` (`MOMENTS_HIST_FORMAT`), so the moments-RGBA16F line is fine;
  only the indirect-history format line is wrong.
- **Impact**: Documentation only. Misleads a reader about VRAM footprint and the
  denoiser's stage coverage.
- **Related**: REN-2026-07-05-L02; #1872 (footprint accounting).
- **Suggested Fix**: Change the `indirect_history` format note to `B10G11R11` and
  update the "Phase 3 only" sentence to include the à-trous spatial pass.

---

## Corroborated Existing Issues

### Existing: #1872 — `memory-budget.md` doesn't track the screen-sized RT-denoiser images
- **Dimension**: Denoiser/Composite (Memory)
- **Status**: Existing: #1872 (OPEN, `documentation`)
- **Fresh evidence**: Per frame-in-flight, `svgf.rs::new` allocates `indirect_history`
  (B10G11R11) + `moments_history` (RGBA16F) + **2× à-trous ping-pong** (B10G11R11) —
  four screen-sized images/FIF, eight across 2 FIF. The two à-trous ping-pong buffers/FIF
  are the Session-49 addition and are exactly the untracked class #1872 names. Composite
  adds 2× HDR `R16G16B16A16` (~16 MB at 1080p per its own docstring). None of these
  appear in `memory-budget.md`. Recommend the ledger add: moments (RGBA16F ×2 FIF),
  à-trous ping-pong (B10G11R11 ×2 ×2 FIF), indirect_history (×2 FIF), composite HDR (×2).
- **Action**: Fold this evidence into #1872 when it's next worked; no separate issue.

### Existing: #1874 — Ghosted diagonal double-image in TES interiors — SVGF path ruled out as origin
- **Dimension**: Denoiser/Composite
- **Status**: Existing: #1874 (OPEN, `renderer`/`high`)
- **Fresh evidence**: The SVGF temporal + à-trous passes were read specifically for a
  disocclusion/reprojection gap that could produce a diagonal double-image. None found:
  reprojection uses masked mesh_id + 25° normal-cone rejection + per-tap NaN/Inf drop +
  the hoisted firefly clamp + weighted-average `histAge` (the `#422` disocclusion-streak
  fix, not `max()`). The à-trous pass applies the same hard mesh_id rejection per tap.
  This is consistent with the standing MEMORY hypothesis pinning the mechanism on the
  TAA shared-bad-motion-vector + parked-camera clamp bypass, **not** SVGF. Remains
  RenderDoc-gated; not fixable from the denoiser files.
- **Action**: Keep #1874 open; note SVGF is not the origin. TAA (Dim 13, out of this
  sweep's scope) is the more likely suspect.

---

## Disproven candidates (verified, not reported)

Recorded for transparency — both were initial candidates that verification refuted:

- **Dim 1 — single-sided RT culling divergence under negative-determinant transforms.**
  `tlas.rs` gates `TRIANGLE_FACING_CULL_DISABLE` on `draw_cmd.two_sided` (#416). The
  premise (raster renders negative-scale single-sided meshes correctly while RT culls
  them) does **not** hold: the raster pipelines use a fixed `front_face(COUNTER_CLOCKWISE)`
  with no per-instance cull-mode flip or winding correction for negative-determinant
  instances (`pipeline.rs`). Raster and RT therefore share the same un-corrected winding
  convention and do not diverge on such placements. Dropped.
- **Dim 2 — `water.frag` main refraction ray uses a `+N` origin bias.** The `+N*0.05`
  bias could self-intersect the water surface *if the water plane were in the TLAS*, but
  `tlas.rs` explicitly filters water surfaces out of the TLAS
  (`draw_command_eligible_for_tlas` / `DrawCommand::is_water == true` skip). With water
  excluded, the only effect is a ≤0.05-unit Beer-Lambert distance error — negligible.
  Dropped.

---

## Prioritized Fix Order

All findings are documentation. In descending trap-potential:

1. **REN-2026-07-05-L03** — depth reverse-Z row in the authoritative reference doc
   (highest silent-inversion risk for a future depth-pipeline edit).
2. **REN-2026-07-05-L02** + **L04** — Session-49 à-trous pass missing from the pipeline
   doc and the `svgf.rs` docstring (can batch-fix together).
3. **REN-2026-07-05-L01** — `STATIC_BLAS_FLAGS` compaction comment.
4. Fold the #1872 evidence into that issue.

## Needs-RenderDoc

- **#1874** diagonal ghost — origin confirmed *not* in the SVGF path; requires a capture
  to confirm the TAA-motion-vector hypothesis. No code change proposed (per the
  no-speculative-Vulkan-change discipline).

---

*Generated by `/audit-renderer` restricted to dimensions 1, 2, 8. Dedup baseline:
38 open issues (`/tmp/audit/renderer/issues.json`). No prior renderer audit report in
`docs/audits/` on the RT/denoiser dimensions.*
