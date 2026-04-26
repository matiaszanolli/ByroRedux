# AUDIT_RENDERER — 2026-04-25

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: 20b8ef0 (`Fix #545: parse + emit NiFlipController texture-flipbook channels`)
**Reference report**: `docs/audits/AUDIT_RENDERER_2026-04-22.md`
**Dimensions**: 10 (Sync · GPU Memory · Pipeline State · Render Pass · Command Recording · Shader Correctness · Resource Lifecycle · Acceleration Structures · RT Ray Queries · Denoiser & Composite)
**Open issues baseline**: `gh issue list … --state=open` → 53 issues

---

## Executive Summary

**0 CRITICAL · 4 HIGH · 19 MEDIUM · 26 LOW · 14 INFO** — across 63 findings.

The pipeline is broadly correct. The new HIGH-severity findings cluster around **two surfaces that landed since 04-22**: M29 GPU pre-skinning and the per-frame BLAS refit chain it enables. Both regressions are spec-level write/race issues that today happen to work (host-side fence wait masks them) but become real hazards under any future MAX_FRAMES_IN_FLIGHT bump or timeline-semaphore refactor. The third HIGH (LIFE-H1) is a leak of every `pending_destroy_blas` entry on shutdown that's been latent since the deferred-destroy queue landed; LIFE-H2 (open as #33) is a refinement of the long-standing "swapchain recreate doesn't re-bind every dependent descriptor" gap.

Two prior-audit fixes are confirmed holding: composite COMPUTE_SHADER dep (#572 → 31e8ecd), tick_deferred_destroy after fence wait (#418 → d4095ce), TerminateOnFirstHit on shadow/reflection/glass/portal rays, BLAS scratch pooled, instance_custom_index parity, TRIANGLE_FACING_CULL_DISABLE two-sided gating, hostQueryReset for compaction (c46dc78), histAge weighted average in SVGF.

### What's new since 04-22

- **M29 GPU pre-skinning** (de1ea1f, 1ae235b) introduces `skin_compute.rs`, `skin_vertices.comp`, per-entity skinned BLAS refit. Three findings against this surface: SH-3 (motion vectors wrong on every skinned actor pixel — fixable now that GPU skinning exists), SH-6 (no bounds check on `bone_offset + boneIdx.x` — corrupted index byte → BLAS corruption), AS-8-1 (per-frame refit loop has no inter-build scratch barrier — write-write hazard with ≥2 skinned NPCs).
- **Caustic compute** (caustic_splat.comp): SH-2/RT-5 — no ray flags, no rtEnabled gate. Pays full closest-hit cost across 1000-unit reach for every (light × pixel) pair regardless of RT toggle.
- **MEM-2-x** finds three grow-only buffers without a shrink path (skin_slots map, TLAS instance buffer, TLAS scratch); BLAS scratch already has `shrink_blas_scratch_to_fit` from #495.

### What's still open from prior audits

- **#33** — destruction order between recreate_swapchain and Drop (refined by LIFE-H2: scene_buffers G-buffer descriptor sets are not rewritten on resize → stale view UB on next bind).
- **#573 (SY-2)** — `BOTTOM_OF_PIPE` in main render pass outgoing dst_stage_mask. Plus two sibling sites: SY-3 in composite render pass, CMD-3 in screenshot copy.
- **#574 (RT-2)** — buildOrthoBasis NaN at exact (0,1,0). Cross-linked from RT-7 as the visible failure surface.
- **#575 (SH-1)** — vertex SSBO read as `float[]` while bone_indices/splat are non-floats. Cross-linked from SH-8 (skin_vertices.comp inherits the same contract; M29 BLAS refit extends the blast radius).
- **#578**, **#576**, **#577** — all closed and verified clean.

---

## RT Pipeline Assessment

**BLAS/TLAS correctness**: Sound for static, latent issues for skinned (M29).
- Static: vertex format R32G32B32_SFLOAT@0, UINT32 indices, OPAQUE flag, PREFER_FAST_TRACE + ALLOW_COMPACTION (batched only — single-shot path lacks ALLOW_COMPACTION, AS-8-3). Transform conversion correct. UPDATE/REBUILD decision (decide_use_update) sound except for the empty-instance-list edge case (AS-8-2). instance_custom_index parity verified via shared `build_instance_map` (AS-H1 fix holds).
- Skinned (M29 fresh code): BLAS uses PREFER_FAST_BUILD + ALLOW_UPDATE (correct flag stratification, AS-8-10), but **per-frame refit chain has no inter-build scratch barrier** (AS-8-1, HIGH). One scratch buffer feeds three callers (one-time BUILD via fence, per-frame REFIT via main cmd buf) with only host-side fence ordering between them (MEM-2-2). Refit chain accumulates BVH inefficiency over time with no rebuild-from-scratch policy (AS-8-9). And `skin_slots` + `skinned_blas` leak across cell transitions (MEM-2-1) since the LRU eviction path was never wired.

**Ray query safety**: triangle.frag is clean; caustic_splat.comp is the outlier.
- triangle.frag: all 6 ray sites gate on `rtEnabled`, all carry TerminateOnFirstHitEXT. Reflection bias lacks the `dot(N,V)` flip the glass path uses (RT-3). GI tMin (0.5u) is anomalously large vs bias (0.1u) (RT-4).
- caustic_splat.comp: **no ray flags, no rtEnabled gate** (SH-2 HIGH, RT-5 MEDIUM). Pays full closest-hit traversal cost.
- buildOrthoBasis NaN at exact (0,1,0) (#574) propagates into GI as flat-shaded sky-only fill on perfectly horizontal terrain (RT-7).

**Denoiser stability**: Mostly correct.
- SVGF ping-pong correct, motion-vector convention matches producer, mesh ID rejection wired. Mesh-ID-only consistency check rejects only cross-mesh disocclusions; same-mesh wall pans ghost (SH-5 MEDIUM — paper SVGF needs depth+normal too).
- **SVGF/TAA history-slot read against producer write is unsynchronized in spec terms** (DEN-3 MEDIUM, latent) — producer's post-dispatch barrier targets only FRAGMENT_SHADER, but next frame's consumer is COMPUTE_SHADER. Fence-based serialization masks this today.
- **Vertex shader writes `fragPrevClipPos` using current-frame bone palette** (SH-3 HIGH) — wrong motion vectors on every skinned actor pixel; downstream SVGF/TAA reproject the wrong source pixel and ghost limbs in motion. Now fixable via M29 (push `bones_prev[]` SSBO).

---

## Rasterization Assessment

**Pipeline state**: No correctness defects post-#576/#578. Cross-checks pass for vertex input, push constants, descriptor set/binding, two-sided pipeline shape, blend cache key. Skin compute (M29) is correctly wired (workgroup size, push constants, descriptor layout — validated against SPIR-V reflection at startup). PIPE-3 is a sibling of #573 at the composite render pass site.

**Render pass & G-buffer**: Attachment ops correct (CLEAR+STORE on every G-buffer target, depth STORED for SSAO), formats match shader output types, all images carry SAMPLED. RP-1 flags an off-by-one in the `mesh_id` ceiling comment (actual = 65535, not 65534) plus a missing runtime overflow guard. RP-2 flags first-frame black-bloom risk on resize because freshly-allocated G-buffer images are sampled by SVGF temporal before any color write lands; cleanest fix is an epoch counter that forces SVGF reset on resize.

**Command recording**: Begin/End balanced, render pass scope intact, TLAS build pre-render-pass, all post-render-pass compute (SVGF/TAA/caustic/SSAO/composite) correctly outside. UI overlay path inherits dynamic depth/cull state from last main batch (CMD-4 LOW) — works only because the last batch happens to align. The screenshot copy uses BOTTOM_OF_PIPE in dst_stage (CMD-3 — sibling of #573).

**Lifecycle**: SVGF/Composite/GBuffer destroy() chains (LIFE-C1/C2/C3) confirmed clean. M29 skin slot teardown ordering correct. The leak surface is the AccelerationManager's `pending_destroy_blas` queue not being drained in destroy() (LIFE-H1 HIGH).

---

## Findings

### HIGH

#### [LIFE-H1] AccelerationManager::destroy leaks every entry queued in `pending_destroy_blas`
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/acceleration.rs:2198-2223](crates/renderer/src/vulkan/acceleration.rs#L2198-L2223)
- **Existing issue**: none
- **Finding**: `drop_blas()` queues entries on `pending_destroy_blas` with a 2-frame countdown; `tick_pending_destroy` only fires from `draw_frame`. On Drop, `destroy()` only drains `blas_entries` and `tlas` — every entry whose countdown was still >0 leaks one VkAccelerationStructureKHR + one GpuBuffer. Easy repro: load a cell, fast-travel, quit the next frame.
- **Fix**: At the top of `destroy()`, drain `self.pending_destroy_blas` and call the AS loader + buffer destroy on each entry. `device_wait_idle` in the parent Drop already covers in-flight cmd buf references.

#### [LIFE-H2] `recreate_swapchain` leaves `scene_buffers` G-buffer descriptor bindings pointing at destroyed image views
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/context/resize.rs](crates/renderer/src/vulkan/context/resize.rs)
- **Existing issue**: #33 — refines the inventory
- **Finding**: After `gbuffer.recreate_on_resize` destroys raw_indirect / motion / mesh_id / albedo / normal views, only the SVGF/composite/caustic/taa/ssao subsystems rewrite their descriptor sets. The per-frame `scene_buffers` UBO/SSBO sets (used by triangle.frag samplers) are silent. Validation layer reports "VkImageView … has been destroyed" on the next bind on any window drag that triggers resize.
- **Fix**: Either audit every `scene_buffers.write_*` call site so each binding referencing a recreated image is re-issued during resize, or add a `scene_buffers.recreate_descriptor_sets` call symmetric to texture_registry's existing call.

#### [SH-2] `caustic_splat.comp` ray query has no flags AND no `rtEnabled` gate
- **Dimension**: Shader Correctness
- **File**: [crates/renderer/shaders/caustic_splat.comp:229](crates/renderer/shaders/caustic_splat.comp#L229)
- **Existing issue**: none
- **Finding**: `rayQueryInitializeEXT(rq, topLevelAS, 0u, 0xFFu, …)` passes `0u` as flags → no `OpaqueEXT`, no `TerminateOnFirstHitEXT`. Driver runs full closest-hit traversal across the 1000-unit reach for every (light × pixel) pair, when only ANY opaque hit is needed. Compounding: the whole compute pass has no `sceneFlags.x > 0.5` early-out — pays full TLAS-query cost even when RT is disabled at the camera UBO.
- **Fix**: Add `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` to the flags arg. Gate the light loop on `if (sceneFlags.x < 0.5) return;` after the meshId reject. CPU-side: skip dispatch when RT is disabled.

#### [SH-3] Vertex shader writes `fragPrevClipPos` using current-frame bone palette → wrong motion vectors on every skinned actor pixel
- **Dimension**: Shader Correctness
- **File**: [crates/renderer/shaders/triangle.vert:147-204](crates/renderer/shaders/triangle.vert#L147-L204)
- **Existing issue**: none
- **Finding**: `fragPrevClipPos = prevViewProj * worldPos;` where `worldPos = xform * inPosition` and `xform` is the CURRENT-frame bone palette. The previous frame's bone palette is not consulted, so the motion vector encodes only camera + rigid motion, not per-vertex skin motion. SVGF's mesh-ID consistency check ejects cross-mesh disocclusions but for in-mesh disocclusions (forearm crossing torso, both same mesh_id), reprojection writes ghost trails — visible as feathered shadows trailing actor limbs. The vertex shader comment at lines 135-137 acknowledged this as accepted pre-M29; now that GPU pre-skinning landed, fixable cheaply.
- **Fix**: Plumb `bones_prev[]` as a new readonly SSBO at `set 1, binding 12`, populated CPU-side from the previous frame's palette upload. Compute `prevWorldPos = xformPrev * inPosition` in the vertex shader. ~256 KB/frame.

#### [AS-8-1] Per-frame `refit_skinned_blas` loop has no inter-build scratch barrier — write-write hazard with ≥2 skinned NPCs
- **Dimension**: Acceleration Structures
- **File**: [crates/renderer/src/vulkan/context/draw.rs:573-591](crates/renderer/src/vulkan/context/draw.rs#L573-L591) + [acceleration.rs:803-885](crates/renderer/src/vulkan/acceleration.rs#L803-L885)
- **Existing issue**: none (sibling to MEM-2-2)
- **Finding**: Per-frame skinned-refit loop records `cmd_build_acceleration_structures(mode = UPDATE)` for every skinned entity into the same per-frame command buffer. Each refit reads from + writes to the shared `blas_scratch_buffer`. Vulkan spec requires `AS_BUILD_WRITE → AS_BUILD_READ` between consecutive AS builds sharing scratch. The static batched path enforces this barrier ([acceleration.rs:1183-1198](crates/renderer/src/vulkan/acceleration.rs#L1183-L1198)) but the per-frame skinned refit loop does not. With ≥2 skinned NPCs in a populated cell, GPU-level scratch corruption is possible (most drivers serialize implicitly via cache flushes, masking the bug — but it is UB per spec).
- **Fix**: Between iterations, emit `MemoryBarrier(AS_WRITE → AS_WRITE)` at `AS_BUILD_KHR → AS_BUILD_KHR` stage. Cleanest: lift into `AccelerationManager::record_scratch_serialize_barrier(&self, cmd)` and call at top of every iteration except the first.

### MEDIUM

#### [SY-2] Main render pass outgoing dep keeps `BOTTOM_OF_PIPE` in `dst_stage_mask`
- **Dimension**: Vulkan Sync
- **File**: [crates/renderer/src/vulkan/context/helpers.rs:153-158](crates/renderer/src/vulkan/context/helpers.rs#L153-L158)
- **Existing issue**: #573
- **Finding**: `dst_stage_mask = FRAGMENT_SHADER | COMPUTE_SHADER | BOTTOM_OF_PIPE` paired with `dst_access_mask = SHADER_READ`. `BOTTOM_OF_PIPE` is deprecated in `dst_stage_mask` per Sync2; using it with a non-empty access mask trips validation.
- **Fix**: Drop `BOTTOM_OF_PIPE` from the mask; keep `FRAGMENT_SHADER | COMPUTE_SHADER`.

#### [SY-3] Composite render pass outgoing dep uses `dst_stage_mask = BOTTOM_OF_PIPE`
- **Dimension**: Vulkan Sync
- **File**: [crates/renderer/src/vulkan/composite.rs:397-403](crates/renderer/src/vulkan/composite.rs#L397-L403)
- **Existing issue**: none (#573 sibling)
- **Finding**: Same `(BOTTOM_OF_PIPE, empty access)` legacy idiom. Present-engine wait is already covered by the binary semaphore; this dep does nothing useful and trips Sync2.
- **Fix**: Drop the dep entirely (the implicit external-out covers PRESENT_SRC layout transition) or switch to sync2 `NONE`. Bundle with #573.

#### [PIPE-3] Composite render pass `dst_access_mask = empty` on outgoing dependency
- **Dimension**: Pipeline State
- **File**: [crates/renderer/src/vulkan/composite.rs:397-403](crates/renderer/src/vulkan/composite.rs#L397-L403)
- **Existing issue**: kindred to #573
- **Finding**: Same site as SY-3, viewed as VUID for `(BOTTOM_OF_PIPE, empty)` pair under sync2 BestPractices.
- **Fix**: Bundle with #573 / SY-3 fix.

#### [MEM-2-1] M29 SkinSlots and skinned_blas leak across cell transitions
- **Dimension**: GPU Memory
- **File**: [crates/renderer/src/vulkan/context/mod.rs:1306-1311](crates/renderer/src/vulkan/context/mod.rs#L1306-L1311) + [draw.rs:460-478](crates/renderer/src/vulkan/context/draw.rs#L460-L478)
- **Existing issue**: none
- **Finding**: `skin_slots` is only ever inserted; only ever drained inside Drop. Same for `skinned_blas`: `drop_skinned_blas` exists but has no runtime call site. Long sessions retain SkinSlots for every NPC ever rendered → eventually exhausts the FREE_DESCRIPTOR_SET pool (`max_slots × MAX_FRAMES_IN_FLIGHT = 64`).
- **Fix**: Wire a per-frame check (or despawn hook) that drops both the SkinSlot and matching `skinned_blas` when the entity is absent from `draw_commands` for N consecutive frames; mirror `evict_unused_blas`'s LRU pattern with deferred-destroy.

#### [MEM-2-2] BLAS scratch buffer used outside one-time-command fence in skinned BLAS path
- **Dimension**: GPU Memory
- **File**: [crates/renderer/src/vulkan/acceleration.rs:611-720, 803-885, 394-588](crates/renderer/src/vulkan/acceleration.rs)
- **Existing issue**: none
- **Finding**: `blas_scratch_buffer` is shared across `build_blas` (one-time cmd + fence), `build_skinned_blas` (one-time cmd + fence), and `refit_skinned_blas` (per-frame cmd). If a skinned-entity first-sight BUILD lands in the same frame as another entity's per-frame refit, both reference the same scratch — fence covers host-side visibility but not GPU pipeline ordering against the per-frame cmd.
- **Fix**: Either give skinned builds their own scratch, or insert `AS_BUILD_KHR → AS_BUILD_KHR` `MemoryBarrier` on the scratch range at top of every per-frame skinned-refit dispatch when a sync BUILD ran the same frame.

#### [MEM-2-3] TLAS instance staging buffer never shrinks after exterior-cell padding
- **Dimension**: GPU Memory
- **File**: [crates/renderer/src/vulkan/acceleration.rs:1656-1666, 1866-1892](crates/renderer/src/vulkan/acceleration.rs#L1656-L1666)
- **Existing issue**: none
- **Finding**: `instance_buffer` is HOST_VISIBLE at `padded_count × 64 B` (8192 instances → 512 KB), grow-only. After a single big exterior frame (32k+ instances), two 2 MB host-visible BAR buffers + a 2 MB DEVICE_LOCAL stage stay resident for the rest of the session. BLAS scratch has `shrink_blas_scratch_to_fit` (#495); TLAS doesn't.
- **Fix**: Add `shrink_tlas_to_fit` mirroring #495's hysteresis (2× ratio + slack rule). Verify against TLAS fence's frame slot before destroying.

#### [MEM-2-4] Dead `instance_address` query in `need_new_tlas` block — footgun
- **Dimension**: GPU Memory
- **File**: [crates/renderer/src/vulkan/acceleration.rs:1679-1681](crates/renderer/src/vulkan/acceleration.rs#L1679-L1681)
- **Existing issue**: none
- **Finding**: After #289, AS-build reads `instance_buffer_device` (DEVICE_LOCAL); the staging address query at line 1679 is leftover dead code. Not a bug today, but a footgun that could re-introduce "use staging buffer's address" by mistake.
- **Fix**: Delete the unused `instance_address` query. The live address call at line 1935 stands on its own.

#### [RP-1] `mesh_id` is `R16_UINT` — comment ceiling off-by-one + no overflow guard
- **Dimension**: Render Pass
- **File**: [crates/renderer/src/vulkan/gbuffer.rs:39](crates/renderer/src/vulkan/gbuffer.rs#L39), [helpers.rs:54-56](crates/renderer/src/vulkan/context/helpers.rs#L54-L56)
- **Existing issue**: none
- **Finding**: Comment says "65534-instance ceiling"; actual usable range is `[1, 65535]`. More importantly: triangle.vert writes `instance_index + 1` blindly. If the caller batches >65535 visible instances into a single draw, the value wraps in R16_UINT, mapping multiple distinct meshes to the same ID and breaking SVGF disocclusion (history reads accept stale samples → ghosting).
- **Fix**: (a) Update comment to "65535-instance ceiling". (b) Add debug `assert!(visible_instances < 0xFFFF)` in the per-frame instance gather. (c) On overflow, partition draws across frames or bump to R32_UINT.

#### [RP-2] G-buffer images sampled by SVGF temporal before any color write on first 2-3 frames after resize
- **Dimension**: Render Pass / Layouts
- **File**: [crates/renderer/src/vulkan/gbuffer.rs:246-305](crates/renderer/src/vulkan/gbuffer.rs#L246-L305), [composite.rs:887-953](crates/renderer/src/vulkan/composite.rs#L887-L953)
- **Existing issue**: none
- **Finding**: After resize, `initialize_layouts` transitions the new (uninitialized) G-buffer images to SHADER_READ_ONLY_OPTIMAL with `src_access = empty`. Between this and the first main render pass, SVGF temporal reads previous-frame-in-flight slot's raw_indirect/motion/mesh_id/normal — driver returns whatever the freshly-allocated memory holds (typically black). SVGF's history weight can amplify this into a black-frame bloom on the first 2-3 frames after every resize.
- **Fix**: Either `vkCmdClearColorImage` each G-buffer attachment after init layout transition, or have SVGF detect "history is unusable" via a per-frame epoch counter that resize bumps and force `alpha = 1.0` (full reset) on the first 2 frames after resize. Epoch counter is the cleaner fix.

#### [SH-4] caustic_splat ray origin self-intersects on thin refractive geometry
- **Dimension**: Shader Correctness
- **File**: [crates/renderer/shaders/caustic_splat.comp:229-231](crates/renderer/shaders/caustic_splat.comp#L229-L231)
- **Existing issue**: none
- **Finding**: `origin = G + refr * 0.5, tmin = 0.0`. Bethesda glass meshes routinely have wall thickness < 0.5 units; the 0.5-unit step into the refracted direction places the start point OUTSIDE the back face. tMin = 0.0 then accepts the back face → caustic deposited on the surface the light came through. Compare triangle.frag glass refraction at line 1063: `fragWorldPos - N_geom_view * 0.1, 0.05` (surface-normal offset, non-zero tmin).
- **Fix**: `origin = G - N * 0.1, tmin = 0.05` to mirror the triangle.frag pattern.

#### [SH-5] svgf_temporal.comp 2×2 bilinear consistency uses ONLY mesh_id — same-mesh disocclusions ghost
- **Dimension**: Shader Correctness
- **File**: [crates/renderer/shaders/svgf_temporal.comp:85-134](crates/renderer/shaders/svgf_temporal.comp#L85-L134)
- **Existing issue**: none
- **Finding**: When camera orbits a long static mesh and a previously self-occluded part becomes visible (same mesh_id, different position), the bilinear sample picks the wrong point and blends in a different lighting integration. Visible as ghost streaks on receding walls during fast pans on interior cells. Schied 2017 §4.2 specifies depth + normal rejection too.
- **Fix**: Bind `prevNormalTex` (previous-frame RG16_SNORM normal G-buffer) at set 0, binding 9 of svgf_temporal.comp. In the consistency loop, reject taps with `dot(currN, prevN) < 0.9` (~25° cone).

#### [SH-6] skin_vertices.comp has no bounds check on `bone_offset + boneIdx.{x,y,z,w}`
- **Dimension**: Shader Correctness
- **File**: [crates/renderer/shaders/skin_vertices.comp:118-122](crates/renderer/shaders/skin_vertices.comp#L118-L122)
- **Existing issue**: none
- **Finding**: No bounds check against MAX_BONES_PER_MESH or the per-mesh bone count. Bethesda meshes ship with 4-byte bone indices where only the bottom byte is used; corrupted upper bytes (rare but observed in modded NIFs) read outside the per-mesh palette into another mesh's bones, producing wild transforms. The skinned vertex output feeds Phase 2 BLAS refit; out-of-bounds vertices place triangles at gigantic distances in the TLAS, breaking ray queries cluster-wide for that frame.
- **Fix**: Either clamp in-shader (`uvec4 idxClamped = min(boneIdx, uvec4(maxIdx - 1u))`) or validate the bone palette range CPU-side before dispatch.

#### [SH-7] cluster_cull.comp single-thread workgroups → ~1.5% GPU utilization
- **Dimension**: Shader Correctness (perf, but visible at scale)
- **File**: [crates/renderer/shaders/cluster_cull.comp:11](crates/renderer/shaders/cluster_cull.comp#L11)
- **Existing issue**: none
- **Finding**: `local_size_x=local_size_y=local_size_z=1` — every cluster is a 1-thread workgroup → 3456 dispatched WGs, each iterating 80–200 lights serially. ~1.5% of 4070 Ti compute capacity. Re-running every frame adds noticeable per-frame stall during cell load.
- **Fix**: `layout(local_size_x = 32) in;` with shared-memory accumulation and atomic counter. Estimated 8-16× speedup.

#### [DEN-3] SVGF / TAA history slot read is unsynchronized against producer-frame's write — masked today by fence
- **Dimension**: Denoiser & Composite
- **File**: [crates/renderer/src/vulkan/svgf.rs:670-699](crates/renderer/src/vulkan/svgf.rs#L670-L699), [taa.rs:606-631](crates/renderer/src/vulkan/taa.rs#L606-L631)
- **Existing issue**: none
- **Finding**: Producer-frame's post-dispatch barrier targets only `FRAGMENT_SHADER` in dst_stage_mask. Consumer (next frame's SVGF/TAA) reads from `COMPUTE_SHADER`. Strictly per spec, dst_stage must include every stage that will eventually read. Today the per-frame fence implicitly serializes submissions, masking the bug. Becomes real if MAX_FRAMES_IN_FLIGHT bumps to 3 or a timeline-semaphore refactor relaxes the fence wait.
- **Fix**: Widen `dst_stage_mask` to `FRAGMENT_SHADER | COMPUTE_SHADER` in both [svgf.rs:735](crates/renderer/src/vulkan/svgf.rs#L735) and [taa.rs:664](crates/renderer/src/vulkan/taa.rs#L664).

#### [LIFE-M1] `recreate_swapchain` destroys old image views BEFORE the new swapchain is created
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/context/resize.rs:61-74](crates/renderer/src/vulkan/context/resize.rs#L61-L74)
- **Existing issue**: none
- **Finding**: Image views (children of old swapchain) destroyed at lines 61-63, then `old_swapchain` passed as `oldSwapchain` to `vkCreateSwapchainKHR` at line 74. Spec is ambiguous; most reference implementations defer view destruction until after new swapchain is created. Not a hard violation but trips validation noise.
- **Fix**: Move the view-destroy loop to execute after `swapchain::create_swapchain` returns, before `destroy_swapchain(old_swapchain, …)`.

#### [LIFE-M2] `SwapchainState::destroy` takes `&self` — populates handles, masking double-destroy on future error paths
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/swapchain.rs:202-208](crates/renderer/src/vulkan/swapchain.rs#L202-L208)
- **Existing issue**: none
- **Finding**: `destroy(&self)` doesn't clear `image_views`. If ever called twice (currently doesn't, but on any future panic cleanup), every view + swapchain handle is destroyed twice.
- **Fix**: Change to `&mut self`, clear `image_views`, set `swapchain = vk::SwapchainKHR::null()` after destruction.

#### [LIFE-M3] `Texture::Drop` only logs in release — every dropped-without-destroy Texture leaks GPU memory
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/texture.rs:598-607](crates/renderer/src/vulkan/texture.rs#L598-L607)
- **Existing issue**: none
- **Finding**: `Texture::Drop` debug_asserts + log::warn but in release silently drops the allocation. `gpu-allocator::Allocation::Drop` does NOT free the GPU memory back to the allocator — every dropped Texture leaks VkImage, VkImageView, VkSampler, AND the GPU memory chunk.
- **Fix**: Hold `Arc<SharedAllocator>` inside Texture so Drop can self-free.

#### [AS-8-2] `decide_use_update` returns `(true, true)` for empty instance lists — fragile contract
- **Dimension**: Acceleration Structures
- **File**: [crates/renderer/src/vulkan/acceleration.rs:165-183](crates/renderer/src/vulkan/acceleration.rs#L165-L183)
- **Existing issue**: none
- **Finding**: For `instances.len() == 0`, the zip-compare considers two empty lists identical and chooses UPDATE. Today safe by virtue of `needs_full_rebuild = true` at TLAS creation; fragile under any future refactor that resets `needs_full_rebuild` after BUILD without checking instance_count > 0.
- **Fix**: In `decide_use_update`, return `(false, false)` when `current_addresses.is_empty()`.

#### [AS-8-3] Static BLAS single-shot path lacks `ALLOW_COMPACTION` flag
- **Dimension**: Acceleration Structures
- **File**: [crates/renderer/src/vulkan/acceleration.rs:443-447](crates/renderer/src/vulkan/acceleration.rs#L443-L447)
- **Existing issue**: none
- **Finding**: Single-shot `build_blas` flags = `PREFER_FAST_TRACE` only; batched path adds `ALLOW_COMPACTION` (saves 30-50%). If a future caller routes an RT mesh through the single-shot path (lazy mesh upload after first sight), that BLAS will be uncompacted.
- **Fix**: Add `ALLOW_COMPACTION` to single-shot flags (no-op cost; enables future compaction copy).

#### [AS-8-4] BLAS scratch alignment to `minAccelerationStructureScratchOffsetAlignment` never asserted
- **Dimension**: Acceleration Structures
- **File**: [crates/renderer/src/vulkan/acceleration.rs:506-513](crates/renderer/src/vulkan/acceleration.rs#L506-L513)
- **Existing issue**: #260 (audit reference, not GH)
- **Finding**: Comment acknowledges the alignment requirement but no assert. Works on every desktop GPU because GpuOnly buffers come back ≥256 B aligned. Future driver / mobile GPU with higher alignment → silent spec violation.
- **Fix**: Query `min_accel_struct_scratch_offset_alignment` at device init, store on `AccelerationManager`, `debug_assert!(scratch_address % min_align == 0)` at every `scratch_data(...)` call.

#### [AS-8-5] `current_addresses_scratch` Vec freshly allocated every frame — defeats `tlas_instances_scratch` amortization
- **Dimension**: Acceleration Structures
- **File**: [crates/renderer/src/vulkan/acceleration.rs:1821-1826](crates/renderer/src/vulkan/acceleration.rs#L1821-L1826)
- **Existing issue**: none
- **Finding**: At 8k instances on exterior cell that's 64 KB of heap churn per frame (3.84 MB/s at 60 FPS) to feed a 4-byte boolean.
- **Fix**: Stash a parallel `current_addresses_scratch: Vec<u64>` on `AccelerationManager` next to `tlas_instances_scratch`; same shrink-on-oversize policy.

#### [RT-5] Caustic ray bypasses `TerminateOnFirstHit` and `OpaqueEXT` (`flags = 0u`)
- **Dimension**: RT Ray Queries
- **File**: [crates/renderer/shaders/caustic_splat.comp:229](crates/renderer/shaders/caustic_splat.comp#L229)
- **Existing issue**: none (cross-link with SH-2 HIGH for the gating side)
- **Finding**: The closest-hit cost multiplier #420 fixed elsewhere is still present here. Per (light × pixel) work product makes this expensive at scale.
- **Fix**: Add `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT`. (See SH-2 for the rtEnabled side.)

### LOW

#### [SY-4] Skin-compute → BLAS-refit barrier uses legacy `ACCELERATION_STRUCTURE_READ_KHR`
- **Dimension**: Vulkan Sync
- **File**: [crates/renderer/src/vulkan/context/draw.rs:559-572](crates/renderer/src/vulkan/context/draw.rs#L559-L572)
- **Finding**: Barrier writes `dst_access = ACCELERATION_STRUCTURE_READ_KHR`. The documented intent (per acceleration.rs:603-605) is `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. Aliased on most drivers today; inconsistency between code and comment.
- **Fix**: When migrating to sync2, switch to `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. Until then, align the comment with the code.

#### [PIPE-4] Skin-compute push constants `_pad: u32` decorative — could trim to 12 B
- **Dimension**: Pipeline State
- **File**: [crates/renderer/src/vulkan/skin_compute.rs:52](crates/renderer/src/vulkan/skin_compute.rs#L52), [skin_vertices.comp:66](crates/renderer/shaders/skin_vertices.comp#L66)
- **Finding**: 16 B push range; only 12 B used. Pad doesn't help std430 (no vec4 follows). One extra dword in command stream.
- **Fix**: Drop `_pad`, change PUSH_CONSTANTS_SIZE to 12; or document it as future-proofing slot.

#### [PIPE-5] Composite/SVGF descriptor pools omit `FREE_DESCRIPTOR_SET` while skin-compute sets it — undocumented policy split
- **Dimension**: Pipeline State
- **File**: [composite.rs:533-542](crates/renderer/src/vulkan/composite.rs#L533-L542) vs [skin_compute.rs:219-233](crates/renderer/src/vulkan/skin_compute.rs#L219-L233)
- **Finding**: Skin-compute needs FREE_DESCRIPTOR_SET because slots churn with cell loads; composite/SVGF have fixed-cardinality (MAX_FRAMES_IN_FLIGHT == 2). Correct but undocumented.
- **Fix**: One-liner comment at each pool-create site stating the lifecycle rationale.

#### [RP-3] Main render pass incoming `src_access_mask = empty` is misleading documentation
- **Dimension**: Render Pass / Sync
- **File**: [crates/renderer/src/vulkan/context/helpers.rs:119-136](crates/renderer/src/vulkan/context/helpers.rs#L119-L136)
- **Finding**: `initial_layout = UNDEFINED` makes `empty` legal, but the real upstream producer between two consecutive frames using the same FIF G-buffer image is the previous frame's composite+SVGF (SHADER_READ). If someone later changes initial_layout to SHADER_READ_ONLY_OPTIMAL, the empty `src_access_mask` becomes a hazard.
- **Fix**: SAFETY comment naming the fence wait as the producer-side sync, OR set `src_access_mask = SHADER_READ` for forward-compatibility.

#### [RP-4] Composite render pass relies implicitly on `image_available` semaphore wait stage
- **Dimension**: Render Pass
- **File**: [crates/renderer/src/vulkan/composite.rs:346-394](crates/renderer/src/vulkan/composite.rs#L346-L394)
- **Finding**: `(initial_layout=UNDEFINED, image_available wait at COLOR_ATTACHMENT_OUTPUT)` is the load-bearing invariant. If a future change adds compute pre-pass writing the swapchain image, it would race acquire.
- **Fix**: Comment documenting the contract.

#### [RP-5] G-buffer attachments lack `TRANSIENT_ATTACHMENT` — INFO for desktop, MEDIUM for future mobile
- **Dimension**: Render Pass / Memory
- **File**: [crates/renderer/src/vulkan/gbuffer.rs:88](crates/renderer/src/vulkan/gbuffer.rs#L88)
- **Finding**: On desktop the bit is ignored. On tile-based GPUs (mobile/Apple Silicon, not the dev target) `LAZILY_ALLOCATED + TRANSIENT_ATTACHMENT` would let the driver keep them in tile memory.
- **Fix**: No change for desktop; revisit if mobile is added to roadmap.

#### [CMD-1] `cmd_set_depth_bias` per batch transition (DUPLICATE of #51)
- **Dimension**: Command Recording
- **Existing issue**: #51 — already filed.

#### [CMD-2] Misleading "harmless" comment on per-batch `cmd_set_cull_mode`
- **Dimension**: Command Recording
- **File**: [draw.rs:1151-1156, 1263-1268](crates/renderer/src/vulkan/context/draw.rs#L1151-L1156)
- **Finding**: First comment says "harmless host-side state the static pipeline ignores"; second comment 8 lines later correctly states it's required because pipelines declare CULL_MODE as dynamic. Drift risk if someone reads only the first.
- **Fix**: Delete or rewrite comment at 1151-1156.

#### [CMD-3] `screenshot_record_copy` uses `dst_stage = BOTTOM_OF_PIPE`
- **Dimension**: Command Recording / Sync
- **File**: [crates/renderer/src/vulkan/context/screenshot.rs:161-169](crates/renderer/src/vulkan/context/screenshot.rs#L161-L169)
- **Finding**: Same Sync2 anti-pattern as #573 in a different code path.
- **Fix**: Drop the second barrier (present-acquire semaphore wait covers ordering) or replace with `ALL_COMMANDS + empty`.

#### [CMD-4] UI overlay relies on inherited depth/cull dynamic state from last main batch
- **Dimension**: Command Recording
- **File**: [crates/renderer/src/vulkan/context/draw.rs:1396-1436](crates/renderer/src/vulkan/context/draw.rs#L1396-L1436)
- **Finding**: UI re-binds viewport+scissor but not depth_test_enable / depth_write_enable / depth_compare_op / depth_bias / cull_mode. UI pipeline declares those as dynamic; works only because last main batch happens to align.
- **Fix**: Either make UI pipeline use static depth/cull state, or explicitly set every dynamic state right after `cmd_bind_pipeline(pipeline_ui)`.

#### [CMD-5] Per-mesh fallback rebinds VB/IB every batch when global VB/IB is absent
- **Dimension**: Command Recording (perf)
- **File**: [draw.rs:1305-1339](crates/renderer/src/vulkan/context/draw.rs#L1305-L1339)
- **Finding**: Two-sided alpha-blend split calls `dispatch_direct` twice for the same batch → 2× redundant VB/IB binds when global VB/IB is missing.
- **Fix**: Cache `last_bound_mesh_handle` across `dispatch_direct` invocations.

#### [LIFE-L1] Allocator-Arc-leak fall-through proceeds to `device.destroy_device` → driver-level UAF risk
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/context/mod.rs:1390-1403](crates/renderer/src/vulkan/context/mod.rs#L1390-L1403)
- **Finding**: When `Arc::try_unwrap` fails (forgotten clone), debug_assert fires but release falls through to destroy device. Allocator's eventual Drop calls `vkFreeMemory` on a destroyed device → use-after-free.
- **Fix**: After the `Err(arc)` arm, `return;` instead of falling through. OS reclaims on process exit. Better than UB.

#### [SH-9] Boolean-as-float convention drift across SVGF/TAA/composite UBOs
- **Dimension**: Shader Correctness
- **File**: [svgf_temporal.comp:39-42](crates/renderer/shaders/svgf_temporal.comp#L39-L42), [taa.comp:84](crates/renderer/shaders/taa.comp#L84), [composite.frag:261](crates/renderer/shaders/composite.frag#L261)
- **Finding**: Mix of `> 0.5` and `< 0.5` checks on float fields that store boolean flags. Vulkan has no `bool` UBO type so float-clamp is conventional but inconsistent.
- **Fix**: Comment-only fix per site or migrate to a uint flag block (one bit per state).

#### [SH-10] Composite fog-far guard doesn't reject negative `fog_near`
- **Dimension**: Shader Correctness
- **File**: [composite.frag:262](crates/renderer/shaders/composite.frag#L262)
- **Finding**: FNV CLMT records occasionally ship negative fog_near; passes the gate but `smoothstep(neg, pos, dist)` produces nonzero attenuation at dist=0 → fog floor on every pixel.
- **Fix**: CPU-side clamp at scene-buffer upload (`fog_near = max(fog_near, 0.0)`) — cheaper than per-fragment.

#### [SH-11] caustic_splat.comp comment block contradicts actual logic
- **Dimension**: Shader Correctness
- **File**: [caustic_splat.comp:163-176](crates/renderer/shaders/caustic_splat.comp#L163-L176)
- **Fix**: Rewrite comment to match actual logic (mesh_id high bit = ALPHA_BLEND_NO_HISTORY marker; flag bit 2u = caustic-source flag, separate concept).

#### [SH-12] Composite caustic fixed-scale magic number (65536.0) duplicated across 3 sites with no shared constant
- **Dimension**: Shader Correctness
- **File**: [composite.frag:47](crates/renderer/shaders/composite.frag#L47), caustic.rs, caustic_splat.comp
- **Fix**: Route via `CompositeParams` UBO, or add Rust `const_assert!` + build-time grep test on shader source.

#### [RT-3] Reflection ray bias `+ N * 0.1` lacks the `dot(N,V)` flip the glass path uses
- **Dimension**: RT Ray Queries
- **File**: [triangle.frag:1216](crates/renderer/shaders/triangle.frag#L1216)
- **Fix**: Reuse `N_view = dot(N, V) < 0.0 ? -N : N` pattern from line 1018.

#### [RT-4] GI ray tMin (0.5u) > bias (0.1u) — inverted size relationship
- **Dimension**: RT Ray Queries
- **File**: [triangle.frag:1497, 1503](crates/renderer/shaders/triangle.frag#L1497)
- **Fix**: Either raise bias to `N * 0.5` or drop tMin to 0.05 (matches shadow ray pattern).

#### [RT-6] Caustic ray tMin = 0.0 with no normal-aligned bias
- **Dimension**: RT Ray Queries
- **File**: [caustic_splat.comp:230](crates/renderer/shaders/caustic_splat.comp#L230)
- **Fix**: `G - N * 0.1, 0.05, refr, 1000.0` — mirror triangle.frag refraction (line 1063).

#### [RT-7] `cosineWeightedHemisphere` propagates `buildOrthoBasis` axial-normal singularity into GI
- **Dimension**: RT Ray Queries
- **File**: [triangle.frag:278-284, 1496](crates/renderer/shaders/triangle.frag#L278-L284)
- **Existing issue**: #574 (RT-2) — same root cause; this is the visible failure surface.
- **Finding**: On flat ground at exactly `N = (0, 1, 0)` the basis collapses; every GI ray fires straight up regardless of (u1, u2). Visible as flat-shaded sky-only fill on perfectly horizontal Bethesda terrain / interior floors.

#### [RT-8] GI miss uses hardcoded sky color, ignores per-cell ambient
- **Dimension**: RT Ray Queries
- **File**: [triangle.frag:1539](crates/renderer/shaders/triangle.frag#L1539)
- **Finding**: `indirect = vec3(0.6, 0.75, 1.0) * 0.06` ignores `sceneFlags.yzw` (XCLL/LGTM ambient). In red-lit interiors, GI miss injects unauthored blue light.
- **Fix**: Gate by exterior flag, or fall back to `sceneFlags.yzw * 0.5`.

#### [RT-9] Shadow ray jitter floor of 1.5 units — radius=0 lights collapse to point
- **Dimension**: RT Ray Queries
- **File**: [triangle.frag:1425](crates/renderer/shaders/triangle.frag#L1425)
- **Fix**: Confirm importer always writes `radius > 0` for visible lights (#277), or scale floor with cell extent.

#### [DEN-1] SSAO is one-frame-lagged but `draw.rs:1521` comment claims current-frame
- **Dimension**: Denoiser & Composite
- **File**: [draw.rs:1517-1535](crates/renderer/src/vulkan/context/draw.rs#L1517-L1535), [triangle.frag:179](crates/renderer/shaders/triangle.frag#L179)
- **Fix**: Replace `draw.rs:1517-1521` comment with the lag-aware description.

#### [DEN-2] SSAO emits UNDEFINED→GENERAL every frame, discarding `initialize_ao_images` clear
- **Dimension**: Denoiser & Composite
- **File**: [crates/renderer/src/vulkan/ssao.rs:514-536](crates/renderer/src/vulkan/ssao.rs#L514-L536)
- **Fix**: `old_layout = SHADER_READ_ONLY_OPTIMAL` (steady state) on every frame after one-time init.

#### [DEN-4] SVGF temporal α hardcoded to 0.2 — no host-side knob for cell-load discontinuity
- **Dimension**: Denoiser & Composite
- **File**: [svgf.rs:641-649](crates/renderer/src/vulkan/svgf.rs#L641-L649)
- **Fix**: Wire α into a per-frame parameter on `VulkanContext` so cell-loader / weather change can bump it for ~5 frames after a discontinuity.

#### [DEN-5] SVGF early-out paths write `histAge=1.0` to moments — should be 0 to distinguish "never accumulate" from "first frame"
- **Dimension**: Denoiser & Composite
- **File**: [svgf_temporal.comp:64-68, 148-151](crates/renderer/shaders/svgf_temporal.comp#L64-L68)
- **Fix**: Distinguish the two cases. Becomes load-bearing when Phase 4 spatial filter lands.

#### [DEN-6] TAA writes `alpha=1.0`, destroying alpha-blend marker bit from main pass
- **Dimension**: Denoiser & Composite
- **File**: [taa.comp:114, 157](crates/renderer/shaders/taa.comp#L114)
- **Fix**: `imageStore(uOutput, pix, vec4(outRgb, alpha_passthrough))`.

#### [DEN-8] `CompositeParams.depth_params` carries one bit but is named for its slot
- **Dimension**: Denoiser & Composite
- **File**: [composite.rs:51-53](crates/renderer/src/vulkan/composite.rs#L51-L53)
- **Fix**: Rename to `flags`, document bit layout.

#### [DEN-9] SVGF/TAA `recreate_on_resize` doesn't re-issue the UNDEFINED→GENERAL one-time barrier
- **Dimension**: Denoiser & Composite
- **File**: [svgf.rs:759-812](crates/renderer/src/vulkan/svgf.rs#L759-L812), [taa.rs:677-721](crates/renderer/src/vulkan/taa.rs#L677-L721)
- **Finding**: Relies on `frames_since_creation = 0` + `first_frame=1.0` branch. But the first dispatch's pre-barrier declares `old_layout = GENERAL` — if the image is actually still UNDEFINED post-resize, validation layer fires.
- **Fix**: `recreate_on_resize` should re-issue the UNDEFINED→GENERAL one-time barrier (factor out a private helper, or call `initialize_layouts` from inside).

#### [LIFE-L2] Misleading "meshes outlive pipelines" comment
- **Dimension**: Resource Lifecycle
- **File**: [crates/renderer/src/vulkan/context/mod.rs:1374-1378](crates/renderer/src/vulkan/context/mod.rs#L1374-L1378)
- **Finding**: `device_wait_idle` already drained GPU work; the ordering doesn't matter for correctness.
- **Fix**: Update comment.

#### [AS-8-6] `build_tlas` "missing instances" warning miscounts `!in_tlas` as missing BLAS
- **Dimension**: Acceleration Structures
- **File**: [acceleration.rs:1611-1627](crates/renderer/src/vulkan/acceleration.rs#L1611-L1627)
- **Fix**: Count only `in_tlas && (no BLAS for mesh)` and `in_tlas && instance_map[i].is_none()`.

#### [AS-8-7] `tlas.last_blas_addresses` not cleared on TLAS destroy — stale data carries through resize
- **Dimension**: Acceleration Structures
- **File**: [acceleration.rs:1640-1651, 1786-1795](crates/renderer/src/vulkan/acceleration.rs#L1640-L1651)
- **Finding**: Today safe because gen-mismatch short-circuits before zip runs. Fragile.
- **Fix**: Document the invariant in a comment so a future refactor doesn't accidentally `take()` the field.

#### [AS-8-8] Skinned BLAS `last_used_frame` set against `frame_counter` that's bumped inside `build_tlas` — artificially newer
- **Dimension**: Acceleration Structures
- **File**: [acceleration.rs:776, 1504](crates/renderer/src/vulkan/acceleration.rs#L776)
- **Fix**: Bump `frame_counter` once at top of `draw_frame` instead of inside `build_tlas`.

#### [AS-8-9] M29 `build_skinned_blas` — refit chain accumulates BVH inefficiency over time
- **Dimension**: Acceleration Structures
- **File**: [acceleration.rs:660-662](crates/renderer/src/vulkan/acceleration.rs#L660-L662)
- **Finding**: Long animation cycle on a long-lived NPC eventually has the refit BLAS noticeably slower to traverse than a fresh BUILD. Renderer never periodically rebuilds.
- **Fix**: Track per-skinned-BLAS frame count or animated-bbox ratio; rebuild every ~600 frames or when bbox grows >2× original.

#### [SH-8] Vertex SSBO float reinterpret extends to skin_vertices.comp — corollary on #575
- **Dimension**: Shader Correctness
- **Existing issue**: **#575 (SH-1, OPEN)** — corollary observation, NOT a re-file
- **File**: [triangle.frag:187-192, 289-313](crates/renderer/shaders/triangle.frag#L187-L192), [skin_vertices.comp:39-41, 79-103](crates/renderer/shaders/skin_vertices.comp#L39-L41)
- **Finding**: skin_vertices.comp inherits the same float-array reinterpretation contract; M29 Phase 2 BLAS refit extends the blast radius. Fix #575 with a typed-struct SSBO and skin_vertices.comp gets the upgrade for free.

#### [MEM-2-5] Per-frame HOST_VISIBLE buffers rely on `gpu-allocator` persistent mapping without runtime assert
- **Dimension**: GPU Memory
- **File**: [crates/renderer/src/vulkan/buffer.rs:443-490](crates/renderer/src/vulkan/buffer.rs#L443-L490)
- **Fix**: `debug_assert!(allocation.mapped_slice().is_some())` in `create_host_visible` to fail loudly at startup.

#### [MEM-2-6] Skin compute output buffer requests unused `VERTEX_BUFFER` usage flag
- **Dimension**: GPU Memory
- **File**: [skin_compute.rs:274-282](crates/renderer/src/vulkan/skin_compute.rs#L274-L282)
- **Finding**: Phase 3 raster-VBO path is deferred. Unused flag tightens the memory-type mask unnecessarily on unified-memory configs.
- **Fix**: Drop until Phase 3 lands.

#### [MEM-2-7] TLAS scratch buffer never shrinks (parallel of MEM-2-3)
- **Dimension**: GPU Memory
- **File**: [acceleration.rs:1752-1778](crates/renderer/src/vulkan/acceleration.rs#L1752-L1778)
- **Fix**: Add `shrink_tlas_scratch_to_fit` mirroring #495's `shrink_blas_scratch_to_fit`.

#### [MEM-2-8] `scene_buffers` ray-budget HOST_VISIBLE buffer is 4 B — wastes a 64 KB block alignment
- **Dimension**: GPU Memory
- **File**: [scene_buffer.rs:599-604](crates/renderer/src/vulkan/scene_buffer.rs#L599-L604)
- **Fix**: Fold ray_budget into the camera UBO tail.

#### [LIFE-M4] `frame_sync.recreate_for_swapchain` relies on parent `device_wait_idle`
- **Dimension**: Resource Lifecycle
- **File**: [resize.rs:434-437](crates/renderer/src/vulkan/context/resize.rs#L434-L437)
- **Fix**: SAFETY comment documenting the precondition.

#### [CMD-6] Composite re-emits viewport+scissor already current
- **Dimension**: Command Recording (perf, micro)
- **File**: [composite.rs:766, 774](crates/renderer/src/vulkan/composite.rs#L766)
- **Fix**: Optional. Probably not worth the diff.

#### [RT-10] Reflection ray cone jitter uses `roughness²` raw — no PI-normalised GGX lobe sampling
- **Dimension**: RT Ray Queries (algorithmic accuracy)
- **File**: [triangle.frag:1214](crates/renderer/shaders/triangle.frag#L1214)
- **Finding**: Acceptable for current 1-SPP single-bounce design; document or accept as Phase-1 simplification.

### INFO

#### [SY-5] SVGF "previous-use" image barrier widens src to `SHADER_READ | SHADER_WRITE` — extra bit harmless
- **File**: [svgf.rs:675-699](crates/renderer/src/vulkan/svgf.rs#L675-L699)
- **Fix**: Drop `SHADER_READ` from src_access.

#### [SY-6] Two-fence wait is load-bearing for SVGF/TAA/caustic temporal reads
- **File**: [draw.rs:96-110](crates/renderer/src/vulkan/context/draw.rs#L96-L110)
- **Existing issue**: #282 — closed
- **Fix**: SAFETY comment naming SVGF specifically.

#### [PIPE-6] All pipeline create sites share a single `vk::PipelineCache` — verified.
#### [PIPE-7] Vertex attribute descriptions match shader inputs — verified.
#### [PIPE-8] Two-sided pipeline correctly differs from main only in `cull_mode` — verified.
#### [PIPE-9] Blend pipeline cache key fully discriminates Gamebryo factor space — verified.
#### [PIPE-10] Skin-compute push constant + descriptor layout match shader contract — verified.

#### [RP-6] G-buffer formats correctly match shader output types — verified all 7 main-pass attachments.
#### [RP-7] Per-frame-in-flight images correctly sized post-#576 — verified.

#### [SH-13] Push constant ranges and descriptor binding indices consistent across all consumers — verified.
#### [SH-14] All RT hits in triangle.frag use `instance_custom_index` (not `gl_InstanceID`) — verified.
#### [SH-15] RT flag gating `sceneFlags.x > 0.5` consistent across every triangle.frag ray site — caustic_splat.comp is the outlier (see SH-2).

#### [LIFE-INFO-1] LIFE-C1/C2/C3 (SVGF/Composite/GBuffer destroy()) confirmed fixed — fix holds.
#### [LIFE-INFO-2] AccelerationManager destroy correctly handles `tlas[0..1]`, scratch buffers — except `pending_destroy_blas` (LIFE-H1).
#### [LIFE-INFO-3] Skin slot teardown ordering correct (skin_slots before skin_compute) — verified.

#### [AS-8-10] BLAS build flag stratification by lifecycle is correct — verified.
#### [AS-8-11] `instance_custom_index` parity with SSBO via shared `build_instance_map` (#419) — verified.
#### [AS-8-12] `TRIANGLE_FACING_CULL_DISABLE` two-sided gating + `hostQueryReset` — verified.

#### [MEM-2-9] Allocator drop ordering: warn-only on outstanding Arc — debug_assert correct, release leaks (see LIFE-L1).
#### [MEM-2-10] Resize flow correctly reallocates SVGF/TAA/GBuffer/depth/SSAO/composite/caustic — verified.

---

## Prioritized Fix Order

**P0 — Correctness (do first)**
1. **AS-8-1**: Inter-build scratch barrier in per-frame skinned-refit loop. Spec violation; manifests with ≥2 skinned NPCs. Lift helper into `AccelerationManager::record_scratch_serialize_barrier` and call between iterations.
2. **SH-2**: caustic_splat.comp rtEnabled gate + ray flags. Wasted GPU cycles every frame regardless of toggle.
3. **SH-3**: `bones_prev[]` SSBO for skinned motion vectors. Now fixable post-M29; fixes ghost trails on every actor in motion.
4. **LIFE-H1**: Drain `pending_destroy_blas` in `AccelerationManager::destroy`. Trivial.
5. **LIFE-H2** (#33): `scene_buffers.recreate_descriptor_sets` on resize, OR audit every write_* call site so each binding referencing a recreated G-buffer image is re-issued.
6. **SH-6**: skin_vertices.comp bone-index bounds check. Either CPU-side validation pre-dispatch or shader clamp.

**P1 — Spec cleanups (bundle)**
7. **#573 / SY-2 / SY-3 / PIPE-3 / CMD-3**: BOTTOM_OF_PIPE in dst_stage_mask across 4 sites. Single PR.
8. **DEN-3**: Widen SVGF/TAA producer barrier dst_stage to `FRAGMENT_SHADER | COMPUTE_SHADER`. Latent today, real if MAX_FRAMES_IN_FLIGHT bumps.
9. **MEM-2-1**: Wire skin_slots / skinned_blas LRU eviction. Long-session leak.
10. **AS-8-4**: Query and assert `minAccelerationStructureScratchOffsetAlignment`.

**P2 — Quality / robustness**
11. **RP-1**: mesh_id overflow guard + comment fix.
12. **RP-2**: SVGF reset epoch counter on resize (eliminates first-frame black bloom).
13. **SH-5**: prevNormalTex binding + dot rejection in SVGF temporal (closes wall-pan ghosting).
14. **SH-7**: cluster_cull workgroup size 32 with shared-memory accumulation. 8-16× speedup at city-cell scales.
15. **DEN-9**: SVGF/TAA `recreate_on_resize` re-issues UNDEFINED→GENERAL barrier (silences validation layer post-resize).
16. **MEM-2-3 / MEM-2-7**: TLAS instance + scratch shrink-to-fit (mirror #495).

**P3 — Hygiene**
17. CMD-2 / SH-9 / SH-11 / DEN-1 / DEN-8: documentation cleanups.
18. CMD-4: UI overlay explicit dynamic-state binds (defensive against future drift).
19. Remaining LOW findings as opportunity arises.

---

## Notes

- The dim_7 summary stated "1 CRITICAL · 2 HIGH" but the report contains 0 CRITICAL · 2 HIGH (LIFE-H1, LIFE-H2). The actual finding totals are reflected in this merged executive summary.
- Several findings are **cross-cutting**: the `BOTTOM_OF_PIPE` deprecation surfaces in 4 places (helpers.rs, composite.rs, screenshot.rs, with PIPE-3 as the pipeline-state view of composite.rs). Fixing the helper-pattern in one PR closes all four with low diff.
- The stale-renderer-doc thread keeps producing low findings (DEN-1, CMD-2, RP-1 comment, SH-11, DEN-8). Most are one-line edits.

---

**Next step**: `/audit-publish docs/audits/AUDIT_RENDERER_2026-04-25.md`
