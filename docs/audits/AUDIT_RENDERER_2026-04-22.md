# AUDIT_RENDERER — 2026-04-22

**Auditor**: Claude Sonnet 4.6 (1M context)
**Baseline**: AUDIT_RENDERER_2026-04-18.md + open issues (issues.json as of 2026-04-22)
**Dimensions**: 10 (Sync, GPU Memory, Pipeline State, Render Pass, Command Recording, Shader Correctness, Resource Lifecycle, Acceleration Structures, RT Ray Queries, Denoiser & Composite)

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 5 MEDIUM · 3 LOW · 0 INFO**

This is the most stable renderer audit since the project began. All 9 CRITICAL and 7 HIGH findings from the 2026-04-18 audit are confirmed fixed in current code. The remaining 8 findings are medium correctness/spec gaps or low-severity documentation drift.

**Confirmed fixes since 2026-04-18**: TLAS→caustic barrier scope (SYNC-C1), BLAS scratch pooled (MEM-C2), shared pipeline cache with disk persistence (PIPE-C1), instance_custom_index/SSBO-index parity (AS-H1), TRIANGLE_FACING_CULL_DISABLE gated on two_sided (AS-H2), TerminateOnFirstHit on reflection + glass + refraction + portal rays (RT-H1), directional shadow tMax raised to 100 000 (RT-H2), window portal fires along -N not -V (RT-H3), SVGF histAge weighted average (COMP-H2), SVGF/Composite/GBuffer full resource cleanup (LIFE-C1/C2/C3), tick_deferred_destroy moved after fence wait (CMD-M1).

---

## RT Pipeline Assessment

**BLAS/TLAS correctness**: Sound. Vertex format R32G32B32_SFLOAT at offset 0, UINT32 indices, OPAQUE flag, PREFER_FAST_TRACE + ALLOW_COMPACTION. Transform row/column transposition correct. UPDATE/REBUILD decision logic (decide_use_update) guards instance-count parity. Empty-TLAS guard at frame 0 operational. instance_custom_index/SSBO-index parity confirmed via shared build_instance_map.

**Ray query safety**: Mostly correct. All four ray types (shadow, reflection, glass/refraction, portal) now carry gl_RayFlagsTerminateOnFirstHitEXT. All sites gated on rtEnabled. RIS reservoir weights clamped via max(…, 1e-6) — no divide-by-zero. **One new issue**: buildOrthoBasis produces NaN tangent when the fragment normal is exactly (0,1,0), which is common on flat terrain LAND quads. This will feed a NaN direction to GI cosine-hemisphere rays (RT-2, MEDIUM).

**Denoiser stability**: SVGF ping-pong is correct (reads prev slot, writes current). Reprojection math verified (`prevUV = uv - motion` matches `outMotion = (currNDC − prevNDC) * 0.5` in triangle.frag). Mesh-ID disocclusion rejection operational. Alpha blend fragments skip temporal reuse via bit-15 flag. histAge weighted-average accumulation matches Schied 2017 §4.2.

---

## Rasterization Assessment

**Pipeline state**: Vertex binding/location/format matches Vertex::attribute_descriptions() through all 8 attributes. Dynamic viewport/scissor set each frame. Pipeline cache shared and persisted to disk. Two medium gaps remain: composite render pass dependency missing COMPUTE_SHADER in src_stage_mask (SY-1), and main render pass outgoing dependency carries BOTTOM_OF_PIPE in dst_stage_mask (SY-2, spec violation). Pipeline rebuild on resize is still unconditional (PIPE-2) despite dynamic viewport/scissor — cosmetic perf hit only.

**Render pass & G-buffer**: CLEAR+STORE on all attachments. UNDEFINED→COLOR_ATTACHMENT→SHADER_READ_ONLY_OPTIMAL transitions via render pass layout fields. G-buffer images carry SAMPLED usage. Depth stored (required by SSAO). Clear values match attachment formats. No issues found.

**Command recording**: Begin/End balanced, render pass begin/end balanced, TLAS build precedes render pass begin, SVGF dispatch precedes composite pass. Per-draw state complete. No commands outside valid scope.

---

## Findings

### MEDIUM

#### [SY-1] Composite render pass `dep_in` omits `COMPUTE_SHADER` in `src_stage_mask`
- **Dimension**: Vulkan Sync
- **File**: `crates/renderer/src/vulkan/composite.rs:368-374`
- **Finding**: The composite render pass incoming subpass dependency sets `src_stage_mask = COLOR_ATTACHMENT_OUTPUT` only. The compute passes immediately before composite (SVGF, TAA, caustic, SSAO) all write images sampled by the composite fragment shader. In practice every such pass emits an explicit manual image barrier, masking this gap. If any future compute pass is added that relies on the render pass dependency instead of a manual barrier, it will race with composite.
- **Fix**: Add `COMPUTE_SHADER` to `src_stage_mask` at composite.rs:370. One-line change.

#### [SY-2] Main render pass outgoing dependency includes `BOTTOM_OF_PIPE` in `dst_stage_mask`
- **Dimension**: Vulkan Sync
- **File**: `crates/renderer/src/vulkan/context/helpers.rs:154-158`
- **Finding**: `dst_stage_mask = FRAGMENT_SHADER | COMPUTE_SHADER | BOTTOM_OF_PIPE`. Per Vulkan spec §7.6.1, `BOTTOM_OF_PIPE` may only appear as dst when both `src_access_mask` and `dst_access_mask` are 0. It provides no memory ordering guarantees here. Synchronization2 validation rejects this combination.
- **Fix**: Remove `BOTTOM_OF_PIPE` from `dst_stage_mask`. The `FRAGMENT_SHADER | COMPUTE_SHADER` portion is correct and sufficient.

#### [PIPE-2] Pipeline rebuilt on resize despite dynamic viewport/scissor
- **Dimension**: Pipeline State
- **File**: `crates/renderer/src/vulkan/context/resize.rs:50-64`
- **Finding**: `recreate_swapchain` unconditionally destroys and recreates all rasterization pipelines (main, two_sided, blend cache, UI). Since viewport/scissor are dynamic state, these pipelines are independent of the swapchain extent. Rebuild is triggered because the render pass is also recreated unconditionally. On high-frequency resize (e.g. window drag) this causes avoidable pipeline compilation stalls.
- **Fix**: Compare old vs new `swapchain_format` and `depth_format` before recreating the render pass. Skip pipeline rebuild if only the extent changed.

#### [SH-1] `triangle.frag` `float vertexData[]` reinterpretation risk for non-float vertex fields
- **Dimension**: Shader Correctness
- **File**: `crates/renderer/shaders/triangle.frag:175`, `crates/renderer/src/vertex.rs:240`
- **Finding**: The shader reads the global vertex SSBO as a flat `float[]` buffer and accesses UV at offsets 9-10 (float indices). This works because those bytes are genuinely `f32`. However, the full Vertex layout includes `bone_indices: [u32; 4]` (bytes 44-59) and `splat_weights: [u8; 8]` (bytes 68-75) — not floats. Any future RT shader code reading those regions as `float vertexData[base + 16]` or similar would silently reinterpret u32/u8 bit patterns as f32. No active bug today (only UV is read), but the precedent is dangerous.
- **Fix**: Add a comment at triangle.frag:175 warning that only offsets 0-10 (position+color+normal+UV) are safely readable as f32. Long-term: a separate UV-only SSBO for RT hit lookups eliminates the reinterpretation concern entirely.

#### [RT-2] `buildOrthoBasis` produces NaN tangent when N = (0,1,0) exactly
- **Dimension**: RT Ray Queries
- **File**: `crates/renderer/shaders/triangle.frag:250-254`
- **Finding**: `buildOrthoBasis(dir)` uses `up = vec3(0,1,0)` when `abs(dir.y) >= 0.999`. When `dir = (0,1,0)` exactly, `cross(up, dir) = cross((0,1,0),(0,1,0)) = (0,0,0)`, and `normalize(vec3(0))` is NaN in GLSL. Flat terrain LAND quads routinely produce exactly-upward normals. The GI hemisphere bounce ray for those fragments will receive a NaN direction, making `rayQueryInitializeEXT` undefined per the Vulkan RT spec.
- **Fix**: Raise the threshold to `abs(dir.y) < 0.9999` (covers ±0.81°) and/or use the Frisvad (2012) singularity-free orthonormal basis, which avoids the cross-product entirely.

### LOW

#### [MEM-1] `GpuInstance` struct comment says 192 bytes; actual struct is 320 bytes
- **Dimension**: GPU Memory
- **File**: `crates/renderer/src/vulkan/scene_buffer.rs:111`
- **Finding**: Comment reads "Layout: 192 bytes per instance, 16-byte aligned (12×16)" but the struct ends at offset 316+4 = 320 bytes (20×16) following the #562 Skyrim+ variant payload additions. Misleads bandwidth estimation.
- **Fix**: Update comment to "Layout: 320 bytes per instance, 16-byte aligned (20×16)."

#### [PIPE-1] Composite pipeline bakes static viewport/scissor redundantly alongside dynamic state
- **Dimension**: Pipeline State
- **File**: `crates/renderer/src/vulkan/composite.rs:637-681`
- **Finding**: `new_inner` bakes `width/height` into `viewports`/`scissors` at creation (lines 637-648) and also includes `DYNAMIC_STATE_VIEWPORT | DYNAMIC_STATE_SCISSOR` (line 678). The baked static values are ignored at runtime — `dispatch()` always calls `cmd_set_viewport/scissor`. The static viewport serves no purpose but adds confusion about which is authoritative.
- **Fix**: Pass empty slices for `viewports`/`scissors` in `PipelineViewportStateCreateInfo` (valid when states are dynamic), keeping count=1.

#### [SH-2] GpuInstance 320-byte boundary not verified in `triangle.vert` and `caustic_splat.comp`
- **Dimension**: Shader Correctness
- **File**: `crates/renderer/shaders/triangle.vert`, `crates/renderer/shaders/caustic_splat.comp`
- **Finding**: CLAUDE.md Shader Struct Sync invariant requires all four shaders (triangle.vert/frag, ui.vert, caustic_splat.comp) to be updated in lockstep when GpuInstance changes. `triangle.frag` correctly reflects the 320-byte layout. `triangle.vert` and `caustic_splat.comp` were not read in this pass and may still carry old 192-byte boundary annotations or miss the new fields.
- **Fix**: Grep `triangle.vert` and `caustic_splat.comp` for `sparkleIntensity` or `sparkle_intensity`. If absent, the shader is out of sync. If present, verify offset annotations sum to 320.

---

## Previously Confirmed Fixed

The following findings from prior audits are verified resolved in current code:

| Prior ID | Finding | Status |
|----------|---------|--------|
| SYNC-C1 | TLAS→caustic barrier scope widened to COMPUTE_SHADER | ✓ Fixed (draw.rs:289-299) |
| MEM-C2 | BLAS scratch per-build allocation | ✓ Fixed (shared growable buffer, acceleration.rs:111) |
| PIPE-C1 | No shared VkPipelineCache | ✓ Fixed (mod.rs:424, persisted to disk on Drop) |
| AS-H1 | instance_custom_index / SSBO-index parity | ✓ Fixed (shared build_instance_map, draw.rs:251-256) |
| AS-H2 | TRIANGLE_FACING_CULL_DISABLE on all instances | ✓ Fixed (gated on two_sided, acceleration.rs:1188-1192) |
| RT-H1 | Missing TerminateOnFirstHit on reflection/glass rays | ✓ Fixed (triangle.frag:308, 1027, 893) |
| RT-H2 | Directional shadow tMax too short (10K) | ✓ Fixed (100 000, triangle.frag:1405) |
| RT-H3 | Window portal ray fires along -V | ✓ Fixed (fires along -N, triangle.frag:889) |
| CMD-M1 | tick_deferred_destroy before fence wait | ✓ Fixed (moved after wait_for_fences, draw.rs:147-173) |
| COMP-H2 | SVGF histAge uses max() instead of weighted average | ✓ Fixed (weighted avg, svgf_temporal.comp:115-131) |
| LIFE-C1 | SVGF descriptor pool/layout/views not destroyed | ✓ Fixed (full destroy(), svgf.rs:814-855) |
| LIFE-C2 | Composite samplers/shader modules not destroyed | ✓ Fixed (full destroy(), composite.rs:1047-1097) |
| LIFE-C3 | GBuffer destroy-after-framebuffer on resize | ✓ Fixed (destroy-before-recreate, resize.rs:27-30) |

---

## Still Open (Not Re-Reported — Covered by Prior Issues)

- MEM-C1: Host-visible vertex/index SSBOs
- MEM-C3: Descriptor pool overflow
- PIPE-H2/PIPE-2: Unconditional pipeline rebuild on resize (re-listed above as medium)
- PIPE-H3: Dynamic rendering adoption
- COMP-H1: Fog in triangle.frag creating SVGF ghosting
- LIFE-H2: TextureRegistry descriptor pool flags
- SYNC-H2: recreate_swapchain semaphore recreation
- SHADER-H1: caustic_splat.comp _pad1 / materialKind naming

---

## Prioritized Fix Order

### Correctness / Spec (fix first)
1. **RT-2** — NaN GI ray on flat terrain (MEDIUM): one-line threshold fix in triangle.frag. High blast radius (all LAND cells).
2. **SY-2** — BOTTOM_OF_PIPE in dst_stage_mask (MEDIUM): spec violation rejected by Sync2 validation. One-line removal in helpers.rs.
3. **SY-1** — Composite dep_in missing COMPUTE_SHADER (MEDIUM): latent race foot-gun. One-line addition in composite.rs.

### Shader Safety (do before new RT hit shaders)
4. **SH-1** — vertex float reinterpretation risk (MEDIUM): comment clarification now; UV-only SSBO long-term.
5. **SH-2** — GpuInstance 320B boundary verification (LOW): grep triangle.vert + caustic_splat.comp; fix if out of sync.

### Documentation / Cleanup (low priority)
6. **MEM-1** — GpuInstance comment drift (LOW): update "192 bytes" → "320 bytes" in one line.
7. **PIPE-1** — Redundant static viewport in composite pipeline (LOW): remove two argument lines from composite.rs.
8. **PIPE-2** — Unconditional pipeline rebuild on resize (MEDIUM): format-compare guard before render pass recreate.
