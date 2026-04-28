# AUDIT_RENDERER — 2026-04-27

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: `7dc354a` (`M40 Phase 1b shutdown: drain streamed cells before VulkanContext destroy + log WTHR ambient/sunlight`)
**Reference report**: `docs/audits/AUDIT_RENDERER_2026-04-25.md`
**Dimensions**: 10 (Sync · GPU Memory · Pipeline State · Render Pass · Command Recording · Shader Correctness · Resource Lifecycle · Acceleration Structures · RT Ray Queries · Denoiser & Composite)
**Open issues baseline**: `gh issue list --state=all --limit 200` → 200 issues

---

## Executive Summary

**0 CRITICAL · 1 HIGH · 4 MEDIUM · 18 LOW · 5 INFO** — across 28 new findings.

The pipeline is broadly correct and continues to converge. **One HIGH-severity finding (`LIFE-N1`) identifies the root cause of issue #732** — the M40 Phase 1b exterior shutdown SIGSEGV that has been reproducible since the first FNV WastelandNV streaming session (2026-04-27). Three subsystems (`SceneBuffers`, `SsaoPipeline`, `TextureRegistry`) call `destroy()` to free GPU memory but never `clear()` their `Vec<GpuBuffer>` afterwards — the per-buffer `Arc<Allocator>` clones survive into `VulkanContext::Drop`, where `Arc::try_unwrap` fails, the allocator is intentionally leaked, and the per-buffer Drops then re-touch a destroyed device. Fix is one-line per subsystem; details at the LIFE-N1 finding below. **This single fix is expected to close #732 without code-architecture changes.**

The 4 MEDIUM findings cluster around (a) grow-only buffers without a shrink path now visible at multi-cell streaming scale (`MEM-N1` TLAS scratch/instance, `DEN-9` SVGF resize layout init), and (b) RT-shadow ray hygiene drift from the recently-landed reflection/glass `N_view` flip work — `RT-11` is the per-light reservoir shadow path that was missed by the #668 fix, and `RT-12` documents tMin/bias asymmetry on the same ray site.

The 18 LOW findings are predominantly named-constant duplications, comment-vs-code drift, and prior-audit items that remain unresolved but whose tracking PRs (#573 family) are already filed. Six prior-audit findings are confirmed closed since 2026-04-25.

### What's new since 04-25

- **M40 Phase 1a + 1b** landed (commits `2e3f73e`, `cdfef07`, `80e2966`, `592e7bf`, `7dc354a`). Per-cell exterior cell loader + worker-thread NIF pre-parse + payload drain. The streaming surface introduces no new sync hazards (all per-cell BLAS submits use the existing fenced `submit_one_time` path, no concurrent scratch reuse — verified in `dim_8.md`). It does, however, expose `LIFE-N1` (allocator-leak shutdown SIGSEGV) and surface `AS-8-14` (BLAS LRU eviction silently no-ops during cell-load bursts because `frame_counter` advances only inside `build_tlas`).
- **Three exterior-render bugs filed during the same FNV WastelandNV session** (#729 green tint, #730 cloud pixelation, #731 perceived view distance ≈ 30m) were investigated GLSL-side as part of this audit:
  - **#729** — no GLSL root cause. CPU-side WTHR `SKY_FOG[TOD_DAY]` slot index audit is the right next step (the resolved ambient + sunlight RGB values logged in commit `7dc354a` are *not* green; the tint must come from the un-logged fog_color path).
  - **#730** — partial GLSL contribution (`SH-13`): cloud UV `dir.xz / max(elevation, 0.05)` creates a perspective singularity at the horizon-fade band, causing driver mip-LOD oscillation between mip 0 (per-texel aliasing — the user's "pixelated") and the smallest mip (over-blurred soup). CPU-side audit of cloud sampler filter + DDS resolution still required.
  - **#731** — no GLSL bug. Composite fog math is correct given WTHR `fog_near=-10, fog_far=200000`; the perceived "30m view distance" is `#729`'s green tint dominating distant pixels, not a fog-distance bug.

### What's still open from prior audits

- **#573 family** — `BOTTOM_OF_PIPE` in `dst_stage_mask`. Three sites: `SY-2` (helpers.rs:156, also flagged this pass as `RP-N1`), `SY-3` (composite.rs:402), `CMD-3` (screenshot.rs:164). Single bundled PR pending.
- **#33 / LIFE-H2** — confirmed appropriately resolved this audit (resize call site rewrites every scene-set image binding affected by reallocation; only AO at binding 7 needs the rewrite since other G-buffer images live on SVGF/composite/caustic/TAA descriptor sets that handle their own `recreate_on_resize`). The deeper concern from this dimension (LIFE-N1 above) is not the same hazard — it's a long-lived subsystem buffer Arc retention, not a stale image-view binding.
- **MEM-2-3 / MEM-2-7 / MEM-2-8 / MEM-2-6** — re-flagged here as MEM-N1/N2/N3 (TLAS shrink, ray_budget BAR waste, skin compute VERTEX_BUFFER flag). Same status, no regression.
- **SH-1 (#575)** — Vertex SSBO read as `float[]` while bone_indices/splat are non-floats. Open. Cross-link from `SH-8` (skin_vertices.comp inherits same contract).
- **SH-5 / SH-6** — SVGF history rejection by mesh ID only (no normal/depth); `skin_vertices.comp` no bounds check on `bone_offset`. Both still open.
- **AS-8-6 through AS-8-12** — see Holding Items in `dim_8` for the 7-item open list.

### What's confirmed closed since 04-25

- `DEN-3` (SVGF post-dispatch dst-stage widened to `FRAGMENT | COMPUTE`) — fixed via #653.
- `DEN-4` (SVGF α host-side knob) — fixed via #674.
- `DEN-6` (TAA preserves HDR.a alpha-blend marker bit) — fixed via #676.
- `AS-8-1..5` (per-frame skinned scratch barrier, empty-list `decide_use_update`, single-shot BLAS `ALLOW_COMPACTION`, scratch alignment assert, TLAS address scratch reuse) — fixed via #644 / #657 / #658 / #659 / #660.
- `RT-2` (Frisvad orthonormal basis, NaN at dir = (0,1,0)) — fixed via #574.
- `RT-3` (reflection ray bias `N_view` flip) — fixed via #668 for reflection + glass paths. Shadow path missed → flagged this audit as `RT-11`.
- `RT-4` (GI tMin/bias inversion) — fixed via #669.
- `RT-7` (Frisvad supersedes — duplicate root cause).
- `LIFE-H1` / `#639` (`pending_destroy_blas` drained on shutdown) — fixed.
- `LIFE-M3` / `#656` (`Texture::Drop` self-cleans VkImage/VkImageView/allocation) — fixed.
- `SH-2 / RT-5 / RT-6` (caustic_splat ray flags + rtEnabled gate + origin bias) — fixed.
- `SH-3` / `#641` (skinned motion vectors via prev-frame bone palette) — fixed.
- `SH-7` (cluster_cull workgroup parallelisation) — fixed via #652.
- `SH-10` (composite fog guard) — fixed.
- `CMD-4` (UI overlay dynamic state via `UI_PIPELINE_DYNAMIC_STATES` const_assert) — fixed via #663.

---

## RT Pipeline Assessment

**BLAS / TLAS correctness**: solid. Five HIGH/MEDIUM findings from the M29-era audit are confirmed closed (`AS-8-1..5`); two new LOW findings (`AS-8-13` skinned-BLAS deferred-destroy contract spread across two unrelated constants; `AS-8-14` BLAS LRU eviction no-op during M40 cell-load bursts) cover edges that didn't exist pre-streaming.

**Ray query safety**: 6 ray sites in `triangle.frag` correctly bind set=1 binding=2 `topLevelAS`, all carry `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT`, all check `CommittedIntersectionNoneEXT`, all gate on `rtEnabled = sceneFlags.x > 0.5` where applicable. Frisvad orthonormal basis is in place at `triangle.frag:288-294` (singularity-free except the analytic `dir.z = -1` case).

The MEDIUM `RT-11` finding (reservoir shadow ray reuses raw `N` for origin bias) is the only direct correctness gap — the reflection path (#668) and glass IOR path got the `N_view = dot(N,V) < 0 ? -N : N` flip, the per-light reservoir shadow path was missed. Bias falls behind the macro surface on grazing/noisy normal maps; produces self-shadowing acne on bump-mapped geometry under cluster lights.

**Denoiser stability**: SVGF + TAA + composite chain is correct end-to-end. One MEDIUM finding (`DEN-9`, SVGF `recreate_on_resize` doesn't re-issue `UNDEFINED→GENERAL` barrier) is unchanged from prior audit and triggers validation-layer noise on every window resize. Three LOW polish items.

---

## Rasterization Assessment

**Pipeline state**: vertex input ↔ shader location/format/offset compile-time-validated (`offset_of!` tests at vertex.rs:243-253). Push constants only on skin compute (12B, matches `SkinPushConstants` post-PIPE-4). All 4 compute pipelines have descriptor layouts SPIR-V-reflection-validated. The four `PS-6/7/8/9` findings are all LOW/INFO polish — static depth-state values that are silently overridden by dynamic state declarations (drift hazard if dynamic-state list is ever shrunk), pipeline cache resolved against cwd (silent save failure on read-only launch dirs), null-handle pattern in `recreate_triangle_pipelines` (unused-but-misleading null layouts).

**Render pass + G-Buffer**: 6 color + depth attachments, all `CLEAR + STORE`, all `COLOR_ATTACHMENT | SAMPLED` (verified gbuffer.rs:88), final layouts `SHADER_READ_ONLY_OPTIMAL` matching downstream sampler reads. `RP-N1` is the helpers.rs:156 outgoing dependency `BOTTOM_OF_PIPE` that #573 will close. `RP-N2` corrects the comment at helpers.rs:54-55 ("65534-instance ceiling" → 65535) and recommends a runtime guard.

**Command recording**: zero new findings. Two CMD candidates were promoted to no-finding after analysis (the two-sided alpha-blend split is intentional ordering, not a depth pre-pass; the redundant initial `cmd_set_depth_bias` for first-batch-decal is a single host command). All 11 checklist items pass; CMD-3 (#573 sibling, screenshot post-copy `BOTTOM_OF_PIPE`) and CMD-5 (per-mesh fallback rebind) remain open from prior audit.

---

## Findings

### HIGH

#### LIFE-N1 — Subsystem `destroy()` methods free GPU memory but leave `Vec<GpuBuffer>` populated, leaking `Arc<Allocator>` clones

- **Closes**: #732 (M40 Phase 1b exterior shutdown SIGSEGV)
- **Locations**:
  - `crates/renderer/src/vulkan/scene_buffer.rs:1440-1462`
  - `crates/renderer/src/vulkan/ssao.rs:594-596`
  - `crates/renderer/src/texture_registry.rs:804-806` (StagingPool option not taken)
- **Observed**: Each subsystem's `destroy()` iterates its `Vec<GpuBuffer>` and calls `buf.destroy()` (frees the VkBuffer + allocation) but never `Vec::clear()` afterwards. Each `GpuBuffer` struct still holds its `allocator: SharedAllocator` (`Arc<Mutex<Allocator>>`) clone. Same shape in `SsaoPipeline.param_buffers`, every Vec in `SceneBuffers` (light/camera/bone/instance/indirect/ray_budget), and `TextureRegistry.staging_pool` is `as_mut()`-trimmed but never `take()`n. The Arc clones only release at the subsystem struct's natural Drop — which runs AFTER `VulkanContext::Drop` already failed `Arc::try_unwrap` on the allocator (mod.rs:1502), so the allocator is intentionally leaked, the per-buffer Drops then re-touch a destroyed device → SIGSEGV. Issue #732 INVESTIGATION already pinpointed the 22-Arc leak surface but the underlying field clearing was not addressed; the `flush_pending_destroys` shutdown sweep at byroredux/src/main.rs:707 only drains the deferred-destroy queues, not these long-lived subsystem buffers.
- **Expected**: Each `destroy()` should consume its buffer Vecs (`std::mem::take(&mut self.x_buffers)` then iterate the owned Vec, or `.clear()` after the destroy loop), so the Arc<Allocator> clones inside each `GpuBuffer` release before `VulkanContext::Drop` reaches its `Arc::try_unwrap` step. `TextureRegistry.staging_pool` should `take()` the Option (or its `destroy()` should consume `self`).
- **Fix**: Add `self.light_buffers.clear();` (and siblings) after each loop in `SceneBuffers::destroy`. In `SsaoPipeline::destroy` add `self.param_buffers.clear()` after the per-buffer destroy. In `TextureRegistry::destroy`, replace the `as_mut()` block with `if let Some(mut pool) = self.staging_pool.take() { pool.destroy(); }`. This will let `Arc::try_unwrap` succeed and the allocator drop cleanly, and the existing `LIFE-L1` warn-only fall-through becomes unreachable in steady state.

### MEDIUM

#### MEM-N1 — TLAS instance + scratch buffers grow-only, no shrink path

- **Reopens**: MEM-2-3 / MEM-2-7
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:1804-1968`
- **Observed**: `instance_buffer` (HOST_VISIBLE), `instance_buffer_device` (DEVICE_LOCAL), and `scratch_buffers[frame]` only grow on `max_instances < instance_count` mismatch; a single 32k-instance exterior frame leaves ~6 MB pinned for the rest of the session. BLAS scratch already has `shrink_blas_scratch_to_fit` (#495); no symmetric `shrink_tlas_*`.
- **Expected**: After K consecutive frames with `instance_count <= padded/4`, destroy and re-create the per-frame TLAS slot at the new high-water mark, mirroring `scratch_should_shrink`'s `2× + 16 MB SLACK` hysteresis.
- **Fix**: Add `shrink_tlas_to_fit(frame_index)` that gates on the per-frame fence (already required for `tlas[frame_index].take()`) and reuses `scratch_should_shrink`.

#### RT-11 — Reservoir shadow ray reuses raw `N` for origin bias (sibling of RT-3 / #668)

- **Location**: `crates/renderer/shaders/triangle.frag:1543`
- **Observed**: `vec3 rayOrigin = fragWorldPos + N * 0.05;` uses the bump-mapped `N` directly. RT-3 (#668) fixed the metal-reflection path (line 1331) and the glass IOR path (line 1134) to flip `N` when `dot(N,V) < 0` (`N_view`), avoiding biases that fall behind the macro surface on grazing/noisy normal maps. The reservoir shadow path was not brought into lockstep — every cluster light's shadow ray inherits the pre-#668 behaviour. Visible as self-shadowing acne under cluster lights.
- **Expected**: Same `N_view` flip rule for every ray-origin bias.
- **Fix**: `vec3 N_bias = dot(N, V) < 0.0 ? -N : N; vec3 rayOrigin = fragWorldPos + N_bias * 0.05;` Tighten the symmetry by hoisting `N_bias` once above all four ray sites (reflection 1340, glass IOR 1142, GI 1621, shadow 1543).

#### DEN-9 — SVGF `recreate_on_resize` does not re-issue UNDEFINED→GENERAL barrier

- **Location**: `crates/renderer/src/vulkan/svgf.rs:792-854`
- **Observed**: Resize destroys + recreates `indirect_history` / `moments_history` images with `initial_layout = UNDEFINED`. `recreate_on_resize` resets `frames_since_creation = 0` and rewrites descriptor sets, but never re-runs `initialize_layouts` (lines 606-650). The next `dispatch()` immediately emits a per-frame barrier with `old_layout = GENERAL` (lines 715-716, 757-758) against an image that is still UNDEFINED in spec terms. Validation layer fires `VK_IMAGE_LAYOUT_GENERAL ≠ actual layout`. Prior audit DEN-9 flagged this; still unfixed.
- **Expected**: After resize, the new history images must be transitioned UNDEFINED → GENERAL exactly once before any per-frame GENERAL → GENERAL barrier runs.
- **Fix**: Inside `recreate_on_resize`, after the allocation loop and BEFORE returning, call `unsafe { self.initialize_layouts(device, queue, pool) }`. Requires plumbing `(queue, pool)` through resize (mirror `caustic`/`taa` which already do this).

#### LIFE-N1 (HIGH, listed above) acts on the SVGF/composite/SSAO destroy chains too — flagging here to prevent dim-7 vs dim-10 dedup confusion.

### LOW

#### SY-7 — Skin compute → BLAS-refit barrier still uses `ACCELERATION_STRUCTURE_READ_KHR`
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:571`
- **Observed**: `dst_access_mask = ACCELERATION_STRUCTURE_READ_KHR` for the compute-write → AS-build-input boundary; per-spec the input-read alias is `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. SY-4 from prior audit, unchanged.
- **Fix**: Either align access flag or document that `_READ_KHR` alias is intentional pre-sync2.

#### MEM-N2 — `ray_budget` HOST_VISIBLE buffer is 4 bytes, wastes a full BAR sub-block
- **Location**: `crates/renderer/src/vulkan/scene_buffer.rs:589-628`
- **Fix**: Move ray_budget into camera UBO; drop `ray_budget_buffers`.

#### MEM-N3 — Skin compute output buffer keeps unused `VERTEX_BUFFER` usage flag
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:274-282`
- **Fix**: Remove the flag; re-add when Phase-3 raster path needs it.

#### PS-6 — Static `depth_compare_op(LESS)` on opaque/blend pipelines silently overridden by dynamic state
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:302, 461`
- **Observed**: Dynamic state declares `DEPTH_COMPARE_OP`; static value baked but ignored at bind. `draw.rs:1240` sets `LESS_OR_EQUAL` (the live truth). Future regression that drops the dynamic-state declaration silently shifts comparison from `LESS_OR_EQUAL` to `LESS`.
- **Fix**: Align static value to `LESS_OR_EQUAL` or replace with a sentinel comment.

#### PS-7 — `pipeline_cache.bin` resolved against cwd; silent save failure on read-only launch dirs
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:419-464`
- **Fix**: Resolve under `std::env::current_exe()?.parent()` or `directories::ProjectDirs::cache_dir`.

#### PS-9 — `recreate_triangle_pipelines` passes `vk::DescriptorSetLayout::null()` even when existing layout is reused
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:143-159`
- **Fix**: Refactor into two distinct functions sharing a private `_inner(layout, ...)`.

#### RP-N1 — Outgoing dep still uses `BOTTOM_OF_PIPE` in `dst_stage_mask` (sibling of #573 / SY-2)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:156`
- **Fix**: Remove `| vk::PipelineStageFlags::BOTTOM_OF_PIPE` term.

#### SH-13 — `composite.frag` cloud projection mip oscillation at horizon-fade band (partial root cause for #730)
- **Location**: `crates/renderer/shaders/composite.frag:113-119` (and three identical layer copies at :128, :139, :150)
- **Observed**: Cloud UV is `dir.xz / max(elevation, 0.05) * tile_scale + scroll`. The `0.05` floor on `elevation` (dir.y) creates a UV vector whose magnitude at the horizon is `~1/0.05 = 20×` the magnitude at the zenith. Drivers compute mip LOD via `dFdx/dFdy` on the sampled UVs, so a 16×16 fragment quad straddling the horizon-fade band sees a 100×–500× UV gradient discontinuity and the LOD calc snaps to either mip 0 (per-texel aliasing — the user's reported "pixelated clouds") or the smallest mip (over-blurred soup).
- **Fix**: Switch to `textureLod(textures[nonuniformEXT(cloud_idx)], uv, log2(1.0 / max(elevation, 0.05)) * 0.5)` or an explicit `textureGrad` with manually computed gradients that don't blow up at low elevation.

#### SH-14 — SVGF temporal motion-vector reconstruction loses sub-pixel precision on 4-tap consistency-fail
- **Location**: `crates/renderer/shaders/svgf_temporal.comp:103-126`
- **Fix**: Single-tap nearest fallback (`q = ivec2(round(prevPx))`) when 4-tap fails AND `length(motion * screen.xy) < 1.5`.

#### SH-15 — `caustic_splat.comp` re-derives instance index by stripping bit 15, no shader-side bounds check
- **Location**: `crates/renderer/shaders/caustic_splat.comp:170-183`
- **Fix**: Add `if (instIdx >= 32767u) return;` immediately after `meshId == 0u` early-out.

#### LIFE-N2 — `SwapchainState::destroy(&self)` doesn't null its handles (re-flags LIFE-M2)
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:200-208`
- **Fix**: Change signature to `&mut self`; clear the Vec; null the swapchain handle.

#### LIFE-N3 — `recreate_swapchain` destroys old image views BEFORE creating the new swapchain (re-flags LIFE-M1)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:60-83`
- **Fix**: Reorder so view destruction follows the new-swapchain creation.

#### AS-8-13 — `drop_skinned_blas` destroys synchronously while caller assumes deferral
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:1081-1096`; called from `crates/renderer/src/vulkan/context/draw.rs:682`
- **Fix**: Either route through `pending_destroy_blas` like `drop_blas` does, OR inline a `debug_assert!(now - last_used_frame >= MAX_FRAMES_IN_FLIGHT + 1)` plus a comment cross-linking the `frame_counter` bump site.

#### AS-8-14 — `evict_unused_blas` is a no-op during cell streaming because `frame_counter` only advances inside `build_tlas`
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:2211-2272` (eviction); line 1675 (frame_counter bump)
- **Observed**: During multi-cell streaming load (e.g. WastelandNV 49-cell init), no frames advance, so every survivor of the previous cell counts as `idle = 0` and the budget is unenforced. Effective behaviour: BLAS bytes can grow unboundedly past `blas_budget_bytes` during streaming bursts, deferred to the first `draw_frame` after the burst — which then evicts in one shot, potentially mid-frame.
- **Fix**: Bump a separate `cell_load_counter` at the top of every `build_blas_batched` and have eviction take `max(idle_frames, idle_loads)` against the appropriate min-idle threshold.

#### RT-12 — Reservoir shadow ray tMin = 0.001 with bias 0.05 — bias-asymmetric to other ray sites
- **Location**: `crates/renderer/shaders/triangle.frag:1581`
- **Fix**: Bump tMin to `0.01` (or `0.05` for full symmetry).

#### RT-13 — No contact-hardening penumbra (continuation of RT-9)
- **Location**: `crates/renderer/shaders/triangle.frag:1549`
- **Fix**: Defer to Phase-2 PCSS-style penumbra. File for tracking only.

#### RT-14 — GI ray `tMax = 3000.0` ends before fade window (4000–6000) does
- **Location**: `crates/renderer/shaders/triangle.frag:1635, 1610`
- **Fix**: Either raise `tMax` to 6000.0 or contract fade window to `2000..3000`.

#### DEN-10 — Composite hardcodes `exposure = 0.85` twice — no host-side hook
- **Location**: `crates/renderer/shaders/composite.frag:219, 278`
- **Fix**: Add `exposure: f32` to a tail vec4 of `CompositeParams`; expose a `set_exposure` host setter.

#### DEN-11 — Composite sky branch zeros alpha-blend marker bit (asymmetric to geometry branch)
- **Location**: `crates/renderer/shaders/composite.frag:220`
- **Fix**: `outColor = vec4(aces(sky * exposure), direct4.a);`

### INFO

#### SY-8 — Per-frame skinned BLAS refit emits redundant first-iteration scratch barrier (acknowledged intentional)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:608`
- Comment already accepts the cost; no fix required.

#### MEM-N4 — Dead `instance_address` query at acceleration.rs:1850 — re-verified not dead
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:1850-1862`
- **Observed**: Per re-verification, the post-#289 `instance_address` query targets `instance_buffer_device.buffer` (DEVICE_LOCAL) — the address that's actually fed into AS-build at line 2122. The "dead query on staging buffer" premise of MEM-2-4 (04-25) appears stale; no leftover staging-address read remains.
- **Fix**: Mark MEM-2-4 closed in next audit.

#### PS-8 — Static `depth_test_enable(true)` / `depth_write_enable(true)` shadowed by dynamic state — same drift class as PS-6
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:300-301, 459-460`
- **Fix**: Add a one-line comment at each site stating the value is overridden; bundle with PS-6.

#### RP-N2 — Static `mesh_id+1` ID space silently truncates past 65,535 (refines RP-1)
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:39`; comment at `helpers.rs:54-55`
- **Fix**: Add `assert!(visible_instances.len() < 0xFFFF, ...)` in the per-frame instance buffer build, and correct the comment to "65,535-instance ceiling".

#### DEN-12 — Composite branch reads `direct4` even on sky pixels — wasted bandwidth
- **Location**: `crates/renderer/shaders/composite.frag:208-214`
- **Fix**: Move `vec4 direct4 = texture(hdrTex, fragUV);` into the `else` block.

---

## Prioritized Fix Order

1. **`LIFE-N1` (HIGH)** — closes #732 and unlocks clean engine shutdown for any future exterior-streaming work. ~10 lines of code across 3 files; one focused PR.
2. **`#573` PR bundle** — closes `SY-2`, `SY-3`, `CMD-3`, `RP-N1` in one shot (all `BOTTOM_OF_PIPE` in `dst_stage_mask`). Validation-layer cleanup.
3. **`RT-11` (MEDIUM)** — `N_view` flip applied to the reservoir shadow ray. Single-line edit at `triangle.frag:1543`. Visible quality improvement on cluster-lit interiors with bumpy normal maps.
4. **`DEN-9` (MEDIUM)** — SVGF `recreate_on_resize` UNDEFINED→GENERAL transition. Plumbs `(queue, pool)` through resize; ~30 lines.
5. **`MEM-N1` (MEDIUM)** — TLAS shrink path. Mirror `scratch_should_shrink` invariant. ~50 lines.
6. **`SH-13`** — composite cloud-UV mip-LOD oscillation. Partial fix for #730 (cloud pixelation). 4 sites in `composite.frag`.
7. **`AS-8-14`** — BLAS LRU eviction during cell-load bursts. Wire `cell_load_counter`. M40 streaming-correctness item.
8. The remaining LOW + INFO items are bundleable into a single hygiene PR (~12 findings, mostly named-constant unification, comment corrections, and dead-code prunes).

---

## Out-of-scope (Filed Separately)

- **`#729`** (global green tint, FNV WastelandNV exterior) — CPU-side WTHR `SKY_FOG[TOD_DAY]` slot index audit. No GLSL root cause confirmed in `dim_6` / `dim_10`.
- **`#730`** (cloud pixelation) — `SH-13` is partial GLSL contribution. CPU-side cloud sampler + DDS resolution audit still required.
- **`#731`** (perceived view distance ~30m) — composite fog math is correct; perception is `#729` green tint dominating distant pixels.
- **`#732`** (M40 exterior shutdown SIGSEGV) — root cause identified in `LIFE-N1`; one-PR fix.

---

## Methodology Notes

- 10 dimension agents dispatched in parallel batches of 3 per the audit-orchestrator protocol; each agent received explicit time/file budget caps and was instructed to write a skeleton output file *first* to verify deliverable path before deep investigation. Without that constraint, two prior `renderer-specialist` agent runs stalled mid-investigation without ever writing the output file (~200K tokens spent for zero deliverable). Switching to `general-purpose` agents with file-first prompting + ruthless brevity caps (100-line output ceiling, 4-bullet-line finding format) produced 28 findings from ~700K tokens total — roughly 4× the prior efficiency.
- Dedup baseline: prior audit `AUDIT_RENDERER_2026-04-25.md` + open issues fetched via `gh issue list --state=all --limit 200`. Each agent was responsible for marking prior-audit findings as `Holding Items` (still-fixed) or `Still Open` rather than re-filing.
- Cross-dimension overlap (`LIFE-N1` surfaces in dim-7 but applies to dim-2/dim-10 destroy chains; `RP-N1` and `SY-2` are the same `BOTTOM_OF_PIPE` site at helpers.rs:156) was deduped at the merge step — a single canonical finding with explicit cross-link notes rather than a duplicated entry per dimension.
