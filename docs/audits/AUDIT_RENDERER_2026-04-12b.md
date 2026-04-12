# Renderer Audit Report — 2026-04-12b (post-fix re-audit)

**Scope**: Full 10-dimension deep audit. Follow-up to AUDIT_RENDERER_2026-04-12.md after fixes #256, #257, #260 landed.

**Dedup baseline**: 48 open GitHub issues.

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 1 |
| LOW      | 6 |
| **Total** | **8** |

The three fixes from the earlier audit (#256 cluster cull UBO, #257 penumbra inversion, #260 stale comments/scratch/fog) are all **verified correct**.

One new **HIGH** finding: the single SSAO AO image is shared across frame-in-flight slots without synchronization, creating a real RAW hazard between consecutive frames. One new MEDIUM: SVGF filters with albedo baked in (documented design debt). Six LOW findings are micro-optimizations, error-path partial cleanup, and informational notes.

## Prior Fix Verification

| Issue | Finding | Status |
|-------|---------|--------|
| #256 | R-01: cluster_cull.comp missing prevViewProj | **VERIFIED FIXED** — `mat4 prevViewProj` present, comment cites #256 |
| #257 | R-02: penumbra scaling inverted | **VERIFIED FIXED** — point/spot vs directional correctly separated |
| #260 | R-05: scratch alignment | **VERIFIED FIXED** — documented with NOTE comment |
| #260 | R-06: stale size comments | **VERIFIED FIXED** — 128/192 bytes correct |
| #260 | R-07: "4 attachments" comment | **VERIFIED FIXED** — now says 6 |
| #260 | R-08: depth store comment | **VERIFIED FIXED** — stale line removed |
| #260 | R-09: unused fog UBO | **VERIFIED FIXED** — fog fields removed from CompositeParams |

## Still-Open Prior Issues

| Issue | Finding | Status |
|-------|---------|--------|
| #258 | R-03: SSBO no growth | Still open (MEDIUM) |
| #259 | R-04: CB/fence indirection | Still open (MEDIUM) |

---

## Findings

### R2-01: Single SSAO AO image shared across frame-in-flight slots — cross-frame RAW hazard
- **Severity**: HIGH
- **Dimension**: Denoiser & Composite / Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/ssao.rs:35`, `crates/renderer/src/vulkan/context/draw.rs:569-582`
- **Status**: NEW
- **Description**: `SsaoPipeline` owns a single `ao_image` with one `ao_image_view`. All frame-in-flight descriptor sets reference the same view. SSAO compute writes the AO image as the LAST operation in each frame's command buffer. The main render pass (earlier in the same buffer) reads it via the fragment shader.

  With MAX_FRAMES_IN_FLIGHT=2, frame slot 1 waits on fence[1] (signaled two frames ago, not one). Frame N+1's fragment shader can read the AO image while frame N's SSAO compute is still writing it. This is a RAW hazard — the Vulkan spec requires explicit synchronization between submissions that access the same resource.

  Other per-frame resources are correctly double-buffered: SVGF history images (per-slot), composite HDR images (per-slot), UBO param buffers (per-slot). SSAO was missed.
- **Evidence**: `ssao.rs:35` — single `ao_image`. `context/mod.rs:371-372` — `write_ao_texture()` called with same view for all frame sets. `draw.rs:49-53` — fence[frame] only waits on same-slot previous use.
- **Impact**: GPU data race. On most NVIDIA/AMD drivers this manifests as intermittent AO flickering or stale AO data rather than a crash, but it is a spec violation.
- **Suggested Fix**: Duplicate `ao_image`, `ao_image_view`, and `ao_allocation` to per-frame-in-flight arrays (cost: ~2 MB at 1080p for one extra R8_UNORM image). Match the pattern in `composite.rs:59` (per-frame HDR images).

### R2-02: SVGF denoiser operates on albedo-baked indirect — blurs texture detail
- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/triangle.frag:666-671`
- **Status**: NEW
- **Description**: The indirect lighting signal passed to SVGF contains `albedo * (1 - metalness)` baked in. The denoiser blurs both lighting noise AND texture detail together. This is internally consistent and documented in the code, but will become a visible artifact when spatial filtering (a-trous) is added.
- **Impact**: Currently subtle (temporal-only). Will cause visible texture blurring when spatial filtering lands.
- **Suggested Fix**: Demodulate albedo before SVGF (divide out), remodulate after. The albedo G-buffer attachment is already available.

### R2-03: GBuffer.recreate_on_resize partial-failure leaves dangling attachments
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:237-248`
- **Status**: NEW
- **Description**: If a mid-sequence `allocate()` fails via `?`, earlier-allocated attachments are not cleaned up. The struct enters an inconsistent half-resized state. The constructor handles this correctly (lines 183-189) but the resize path does not. Only triggers on OOM during resize.
- **Suggested Fix**: Mirror the constructor's cleanup pattern in `recreate_on_resize`.

### R2-04: SVGF.recreate_on_resize same partial-failure pattern
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/svgf.rs:758-773`
- **Status**: NEW
- **Description**: Same issue as R2-03 for SVGF history image recreation.
- **Suggested Fix**: Add `try_or_cleanup!` pattern matching the constructor.

### R2-05: SSAO compute inverts viewProj per-pixel (~2M mat4 inversions at 1080p)
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/ssao.comp:51`
- **Status**: NEW
- **Description**: `inverse(viewProj)` computed per-pixel in the SSAO shader. Should be precomputed CPU-side and uploaded as a UBO field.
- **Suggested Fix**: Add `invViewProj` field to `SSAOParams` UBO, compute CPU-side.

### R2-06: cluster_cull.comp inverts viewProj per-workgroup (3456 redundant inversions)
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/cluster_cull.comp:83`
- **Status**: NEW
- **Description**: Same pattern as R2-05 but per-workgroup (16x9x24 = 3456). Less severe than SSAO.
- **Suggested Fix**: Add `invViewProj` to CameraUBO, shared with SSAO.

### R2-07: GBuffer doc table lists 3 attachments, actual count is 5
- **Severity**: LOW
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:6-11`
- **Status**: NEW
- **Description**: Module-level doc lists only normal, motion, mesh_id. Missing raw_indirect and albedo.
- **Suggested Fix**: Update the doc comment.

### R2-08: Window portal ray origin offset may skip thin walls
- **Severity**: LOW
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:385`
- **Status**: NEW
- **Description**: Window portal ray starts 0.5 units behind surface + tMin=0.1 = 0.6 units blind zone. Thin walls (< 0.5 units) would be skipped. Bethesda content is generally thicker, so unlikely to manifest.

---

## Dimensions Audited

| # | Dimension | Findings |
|---|-----------|----------|
| 1 | Vulkan Synchronization | 0 (R-04 still open #259) |
| 2 | GPU Memory | 2 (LOW: R2-03, R2-04) |
| 3 | Pipeline State | 0 (R-01 verified fixed) |
| 4 | Render Pass & G-Buffer | 1 (LOW: R2-07) |
| 5 | Command Buffer Recording | 0 |
| 6 | Shader Correctness | 2 (LOW: R2-05, R2-06) |
| 7 | Resource Lifecycle | 0 |
| 8 | Acceleration Structures | 0 |
| 9 | RT Ray Queries | 1 (LOW: R2-08) |
| 10 | Denoiser & Composite | 2 (HIGH: R2-01, MEDIUM: R2-02) |

## Prioritized Fix Order

1. **R2-01** (HIGH) — SSAO cross-frame RAW hazard: duplicate AO image per frame-in-flight
2. **R2-02** (MEDIUM) — Albedo demodulation before SVGF (deferred until spatial pass)
3. **R2-03/R2-04** (LOW) — Partial-failure cleanup in resize paths
4. **R2-05/R2-06** (LOW) — Precompute inverse viewProj CPU-side
5. **R2-07/R2-08** (LOW) — Doc update, window ray offset
