//! VulkanContext resource management methods (BLAS, UI quad, extent, memory).

use super::VulkanContext;
use anyhow::Result;

use crate::vulkan::scene_buffer::GpuTerrainTile;

/// Free-function core of `fill_terrain_tile_scratch_if_dirty` — lifted
/// out of the `VulkanContext` method so unit tests can exercise it
/// without standing up a full Vulkan device. When `*dirty` is set,
/// clears `dest` (preserving capacity), refills it from `tiles`, and
/// clears the flag. Returns `true` when the caller should perform the
/// GPU upload. See #496 / #497.
pub(super) fn fill_terrain_tiles(
    tiles: &[Option<GpuTerrainTile>],
    dirty: &mut bool,
    dest: &mut Vec<GpuTerrainTile>,
) -> bool {
    if !*dirty {
        return false;
    }
    *dirty = false;
    dest.clear();
    dest.extend(tiles.iter().map(|t| t.unwrap_or_default()));
    true
}

/// Free-function core of `VulkanContext::free_terrain_tile` — Vulkan-free
/// so unit tests can exercise the state transition. Releases `slot` back
/// to `free_list`, clears the corresponding `tiles` entry, sets `*dirty`,
/// and returns the previous tile so the caller can walk every owned
/// texture index and
/// release the per-layer texture refcounts they bumped through
/// `acquire_by_path` at allocation time. Returns `None` when the slot
/// index is out of range or already vacant. See #627.
pub(super) fn release_terrain_tile_slot(
    tiles: &mut [Option<GpuTerrainTile>],
    free_list: &mut Vec<u32>,
    dirty: &mut bool,
    slot: u32,
) -> Option<GpuTerrainTile> {
    let idx = slot as usize;
    if idx >= tiles.len() {
        return None;
    }
    let tile = tiles[idx].take()?;
    free_list.push(slot);
    *dirty = true;
    Some(tile)
}

impl VulkanContext {
    /// Allocate a terrain tile slot and store its 8 bindless texture
    /// indices. Returns the slot index (0..`MAX_TERRAIN_TILES`) that
    /// the caller packs into the top 16 bits of `GpuInstance.flags`
    /// alongside `INSTANCE_FLAG_TERRAIN_SPLAT`. Returns `None` when the
    /// registry is full — caller falls back to the single-texture
    /// path. See #470.
    pub fn allocate_terrain_tile(&mut self, tile: GpuTerrainTile) -> Option<u32> {
        let slot = self.terrain_tile_free_list.pop()?;
        let idx = slot as usize;
        debug_assert!(idx < self.terrain_tiles.len());
        self.terrain_tiles[idx] = Some(tile);
        self.terrain_tiles_dirty = true;
        Some(slot)
    }

    /// Release a terrain tile slot back to the free list and schedule
    /// the SSBO to be reuploaded to every frame-in-flight. Must be
    /// called from `unload_cell` before the mesh / BLAS drop so a late
    /// frame-in-flight reads stale-but-valid data rather than
    /// undefined.
    ///
    /// Returns the previous slot so the
    /// caller can issue symmetric `drop_texture` calls on the refcounts
    /// that `resolve_texture` bumped at allocation time. Returns `None`
    /// when the slot is out of range or already vacant. See #627.
    pub fn free_terrain_tile(&mut self, slot: u32) -> Option<GpuTerrainTile> {
        release_terrain_tile_slot(
            &mut self.terrain_tiles,
            &mut self.terrain_tile_free_list,
            &mut self.terrain_tiles_dirty,
            slot,
        )
    }

    /// Read and reset this frame slot's image-health counters (EX-05 / #2736).
    ///
    /// Must be called only when `frame`'s in-flight fence has been waited on,
    /// which is what makes the host read of a device-written buffer safe
    /// without a barrier. `draw_frame` calls it immediately after that wait.
    ///
    /// Counters are *accumulated* into a running total as well as latched as
    /// the last-frame value, because a NaN is frequently transient — it
    /// appears for the frames a bad material or a degenerate light is on
    /// screen and then goes. A gate that only sampled the current frame would
    /// miss it; the total is what makes the smoke check reliable.
    ///
    /// #2793 (REN-D5-05) described the visibility gaps here; #2752
    /// (REN-D4-05) closes them, and the two halves had to move together.
    ///
    /// This reads GPU-written data through `mapped_slice_mut()` and then
    /// writes the reset-to-zero bytes back through the same mapping. Both
    /// directions need an explicit step on non-coherent memory: an
    /// **invalidate** before the read (a fence proves the submission
    /// completed, but its memory-dependency access scope covers device
    /// access only — #2740 / REN-D4-04), and a **flush** after the zeroing so
    /// the next frame's shader `atomicAdd` doesn't accumulate onto a stale
    /// line.
    ///
    /// Both were previously absent, benign only because the buffer was a
    /// `CpuToGpu` allocation whose gpu-allocator preset *requires*
    /// `HOST_COHERENT`. That is a property of one allocator version, not a
    /// spec guarantee — and #2752 deliberately moved this buffer to
    /// `GpuToCpu` (which merely *prefers* `HOST_CACHED`), so the coincidence
    /// no longer covers it. Both calls are no-ops on a coherent allocation.
    pub(super) fn collect_image_health(&mut self, frame: usize) {
        // Split-borrow: `invalidate_if_needed`/`flush_if_needed` take the
        // device, which lives on `self` alongside the buffer vec.
        let device = self.device.clone();
        let Some(buffer) = self.image_health_buffers.get_mut(frame) else {
            return;
        };
        // Before the read. A failure here means the counters may be stale, so
        // the honest move is to skip this frame's sample rather than fold a
        // possibly-stale value into the running total the smoke gate asserts on.
        if let Err(e) = buffer.invalidate_if_needed(&device) {
            log::warn!(
                "image-health readback invalidate failed: {e} — skipping this frame's sample"
            );
            return;
        }
        let Ok(bytes) = buffer.mapped_slice_mut() else {
            return;
        };
        if bytes.len() < 8 {
            return;
        }
        let rgb = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let alpha = u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        bytes[..8].fill(0);
        // After the zeroing write, so the shader's next `atomicAdd` starts
        // from zero rather than a dirty host cache line.
        if let Err(e) = buffer.flush_if_needed(&device) {
            log::warn!("image-health counter reset flush failed: {e}");
        }

        self.image_health_last = (rgb, alpha);
        self.image_health_total.0 = self.image_health_total.0.saturating_add(u64::from(rgb));
        self.image_health_total.1 = self.image_health_total.1.saturating_add(u64::from(alpha));
        if rgb != 0 || alpha != 0 {
            log::warn!(
                "image health: {rgb} non-finite RGB pixel(s), {alpha} non-finite alpha pixel(s) \
                 in the pre-tonemap scene (frame slot {frame}); running total {}/{}",
                self.image_health_total.0,
                self.image_health_total.1,
            );
        }
    }

    /// `(last frame, running total)` non-finite pre-tonemap pixel counts as
    /// `((rgb, alpha), (rgb, alpha))`. Surfaced by `r.health` and the bench
    /// summary so the exterior smoke gate can hard-fail on a non-zero total.
    pub fn image_health(&self) -> ((u32, u32), (u64, u64)) {
        (self.image_health_last, self.image_health_total)
    }

    /// Number of occupied terrain tile slots.
    ///
    /// The backing `Vec` is fixed at `MAX_TERRAIN_TILES`, so only the occupied
    /// count carries ownership meaning. Each occupied slot holds 8 layer
    /// texture refcounts that `free_terrain_tile` hands back (#627), which is
    /// why the EX-08 soak (#2374) tracks it as an exact-return class: a
    /// surplus here is also a surplus of leaked texture references.
    pub fn occupied_terrain_tile_count(&self) -> usize {
        self.terrain_tiles.iter().filter(|t| t.is_some()).count()
    }

    /// Populate `dest` with the current terrain tile slab, filling
    /// empty slots with the zero-tile default so the fragment shader's
    /// `if (layerIdx == 0u) continue;` guard skips them. Returns `true`
    /// when an upload is due + decrements the dirty-frame counter.
    ///
    /// Accepts `dest` by `&mut` rather than returning a slice from
    /// `self` so `draw_frame` can hold `&self.device` + `&mut
    /// self.scene_buffers` while consuming the staged data. The
    /// caller owns a persistent `terrain_tile_scratch` Vec whose
    /// capacity amortizes across frames — same pattern as
    /// `gpu_instances_scratch`. See #496 / #470.
    pub(super) fn fill_terrain_tile_scratch_if_dirty(
        &mut self,
        dest: &mut Vec<GpuTerrainTile>,
    ) -> bool {
        fill_terrain_tiles(&self.terrain_tiles, &mut self.terrain_tiles_dirty, dest)
    }
    /// Build a BLAS for a mesh (RT only). Call after uploading a mesh.
    pub fn build_blas_for_mesh(&mut self, mesh_handle: u32, vertex_count: u32, index_count: u32) {
        let Some(ref mut accel) = self.accel_manager else {
            return;
        };
        let Some(mesh) = self.mesh_registry.get(mesh_handle) else {
            return;
        };
        let allocator = self.allocator.as_ref().expect("allocator missing");
        if let Err(e) = accel.build_blas(
            crate::vulkan::GpuUploadCtx {
                device: &self.device,
                allocator,
                queue: &self.graphics_queue,
                command_pool: self.transfer_pool,
            },
            Some(&self.transfer_fence),
            mesh_handle,
            mesh,
            vertex_count,
            index_count,
        ) {
            log::warn!("BLAS build failed for mesh {}: {e}", mesh_handle);
        }
    }

    /// Build BLAS for multiple meshes in a single GPU submission.
    ///
    /// Call this after uploading all meshes during scene/cell load to
    /// avoid the per-mesh fence stall of `build_blas_for_mesh`. Returns
    /// the number of BLAS successfully built.
    pub fn build_blas_batched(&mut self, mesh_specs: &[(u32, u32, u32)]) -> usize {
        let Some(ref mut accel) = self.accel_manager else {
            return 0;
        };
        let allocator = self.allocator.as_ref().expect("allocator missing");

        // Gather raw sources for the batch — ordinary meshes use their
        // dedicated RT-capable buffers at byte offset zero.
        let meshes: Vec<crate::vulkan::acceleration::BlasBuildSource> = mesh_specs
            .iter()
            .filter_map(|&(handle, vc, ic)| {
                let mesh = self.mesh_registry.get(handle)?;
                if !mesh.rt_capable {
                    return None;
                }
                Some(crate::vulkan::acceleration::BlasBuildSource {
                    mesh_handle: handle,
                    vertex_buffer: mesh.vertex_buffer.as_ref()?.buffer,
                    index_buffer: mesh.index_buffer.as_ref()?.buffer,
                    vertex_byte_offset: 0,
                    index_byte_offset: 0,
                    vertex_count: vc,
                    index_count: ic,
                })
            })
            .collect();

        match accel.build_blas_batched(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            Some(&self.transfer_fence),
            &meshes,
        ) {
            Ok(count) => count,
            Err(e) => {
                log::warn!("Batched BLAS build failed: {e}");
                0
            }
        }
    }

    /// Build the nearby global-only LOD meshes selected for this frame's TLAS.
    ///
    /// Sources are byte-offset subranges of the global buffers, so exterior
    /// terrain/object LOD gains structure shadows without restoring per-block
    /// buffer allocations or upload fence waits. Existing BLAS are filtered
    /// before batching; the per-frame scan also catches an already-resident
    /// LOD block the moment camera motion brings it into shadow range.
    pub fn build_global_blas_for_draws(&mut self, draw_commands: &[super::DrawCommand]) -> usize {
        let (Some(global_vb), Some(global_ib), Some(accel)) = (
            self.mesh_registry.global_vertex_buffer.as_ref(),
            self.mesh_registry.global_index_buffer.as_ref(),
            self.accel_manager.as_ref(),
        ) else {
            return 0;
        };

        let mut handles: Vec<u32> = draw_commands
            .iter()
            .filter_map(|cmd| {
                if !cmd.in_tlas || accel.has_blas(cmd.mesh_handle) {
                    return None;
                }
                let mesh = self.mesh_registry.get(cmd.mesh_handle)?;
                (mesh.vertex_buffer.is_none() && mesh.index_buffer.is_none())
                    .then_some(cmd.mesh_handle)
            })
            .collect();
        handles.sort_unstable();
        handles.dedup();

        let vertex_stride = std::mem::size_of::<crate::Vertex>() as u64;
        let index_stride = std::mem::size_of::<u32>() as u64;
        let sources: Vec<crate::vulkan::acceleration::BlasBuildSource> = handles
            .into_iter()
            .filter_map(|handle| {
                let mesh = self.mesh_registry.get(handle)?;
                Some(crate::vulkan::acceleration::BlasBuildSource {
                    mesh_handle: handle,
                    vertex_buffer: global_vb.buffer,
                    index_buffer: global_ib.buffer,
                    vertex_byte_offset: u64::from(mesh.global_vertex_offset) * vertex_stride,
                    index_byte_offset: u64::from(mesh.global_index_offset) * index_stride,
                    vertex_count: mesh.vertex_count,
                    index_count: mesh.index_count,
                })
            })
            .collect();

        if sources.is_empty() {
            return 0;
        }
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let accel = self
            .accel_manager
            .as_mut()
            .expect("checked acceleration manager above");
        match accel.build_blas_batched(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            Some(&self.transfer_fence),
            &sources,
        ) {
            Ok(count) => {
                log::debug!("Built {count} global-buffer LOD shadow BLAS");
                count
            }
            Err(e) => {
                log::warn!("Global-buffer LOD BLAS batch failed: {e}");
                0
            }
        }
    }

    /// Register the fullscreen quad mesh for UI overlay rendering.
    /// Call this once after creating the context.
    pub fn register_ui_quad(&mut self) -> Result<()> {
        let (vertices, indices) = crate::mesh::fullscreen_quad_ui_vertices();
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let handle = self.mesh_registry.upload(
            crate::vulkan::GpuUploadCtx {
                device: &self.device,
                allocator,
                queue: &self.graphics_queue,
                command_pool: self.transfer_pool,
            },
            &vertices,
            &indices,
            false, // UI quad doesn't need RT
            None,
        )?;
        self.ui_quad_handle = Some(handle);
        log::info!("UI fullscreen quad registered (mesh handle {})", handle);
        Ok(())
    }

    /// Register the unit XY quad used by the CPU particle billboard path
    /// (#401). Pushed per-particle by `build_render_data` with a precomputed
    /// face-camera rotation in the model matrix. RT is skipped because
    /// particles are screen-space alpha-blend overlays, not world geometry
    /// that needs to participate in shadow / GI ray queries.
    pub fn register_particle_quad(&mut self) -> Result<()> {
        let (vertices, indices) = crate::mesh::quad_vertices();
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let handle = self.mesh_registry.upload(
            crate::vulkan::GpuUploadCtx {
                device: &self.device,
                allocator,
                queue: &self.graphics_queue,
                command_pool: self.transfer_pool,
            },
            &vertices,
            &indices,
            false, // particles skip TLAS
            None,
        )?;
        self.particle_quad_handle = Some(handle);
        log::info!(
            "Particle billboard quad registered (mesh handle {})",
            handle
        );
        Ok(())
    }

    /// Get the current swapchain extent (viewport dimensions).
    pub fn swapchain_extent(&self) -> (u32, u32) {
        (
            self.swapchain_state.extent.width,
            self.swapchain_state.extent.height,
        )
    }

    /// Get the scene-render extent. This differs from the swapchain extent
    /// when an FSR quality preset renders below output resolution.
    pub fn render_extent(&self) -> (u32, u32) {
        (
            self.frame_extents.render.width,
            self.frame_extents.render.height,
        )
    }

    /// Log current GPU memory allocation statistics. Threshold for the
    /// "high usage" WARN scales with the physical device's smallest
    /// DEVICE_LOCAL heap — see #505.
    pub fn log_memory_usage(&self) {
        if let Some(ref alloc) = self.allocator {
            super::super::allocator::log_memory_usage(alloc, &self.instance, self.physical_device);
        }
    }

    /// Compute a per-block fragmentation report off the live allocator.
    /// Explicit-call only — never wire into a per-frame path. Returns
    /// formatted lines so the same data can flow to the log
    /// (engine-init / debug shortcut) and to the `mem.frag` console
    /// command output. Empty when the allocator hasn't been
    /// initialised. See #503 / `AUDIT_PERFORMANCE_2026-04-20.md`
    /// finding D2-L1.
    pub fn fragmentation_report_lines(&self) -> Vec<String> {
        let Some(ref alloc) = self.allocator else {
            return Vec::new();
        };
        let report = alloc
            .lock()
            .expect("allocator lock poisoned")
            .generate_report();
        let frags = super::super::allocator::compute_block_fragmentation(&report);
        super::super::allocator::fragmentation_report_lines(&frags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::scene_buffer::MAX_TERRAIN_TILES;

    /// Regression for #496 / #497: the fill helper must reuse the
    /// caller's scratch buffer capacity across repeated dirty refills.
    /// Pre-#496 `drain_terrain_tile_uploads` allocated a fresh 32 KB Vec
    /// per call. Since #497 the dirty signal is a single bool (the
    /// DEVICE_LOCAL SSBO needs exactly one upload per cell transition),
    /// so the capacity reuse is verified by toggling the flag back on
    /// manually after each consumption.
    #[test]
    fn fill_reuses_scratch_capacity_across_dirty_refills() {
        let mut tiles: Vec<Option<GpuTerrainTile>> = vec![None; MAX_TERRAIN_TILES];
        tiles[0] = Some(GpuTerrainTile {
            layer_diffuse_index: [1, 2, 3, 4, 5, 6, 7, 8],
            layer_normal_index: [11, 12, 13, 14, 15, 16, 17, 18],
            layer_specular_index: [21, 22, 23, 24, 25, 26, 27, 28],
        });
        let mut dest: Vec<GpuTerrainTile> = Vec::new();
        let mut dirty = true;

        // First call — allocates the Vec.
        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        let cap_after_first = dest.capacity();
        assert!(cap_after_first >= MAX_TERRAIN_TILES);
        assert_eq!(dest.len(), MAX_TERRAIN_TILES);
        assert_eq!(dest[0].layer_diffuse_index, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(dest[0].layer_normal_index, [11, 12, 13, 14, 15, 16, 17, 18]);
        assert_eq!(
            dest[0].layer_specular_index,
            [21, 22, 23, 24, 25, 26, 27, 28]
        );
        assert!(!dirty);

        // Subsequent refills MUST NOT grow capacity — clear + extend
        // reuses the buffer. This is the whole point of the refactor.
        dirty = true;
        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert_eq!(dest.capacity(), cap_after_first);
        assert!(!dirty);

        dirty = true;
        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert_eq!(dest.capacity(), cap_after_first);
        assert!(!dirty);
    }

    /// Clean flag short-circuits — no fill, no work.
    #[test]
    fn fill_noop_when_not_dirty() {
        let tiles: Vec<Option<GpuTerrainTile>> = vec![None; MAX_TERRAIN_TILES];
        let mut dest: Vec<GpuTerrainTile> = Vec::with_capacity(16);
        let cap_before = dest.capacity();
        let mut dirty = false;

        assert!(!fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert!(!dirty);
        // Scratch buffer untouched — capacity preserved, len unchanged.
        assert!(dest.is_empty());
        assert_eq!(dest.capacity(), cap_before);
    }

    /// Empty slots render as the zero tile so the fragment-shader guard
    /// `if (layerIdx == 0u) continue;` skips them.
    #[test]
    fn empty_slots_fill_with_zero_default() {
        let tiles: Vec<Option<GpuTerrainTile>> = vec![None; 4];
        let mut dest: Vec<GpuTerrainTile> = Vec::new();
        let mut dirty = true;

        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert_eq!(dest.len(), 4);
        for tile in &dest {
            assert_eq!(tile.layer_diffuse_index, [0; 8]);
            assert_eq!(tile.layer_normal_index, [0; 8]);
            assert_eq!(tile.layer_specular_index, [0; 8]);
        }
    }

    /// Regression for #627 — releasing a populated slot must surface
    /// the previous layer indices so `unload_cell` can drop the
    /// per-layer texture refcounts that `resolve_texture` bumped at
    /// allocation time. Pre-fix the function returned `()` and the
    /// indices were silently lost, leaking ~150 refcounts per 7×7
    /// WastelandNV reload.
    #[test]
    fn release_returns_previous_layer_indices_and_clears_slot() {
        let mut tiles: Vec<Option<GpuTerrainTile>> = vec![None; 4];
        tiles[2] = Some(GpuTerrainTile {
            layer_diffuse_index: [11, 22, 33, 44, 55, 66, 77, 88],
            layer_normal_index: [111, 122, 133, 144, 155, 166, 177, 188],
            layer_specular_index: [211, 222, 233, 244, 255, 266, 277, 288],
        });
        let mut free_list: Vec<u32> = vec![0, 1, 3];
        let mut dirty = false;

        let released = release_terrain_tile_slot(&mut tiles, &mut free_list, &mut dirty, 2);

        let released = released.expect("populated tile");
        assert_eq!(
            released.layer_diffuse_index,
            [11, 22, 33, 44, 55, 66, 77, 88]
        );
        assert_eq!(
            released.layer_normal_index,
            [111, 122, 133, 144, 155, 166, 177, 188]
        );
        assert_eq!(
            released.layer_specular_index,
            [211, 222, 233, 244, 255, 266, 277, 288]
        );
        assert!(tiles[2].is_none(), "slot must be vacated after release");
        assert_eq!(free_list, vec![0, 1, 3, 2], "slot returned to free list");
        assert!(dirty, "release schedules SSBO refresh");
    }

    /// Releasing an already-vacant slot must be a no-op — no double
    /// `drop_texture` calls (which would underflow refcount), no
    /// duplicate free-list entry, no spurious dirty-flag.
    #[test]
    fn release_vacant_slot_is_noop() {
        let mut tiles: Vec<Option<GpuTerrainTile>> = vec![None; 4];
        let mut free_list: Vec<u32> = vec![0, 1, 2, 3];
        let mut dirty = false;

        let released = release_terrain_tile_slot(&mut tiles, &mut free_list, &mut dirty, 1);

        assert_eq!(released, None);
        assert_eq!(free_list, vec![0, 1, 2, 3], "no double-free");
        assert!(!dirty, "no SSBO refresh for vacant release");
    }

    /// Releasing an out-of-range slot must be a no-op — guards against
    /// a corrupt `TerrainTileSlot` ECS component or stale slot ID.
    #[test]
    fn release_out_of_range_slot_is_noop() {
        let mut tiles: Vec<Option<GpuTerrainTile>> = vec![None; 4];
        let mut free_list: Vec<u32> = Vec::new();
        let mut dirty = false;

        let released = release_terrain_tile_slot(&mut tiles, &mut free_list, &mut dirty, 99);

        assert_eq!(released, None);
        assert!(
            free_list.is_empty(),
            "out-of-range slot must not pollute free list"
        );
        assert!(!dirty);
    }

    /// #2740 (REN-D4-04) — a fence's memory dependency only covers
    /// *device*-side access; it does not by itself guarantee a device
    /// write is host-visible (that additionally needs a coherent memory
    /// type or an explicit `vkInvalidateMappedMemoryRanges`). Three
    /// comments around the image-health readback used to claim the fence
    /// wait alone was sufficient, which the spec explicitly denies. This
    /// pins that the two still-editable sites (the `image_health_buffers`
    /// field doc and the `collect_image_health` call site — the third,
    /// `collect_image_health`'s own doc comment, was already corrected
    /// under #2793) no longer make that claim, and that the corrected
    /// (coherent-allocator-specific) reasoning is present in all three.
    /// Source-scan only, no device needed.
    #[test]
    fn image_health_docs_no_longer_claim_fence_alone_proves_host_visibility() {
        // Scope resources.rs's own source to the production half (before
        // `mod tests`) so this test's own explanatory strings — which
        // necessarily name the retired claim to describe what it checks for
        // — don't trip the self-scan.
        let resources_src_full = include_str!("resources.rs");
        let resources_src = &resources_src_full[..resources_src_full
            .find("mod tests {")
            .expect("resources.rs must have a `mod tests` block")];
        let mod_src = include_str!("mod.rs");
        let draw_src = include_str!("draw.rs");

        // Build the two retired-claim needles from parts so this file's own
        // source doesn't contain them as a literal (defeating the point of
        // the check once this test itself is included via include_str!).
        let idle_claim = ["provably", "idle"].join(" ");
        let barrier_claim = [
            "needs no barrier, no transfer",
            "and no extra synchronisation.",
        ]
        .join(" ");

        for (label, src) in [
            ("resources.rs (collect_image_health doc)", resources_src),
            ("mod.rs (image_health_buffers field doc)", mod_src),
            ("draw.rs (collect_image_health call site)", draw_src),
        ] {
            assert!(
                !src.contains(&idle_claim),
                "{label} still claims the fence wait alone makes the buffer \
                 \"provably idle\" for a host read — that's the incorrect \
                 claim #2740 corrects (a fence's access scope is device-side \
                 only)"
            );
            assert!(
                !src.contains(&barrier_claim),
                "{label} still claims a fence wait needs no barrier for a \
                 host read of device-written memory — #2740"
            );
        }

        // And the corrected reasoning must actually be present, not just
        // the wrong claim removed.
        assert!(
            resources_src.contains("HOST_COHERENT"),
            "collect_image_health's doc must explain the coherent-allocator \
             reasoning that actually makes the host read safe (#2793)"
        );
        assert!(
            mod_src.contains("HOST_COHERENT") || mod_src.contains("collect_image_health"),
            "image_health_buffers field doc must point to the corrected \
             explanation (#2740)"
        );
    }

    /// #2752 (REN-D4-05) — the readback path's two halves must stay together.
    /// #2740 said not to blind-fix the location, because switching to
    /// `GpuToCpu` (which prefers `HOST_CACHED`) is exactly what turns a
    /// missing invalidate from theoretical into observable. So the allocation
    /// change and the invalidate call are one change, and this pins that
    /// neither can be removed without the other.
    ///
    /// Source-scan for the same reason as the sibling test above: on the dev
    /// card gpu-allocator resolves both presets to coherent memory, so
    /// `is_coherent` is true and the invalidate is a no-op — there is no
    /// device-free way to observe the difference. What this *can* catch is
    /// the realistic regression: the invalidate being deleted as dead code,
    /// or the buffer drifting back to the upload constructor.
    #[test]
    fn image_health_readback_allocates_gputocpu_and_invalidates_before_reading() {
        let resources_src_full = include_str!("resources.rs");
        let production = &resources_src_full[..resources_src_full
            .find("mod tests {")
            .expect("resources.rs must have a `mod tests` block")];
        let mod_src = include_str!("mod.rs");

        assert!(
            mod_src.contains("GpuBuffer::create_host_readback("),
            "image_health_buffers must be allocated through the readback \
             constructor — `create_host_visible` is CpuToGpu, an upload \
             preset, on a buffer the host drains every frame (#2752)"
        );

        // Scope to the function BODY — the doc comment above it necessarily
        // names both `mapped_slice_mut` and the invalidate while explaining
        // the ordering, which would make a whole-file scan self-satisfying.
        let body_start = production
            .find("pub(super) fn collect_image_health(")
            .expect("collect_image_health must still exist");
        let body = &production[body_start..];
        let body = &body[..body
            .find("\n    /// ")
            .expect("collect_image_health must be followed by another doc-commented item")];

        let invalidate = body
            .find("invalidate_if_needed")
            .expect("collect_image_health must invalidate before reading (#2752)");
        let read = body
            .find("mapped_slice_mut")
            .expect("collect_image_health must still read through mapped_slice_mut");
        assert!(
            invalidate < read,
            "the invalidate must precede the read — after it, the host may \
             already have consumed a stale cache line"
        );

        // The zeroing write that follows the read needs the other half of the
        // pair, or the next frame's atomicAdd accumulates onto a dirty line.
        assert!(
            body[read..].contains("flush_if_needed"),
            "the counter reset written through the same mapping must be \
             flushed (#2793's write-side half)"
        );
    }

    /// The screenshot staging buffer has always been `GpuToCpu`, so it was
    /// the site most exposed to the missing-invalidate gap even before #2752
    /// moved the image-health buffers there. A stale read hands a torn or
    /// previous-frame image to the golden-frame comparison — the one consumer
    /// that cannot tell the difference.
    #[test]
    fn screenshot_readback_invalidates_before_mapping() {
        let src = include_str!("screenshot.rs");
        let invalidate = src
            .find("invalidate_mapped_memory_ranges")
            .expect("screenshot readback must invalidate (#2752 sibling)");
        let read = src
            .find("allocation.mapped_slice()")
            .expect("screenshot readback must still read the staging mapping");
        assert!(
            invalidate < read,
            "the invalidate must precede the staging-buffer read"
        );
    }
}
