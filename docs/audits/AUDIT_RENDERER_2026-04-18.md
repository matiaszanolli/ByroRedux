# Renderer Audit — 2026-04-18

**Scope**: Full Vulkan renderer — rasterization, RT (BLAS/TLAS, ray queries, shadows, reflections, GI), deferred indirect (G-buffer, SVGF), compositing, sync, GPU memory, lifecycle.
**Method**: 10 dimension agents (max 3 concurrent). Findings below are deduplicated across dimensions.
**Baseline**: Audit follows the 2026-04-12c audit + closures of #309, #313, #317, #303. Prior audit findings already landed are not re-flagged.

---

## Executive Summary

Total findings across 10 dimensions: **~50 items** (~8 Critical, ~15 High, ~18 Medium, ~10 Low). Deduplicated to **~35 unique issues** below.

| Pipeline area | Severity breakdown |
|---|---|
| GPU Memory | 3 Critical (host-visible vertex/index, unpooled BLAS scratch, fixed descriptor pools) + 5 High + Medium/Low |
| Pipeline State | 3 Critical (no pipeline cache, binding drift risk, push-constant stages) + 4 High |
| Vulkan Sync | 1 Critical (TLAS→caustic barrier missing) + 2 High |
| Acceleration Structures | 0 Critical + 2 High (SSBO/TLAS custom_index parity, unconditional cull-disable) + Medium |
| RT Ray Queries | 0 Critical + 3 High (TerminateOnFirstHit, #102 shadow tmax, window portal direction) |
| Shader Correctness | 0 Critical + 1 High (GpuInstance name drift in caustic_splat.comp) |
| Render Pass / G-Buffer | 0 Critical + 1 High (format divergence from checklist expectations — deliberate but worth documenting) |
| Command Recording | 0 Critical + 0 High, Medium only |
| Resource Lifecycle | 3 Critical (SVGF pool/layout/view leak, composite sampler+modules, GBuffer resize ordering) + partly hypothetical |
| Denoiser & Composite | 0 Critical + 2 High (fog dead weight / SVGF ghosting risk, bilinear histAge uses `max` instead of weighted avg) |

**Top three takeaways**:
1. **Memory shape is the biggest correctness+scalability gap.** GPU Memory C1–C3 together (host-visible vertex buffers, unpooled BLAS scratch, fixed descriptor pools) are what separates "runs on the 12 GB dev box only" from "runs on the 6 GB minimum spec". These predate current milestones and have compounded.
2. **One real correctness time-bomb**: AS dim H1 — TLAS `instance_custom_index` and the SSBO builder use two different filter predicates. They happen to match by construction today, but any future draw-command rejection (mesh eviction, handle invalidation) shifts SSBO indices while TLAS indices stay — producing silently wrong instance data for every RT hit downstream.
3. **One open CVE-adjacent safety issue + one sync latent**: Dim 1 C1 missing TLAS→caustic compute barrier — strict validation layers should flag it; real hardware has masked it so far.

---

## RT Pipeline Assessment

**BLAS / TLAS correctness** (Dim 8):
- BLAS inputs: vertex format, index type, opacity/trace-prefer flags, scratch & result usage flags — all correct.
- TLAS build/update decision: `decide_use_update` is sound, with correct instance-count guard via zip-compare.
- Empty TLAS at init: #317 fix holds.
- TLAS instance barrier: #303 fix verified — uses `copy_size`, not `VK_WHOLE_SIZE`.
- **H1 (fragile)**: `instance_custom_index` vs SSBO-index parity depends on identical filters in two files (`acceleration.rs:958-991` vs `draw.rs:425-430`). No enforcing invariant.
- **H2**: `TRIANGLE_FACING_CULL_DISABLE` set unconditionally instead of gated on `DrawCommand.two_sided`. Ray queries see backfaces on single-sided closed meshes — doubles ray cost + self-shadow artifacts.
- **M1–M4**: scratch alignment against `minAccelerationStructureScratchOffsetAlignment`, UPDATE-gate documentation, single-path BLAS missing `ALLOW_COMPACTION`, 3000-mesh batched build blocks the graphics queue.

**Ray query safety** (Dim 9):
- All 5 sites correctly gated by `sceneFlags.x > 0.5` (matches Dim 6 M1 verification).
- WRS shadow estimator is mathematically unbiased; 64× firefly clamp reasonable.
- Frame-counter noise seeding consistent across sites.
- **H1**: `traceReflection` helper + glass through-ray omit `gl_RayFlagsTerminateOnFirstHitEXT` — leaves measurable ray cost on the table.
- **H2**: Issue #102 (directional shadow tmax = 10000) still open — exterior cliffs beyond ~10k units never occlude the sun.
- **H3**: Window portal ray fires along `-V` (camera→fragment) instead of surface normal — off-axis window fragments fail the escape test.

**Denoiser stability** (Dim 10):
- Motion-vector convention, ping-pong, mesh-ID rejection, α-clamp, dispatch workgroup math, descriptor updates — all verified.
- **H1**: CompositeParams `fog_color`/`fog_params` are dead weight; fog actually applied in `triangle.frag` to both direct AND indirect, meaning SVGF history carries prior-frame fog → ghosting on cell-load / weather-change transitions.
- **H2**: `svgf_temporal.comp:109` uses `histAge = max(...)` across bilinear taps instead of weighted average (per SVGF paper). Produces ghosting on fine-scale moving occluders at disocclusion boundaries.

---

## Rasterization Assessment

**Pipeline state** (Dim 3): solid fundamentals, scaling concerns.
- **C1**: no shared `VkPipelineCache` — 50-300 ms avoidable cold-start cost.
- **C2**: no SPIR-V reflection / single source of truth for descriptor layout vs shader bindings → binding drift is a recurring regression class.
- **C3**: push-constant range `stageFlags` must be a superset of every accessing shader — an unenforced invariant.
- **H1-H4**: no `DEPTH_BIAS` dynamic state, pipeline rebuilt on resize despite dynamic viewport+scissor, no `VK_KHR_dynamic_rendering` adoption, no pipeline derivatives for G-buffer family.
- **M5**: G-buffer blend state may not be per-attachment — blending config on UINT mesh-id attachment is illegal.

**Render pass / G-Buffer** (Dim 4): no Critical findings.
- **H1**: G-buffer formats diverge from audit-spec expectations — R16G16_SNORM normals (octahedral), R16_UINT mesh_id (65534-visible-instance ceiling), B10G11R11_UFLOAT for indirect/albedo. These are deliberate bandwidth optimizations per #275/#318, but the 65535-instance ceiling is not enforced at the instance uploader.
- **H2**: Outgoing subpass dependency uses `BOTTOM_OF_PIPE` in `dst_stage_mask` — semantically meaningless, rejected by Synchronization2.
- **M2**: `GBuffer::initialize_layouts` error only logs, first frame could sample UNDEFINED.
- **M3**: `GBuffer::recreate_on_resize` partial-failure leaves `Some(empty_gbuffer)` that will panic-index on next use.

**Command recording** (Dim 5): clean — 0 Critical, 0 High.
- **M1**: `accel.tick_deferred_destroy` at `draw.rs:76-84` runs **before** `wait_for_fences` at line 96 — destroys resources that may still be in use on the other frame slot.
- **M2**: monolithic `unsafe{}` block encloses render pass + post-pass compute — readability hazard.

---

## Findings (Deduplicated, Grouped by Severity)

### Critical

**MEM-C1** [Dim 2 C1] — `GpuBuffer` uses `HOST_VISIBLE|HOST_COHERENT` for all vertex/index data
`crates/renderer/src/vulkan/buffer.rs`. Every `GpuBuffer` allocation lands in pinned-host-visible heap (~256 MB BAR on NVIDIA), not DEVICE_LOCAL. Starfield-class interiors exhaust this heap long before the 4 GB budget. Permanent PCIe vertex-fetch bandwidth tax. **Fix**: stage to `GpuOnly` buffers with `TRANSFER_DST|VERTEX_BUFFER|STORAGE_BUFFER`.

**MEM-C2** [Dim 2 C2] — BLAS scratch buffers allocated per-build, not pooled
`crates/renderer/src/vulkan/acceleration.rs`. Cell with 500 unique meshes = 500 transient GPU-only allocations in one frame. Heap fragmentation + allocator lock contention. **Fix**: single growable scratch buffer per frame-in-flight.

**MEM-C3** [Dim 2 C3] — Descriptor pools have no overflow/growth policy
`crates/renderer/src/vulkan/descriptors.rs`. Hard-crash on `VK_ERROR_OUT_OF_POOL_MEMORY` once mesh/texture count exceeds hardcoded bound. **Fix**: `UPDATE_AFTER_BIND` bindless arrays OR `Vec<DescriptorPool>` with grow-on-OOM.

**PIPE-C1** [Dim 3 C1] — No `VkPipelineCache` reuse across pipeline creations
`crates/renderer/src/vulkan/pipeline.rs`, svgf.rs, composite.rs, ssao.rs, compute.rs all pass `PipelineCache::null()`. 50-300 ms of avoidable cold-start work. **Fix**: shared cache + on-disk persistence keyed by `(vendor_id, device_id, driver_version, app_version)`.

**PIPE-C2** [Dim 3 C2] — Descriptor-layout/shader-binding drift risk — no reflection
`crates/renderer/src/vulkan/descriptors.rs`. Hand-written bindings not cross-checked against `layout(set=N, binding=M)`. Vulkan does not strictly validate shader bindings against pipeline layouts; mismatches silently produce wrong reads. **Fix**: `spirv-reflect` cross-check at pipeline build.

**PIPE-C3** [Dim 3 C3] — Push-constant range `stageFlags` not verified against accessing stages
`crates/renderer/src/vulkan/pipeline.rs`. Spec requires stageFlags superset of every shader accessing the block. **Fix**: assertion in `build_push_constant_ranges`.

**SYNC-C1** [Dim 1 C1] — TLAS-build → caustic compute missing `AS_WRITE→SHADER_READ` barrier
`crates/renderer/src/vulkan/context/draw.rs:214-226`. TLAS barrier dst_stage is only `FRAGMENT_SHADER`; caustic compute (`caustic.rs:682+`, `caustic_splat.comp`) issues `rayQueryEXT` on the same TLAS in COMPUTE_SHADER stage without a covering barrier. **Fix**: widen to `FRAGMENT_SHADER | COMPUTE_SHADER`.

**LIFE-C1** [Dim 7 C1] — SVGF descriptor pool/layout not destroyed; image views leaked per resize
`crates/renderer/src/vulkan/svgf.rs`. `SvgfPipeline::destroy()` frees pipeline/layout/sampler/history images but omits `descriptor_pool`, `descriptor_set_layout`, and history `ImageView`s. Leaks grow with resize count. **Primary blocker for closing issue #33.**

**LIFE-C2** [Dim 7 C2] — Composite pipeline sampler + shader module destroy path needs verification
`crates/renderer/src/vulkan/composite.rs`. Verify the fullscreen sampler is destroyed and shader modules are either destroyed immediately after `vkCreateGraphicsPipelines` or appear in destroy() if cached.

**LIFE-C3** [Dim 7 C3] — GBuffer attachments must be destroyed before framebuffers on resize
`crates/renderer/src/vulkan/gbuffer.rs`, `context/resize.rs`. If GBuffer rebuild happens after framebuffer recreation, VUID-vkDestroyImage-image-01000 fires.

### High

**MEM-H1** [Dim 2 H1] — Allocator single-mutex contention during cell load/streaming
`crates/renderer/src/vulkan/allocator.rs`. Every buffer::new, BLAS build, texture upload, per-frame scratch takes the same mutex. Violates "CPU bottleneck = bug" on a 16c/32t CPU. **Fix**: frame-scoped arena allocator for transients.

**MEM-H2** [Dim 2 H2] — Staging buffers allocated per-upload, dropped immediately
`crates/renderer/src/vulkan/texture.rs`. 2000 DDS textures = 2000 alloc+free roundtrips. **Fix**: 64 MB ring-buffer staging allocator.

**MEM-H3** [Dim 2 H3] — No VRAM residency tracking vs budget
`crates/renderer/src/vulkan/context/resources.rs::log_memory_usage` reports but doesn't enforce. 6 GB minimum spec will OOM with no graceful degradation. **Fix**: poll `VK_EXT_memory_budget` + admit/evict policy.

**MEM-H4** [Dim 2 H4] — Global vertex/index SSBOs: growth strategy unknown
`crates/renderer/src/mesh.rs`. Either OOM cliff or leak-on-resize. **Unknown — needs investigation.**

**MEM-H5** [Dim 2 H5] — GBuffer+SVGF+TAA attachments full-res only
At 4K ~650 MB of framebuffer memory — half the 4 GB budget before geometry. **Fix**: render-scale factor or half-res indirect/motion/mesh-id attachments (standard SVGF).

**PIPE-H1** [Dim 3 H1] — No `DEPTH_BIAS` dynamic state
`crates/renderer/src/vulkan/pipeline.rs`. Shadow bias tuning requires pipeline variants per tuple.

**PIPE-H2** [Dim 3 H2] — Pipeline recreated on resize despite dynamic viewport+scissor
`crates/renderer/src/vulkan/context/resize.rs`. Avoidable recompilation hitch. **Fix**: format-compare guard.

**PIPE-H3** [Dim 3 H3] — `VK_KHR_dynamic_rendering` not adopted
Render pass + framebuffer object count will explode with SVGF/SSAO/G-buffer variants.

**AS-H1** [Dim 8 H1] — `instance_custom_index` / SSBO-index parity is fragile
`acceleration.rs:958-991` and `draw.rs:425-430` use independent filter predicates. One eviction or handle mismatch shifts SSBO indices while TLAS custom indices stay put → silently wrong RT hits. **Fix**: shared `draw_idx → gpu_instance_idx` table, OR `debug_assert!(mesh_registry.get(h).is_some())` at both sites.

**AS-H2** [Dim 8 H2] — `TRIANGLE_FACING_CULL_DISABLE` applied unconditionally
`acceleration.rs:984-987`. RT sees backfaces on single-sided closed meshes. **Fix**: gate on `DrawCommand.two_sided`.

**RT-H1** [Dim 9 H1] — Reflection helper + glass through-ray omit `TerminateOnFirstHitEXT`
`triangle.frag:213-218, 531-534`. Measurable ray cost on 5000-unit reflection + 2000-unit glass rays.

**RT-H2** [Dim 9 H2] — Issue #102 still open: directional shadow tmax = 10000
`triangle.frag:832`. Exterior cliffs beyond 10k units never occlude the sun. **Fix**: raise to 100000 or plumb a UBO field.

**RT-H3** [Dim 9 H3] — Window portal ray uses `-V` instead of `-N`
`triangle.frag:456-466`. Off-axis fragments fail portal escape → windows render opaque.

**SHADER-H1** [Dim 6 H1] — `GpuInstance` name drift in `caustic_splat.comp`
`caustic_splat.comp:74` declares `uint _pad1;` at offset 156 while triangle.vert/frag + ui.vert all say `uint materialKind;`. Byte layout matches (std430 scalar uint) but name contract per `feedback_shader_struct_sync.md` is broken. The sync list was not extended to include caustic_splat.comp when issue #344 landed.

**SYNC-H1** [Dim 1 H1] — Composite render pass `dep_in` omits COMPUTE_SHADER in src_stage
`composite.rs:310-316`. Currently masked by per-pass explicit barriers — latent foot-gun on layout-optimization changes.

**SYNC-H2** [Dim 1 H2] — `recreate_swapchain` doesn't recreate `image_available` / `in_flight`
`sync.rs:95-123`. Currently safe (post `device_wait_idle`), fragile if resize flow changes.

**RP-H1** [Dim 4 H1] — G-buffer formats diverge from audit-spec expectations
Deliberate bandwidth optimizations (octahedral normals, R11G11B10F indirect/albedo, R16_UINT mesh_id). **But**: the 65535-visible-instance ceiling is not enforced at the instance uploader.

**RP-H2** [Dim 4 H2] — Outgoing subpass dependency uses `BOTTOM_OF_PIPE` in `dst_stage_mask`
`helpers.rs:153-158`. Semantically meaningless; will be rejected by Synchronization2.

**LIFE-H1** [Dim 7 H1] — BLAS LRU eviction path needs device-idle-wait or deferred destroy
**Unknown — needs verification.**

**LIFE-H2** [Dim 7 H2] — TextureRegistry per-texture descriptor sets: pool flag vs cleanup strategy
**Unknown — needs verification** of pool create flag vs `vkFreeDescriptorSets` usage.

**LIFE-H3** [Dim 7 H3] — SSAO noise texture sampler cleanup
**Unknown — needs verification.**

**LIFE-H4** [Dim 7 H4] — VulkanContext Drop reverse-order audit
**Unknown — needs verification** of full Drop sequence against CLAUDE.md invariant #4.

**COMP-H1** [Dim 10 H1] — CompositeParams fog_color/fog_params are dead; fog actually in triangle.frag
`composite.frag:24-25, composite.rs:44-46`. Wasted UBO bandwidth + SVGF history carries prior-frame fog → ghosting on transitions. **Fix**: remove dead fields + document fog location, OR move fog into composite pass.

**COMP-H2** [Dim 10 H2] — SVGF bilinear `histAge` uses `max` instead of weighted average
`svgf_temporal.comp:109`. Overcommits to old history at disocclusion → ghosting on fine-scale occluders. **Fix**: weighted average per SVGF paper §4.2.

### Medium (condensed)

- **CMD-M1**: `accel.tick_deferred_destroy` runs before `wait_for_fences` (draw.rs:76-84) → use-after-free risk on BLAS eviction.
- **CMD-M2**: monolithic `unsafe` block spans render pass + post-pass compute (readability hazard, draw.rs:634-1030).
- **SYNC-M1**: screenshot staging lacks `TRANSFER_WRITE→HOST_READ` barrier (screenshot.rs:139-169).
- **MEM-M3**: gpu-allocator verbose debug logging dominates cell-load traces.
- **MEM-M4**: TLAS instance buffer allocation size = MAX_INSTANCES even when copy is bounded.
- **MEM-M5**: DDS mip count trusted from header; BSA-reconstructed headers could over-allocate.
- **PIPE-M1**: No specialization constants for material/feature toggles (ubershader branching overhead).
- **PIPE-M2**: Rasterization state (cull/front-face) uncovered by tests — CW/CCW regression risk.
- **PIPE-M5**: Color blend attachment state reused across all color attachments (may be illegal on UINT mesh-id).
- **RP-M2**: `GBuffer::initialize_layouts` only logs on failure; first-frame SVGF samples UNDEFINED.
- **RP-M3**: `GBuffer::recreate_on_resize` partial failure leaves `Some(empty_gbuffer)` panic-indexable.
- **AS-M1**: Scratch buffer alignment not enforced against `minAccelerationStructureScratchOffsetAlignment`.
- **AS-M3**: Single-path `build_blas` missing `ALLOW_COMPACTION` — streaming path wastes ~50% BLAS VRAM.
- **AS-M4**: Batched BLAS build as one command buffer blocks graphics queue on 3000-mesh cells.
- **RT-M1**: Reflection ray origin bias `tMin=0.01` too small at exterior coordinates.
- **RT-M3**: Barycentric interpolation correct but uncommented — easy to "fix" the wrong way.
- **SHADER-M1-M3**: RT gate duplication, motion convention undocumented, cluster depth constants duplicated.
- **LIFE-M1/M2**: Swapchain image views before swapchain, framebuffers before render pass — standard ordering that must hold on resize.
- **LIFE-M5**: `render_finished` semaphore array resize if swapchain image count changes — silent sync hazard.
- **COMP-M1**: First-frame flag over-defensive by one frame (extra noise on resize).
- **COMP-M4**: Composite `dep_in` doesn't list DEPTH_STENCIL_ATTACHMENT_WRITE (covered indirectly; defensive clarity).

### Low (condensed)

- Extended documentation drift items (L1-L6 across dimensions).
- Performance nits: per-attachment blend state, primitive restart, pipeline libraries, memory priority hints, `HOST_COHERENT` flush granularity.
- Hardcoded scale constants: `exposure = 0.85`, reflection fade `exp(-d * 0.0015)`, light-disk radius `radius * 0.025 + 1.5`.

---

## Regression Verification (confirmed previously-closed fixes still hold)

- **#303** (TLAS instance buffer VK_WHOLE_SIZE) — **fixed**. `acceleration.rs:1258, 1289` use `copy_size`, gated on `copy_size > 0`.
- **#309** (multi-draw indirect per-batch collapse) — **fixed**. `draw.rs:742-811` groups by `(pipeline_key, is_decal)`.
- **#313** (lock-order graph) — confirmed landed; not re-tested this pass.
- **#316** (BLAS compaction phase 6 leak) — **closed**. Rollback path at `acceleration.rs:755-836` correct.
- **#317** (empty-TLAS size=0 guard) — **fixed**. Guard at `acceleration.rs:1251`.
- **#51** (unconditional `cmd_set_depth_bias`) — **closed**. State-change-gated at draw.rs:780-789.

---

## Prioritized Fix Order

Ranked by (safety × blast-radius) / (fix-cost):

### Tier 0 — Quick correctness wins (hours)
1. **SYNC-C1** widen TLAS→caustic barrier stage mask at `draw.rs:221` (one line).
2. **AS-H2** gate `TRIANGLE_FACING_CULL_DISABLE` on `DrawCommand.two_sided`.
3. **RT-H1** add `TerminateOnFirstHitEXT` to reflection + glass rays.
4. **RT-H2** fix #102 — raise directional shadow tmax to 100000.
5. **RT-H3** fire window portal ray along `-N`, not `-V`.
6. **SHADER-H1** rename `_pad1` → `materialKind` in caustic_splat.comp, extend shader sync doc.
7. **CMD-M1** move `accel.tick_deferred_destroy` to after `wait_for_fences`.
8. **RP-H2** drop `BOTTOM_OF_PIPE` from subpass dependency dst_stage_mask.
9. **COMP-H2** change SVGF `histAge = max(...)` → weighted average.

### Tier 1 — Lifecycle / verification (days)
10. **LIFE-C1** add descriptor pool + layout + image view destruction to `SvgfPipeline::destroy()`.
11. **LIFE-C2/C3** verify composite sampler+module cleanup + GBuffer destroy-before-framebuffer ordering.
12. **LIFE-H1-H4** line-trace remaining Drop/eviction paths (Dim 7 agent ran out of budget).
13. **AS-H1** promote `draw_idx → gpu_instance_idx` parity to a shared table or a debug-assert.

### Tier 2 — Memory shape (week)
14. **MEM-C1** stage vertex/index data to `GpuOnly` buffers.
15. **MEM-C2** pooled/growable BLAS scratch buffer per frame-in-flight.
16. **MEM-C3** `UPDATE_AFTER_BIND` bindless descriptor arrays OR `Vec<DescriptorPool>` grow-on-OOM.
17. **MEM-H2** ring-buffer staging allocator.
18. **MEM-H5** render-scale factor + half-res SVGF intermediaries.
19. **MEM-H3** VRAM budget resource + `VK_EXT_memory_budget` polling.

### Tier 3 — Pipeline hygiene (week)
20. **PIPE-C1** shared `VkPipelineCache` + on-disk persistence.
21. **PIPE-C2** `spirv-reflect` cross-check at pipeline build.
22. **PIPE-C3** assert push-constant `stageFlags` superset.
23. **PIPE-H1** add `DEPTH_BIAS` to dynamic state.
24. **PIPE-H2** format-compare guard in `recreate_swapchain` to skip pipeline rebuild.
25. **PIPE-M5** per-attachment blend state array for G-buffer (fix UINT attachment blend).

### Tier 4 — Larger architectural (weeks)
26. **PIPE-H3** adopt `VK_KHR_dynamic_rendering`.
27. **COMP-H1** move fog out of `triangle.frag` into composite (eliminates SVGF fog-transition ghosting).
28. **AS-M4** chunk batched BLAS build (~256 per submission) so graphics queue isn't held.
29. **RT-H1 follow-up** drop `OPAQUE` flag + add any-hit for alpha-tested shadows.

---

## Suggested next action

`/audit-publish docs/audits/AUDIT_RENDERER_2026-04-18.md`

Top candidates for immediate issue filing (unique, not overlapping with open issues):
- **SYNC-C1** — TLAS→caustic barrier (critical, one-line fix)
- **AS-H1** — SSBO/TLAS index parity hazard (time-bomb)
- **AS-H2** — Unconditional `TRIANGLE_FACING_CULL_DISABLE`
- **RT-H1** — Missing `TerminateOnFirstHitEXT` on reflection + glass
- **RT-H3** — Window portal ray direction wrong
- **SHADER-H1** — caustic_splat `_pad1` → `materialKind` rename
- **LIFE-C1** — SVGF descriptor pool/layout/view leak (closes #33)
- **COMP-H2** — SVGF histAge weighted average
- **MEM-C1/C2/C3** — memory-shape trilogy (epic-scoped)
- **CMD-M1** — `tick_deferred_destroy` ordering
