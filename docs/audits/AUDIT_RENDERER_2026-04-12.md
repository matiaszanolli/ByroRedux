# Renderer Audit Report — 2026-04-12

**Scope**: Full 10-dimension deep audit covering rasterization pipeline, ray tracing (BLAS/TLAS, ray queries, shadows, reflections, GI), deferred indirect lighting (G-buffer, SVGF), compositing, synchronization, GPU memory, and resource lifecycle.

**Depth**: Deep (trace data flow, validate invariants)

**Dedup baseline**: 40 open GitHub issues checked. Prior reports: 7 renderer audits (2026-04-02 through 2026-04-10b).

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 3 |
| LOW      | 5 |
| **Total** | **9** |

The renderer is in strong shape. No CRITICAL issues. The single HIGH finding is a UBO layout mismatch in the cluster culling compute shader that silently corrupts light assignment during camera movement. Three MEDIUM findings cover a contact-hardening inversion, a missing SSBO growth mechanism, and a command buffer/fence indirection fragility. Five LOW findings are stale comments, portability risks, and cleanup opportunities.

## RT Pipeline Assessment

The RT pipeline (BLAS/TLAS, ray queries, SVGF denoiser) is **solid**:
- Acceleration structure builds are spec-compliant on all 15 checklist items (one LOW portability note on scratch alignment)
- All 4 ray query types (shadow, reflection, GI, window) use correct parameters, origin biasing, and SSBO indexing
- TLAS build/refit decision logic is correct; UPDATE mode satisfies spec requirements
- SVGF temporal denoiser has correct ping-pong, reprojection, and disocclusion detection
- Composite reassembly and ACES tone mapping are correctly ordered

One MEDIUM shader bug: contact-hardening penumbra scaling is inverted (wider for distant fragments instead of near).

## Rasterization Assessment

The rasterization pipeline (render pass, G-buffer, command recording) is **clean**:
- 6 render targets with correct formats, load/store ops, and layout transitions
- Command recording order is correct: TLAS build -> main RP -> SVGF compute -> composite RP
- Pipeline state matches shader declarations (one HIGH exception in cluster_cull.comp)
- Resource lifecycle has textbook reverse-order teardown across all 54+ VulkanContext fields

---

## Findings

### R-01: Cluster cull CameraUBO missing `prevViewProj` field
- **Severity**: HIGH
- **Dimension**: Pipeline State / Shader Correctness
- **Location**: `crates/renderer/shaders/cluster_cull.comp:32-38`
- **Status**: NEW
- **Description**: The cluster culling compute shader declares CameraUBO without the `prevViewProj` mat4 field that exists in the Rust `GpuCamera` struct (`scene_buffer.rs:117-133`). The GLSL struct jumps from `viewProj` (offset 0) directly to `cameraPos`, but the actual UBO has `prev_view_proj` (64 bytes) between them. Every field after `viewProj` reads from the wrong offset: `cameraPos` reads `prev_view_proj` column 0, `sceneFlags` reads column 1, etc.
- **Evidence**: Rust `GpuCamera` is 192 bytes (viewProj + prevViewProj + position + flags + invProj). Shader CameraUBO declares ~128 bytes (missing the 64-byte prevViewProj).
- **Impact**: Camera position used for cluster AABB view-ray computation is wrong. Masked when camera is stationary (prevViewProj == viewProj), but causes incorrect per-cluster light assignment during fast camera movement. Results in visible light popping at cluster boundaries.
- **Suggested Fix**: Add `mat4 prevViewProj;` after `viewProj` in the shader's CameraUBO block.

### R-02: Contact-hardening penumbra scaling is inverted
- **Severity**: MEDIUM
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:548-549`
- **Status**: NEW
- **Description**: `distRatio = clamp(dist / max(radius, 1.0), 0.1, 1.0)` grows as the fragment moves farther from the light. Multiplying it into `lightDiskRadius` makes the jitter disk larger for distant fragments and smaller for nearby ones. This is physically backwards: a nearby fragment sees the light subtend a large solid angle and should get the widest penumbra.
- **Evidence**: `float lightDiskRadius = max(radius * 0.025 * distRatio, 1.5);` — distRatio increases with distance, so penumbra widens with distance.
- **Impact**: Soft shadows look wrong: distant shadows are softer than close shadows instead of the reverse. Contact-hardening effect is inverted.
- **Suggested Fix**: Either drop the `distRatio` multiplier (use fixed angular disk size) or implement proper PCSS-style contact hardening using the shadow ray's actual hit distance (`distRatio = hitDist / dist`).

### R-03: Global geometry SSBO has no growth mechanism
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/mesh.rs:136-180`
- **Description**: `build_geometry_ssbo()` is a one-shot build with no resize/rebuild path. Meshes loaded after the initial build get default offset 0 and are not in the SSBO. RT reflection UV lookups for late-loaded geometry will return garbage UVs from mesh 0's data.
- **Impact**: Currently benign (all meshes loaded before first frame). Will break when streaming or cell transitions are implemented — reflected surfaces on dynamically loaded meshes will have wrong textures.
- **Suggested Fix**: Track a dirty flag on MeshRegistry; rebuild the SSBO when new meshes are added (or implement append-with-resize).

### R-04: Command buffer indexed by swapchain image, fences by frame-in-flight
- **Severity**: MEDIUM
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:49-93`
- **Status**: NEW
- **Description**: Command buffers are indexed by `image_index` (swapchain image) while fences guard `frame` slots (frame-in-flight). The `images_in_flight` array bridges this gap correctly today, but the indirection makes the safety argument non-obvious. If the tracking were ever broken, command buffer corruption would result with no fence guard.
- **Impact**: No bug today. Design fragility that could cause hard-to-diagnose GPU crashes if modified carelessly.
- **Suggested Fix**: Consider aligning command buffers to frame-in-flight slots (like framebuffers already are) for a more robust design.

### R-05: BLAS/TLAS scratch buffer alignment not explicitly enforced
- **Severity**: LOW
- **Dimension**: Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:182-187`, `441-446`
- **Status**: NEW
- **Description**: Scratch buffer device addresses should be aligned to `minAccelerationStructureScratchOffsetAlignment` (typically 128 bytes). The code relies on gpu-allocator naturally returning 256-byte-aligned GpuOnly allocations. This works on NVIDIA/AMD but is not guaranteed by the spec.
- **Impact**: Latent portability risk for integrated/mobile GPUs. No active bug on desktop.
- **Suggested Fix**: Query the alignment property at device selection and pass it to the allocator or round up the device address.

### R-06: Stale struct size comments in scene_buffer.rs
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:45`, `114`
- **Status**: NEW
- **Description**: Line 45 says GpuInstance is "112 bytes" (actual: 128). Line 114 says GpuCamera is "112 bytes" (actual: 192). Documentation-only, but could mislead debugging.
- **Suggested Fix**: Update the comments to reflect actual sizes.

### R-07: Stale "4 color attachments" comment in pipeline.rs
- **Severity**: LOW
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:414`
- **Status**: NEW
- **Description**: Comment says "Main render pass has 4 color attachments" but it has 6 (added raw_indirect and albedo).
- **Suggested Fix**: Update comment to say 6.

### R-08: Contradictory depth store comment
- **Severity**: LOW
- **Dimension**: Render Pass & G-Buffer
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:76-79`
- **Status**: NEW
- **Description**: Line 76-77 says "Depth store DONT_CARE" but code on line 84 correctly uses STORE. Lines 78-79 then correctly explain why STORE is needed (SSAO reads). First comment is stale.
- **Suggested Fix**: Remove or update line 76-77.

### R-09: Composite UBO fog fields uploaded but unused
- **Severity**: LOW
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:545-554`, `crates/renderer/shaders/composite.frag:17-21`
- **Status**: NEW
- **Description**: `CompositeParams` fog fields are populated and uploaded every frame (~48 bytes) but the composite shader never reads them. Fog is applied in the main pass instead.
- **Suggested Fix**: Remove the unused fog fields from `CompositeParams` or apply fog in composite if that was the intent.

---

## Prioritized Fix Order

1. **R-01** (HIGH) — Cluster cull UBO mismatch: 1-line shader fix, immediate correctness improvement for clustered lighting during camera movement
2. **R-02** (MEDIUM) — Penumbra inversion: shader-only fix, improves shadow quality
3. **R-03** (MEDIUM) — SSBO growth: architectural change needed before streaming/cell transitions
4. **R-04** (MEDIUM) — CB/fence alignment: refactor opportunity, not urgent
5. **R-05 through R-09** (LOW) — Comments, portability, cleanup

## Dimensions Audited

| # | Dimension | Items Checked | Findings |
|---|-----------|--------------|----------|
| 1 | Vulkan Synchronization | 9 | 1 (MEDIUM) |
| 2 | GPU Memory | 9 | 1 (MEDIUM) |
| 3 | Pipeline State | 8 | 1 (HIGH) |
| 4 | Render Pass & G-Buffer | 7 | 2 (LOW) |
| 5 | Command Buffer Recording | 9 | 0 |
| 6 | Shader Correctness | 12 | 2 (1 HIGH dup of #3, 1 LOW) |
| 7 | Resource Lifecycle | 11 | 0 |
| 8 | Acceleration Structures | 15 | 1 (LOW) |
| 9 | RT Ray Queries | 17 | 1 (MEDIUM) |
| 10 | Denoiser & Composite | 11 | 1 (LOW) |

## Notable Clean Areas

- **Resource lifecycle**: Textbook reverse-order teardown across all subsystems. Every GPU resource has a matching destroy path.
- **Acceleration structures**: All 15 checklist items pass. BLAS/TLAS builds are fully spec-compliant.
- **SVGF denoiser**: Correct ping-pong, reprojection, disocclusion detection, and blend factor handling.
- **Command recording**: Perfect ordering — TLAS build -> main RP -> SVGF -> composite. No scope violations.
- **Synchronization**: All barriers correct (AS build, SVGF compute, G-buffer transitions, composite dependency chain).
