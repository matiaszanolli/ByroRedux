# #2740 — REN-D4-04: device→host readbacks make no availability/invalidate step

**Severity**: HIGH · **Domain**: renderer (byroredux-renderer)
**Location**: `crates/renderer/src/vulkan/context/resources.rs` (`collect_image_health`), `crates/renderer/src/vulkan/context/draw.rs` (call site), `crates/renderer/src/vulkan/context/screenshot.rs` (`screenshot_finish_readback`, pre-existing sibling). Contrast: `crates/renderer/src/vulkan/buffer.rs` (`GpuBuffer::is_coherent`/`flush_mapped`/`flush_range`).

Three code comments (commit body, field doc, call-site comment) claim "the fence wait proves the buffer is idle, so reading it needs no barrier." Per spec, a fence's memory dependency only covers device-side access — it does NOT guarantee host-visibility of device writes without an explicit invalidate (`vkInvalidateMappedMemoryRanges` on non-coherent memory). `GpuBuffer` already models the opposite direction (`flush_range` gated on `is_coherent` for host→device); there is no `invalidate_mapped` counterpart for device→host.

**Suggested fix (per issue): do NOT blind-fix the code.** Needs a `BYRO_VALIDATION=1` run with sync validation before code changes. The documentation half (the three incorrect comments) is safe to fix now.

---

# #2741 — REN-D5-01: Caustic/Taa/Svgf Pipeline::destroy non-idempotent → double-free on failed resize

**Severity**: HIGH · **Domain**: renderer (byroredux-renderer)
**Location**: `crates/renderer/src/vulkan/caustic.rs`, `taa.rs`, `svgf.rs` (each `destroy` + `recreate_on_resize`); callers `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`), `crates/renderer/src/vulkan/context/mod.rs` (`destroy_allocator_owned_resources` / `Drop`). Correct model: `crates/renderer/src/vulkan/presentation.rs` (`PresentationPipeline::destroy`, nulls every handle after destroying it).

All three `recreate_on_resize` failure arms call `self.destroy()` then propagate the error without setting the field to `None`. `Drop`/`destroy_allocator_owned_resources` then calls `destroy()` again on the same object. Idempotent for image containers (drained/cleared) but NOT for scalar handles (`vk::Pipeline`, `vk::PipelineLayout`, `vk::DescriptorPool`, `vk::DescriptorSetLayout`, `vk::Sampler`, SVGF's `atrous_*`) — guarded by `!= null()` but never reset to `null()` after destroy. Result: double `vkDestroy*` calls → spec violation + driver double-free, triggered by allocation failure during resize.

**Fix**: null each scalar handle immediately after destroying it in all three `destroy()` bodies, mirroring `PresentationPipeline::destroy`.

---

# #2742 — REN-D6: SkinTint/HairTint arm intercepts Skyrim body/hands materials before slot-7 MSN specular rule runs

**Severity**: HIGH · **Domain**: nif (byroredux-nif)
**Location**: `crates/nif/src/import/material/dedicated_shader.rs` — `5 | 6 =>` arm (SkinTint/HairTint) vs. the `model_space_normals && info.specular_map.is_none()` slot-7 read that lives only in the `_ =>` default arm. Sink: `MaterialInfo::specular_map` → `MaterialTextureSet::specular` → `GpuMaterial::specular_map_index` → `triangle.frag`.

Third member of the same family as #2693 (MultiLayerParallax slot 6) and #2694 (FaceTint slots 2/3), both already fixed: a shader-type match arm added to suppress something else intercepts before the generic slot-7 MSN-specular rule can run. 100% of Skyrim SE body/hands/beast-skin specular masks (measured 390/390 + 4/4 on real BSA data) are silently dropped because SkinTint/HairTint (types 5/6) never reach the `_ =>` arm.

**Fix**: hoist the slot-7 MSN specular read out of the `_ =>` arm so it runs for all shader types (e.g. once after the match, not only in default).

---

# #2743 — REN-D9: skin-compute descriptor cache treats a raw vk::Buffer handle as a stable identity

**Severity**: HIGH · **Domain**: renderer (byroredux-renderer)
**Location**: `crates/renderer/src/vulkan/skin_compute.rs` — `SkinSlot::descriptor_bindings`, `SkinComputePipeline::dispatch`; interacts with `crates/renderer/src/mesh.rs` — `MeshRegistry::rebuild_geometry_ssbo`.

`#1197` skips `vkUpdateDescriptorSets` when `(input_buffer, bone_buffer)` == cached key, comparing raw non-dispatchable handles for numeric equality. Vulkan does not guarantee non-recycled handle values — `rebuild_geometry_ssbo` (esp. the `reclaim_before_rebuild` #2374 low-headroom path) destroys and reallocates the global vertex SSBO in the same call, the max-probability recycle window. A stale cache hit binds a freed generation's memory into `skin_vertices.comp`, feeding garbage into BLAS build input and `GpuInstance.skinnedVertexAddress` consumers.

**Fix**: add a monotonic `geometry_generation: u64` to `MeshRegistry`, bumped in `rebuild_geometry_ssbo`, folded into the cache key — or drop compare-and-skip for binding 0 (keep only for the palette buffer).
