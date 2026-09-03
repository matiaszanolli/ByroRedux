//! VulkanContext resource management methods (BLAS, UI quad, extent, memory).

use super::VulkanContext;
use anyhow::{Context, Result};
use std::sync::{Arc, Weak};

use crate::vulkan::acceleration::draw_command_eligible_for_tlas;
use crate::vulkan::scene_buffer::GpuTerrainTile;

fn morph_memory_bytes(
    delta_bytes: impl IntoIterator<Item = u64>,
    weight_bytes: impl IntoIterator<Item = u64>,
) -> u64 {
    delta_bytes.into_iter().chain(weight_bytes).sum()
}

/// Free-function core of `fill_terrain_tile_scratch_if_dirty` — lifted
/// out of the `VulkanContext` method so unit tests can exercise it
/// without standing up a full Vulkan device. When `*dirty` is set,
/// clears `dest` (preserving capacity), refills it through the highest live
/// slot, and clears the flag. Empty slots inside that prefix remain explicit
/// zero tiles; the unused tail is omitted from the upload. Returns `true` when
/// the caller should perform the GPU upload. See #496 / #497 / #3664.
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
    let Some(last_live) = tiles.iter().rposition(Option::is_some) else {
        return true;
    };
    dest.extend(tiles[..=last_live].iter().map(|t| t.unwrap_or_default()));
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
    /// Create a morph slot for one mesh instance. The large immutable delta
    /// buffer is cached by the stable `MeshRegistry` handle; each entity
    /// still receives its own host-visible animated weight buffer.
    pub fn create_morph_slot_for_mesh(
        &mut self,
        mesh_handle: u32,
        deltas: &[[f32; 4]],
        target_count: u32,
        vertex_count: u32,
    ) -> Result<super::super::morph_compute::MorphSlot> {
        debug_assert_eq!(
            deltas.len(),
            target_count as usize * vertex_count as usize,
            "morph delta length must be target_count * vertex_count"
        );

        let cached = self
            .morph_delta_cache
            .get(&mesh_handle)
            .and_then(Weak::upgrade);
        let (delta, cache_new) = if let Some(delta) = cached {
            (delta, false)
        } else {
            // Remove a dead weak entry before replacing it so a long session
            // that revisits meshes does not retain stale cache keys.
            self.morph_delta_cache.remove(&mesh_handle);
            let allocator = self
                .allocator
                .as_ref()
                .context("renderer allocator missing")?;
            let upload_ctx = crate::vulkan::GpuUploadCtx {
                device: &self.device,
                allocator,
                queue: &self.graphics_queue,
                command_pool: self.transfer_pool,
            };
            (
                Arc::new(super::super::morph_compute::MorphDelta::create(
                    upload_ctx, deltas,
                )?),
                true,
            )
        };

        let allocator = self
            .allocator
            .as_ref()
            .context("renderer allocator missing")?;
        match super::super::morph_compute::MorphSlot::create_with_shared_delta(
            &self.device,
            allocator,
            delta.clone(),
            target_count,
            vertex_count,
        ) {
            Ok(slot) => {
                if cache_new {
                    self.morph_delta_cache
                        .insert(mesh_handle, Arc::downgrade(&delta));
                }
                Ok(slot)
            }
            Err(error) => {
                // `create_with_shared_delta` has no owner to clean the new
                // delta when its per-entity weight allocation fails. Free it
                // here when this was the final strong reference; existing
                // slots keep a cached delta alive and therefore remain safe.
                if let Ok(mut delta) = Arc::try_unwrap(delta) {
                    delta.destroy(&self.device, allocator);
                }
                Err(error)
            }
        }
    }

    /// Return `(active entity slots, resident delta + weight bytes)`. Delta
    /// bytes are counted once per live mesh cache entry; weight bytes are
    /// counted once per entity slot.
    pub fn morph_memory_usage(&self) -> (u32, u64) {
        let delta_bytes = self
            .morph_delta_cache
            .values()
            .filter_map(Weak::upgrade)
            .map(|delta| delta.byte_size() as u64);
        let weight_bytes = self
            .morph_slots
            .values()
            .map(|slot| slot.weight_bytes() as u64);
        (
            self.morph_slots.len() as u32,
            morph_memory_bytes(delta_bytes, weight_bytes),
        )
    }

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

    /// Populate `dest` with the current terrain tile prefix, filling holes
    /// with the zero-tile default so the fragment shader's
    /// `if (layerIdx == 0u) continue;` guard skips them. The prefix ends at
    /// the highest occupied slot, so vacant tail slots never reach the GPU.
    /// Returns `true` when an upload is due.
    ///
    /// Accepts `dest` by `&mut` rather than returning a slice from
    /// `self` so `draw_frame` can hold `&self.device` + `&mut
    /// self.scene_buffers` while consuming the staged data. The
    /// caller owns a persistent `terrain_tile_scratch` Vec whose capacity
    /// amortizes across frames — same pattern as `gpu_instances_scratch`.
    /// See #496 / #470 / #3664.
    pub(super) fn fill_terrain_tile_scratch_if_dirty(
        &mut self,
        dest: &mut Vec<GpuTerrainTile>,
    ) -> bool {
        fill_terrain_tiles(&self.terrain_tiles, &mut self.terrain_tiles_dirty, dest)
    }
    /// Build BLAS for multiple meshes in a single GPU submission.
    ///
    /// Call this after uploading all meshes during scene/cell load.
    /// Returns the number of BLAS successfully built.
    ///
    /// The only static-BLAS entry point since #2914 removed the
    /// never-called single-shot `build_blas_for_mesh`, whose per-mesh
    /// fence stall this batched form existed to avoid. It filters on
    /// `mesh.rt_capable`, so global-only meshes are skipped rather than
    /// reaching a per-mesh vertex-buffer `expect`.
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

    /// Restore every missing rigid BLAS needed by this frame before TLAS build.
    ///
    /// Static BLAS eviction is allowed to reclaim an off-screen mesh under
    /// pressure, but a retained `MeshHandle` can become visible again without
    /// another cell-load callback. The old global-LOD-only pass left ordinary
    /// dedicated-buffer meshes absent forever in that case: raster kept
    /// drawing them while shadows, reflections and GI silently lost them.
    ///
    /// This pass covers both retained source layouts:
    /// - ordinary RT-capable meshes use their dedicated vertex/index buffers;
    /// - global-only terrain/object LOD meshes use byte-offset subranges of the
    ///   global geometry buffers.
    ///
    /// All currently eligible rigid handles are LRU-stamped before the batch.
    /// `build_blas_batched` may evict internally to make room, so this ordering
    /// prevents recovery of one visible mesh from evicting another mesh needed
    /// by the same upcoming TLAS.
    pub fn restore_missing_static_blas_for_draws(
        &mut self,
        draw_commands: &[super::DrawCommand],
    ) -> usize {
        let Some(accel) = self.accel_manager.as_mut() else {
            return 0;
        };

        let mut handles: Vec<u32> = draw_commands
            .iter()
            .filter_map(|cmd| {
                if cmd.bone_offset != 0 || !draw_command_eligible_for_tlas(cmd) {
                    return None;
                }
                Some(cmd.mesh_handle)
            })
            .collect();
        handles.sort_unstable();
        handles.dedup();

        // Protect the complete upcoming rigid TLAS set before the builder's
        // pre-/mid-batch budget checks can select eviction candidates.
        accel.mark_static_blas_used(&handles);
        let visible = handles.len();
        handles.retain(|&handle| !accel.has_blas(handle));

        if handles.is_empty() {
            return 0;
        }

        // #3540 — bound the recovery. `build_blas_batched` evicts to stay
        // inside the static-BLAS budget, so this pass is only coherent while
        // the whole visible rigid set fits that budget; past it, every BLAS
        // restored displaces another one the same frame still needs and the
        // next frame rebuilds those instead — a cycle that never converges.
        // Starfield's `citycydoniamainlevel` (~95 k static draws) is the
        // observed case: single-threaded on frame 0 for over ten minutes,
        // RSS oscillating 12 -> 20.6 GB. `plan_static_blas_restore` skips
        // the pass when the set cannot fit and otherwise caps how many BLAS
        // one frame may rebuild.
        let restore_count = crate::vulkan::acceleration::plan_static_blas_restore(
            handles.len(),
            visible,
            accel.static_blas_bytes(),
            accel.live_static_blas_count(),
            accel.blas_budget_bytes(),
            crate::vulkan::acceleration::MAX_STATIC_BLAS_RESTORES_PER_FRAME,
        );
        if restore_count == 0 {
            // One-shot: the condition is a property of the cell, so it holds
            // for every frame spent in it. Warn once rather than per frame.
            static OVER_BUDGET_WARNED: std::sync::Once = std::sync::Once::new();
            OVER_BUDGET_WARNED.call_once(|| {
                log::warn!(
                    "Static BLAS recovery skipped: {visible} rigid draws project past the \
                     {:.1} MB BLAS budget ({} resident, {:.1} MB). Ray-traced shadows / \
                     reflections / GI will miss the over-budget tail; raster is unaffected.",
                    accel.blas_budget_bytes() as f64 / (1024.0 * 1024.0),
                    accel.live_static_blas_count(),
                    accel.static_blas_bytes() as f64 / (1024.0 * 1024.0),
                );
            });
            return 0;
        }
        handles.truncate(restore_count);

        let vertex_stride = std::mem::size_of::<crate::Vertex>() as u64;
        let index_stride = std::mem::size_of::<u32>() as u64;
        let global_vertex_buffer = self
            .mesh_registry
            .global_vertex_buffer
            .as_ref()
            .map(|buffer| buffer.buffer);
        let global_index_buffer = self
            .mesh_registry
            .global_index_buffer
            .as_ref()
            .map(|buffer| buffer.buffer);
        let sources: Vec<crate::vulkan::acceleration::BlasBuildSource> = handles
            .into_iter()
            .filter_map(|handle| {
                let mesh = self.mesh_registry.get(handle)?;
                let (vertex_buffer, index_buffer, vertex_byte_offset, index_byte_offset) =
                    match (mesh.vertex_buffer.as_ref(), mesh.index_buffer.as_ref()) {
                        (Some(vertex), Some(index)) if mesh.rt_capable => {
                            (vertex.buffer, index.buffer, 0, 0)
                        }
                        (None, None) => (
                            global_vertex_buffer?,
                            global_index_buffer?,
                            u64::from(mesh.global_vertex_offset) * vertex_stride,
                            u64::from(mesh.global_index_offset) * index_stride,
                        ),
                        _ => return None,
                    };
                Some(crate::vulkan::acceleration::BlasBuildSource {
                    mesh_handle: handle,
                    vertex_buffer,
                    index_buffer,
                    vertex_byte_offset,
                    index_byte_offset,
                    vertex_count: mesh.vertex_count,
                    index_count: mesh.index_count,
                })
            })
            .collect();

        if sources.is_empty() {
            return 0;
        }
        let allocator = self.allocator.as_ref().expect("allocator missing");
        match accel.build_blas_batched(
            &self.device,
            allocator,
            &self.graphics_queue,
            self.transfer_pool,
            Some(&self.transfer_fence),
            &sources,
        ) {
            Ok(count) => {
                log::debug!("Restored {count} missing static shadow BLAS before TLAS build");
                count
            }
            Err(e) => {
                log::warn!("Pre-TLAS static BLAS recovery batch failed: {e}");
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
            super::super::allocator::log_memory_usage(
                alloc,
                &self.instance,
                self.physical_device,
                &self.memory_warning_once,
            );
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

    #[test]
    fn morph_memory_budget_row_names_live_telemetry() {
        const BUDGET_MD: &str = include_str!("../../../../../docs/engine/memory-budget.md");
        let section = BUDGET_MD
            .split_once("## Morph-target GPU resources — #3661")
            .expect("memory budget must document morph-target resources")
            .1;
        let section = section
            .split_once("\n---")
            .map(|(head, _)| head)
            .unwrap_or(section);
        for needle in [
            "`morph_slots`",
            "`morph_bytes`",
            "vertex_count × target_count × 16",
            "target_count × 4",
        ] {
            assert!(
                section.contains(needle),
                "morph budget section must retain `{needle}`"
            );
        }
    }

    #[test]
    fn morph_memory_ledger_counts_shared_deltas_once() {
        // Two live meshes share their deltas across multiple entities; the
        // weight side remains one allocation per entity.
        assert_eq!(morph_memory_bytes([12_288, 4_096], [64, 64, 32]), 16_544);
    }

    /// An evicted rigid mesh must be recoverable from either retained source
    /// layout before the next TLAS publication. The current draw set must also
    /// be protected before `build_blas_batched` runs its internal eviction
    /// checks, or restoring one newly-visible mesh can remove another one from
    /// the same frame's TLAS.
    #[test]
    fn static_blas_recovery_covers_both_sources_and_protects_current_draws() {
        let source = include_str!("resources.rs");
        let production = &source[..source
            .find("#[cfg(test)]\nmod tests")
            .expect("resources.rs must retain its test module")];
        let start = production
            .find("pub fn restore_missing_static_blas_for_draws(")
            .expect("pre-TLAS static BLAS recovery entry point must exist");
        let body = &production[start..];
        let body = &body[..body
            .find("\n    /// Register the fullscreen quad")
            .expect("static BLAS recovery must remain a bounded method")];

        assert!(
            body.contains("draw_command_eligible_for_tlas(cmd)")
                && body.contains("cmd.bone_offset != 0"),
            "recovery must share TLAS eligibility and exclude per-entity skinned BLAS"
        );
        assert!(
            body.contains("mesh.rt_capable")
                && body.contains("mesh.vertex_buffer.as_ref()")
                && body.contains("global_vertex_buffer?")
                && body.contains("mesh.global_vertex_offset"),
            "recovery must cover dedicated RT buffers and global-buffer subranges"
        );

        let protect = body
            .find("accel.mark_static_blas_used(&handles)")
            .expect("current-frame rigid handles must be protected from eviction");
        let missing_filter = body
            .find("handles.retain(|&handle| !accel.has_blas(handle))")
            .expect("only missing BLAS should enter the recovery batch");
        let build = body
            .find("accel.build_blas_batched(")
            .expect("missing static BLAS must be rebuilt before TLAS");
        assert!(
            protect < missing_filter && missing_filter < build,
            "protect the complete draw set, then select missing handles, then rebuild"
        );
    }

    /// #3540 — the recovery pass must stay bounded. Handing every missing
    /// handle to `build_blas_batched` in one synchronous batch is what let
    /// Starfield's `citycydoniamainlevel` sit on frame 0 for ten minutes,
    /// so the plan call has to sit between the missing-handle filter and
    /// the build, and its result has to actually shorten the batch.
    #[test]
    fn static_blas_recovery_is_bounded_per_frame() {
        let source = include_str!("resources.rs");
        let production = &source[..source
            .find("#[cfg(test)]\nmod tests")
            .expect("resources.rs must retain its test module")];
        let start = production
            .find("pub fn restore_missing_static_blas_for_draws(")
            .expect("pre-TLAS static BLAS recovery entry point must exist");
        let body = &production[start..];
        let body = &body[..body
            .find("\n    /// Register the fullscreen quad")
            .expect("static BLAS recovery must remain a bounded method")];

        let missing_filter = body
            .find("handles.retain(|&handle| !accel.has_blas(handle))")
            .expect("only missing BLAS should enter the recovery batch");
        let plan = body
            .find("plan_static_blas_restore(")
            .expect("recovery must consult the per-frame restore plan");
        let truncate = body
            .find("handles.truncate(restore_count)")
            .expect("the plan's count must actually bound the batch");
        let build = body
            .find("accel.build_blas_batched(")
            .expect("missing static BLAS must be rebuilt before TLAS");
        assert!(
            missing_filter < plan && plan < truncate && truncate < build,
            "select missing handles, plan the bounded restore, truncate, then rebuild"
        );
        assert!(
            body.contains("MAX_STATIC_BLAS_RESTORES_PER_FRAME"),
            "the per-frame cap must come from the shared tunable, not a local literal"
        );
        assert!(
            body.contains("if restore_count == 0 {"),
            "a zero plan must skip the batch entirely rather than fall through"
        );
    }

    /// Regression for #496 / #497 / #3664: the fill helper must reuse the
    /// caller's scratch buffer capacity across repeated dirty refills while
    /// retaining only the live high-water prefix. Since #497 the dirty signal
    /// is a single bool (the DEVICE_LOCAL SSBO needs exactly one upload per
    /// cell transition), so the capacity reuse is verified by toggling the
    /// flag back on manually after each consumption.
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
        assert!(cap_after_first >= 1);
        assert_eq!(dest.len(), 1);
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

    /// An entirely vacant slab has no high-water prefix, so the upload is
    /// consumed without manufacturing a zero tile for every slot.
    #[test]
    fn empty_slots_produce_empty_upload_prefix() {
        let tiles: Vec<Option<GpuTerrainTile>> = vec![None; 4];
        let mut dest: Vec<GpuTerrainTile> = Vec::new();
        let mut dirty = true;

        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert!(dest.is_empty());
        assert!(!dirty);
    }

    /// Vacancies inside the live range remain zero tiles, while vacant tail
    /// slots are omitted from the staging copy entirely (#3664).
    #[test]
    fn fill_keeps_holes_but_trims_vacant_tail() {
        let mut tiles: Vec<Option<GpuTerrainTile>> = vec![None; 6];
        tiles[1] = Some(GpuTerrainTile {
            layer_diffuse_index: [7; 8],
            ..GpuTerrainTile::default()
        });
        tiles[4] = Some(GpuTerrainTile {
            layer_normal_index: [9; 8],
            ..GpuTerrainTile::default()
        });
        let mut dest = Vec::new();
        let mut dirty = true;

        assert!(fill_terrain_tiles(&tiles, &mut dirty, &mut dest));
        assert_eq!(dest.len(), 5);
        assert_eq!(dest[0], GpuTerrainTile::default());
        assert_eq!(dest[1].layer_diffuse_index, [7; 8]);
        assert_eq!(dest[2], GpuTerrainTile::default());
        assert_eq!(dest[3], GpuTerrainTile::default());
        assert_eq!(dest[4].layer_normal_index, [9; 8]);
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
        // #1749 / TD1-004 — the image_health_buffers allocation loop lives
        // in `init.rs`'s `build_pipelines_and_finish` now, not `mod.rs`.
        let mod_src = include_str!("init.rs");

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
