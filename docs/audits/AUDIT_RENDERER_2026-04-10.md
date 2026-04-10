# Renderer Audit — 2026-04-10

**Auditor**: Renderer Specialist × 2 (Claude Opus 4.6, parallel)
**Scope**: Full Vulkan renderer post-overhaul — instanced drawing, bindless textures, PBR, clustered lighting, soft shadows
**Prior audit**: `AUDIT_RENDERER_2026-04-09.md`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 3     |
| MEDIUM   | 2     |
| LOW      | 6     |

11 findings total. The 3 HIGH findings share a single root cause: the cluster compute shader
reconstructs frustum geometry using `inverse(viewProj)` but treats the result as view-space,
while the fragment shader uses clip-space W for depth slicing. This coordinate space mismatch
means clustered lighting produces wrong results for most camera orientations. Fixing this one
issue resolves all 3 HIGHs.

---

## Findings

### COMP-1: Cluster compute shader frustum reconstruction uses wrong coordinate space
- **Severity**: HIGH
- **Dimension**: Compute Pipeline / Shader Correctness
- **Location**: `crates/renderer/shaders/cluster_cull.comp:77-83, 102`
- **Status**: NEW
- **Description**: `screenToView()` takes `inverse(viewProj)` (clip→world) but treats the result as view-space. The depth scaling `view.xyz * (depth / -view.z)` assumes Z is the camera's depth axis, which is only true in view space. After `inverse(viewProj)`, Z is a world-space coordinate. Cluster AABBs will be distorted for any camera not looking along world -Z.
- **Evidence**: Line 102: `mat4 invProj = inverse(viewProj);` — clip-to-world, not clip-to-view. Line 82: `return view.xyz * (depth / -view.z);` — assumes view-space depth.
- **Impact**: Lights pop in/out or are assigned to wrong clusters when the camera rotates. The chandelier cutoff line (now fixed with AABB inflation) was a symptom of this deeper issue.
- **Suggested Fix**: Reconstruct world-space frustum corners directly: unproject 8 NDC corners (tile min/max UV at near/far NDC Z) through `inverse(viewProj)`, perspective-divide, compute AABB. No depth-scaling hack needed.

### COMP-2: Fragment shader depth slicing uses different space than compute shader
- **Severity**: HIGH
- **Dimension**: Compute Pipeline / Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.frag:229-233`
- **Status**: NEW (same root cause as COMP-1)
- **Description**: Fragment shader computes `viewDepth = viewPos.w` (clip W = view-space Z for perspective). The compute shader's `sliceDepth()` produces view-space values but feeds them into the world-space `screenToView()`. The two depth systems disagree, assigning fragments to wrong clusters.
- **Evidence**: Fragment: `viewDepth = viewPos.w` (view-space). Compute: `sliceDepth()` values fed to `screenToView()` which produces world-space positions.
- **Impact**: Wrong cluster assignment for depth slices → missing or extra lights per fragment.
- **Suggested Fix**: Once COMP-1 is fixed to use proper world-space corners, compute the fragment's world-space distance from camera (`length(fragWorldPos - cameraPos.xyz)`) for the depth slice lookup.

### SYNC-1: UI instance SSBO write not flushed for non-coherent memory
- **Severity**: HIGH
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:436-449`
- **Status**: NEW
- **Description**: After the bulk `upload_instances` call (which flushes), the UI overlay appends one `GpuInstance` directly via raw pointer copy into mapped memory. No `flush_if_needed` is called. On non-HOST_COHERENT memory (AMD GPUs), this write won't be visible to the GPU.
- **Evidence**: Lines 436-449: `copy_nonoverlapping` into mapped buffer, then `cmd_draw_indexed` — no flush between.
- **Impact**: UI quad renders with garbage data on AMD hardware. Silent on NVIDIA (typically coherent).
- **Suggested Fix**: Add `flush_if_needed` after the raw copy, or include the UI instance in the bulk `upload_instances` call.

### SYNC-2: Missing HOST→COMPUTE barrier before cluster cull dispatch
- **Severity**: MEDIUM
- **Dimension**: Vulkan Synchronization
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:152-220`
- **Status**: NEW
- **Description**: Light and camera data are uploaded to host-visible buffers, then immediately dispatched to the cluster cull compute shader. No `HOST_WRITE→SHADER_READ` barrier exists between the uploads and the dispatch. Vulkan spec requires this even for HOST_COHERENT memory.
- **Evidence**: `upload_lights`/`upload_camera` at lines 152-186, then `cc.dispatch` at line 200. No pipeline barrier between them.
- **Impact**: Spec violation. Most desktop drivers serialize implicitly, but mobile GPUs (Mali, Adreno) may read stale data.
- **Suggested Fix**: Insert `vkCmdPipelineBarrier` with `srcStage=HOST, dstStage=COMPUTE_SHADER, srcAccess=HOST_WRITE, dstAccess=SHADER_READ|UNIFORM_READ` before the dispatch.

### LIFE-1: Scene descriptor bindings 5+6 uninitialized when cluster pipeline fails
- **Severity**: MEDIUM
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:265-280`, `crates/renderer/src/vulkan/context/mod.rs:266-292`
- **Status**: NEW
- **Description**: The scene descriptor layout always includes bindings 5 (cluster grid) and 6 (light indices), but these are only written when `ClusterCullPipeline::new()` succeeds. If it fails and lights exist, the fragment shader reads uninitialized descriptors — undefined behavior.
- **Evidence**: Bindings 5+6 in layout (scene_buffer.rs:265-280) but only written at mod.rs:276-285, gated on cluster pipeline success.
- **Impact**: GPU fault if cluster pipeline creation fails and scene has lights.
- **Suggested Fix**: Create dummy zero-filled buffers for bindings 5+6 as fallback, or add a `cluster_available` flag to sceneFlags that the fragment shader checks before accessing cluster data.

### SHDR-1: Unused push constants in cluster compute shader
- **Severity**: LOW
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/cluster_cull.comp:60-64`
- **Status**: NEW
- **Description**: `screenWidth`/`screenHeight` push constants are declared but never read in `main()`. The camera UBO also has `screen.xy`. Dead code.
- **Suggested Fix**: Remove push constant block from shader + pipeline layout.

### SHDR-2: GLSL version mismatch (vert 450 vs frag 460)
- **Severity**: LOW
- **Location**: `crates/renderer/shaders/triangle.vert:1`, `triangle.frag:1`
- **Status**: Existing: #101

### SHDR-3: Fragment shader uses signed int for instance index
- **Severity**: LOW
- **Location**: `crates/renderer/shaders/triangle.frag:10`
- **Status**: NEW
- **Description**: `fragInstanceIndex` is `flat in int` (signed) where `uint` would be more correct. Functionally harmless.
- **Suggested Fix**: Change to `uint` for consistency with `fragTexIndex`.

### PIPE-1: Stale doc comment says 112 bytes, actual GpuInstance is 128
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:44-46`
- **Status**: NEW
- **Description**: Doc comment says "Layout: 112 bytes" but the struct is 128 bytes. The GLSL and field comments are correct.
- **Suggested Fix**: Update doc comment.

### RPASS-1: LATE_FRAGMENT_TESTS in external→subpass0 src_stage is unnecessary
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:83-96`
- **Status**: NEW
- **Description**: The incoming dependency includes `LATE_FRAGMENT_TESTS` in src_stage, but src is SUBPASS_EXTERNAL and the depth attachment starts UNDEFINED+CLEAR. No prior depth work exists to synchronize against. Harmless over-synchronization.

### COMP-3: Single-thread compute workgroups waste GPU occupancy
- **Severity**: LOW
- **Dimension**: Compute Pipeline
- **Location**: `crates/renderer/shaders/cluster_cull.comp:13`
- **Status**: NEW
- **Description**: `local_size = (1,1,1)` means 3456 workgroups each with 1 thread. 31/32 or 63/64 threads per warp/wavefront are idle. At current scale this costs <0.1ms, but the waste is unnecessary.
- **Suggested Fix**: Use `local_size_x=64` with `dispatch(ceil(3456/64), 1, 1)`.

---

## Prior Issues Now Fixed

- #52 (no instanced drawing) — **FIXED** in Phase 1
- #179 SYNC-001 (TLAS shared buffers) — **FIXED** earlier today
- #180 PIPE-001 (push constants >128B) — **FIXED** earlier today
- #181 SYNC-002 (command pool sync) — **FIXED** earlier today
- #182 SHDR-001 (normal transform) — **FIXED** earlier today
- #183 MEM-001 (depth alloc order) — **FIXED** earlier today
- #184 LIFE-001 (mesh/pipeline Drop order) — **FIXED** earlier today
- #185 MEM-003 (BLAS leak on failure) — **FIXED** earlier today
- #186 RPASS-001 (outgoing subpass dep) — **FIXED** earlier today
- #187 PIPE-002 (UI unused attributes) — **FIXED** earlier today (UiVertex)

## Priority Fix Order

1. **COMP-1 + COMP-2** (cluster coordinate space) — root cause of all clustered lighting errors
2. **SYNC-1** (UI instance flush) — real data race on AMD
3. **SYNC-2** (host→compute barrier) — Vulkan spec compliance
4. **LIFE-1** (fallback for missing cluster buffers) — prevents GPU fault on pipeline failure
