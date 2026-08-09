# #2502: REN-D11-2026-08-07-05: G-buffer colour formats are never format-feature-queried, unlike depth

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2502  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/gbuffer.rs:39-72` (format consts) + `crates/renderer/src/vulkan/context/helpers.rs:22` (`find_depth_format` — the only `get_physical_device_format_properties` call in the crate)
**Status**: NEW

## Description
The depth format is chosen by querying `optimal_tiling_features` for `DEPTH_STENCIL_ATTACHMENT`. Every colour attachment format is a hard-coded const with no capability query and no fallback. Most are fine — `R16G16_SFLOAT`, `R32_UINT`, `R8_UNORM`, `B10G11R11_UFLOAT_PACK32` and `R16G16B16A16_SFLOAT` all carry mandatory `COLOR_ATTACHMENT` (and, where blended, `COLOR_ATTACHMENT_BLEND`) in the Vulkan mandatory-format table. The exception is `NORMAL_FORMAT = R16G16_SNORM`: 16-bit SNORM formats are mandatory only for `SAMPLED_IMAGE` / `SAMPLED_IMAGE_FILTER_LINEAR` / `BLIT_SRC` / `VERTEX_BUFFER`, **not** for `COLOR_ATTACHMENT`.

## Evidence
`grep -rn "get_physical_device_format_properties" crates/renderer/src/` returns exactly one hit (`helpers.rs:33`, inside `find_depth_format`). `gbuffer.rs::Attachment::allocate` creates the normal image with `COLOR_ATTACHMENT | SAMPLED` unconditionally.

## Impact
On a conformant device that does not expose `COLOR_ATTACHMENT` for `R16G16_SNORM`, `create_image` fails with `VK_ERROR_FORMAT_NOT_SUPPORTED` during `GBuffer::new` and the engine refuses to start with a generic "Failed to create gb_normal image". Loud, not silent — and no desktop driver in the target hardware class (RTX 4070 Ti dev GPU, and AMD/Intel desktop) actually lacks it. This is a portability / diagnostics gap, not a live defect.

## Related
#275 (introduced octahedral RG16_SNORM normals); REN-D4-NEW-02 (`AUDIT_RENDERER_2026-05-11_DIM4.md`) applied the same "query before you commit to a format" reasoning to depth only.

## Suggested Fix
Add a one-shot startup check that asserts `COLOR_ATTACHMENT` in `optimal_tiling_features` for each G-buffer colour format (plus `COLOR_ATTACHMENT_BLEND` for the four blended by the blend/water pipelines), failing with a format-naming error. A real fallback format for normals is not worth it; a precise error message is.

## Completeness Checks
- [ ] **TESTS**: A startup format-capability check is added and produces a named error on failure (manual verification — no non-conformant device in dev fleet)


---

# #2503: D12-2026-08-07-01: record_post_passes returns a Result that can never be Err -- the caller's recovery branch is dead code that contradicts the #2146 invariant

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2503  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::record_post_passes` (sig at :168, body :194-223); caller `crates/renderer/src/vulkan/context/draw.rs:2914-2943`
**Status**: NEW (pre-existing; predates the #2258 split — verified `7bb517b2^` was equally infallible)

## Description
`record_post_passes` calls eight `record_*_pass` helpers, all of which return `()`, then ends with an unconditional `Ok(())`. It is structurally incapable of returning `Err`. The caller nevertheless wraps it in a 30-line `if let Err(e) = ... { recreate_image_available_for_frame(); return Err(e); }` recovery block. That block is unreachable today — but it is exactly the escape hatch `#2146` warns must not exist. `record_upscale_pass`'s own doc says: "`record` is infallible on purpose. It runs after `svgf.dispatch`/`taa.dispatch` have latched `dispatched_this_frame`, so an error escaping to `draw_frame` would skip `queue_submit` *and* `mark_frame_completed`, leaving those latches set for a dispatch that never reached the GPU." Keeping the fallible signature means a contributor who adds a single `?` inside any of the eight new helpers silently activates that hazard with no compile-time or test signal.

## Evidence
```rust
// post_passes.rs:194-223 — no `?`, no fallible call
self.record_svgf_pass(cmd, frame);
... self.record_presentation_pass(cmd, frame, img, underwater, image_space_modifier);
Ok(())
```
vs `draw.rs:2914` `if let Err(e) = self.record_post_passes(...) { ... return Err(e); }`

## Impact
No runtime effect today. Latent: a future fallible call between the SVGF/TAA `dispatched_this_frame` latch and `queue_submit` would bail the frame with the latches set, so `mark_frame_completed` never runs and the next frame assumes temporal history the GPU never wrote (ghosting / stale-history artifacts) — the precise failure #2146 documented. Blast radius: the whole post chain.

## Related
#2146 (`FrameUpscaler::record` infallibility contract), #2258 (`7bb517b2` per-pass split), #917 / REN-D10-NEW-03 (`mark_frame_completed` moved to post-submit).

## Suggested Fix
Change `record_post_passes` to return `()` and delete the caller's recovery branch, so any future fallible call is a compile error at the point of introduction rather than a silent semantic change; carry the #2146 rationale onto the new signature as a doc comment.

## Completeness Checks
- [ ] **TESTS**: Compile-time check: any future fallible call inside the eight helpers now fails to compile rather than silently changing semantics


---

# #2504: D12-2026-08-07-02: upload_indirect_draws failure is warn-swallowed, but the draw loop still executes cmd_draw_indexed_indirect over the un-updated buffer

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2504  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2672-2685` (upload site) → `crates/renderer/src/vulkan/context/geometry_pass.rs:408-439` (`cmd_draw_indexed_indirect`)
**Status**: NEW

## Description
The indirect-command upload uses the same `unwrap_or_else(|e| log::warn!(...))` soft-fail pattern as the neighbouring `upload_instances` / `upload_materials` / `upload_previous_models` calls. For data SSBOs that is correct — a stale or zero buffer only misrenders. For the **indirect** buffer it is qualitatively different: `index_count` / `first_index` / `vertex_offset` / `first_instance` are *fetched and executed by the GPU*. On a failed upload the draw loop still issues `cmd_draw_indexed_indirect(indirect_buffer, i*stride, group_size, stride)` sized from **this** frame's `batches`, reading commands that belong to a previous frame's global-geometry layout (or, on the first use of a FIF slot, never-written host-visible memory). `upload_indirect_draws` correctly declines to stamp `last_uploaded_indirect_hash` on failure, so the *next* frame re-uploads — but the current frame has already recorded the draw.

## Evidence
```rust
// draw.rs:2682
self.scene_buffers.upload_indirect_draws(&self.device, frame, indirect_scratch)
    .unwrap_or_else(|e| log::warn!("Failed to upload indirect draws: {e}"));
```
no flag is set, and `geometry_pass.rs:428` unconditionally issues the indirect draw whenever `use_indirect` (`global_bound && multi_draw_indirect_supported`).

## Impact
Requires `mapped_slice_mut()` or `flush_range()` to fail (rare — host-visible, persistently mapped). When it does: stale `first_index`/`vertex_offset` after a `rebuild_geometry_ssbo` shrink is an out-of-range index fetch; uninitialised memory on a slot's first frame yields arbitrary `index_count`/`instance_count`. Both are GPU page-fault / TDR class, i.e. the failure mode is much louder than the warn suggests.

## Related
#309 (indirect path), #1809 (`upload_indirect_draws` dirty gate), #1587 (partial flush), #2215 (open indirect-grouping regression).

## Suggested Fix
Have the upload set a per-frame `indirect_upload_ok` flag (or return the `Result` up to `draw_frame`) and force the direct-draw fallback for that frame when it is false — `dispatch_direct` already handles every batch correctly.

## Completeness Checks
- [ ] **TESTS**: A regression test simulates an upload failure and confirms the frame falls back to direct draws instead of issuing stale indirect commands


---

# #2505: D12-2026-08-07-03: Four expect() panics recorded inside the open render pass (water + UI vertex/index buffers) violate the #956 no-panic-while-recording rule

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2505  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs:529-536` (water vb/ib) and `:620-627` (UI vb/ib)
**Status**: NEW

## Description
Between `cmd_begin_render_pass` (`:36`) and `cmd_end_render_pass` (`:637`), the water and UI draw paths use `.expect("water mesh requires a per-mesh vertex buffer")` / `"UI mesh requires a per-mesh vertex buffer"` on `mesh.vertex_buffer` / `mesh.index_buffer`. `#956` / REN-D5-NEW-05 established the opposite house rule for this exact region (a `debug_assert!` was removed from the instance-overflow site because "it leaks the in-flight cmd buffer on unwind"), and the sibling `dispatch_direct` closure twenty lines above handles the identical `None` case gracefully: "A global-only scene mesh (distant terrain LOD, #1370) carries no per-mesh buffers — skip it".

## Evidence
`mesh.rs::upload_scene_mesh_global_only` (`:526-544`) produces `vertex_buffer: None`. Its callers today are LOD-only (`placement_lod.rs:485`, `object_lod.rs:295`, `terrain_lod.rs:657`, `terrain_lod_btr.rs:202`), so water/UI meshes always take the per-mesh path and the panic is unreachable **right now**. The precondition is a call-site convention, not a type-level guarantee.

## Impact
If any future path ever registers a water plane or UI quad global-only (e.g. a WATAL water-LOD tier), the panic unwinds with `cmd` mid-render-pass, `image_available[frame]` signal-pending, and `images_in_flight[img]` already pointing at this frame's fence — the leak class the six explicit `recreate_image_available_for_frame` recovery sites in `draw_frame` exist to prevent.

## Related
#956 / REN-D5-NEW-05, #1370 (global-only meshes), #910 / REN-D5-NEW-01, #1188 / REN-D1-NEW-05.

## Suggested Fix
Replace the four `expect()`s with `let ... else { continue; }` (water loop) / `else { /* skip overlay */ }` (UI block), mirroring `dispatch_direct`'s existing graceful skip, plus a one-line `log::warn!` gated by a `Once`.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a global-only water/UI mesh and confirms the draw path skips gracefully instead of panicking


---
