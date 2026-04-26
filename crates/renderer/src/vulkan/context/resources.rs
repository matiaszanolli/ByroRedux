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
/// and returns the previous layer-texture indices so the caller can
/// release the per-layer texture refcounts they bumped through
/// `acquire_by_path` at allocation time. Returns `None` when the slot
/// index is out of range or already vacant. See #627.
pub(super) fn release_terrain_tile_slot(
    tiles: &mut [Option<GpuTerrainTile>],
    free_list: &mut Vec<u32>,
    dirty: &mut bool,
    slot: u32,
) -> Option<[u32; 8]> {
    let idx = slot as usize;
    if idx >= tiles.len() {
        return None;
    }
    let tile = tiles[idx].take()?;
    free_list.push(slot);
    *dirty = true;
    Some(tile.layer_texture_index)
}

impl VulkanContext {
    /// Allocate a terrain tile slot and store its 8 bindless texture
    /// indices. Returns the slot index (0..`MAX_TERRAIN_TILES`) that
    /// the caller packs into the top 16 bits of `GpuInstance.flags`
    /// alongside `INSTANCE_FLAG_TERRAIN_SPLAT`. Returns `None` when the
    /// registry is full — caller falls back to the single-texture
    /// path. See #470.
    pub fn allocate_terrain_tile(&mut self, layer_texture_index: [u32; 8]) -> Option<u32> {
        let slot = self.terrain_tile_free_list.pop()?;
        let idx = slot as usize;
        debug_assert!(idx < self.terrain_tiles.len());
        self.terrain_tiles[idx] = Some(GpuTerrainTile {
            layer_texture_index,
        });
        self.terrain_tiles_dirty = true;
        Some(slot)
    }

    /// Release a terrain tile slot back to the free list and schedule
    /// the SSBO to be reuploaded to every frame-in-flight. Must be
    /// called from `unload_cell` before the mesh / BLAS drop so a late
    /// frame-in-flight reads stale-but-valid data rather than
    /// undefined.
    ///
    /// Returns the previous slot's 8 layer texture indices so the
    /// caller can issue symmetric `drop_texture` calls on the refcounts
    /// that `resolve_texture` bumped at allocation time. Returns `None`
    /// when the slot is out of range or already vacant. See #627.
    pub fn free_terrain_tile(&mut self, slot: u32) -> Option<[u32; 8]> {
        release_terrain_tile_slot(
            &mut self.terrain_tiles,
            &mut self.terrain_tile_free_list,
            &mut self.terrain_tiles_dirty,
            slot,
        )
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
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
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

        // Gather mesh references for the batch — skip any missing handles.
        let meshes: Vec<(u32, &crate::mesh::GpuMesh, u32, u32)> = mesh_specs
            .iter()
            .filter_map(|&(handle, vc, ic)| {
                self.mesh_registry.get(handle).map(|m| (handle, m, vc, ic))
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

    /// Register the fullscreen quad mesh for UI overlay rendering.
    /// Call this once after creating the context.
    pub fn register_ui_quad(&mut self) -> Result<()> {
        let (vertices, indices) = crate::mesh::fullscreen_quad_ui_vertices();
        let allocator = self.allocator.as_ref().expect("allocator missing");
        let handle = self.mesh_registry.upload(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
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
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
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
            layer_texture_index: [1, 2, 3, 4, 5, 6, 7, 8],
        });
        let mut dest: Vec<GpuTerrainTile> = Vec::new();
        let mut dirty = true;

        // First call — allocates the Vec.
        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        let cap_after_first = dest.capacity();
        assert!(cap_after_first >= MAX_TERRAIN_TILES);
        assert_eq!(dest.len(), MAX_TERRAIN_TILES);
        assert_eq!(dest[0].layer_texture_index, [1, 2, 3, 4, 5, 6, 7, 8]);
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
            assert_eq!(tile.layer_texture_index, [0; 8]);
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
            layer_texture_index: [11, 22, 33, 44, 55, 66, 77, 88],
        });
        let mut free_list: Vec<u32> = vec![0, 1, 3];
        let mut dirty = false;

        let released = release_terrain_tile_slot(&mut tiles, &mut free_list, &mut dirty, 2);

        assert_eq!(released, Some([11, 22, 33, 44, 55, 66, 77, 88]));
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
}
