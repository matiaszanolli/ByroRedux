//! Global geometry-SSBO lifecycle for [`MeshRegistry`] (#3451).
//!
//! Split out of `mesh.rs` when that file crossed 2,000 production LOC. It is
//! not a long-function problem — the largest function here is well under the
//! 200-LOC extraction trigger — but a file-cohesion one: `mesh.rs` had grown
//! three independent responsibilities, and this is the self-contained one.
//!
//! What lives here is the state machine that owns the two global vertex/index
//! SSBOs: the generation counter, the hole-compaction planner, the resumable
//! chunked rebuild with its cursor, and the atomic fallback the chunked path
//! degrades to under low headroom. `mesh.rs` keeps `MeshRegistry` proper
//! (handle allocation, per-mesh metadata, refcount/LRU eviction) and the
//! primitive-geometry helpers.
//!
//! The seam is the one the registry already used internally: this module
//! reaches `MeshRegistry`'s private fields directly, which a child module may
//! do, so the extraction moved code without widening any visibility.

use super::{
    geometry_rebuild_needs_idle, scene_geometry_resident, MeshRegistry,
    GEOMETRY_REBUILD_CHUNK_BYTES, GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES,
};
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use crate::vertex::Vertex;
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::{GpuBuffer, StagingPool};
use crate::vulkan::GpuUploadCtx;
use anyhow::{bail, Result};
use ash::vk;

/// A computed-but-unpublished geometry compaction (#3372).
///
/// Produced by `plan_geometry_compaction`, applied by `apply_compaction_plan`.
/// The chunked rebuild carries one of these for the whole multi-frame copy so
/// the compacted offsets become visible in the same step that binds the
/// compacted buffer.
#[derive(Debug, Clone)]
pub(super) struct CompactionPlan {
    /// `(mesh slot index, new global_vertex_offset, new global_index_offset)`
    /// for every scene mesh live at plan time.
    offsets: Vec<(usize, u32, u32)>,
    /// `meshes.len()` at plan time. Slots at or past this index were appended
    /// *after* the plan, so they already carry compacted-layout offsets and
    /// must stay out of raster/TLAS until swap-in — see `is_geometry_resident`.
    mesh_count: usize,
}

/// State for an in-flight, multi-frame global geometry SSBO rebuild
/// (#3298). Both destination buffers are allocated empty, at their full
/// target size, when the rebuild starts; `vertices_copied`/`indices_copied`
/// track how much of `pending_vertices`/`pending_indices` has landed in
/// them so far.
///
/// The OLD `global_vertex_buffer`/`global_index_buffer` keep serving every
/// draw, completely unmodified, for the whole copy — only
/// `MeshRegistry::advance_geometry_rebuild` swaps them out, and only once
/// both targets are fully copied. That means two full geometry SSBO
/// generations are resident in device-local memory at once for the
/// rebuild's duration. This is an accepted trade-off (#3298) *below*
/// [`super::GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`]: it smooths a
/// multi-hundred-ms atomic stall into several bounded per-frame chunks, at
/// the cost of a temporarily higher VRAM high-water mark.
///
/// #3443 — at or above that threshold the caller
/// (`MeshRegistry::rebuild_geometry_ssbo`) never starts one of these at
/// all; the rebuild goes to the atomic idle-reclaim path up front. #3298
/// shipped this state with no size condition, which routed around the very
/// case #2374 filed the threshold for: on a 6 GB card an FO4 boundary
/// crossing duplicates ~800-900 MiB, the largest non-texture allocation
/// class, on top of a ~1.7 GB steady state. A duplicate allocation that
/// *succeeds* there and dies later cannot be caught by the `Err` arm.
/// The allocation-failure fallback still exists for the sub-threshold case.
///
/// (#4090 — this doc comment previously sat orphaned in `mesh.rs`, ahead of
/// an unrelated function, with nothing between it and its actual subject
/// after this struct moved into this submodule. Relocated to the struct it
/// documents.)
pub(super) struct GeometryRebuildInProgress {
    pub(super) new_vertex_buffer: GpuBuffer,
    pub(super) new_index_buffer: GpuBuffer,
    /// `pending_vertices.len()` / `pending_indices.len()` snapshotted when
    /// this rebuild started — the copy targets exactly this much data.
    /// Streaming can append more to `pending_vertices`/`pending_indices`
    /// while this rebuild is still copying (a later boundary crossing
    /// starting before this one finishes); that tail is deliberately left
    /// uncopied rather than grown into mid-flight. `advance_geometry_rebuild`
    /// notices the mismatch at completion and leaves `geometry_dirty` set so
    /// the next eligible frame starts a follow-up rebuild for it.
    target_vertex_count: usize,
    target_index_count: usize,
    vertices_copied: usize,
    indices_copied: usize,
}

/// What a single `advance_geometry_rebuild` call should do next, given the
/// current copy progress. Pure — no Vulkan/`self` access — so the resumable
/// rebuild's core sequencing decision is unit-testable without a live
/// device (#3298), mirroring the `acceleration/predicates.rs` pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeometryRebuildStep {
    /// Copy `pending_vertices[start..end]` into the target vertex buffer.
    CopyVertices { start: usize, end: usize },
    /// Copy `pending_indices[start..end]` into the target index buffer.
    CopyIndices { start: usize, end: usize },
    /// Both targets are fully copied.
    Finished,
}

/// Vertex phase runs to completion first, then index — never interleaved.
/// Each `_chunk_elems` is clamped to at least 1 so a chunk size smaller than
/// one element can never produce a zero-progress, infinitely-looping step.
fn next_geometry_rebuild_chunk(
    vertices_copied: usize,
    target_vertex_count: usize,
    indices_copied: usize,
    target_index_count: usize,
    vertex_chunk_elems: usize,
    index_chunk_elems: usize,
) -> GeometryRebuildStep {
    if vertices_copied < target_vertex_count {
        let end = (vertices_copied + vertex_chunk_elems.max(1)).min(target_vertex_count);
        GeometryRebuildStep::CopyVertices {
            start: vertices_copied,
            end,
        }
    } else if indices_copied < target_index_count {
        let end = (indices_copied + index_chunk_elems.max(1)).min(target_index_count);
        GeometryRebuildStep::CopyIndices {
            start: indices_copied,
            end,
        }
    } else {
        GeometryRebuildStep::Finished
    }
}

impl MeshRegistry {
    /// Compact `pending_vertices`/`pending_indices` to contain only live
    /// scene meshes' data, and rewrite each survivor's
    /// `global_vertex_offset`/`global_index_offset` to its new position.
    ///
    /// Called implicitly by [`rebuild_geometry_ssbo`](Self::rebuild_geometry_ssbo).
    /// Safe to call with no drops: it exits early unless a scene mesh has been
    /// dropped since the last compaction (`geometry_has_holes`). Pure appends,
    /// and repeat rebuilds with no intervening drop, skip the pass entirely.
    /// Plan **and** publish in one step — the pre-#3372 behaviour, retained
    /// for the #2678 compaction tests that assert on the pass in isolation.
    /// Production callers choose their own publish point: synchronous paths
    /// publish immediately, the chunked rebuild defers to swap-in.
    #[cfg(test)]
    fn compact_pending_geometry(&mut self) {
        if let Some(plan) = self.plan_geometry_compaction() {
            self.apply_compaction_plan(&plan);
        }
    }

    /// Compute the compacted pools and every survivor's new offset **without
    /// publishing those offsets**.
    ///
    /// `pending_vertices`/`pending_indices` are replaced with the compacted
    /// layout immediately (the rebuild copies linear ranges out of them, so
    /// they must be final), but each mesh keeps its *old* offset until the
    /// caller decides it is safe to publish. Returns `None` when there is
    /// nothing to compact.
    ///
    /// #3372 — the two halves used to be inseparable. `#3298` made the upload
    /// resumable across frames while the old buffer keeps serving draws, so
    /// publishing compacted offsets at plan time left mesh offsets describing
    /// the new layout while the *uncompacted* buffer was still bound: every
    /// draw and every BLAS built in that window read the wrong byte ranges.
    /// Splitting plan from publish lets the chunked path defer the publish to
    /// swap-in, where the offsets and the buffer change together.
    fn plan_geometry_compaction(&mut self) -> Option<CompactionPlan> {
        // Fast path: no holes → nothing to compact.
        //
        // #2678 — gated on the explicit `geometry_has_holes` flag, NOT on
        // `meshes.iter().any(|s| s.is_none())`. Dropped slots hold `None`
        // forever (handle stability, #372), so that scan latched true on the
        // first drop of any mesh and left this pass running on every rebuild
        // — re-copying both pools to a byte-identical layout once per cell
        // load, at the ~208 MB typical pool size.
        if !self.geometry_has_holes {
            return None;
        }

        let mut new_vertices: Vec<Vertex> = Vec::with_capacity(self.pending_vertices.len());
        let mut new_indices: Vec<u32> = Vec::with_capacity(self.pending_indices.len());
        let mut offsets: Vec<(usize, u32, u32)> = Vec::new();

        for (idx, slot) in self.meshes.iter().enumerate() {
            let Some(mesh) = slot.as_ref() else { continue };
            if !mesh.is_scene_mesh {
                continue;
            }
            let v_start = mesh.global_vertex_offset as usize;
            let v_end = v_start + mesh.vertex_count as usize;
            let i_start = mesh.global_index_offset as usize;
            let i_end = i_start + mesh.index_count as usize;

            let new_v_offset = new_vertices.len() as u32;
            let new_i_offset = new_indices.len() as u32;

            new_vertices.extend_from_slice(&self.pending_vertices[v_start..v_end]);
            new_indices.extend_from_slice(&self.pending_indices[i_start..i_end]);

            offsets.push((idx, new_v_offset, new_i_offset));
        }

        self.pending_vertices = new_vertices;
        self.pending_indices = new_indices;
        // Pools are hole-free again until the next scene-mesh drop.
        self.geometry_has_holes = false;

        Some(CompactionPlan {
            offsets,
            mesh_count: self.meshes.len(),
        })
    }

    /// Publish a plan's compacted offsets onto the surviving meshes.
    ///
    /// Slots vacated between plan and publish are skipped: `drop_mesh` leaves
    /// `None` behind permanently (#372), and a mesh that died mid-rebuild
    /// simply keeps its span as dead weight in the new buffer until the next
    /// compaction reclaims it.
    fn apply_compaction_plan(&mut self, plan: &CompactionPlan) {
        for &(idx, v_offset, i_offset) in &plan.offsets {
            let Some(slot) = self.meshes.get_mut(idx) else {
                continue;
            };
            let Some(mesh) = slot.as_mut() else { continue };
            mesh.global_vertex_offset = v_offset;
            mesh.global_index_offset = i_offset;
        }
    }

    /// Build the global geometry SSBO from accumulated vertex/index data.
    /// Call once after all scene meshes are loaded.
    ///
    /// Staging-buffer reuse lives on `self.geometry_staging_pool` — lazy-
    /// initialised here on the first call because `MeshRegistry::new()`
    /// runs before the device handle is available. The retained pool
    /// avoids a fresh fire-and-forget staging allocation on every cell
    /// load and frame-loop rebuild. See #242 (StagingPool ship) and
    /// #1055 (consumer-side wiring).
    pub fn build_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        rt_enabled: bool,
    ) -> Result<()> {
        if self.pending_vertices.is_empty() {
            return Ok(());
        }

        let vertex_size =
            (std::mem::size_of::<Vertex>() * self.pending_vertices.len()) as vk::DeviceSize;
        let index_size =
            (std::mem::size_of::<u32>() * self.pending_indices.len()) as vk::DeviceSize;

        if self.geometry_staging_pool.is_none() {
            self.geometry_staging_pool = Some(StagingPool::new(device.clone(), allocator.clone()));
        }

        // Create with STORAGE_BUFFER (RT reflection UV lookups) plus
        // VERTEX_BUFFER / INDEX_BUFFER so the draw loop can bind this
        // single global buffer instead of per-mesh rebinding. See #294.
        let ctx = GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        };
        let rt_usage = if rt_enabled {
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
        } else {
            vk::BufferUsageFlags::empty()
        };
        self.global_vertex_buffer = Some(GpuBuffer::create_device_local_buffer(
            ctx,
            vertex_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER | rt_usage,
            &self.pending_vertices,
            self.geometry_staging_pool.as_mut(),
        )?);
        self.global_index_buffer = Some(GpuBuffer::create_device_local_buffer(
            ctx,
            index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER | rt_usage,
            &self.pending_indices,
            self.geometry_staging_pool.as_mut(),
        )?);
        // #2743 — a fresh vk::Buffer handle pair now backs the global
        // vertex/index SSBO (either this is the first build, or
        // `rebuild_geometry_ssbo` destroyed the old pair above). Bump so
        // any cache keyed on the raw handle (e.g.
        // `SkinComputePipeline::dispatch`'s per-FIF descriptor cache) can
        // tell a same-handle-recycled generation apart from an unchanged
        // buffer.
        self.geometry_generation = self.geometry_generation.wrapping_add(1);

        log::info!(
            "Global geometry SSBO: {} vertices ({:.1} KB), {} indices ({:.1} KB)",
            self.pending_vertices.len(),
            vertex_size as f64 / 1024.0,
            self.pending_indices.len(),
            index_size as f64 / 1024.0,
        );

        // Track the built size so we can detect when new data arrives.
        // pending data is kept alive for potential rebuilds (#258).
        self.ssbo_vertex_count = self.pending_vertices.len();
        self.ssbo_index_count = self.pending_indices.len();
        self.geometry_dirty = false;

        Ok(())
    }

    /// Rebuild the global geometry SSBO after new meshes have been loaded.
    /// Only call when `is_geometry_dirty()` returns true, or every frame
    /// while [`geometry_rebuild_in_progress`](Self::geometry_rebuild_in_progress)
    /// is true (see that method's doc for why the two calls are not the
    /// same gate).
    ///
    /// #3298 — large rebuilds no longer copy the whole buffer atomically.
    /// The common path allocates the replacement vertex/index buffers empty
    /// at their full target size (the OLD pair keeps serving every draw,
    /// untouched) and copies bounded chunks in via
    /// [`advance_geometry_rebuild`](Self::advance_geometry_rebuild), one
    /// chunk per call, across as many frames as it takes — smoothing what
    /// used to be a single multi-hundred-ms stall (the FO4 boundary-
    /// crossing 1.50 s worst frame, #2376/EX-06/07) into several bounded
    /// slices.
    ///
    /// Two conditions send a rebuild to the original atomic
    /// idle-reclaim-then-build path
    /// ([`Self::rebuild_geometry_ssbo_atomic_fallback`]) instead:
    ///
    /// 1. The projected size is at or above
    ///    [`GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`] and an old generation
    ///    exists — [`geometry_rebuild_needs_idle`] (#2374 / #3443). This is
    ///    checked **before** allocating, because the hazard the threshold
    ///    guards against is the duplicate allocation *succeeding* on a
    ///    constrained device and escalating to `VK_ERROR_DEVICE_LOST` later.
    /// 2. The up-front allocation for the second (temporarily duplicate)
    ///    generation fails outright.
    ///
    /// So #2374's device-loss protection is a size-gated route, not merely
    /// a post-hoc `Err` recovery.
    pub fn rebuild_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        rt_enabled: bool,
    ) -> Result<()> {
        // #3467 — bracket the whole call, resumable chunk included. The chunk
        // is bounded in BYTES, not time, and each one is a synchronous staged
        // copy ending in a fence wait, so "one bounded slice" can still be a
        // dropped frame. Nothing could measure it: there is no GPU timer for
        // it (the copy is submitted on its own one-time command buffer,
        // outside `draw_frame`'s recording, so `gpu_timers` cannot bracket
        // it), and on the CPU side it sat inside `render_one_frame`'s
        // `rof_pre_draw` bracket along with `build_render_data`, material
        // interning and the UI tick. Until `GEOMETRY_REBUILD_CHUNK_BYTES` can
        // be re-picked against a real number, its own doc's "chosen
        // conservatively pending live tuning" has no path to a tuned value.
        let rebuild_t0 = std::time::Instant::now();
        let result =
            self.rebuild_geometry_ssbo_inner(device, allocator, queue, command_pool, rt_enabled);
        self.geometry_rebuild_ns = self
            .geometry_rebuild_ns
            .saturating_add(rebuild_t0.elapsed().as_nanos() as u64);
        result
    }

    /// Nanoseconds spent in [`Self::rebuild_geometry_ssbo`] since the last
    /// call to this, then reset to zero.
    ///
    /// #3467 — accumulates rather than overwrites because the frame driver
    /// may call the rebuild more than once per frame (the dirty-append path
    /// runs after the in-progress advance), and a per-frame reading has to
    /// include both.
    pub fn take_geometry_rebuild_ns(&mut self) -> u64 {
        std::mem::take(&mut self.geometry_rebuild_ns)
    }

    fn rebuild_geometry_ssbo_inner(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        rt_enabled: bool,
    ) -> Result<()> {
        if self.geometry_rebuild.is_some() {
            return self.advance_geometry_rebuild(device, allocator, queue, command_pool);
        }

        // If any scene meshes were dropped since the last build, compact the
        // pending buffers. Pure appends (no drops) skip this pass. Only safe
        // to run here, when no rebuild is in flight — running it mid-copy
        // would rewrite the very data a chunked rebuild is reading from
        // underneath it.
        //
        // #3372 — the survivors' new offsets are *not* published yet. The
        // chunked path below carries the plan on the job and publishes it at
        // swap-in, so mesh offsets never describe a buffer that is not bound.
        // Every path that builds synchronously publishes immediately instead.
        let compaction = self.plan_geometry_compaction();

        if self.pending_vertices.is_empty() {
            if let Some(plan) = compaction {
                self.apply_compaction_plan(&plan);
            }
            return Ok(());
        }

        let target_vertex_count = self.pending_vertices.len();
        let target_index_count = self.pending_indices.len();
        let vertex_size = (target_vertex_count * std::mem::size_of::<Vertex>()) as vk::DeviceSize;
        let index_size = (target_index_count * std::mem::size_of::<u32>()) as vk::DeviceSize;
        let projected_bytes = vertex_size + index_size;
        let has_existing_buffers =
            self.global_vertex_buffer.is_some() || self.global_index_buffer.is_some();

        // Only meaningful once there's an old generation to duplicate
        // alongside — a first build has nothing to keep serving draws, so
        // it skips the chunked path below entirely and builds synchronously
        // in `rebuild_geometry_ssbo_atomic_fallback`.
        //
        // #3443 — and only while duplicating that generation is *safe*.
        // `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` exists precisely because
        // above 256 MiB a mid-range GPU may **succeed** at the second
        // full-size allocation (driver-managed residency / system-memory
        // spill) and then escalate to an unrecoverable
        // `VK_ERROR_DEVICE_LOST` under later pressure — which the `Err` arm
        // below cannot catch, because there is no `Err`. #3298 landed the
        // chunked path with no size condition at all, leaving
        // `geometry_rebuild_needs_idle` reachable only from the fallback it
        // routes around. At or above the threshold the rebuild takes
        // #2374's atomic idle-reclaim route again; below it, the chunked
        // path duplicates as designed. On the FO4 boundary crossing that is
        // ~800-900 MiB against a documented 6 GB RT minimum, on top of a
        // ~1.7 GB steady state.
        let duplicate_is_safe = !geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);
        if has_existing_buffers && !duplicate_is_safe {
            log::info!(
                "Geometry SSBO rebuild ({:.1} MiB) is at or above the {} MiB duplication                  ceiling — taking the atomic idle-reclaim path instead of holding two                  generations resident (#2374 / #3443)",
                projected_bytes as f64 / (1024.0 * 1024.0),
                GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES / (1024 * 1024),
            );
        }
        if has_existing_buffers && duplicate_is_safe {
            let rt_usage = if rt_enabled {
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            } else {
                vk::BufferUsageFlags::empty()
            };
            match Self::try_allocate_empty_geometry_buffers(
                device,
                allocator,
                vertex_size,
                index_size,
                rt_usage,
            ) {
                Ok((new_vertex_buffer, new_index_buffer)) => {
                    self.geometry_rebuild = Some(GeometryRebuildInProgress {
                        new_vertex_buffer,
                        new_index_buffer,
                        target_vertex_count,
                        target_index_count,
                        vertices_copied: 0,
                        indices_copied: 0,
                    });
                    self.deferred_compaction = compaction;
                    return self.advance_geometry_rebuild(device, allocator, queue, command_pool);
                }
                Err(e) => {
                    log::warn!(
                        "Geometry SSBO rebuild: could not allocate a second full-size \
                         generation ({:.1} MiB) alongside the current one ({e:#}) — \
                         falling back to the atomic idle-reclaim path (#2374)",
                        projected_bytes as f64 / (1024.0 * 1024.0),
                    );
                }
            }
        }

        // Synchronous path: buffer and offsets change together inside this
        // call, so publish now (#3372).
        if let Some(plan) = compaction {
            self.apply_compaction_plan(&plan);
        }
        self.rebuild_geometry_ssbo_atomic_fallback(
            device,
            allocator,
            queue,
            command_pool,
            rt_enabled,
        )
    }

    /// Whether a chunked global geometry SSBO rebuild is currently copying
    /// (#3298). The frame driver must call `rebuild_geometry_ssbo` every
    /// frame while this is true, **regardless** of
    /// `WorldStreamingState::geometry_batch_in_progress` — that gate only
    /// decides whether to *start* a new rebuild once the current streaming
    /// transaction settles; it says nothing about whether one already
    /// running should keep advancing. Gating the advance call on it too
    /// would stall an in-flight copy indefinitely the moment a second
    /// streaming transaction (e.g. another boundary crossing) begins before
    /// the first rebuild's chunks finish.
    pub fn geometry_rebuild_in_progress(&self) -> bool {
        self.geometry_rebuild.is_some()
    }

    /// Allocate the two empty, full-target-size device-local buffers a
    /// chunked rebuild copies into. On partial failure (vertex succeeds,
    /// index doesn't), destroys the vertex buffer before returning — no
    /// half-started state survives into the caller's fallback path.
    fn try_allocate_empty_geometry_buffers(
        device: &ash::Device,
        allocator: &SharedAllocator,
        vertex_size: vk::DeviceSize,
        index_size: vk::DeviceSize,
        rt_usage: vk::BufferUsageFlags,
    ) -> Result<(GpuBuffer, GpuBuffer)> {
        let new_vertex_buffer = GpuBuffer::create_empty_device_local_buffer(
            device,
            allocator,
            vertex_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER | rt_usage,
        )?;
        let new_index_buffer = match GpuBuffer::create_empty_device_local_buffer(
            device,
            allocator,
            index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER | rt_usage,
        ) {
            Ok(ib) => ib,
            Err(e) => {
                let mut vb = new_vertex_buffer;
                vb.destroy(device, allocator);
                return Err(e);
            }
        };
        Ok((new_vertex_buffer, new_index_buffer))
    }

    /// Copy up to [`GEOMETRY_REBUILD_CHUNK_BYTES`] more of the pending
    /// vertex/index data into the in-flight rebuild's target buffers, and
    /// finish (swap the buffers in, bump the generation, clear dirty) once
    /// both are fully copied. No-op if no rebuild is in flight.
    ///
    /// One phase advances per call — vertex fully, then index — never both
    /// in the same call ([`next_geometry_rebuild_chunk`] decides which).
    /// That keeps each call's chunk bounded to one `vkCmdCopyBuffer` + fence
    /// wait against a single, uniformly-sized element type, rather than
    /// juggling a byte budget shared across two different element sizes.
    fn advance_geometry_rebuild(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        if self.geometry_staging_pool.is_none() {
            self.geometry_staging_pool = Some(StagingPool::new(device.clone(), allocator.clone()));
        }

        let vertex_chunk_elems = GEOMETRY_REBUILD_CHUNK_BYTES / std::mem::size_of::<Vertex>();
        let index_chunk_elems = GEOMETRY_REBUILD_CHUNK_BYTES / std::mem::size_of::<u32>();

        if let Some(job) = self.geometry_rebuild.as_ref() {
            let step = next_geometry_rebuild_chunk(
                job.vertices_copied,
                job.target_vertex_count,
                job.indices_copied,
                job.target_index_count,
                vertex_chunk_elems,
                index_chunk_elems,
            );
            match step {
                GeometryRebuildStep::CopyVertices { start, end } => {
                    let dst_offset = (start * std::mem::size_of::<Vertex>()) as vk::DeviceSize;
                    let slice = &self.pending_vertices[start..end];
                    // SAFETY: `Vertex` is `Copy` / no padding concerns
                    // relevant to a byte-wise copy (mirrors
                    // `create_device_local_buffer`'s identical cast);
                    // `slice` is a valid, live sub-slice of
                    // `self.pending_vertices` for the duration of this call.
                    let bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            slice.as_ptr() as *const u8,
                            std::mem::size_of_val(slice),
                        )
                    };
                    job.new_vertex_buffer.copy_bytes_range(
                        GpuUploadCtx {
                            device,
                            allocator,
                            queue,
                            command_pool,
                        },
                        dst_offset,
                        bytes,
                        self.geometry_staging_pool
                            .as_mut()
                            .expect("just initialised above"),
                    )?;
                    self.geometry_rebuild
                        .as_mut()
                        .expect("checked Some above")
                        .vertices_copied = end;
                }
                GeometryRebuildStep::CopyIndices { start, end } => {
                    let dst_offset = (start * std::mem::size_of::<u32>()) as vk::DeviceSize;
                    let slice = &self.pending_indices[start..end];
                    // SAFETY: `u32` is `Copy` with no padding; `slice` is a
                    // valid, live sub-slice of `self.pending_indices` for
                    // the duration of this call.
                    let bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            slice.as_ptr() as *const u8,
                            std::mem::size_of_val(slice),
                        )
                    };
                    job.new_index_buffer.copy_bytes_range(
                        GpuUploadCtx {
                            device,
                            allocator,
                            queue,
                            command_pool,
                        },
                        dst_offset,
                        bytes,
                        self.geometry_staging_pool
                            .as_mut()
                            .expect("just initialised above"),
                    )?;
                    self.geometry_rebuild
                        .as_mut()
                        .expect("checked Some above")
                        .indices_copied = end;
                }
                GeometryRebuildStep::Finished => {}
            }
        }

        let finished = self.geometry_rebuild.as_ref().is_some_and(|job| {
            next_geometry_rebuild_chunk(
                job.vertices_copied,
                job.target_vertex_count,
                job.indices_copied,
                job.target_index_count,
                vertex_chunk_elems,
                index_chunk_elems,
            ) == GeometryRebuildStep::Finished
        });
        if finished {
            let job = self
                .geometry_rebuild
                .take()
                .expect("finished implies geometry_rebuild is Some");

            let old_vb = self.global_vertex_buffer.take();
            let old_ib = self.global_index_buffer.take();
            if old_vb.is_some() || old_ib.is_some() {
                self.deferred_destroy
                    .push((old_vb, old_ib), DEFAULT_COUNTDOWN);
            }
            // #3372 — publish the compacted offsets in the same step that
            // binds the compacted buffer. Until this line every mesh still
            // described the OLD layout, which is what the old buffer held.
            if let Some(plan) = self.deferred_compaction.take() {
                self.apply_compaction_plan(&plan);
            }

            self.global_vertex_buffer = Some(job.new_vertex_buffer);
            self.global_index_buffer = Some(job.new_index_buffer);
            self.geometry_generation = self.geometry_generation.wrapping_add(1);
            self.ssbo_vertex_count = job.target_vertex_count;
            self.ssbo_index_count = job.target_index_count;

            log::info!(
                "Global geometry SSBO rebuild complete: {} vertices ({:.1} KB), {} indices \
                 ({:.1} KB)",
                job.target_vertex_count,
                (job.target_vertex_count * std::mem::size_of::<Vertex>()) as f64 / 1024.0,
                job.target_index_count,
                (job.target_index_count * std::mem::size_of::<u32>()) as f64 / 1024.0,
            );

            // Only clear dirty if nothing outgrew this rebuild's snapshot
            // while it was copying — see `GeometryRebuildInProgress`'s doc.
            if self.pending_vertices.len() == job.target_vertex_count
                && self.pending_indices.len() == job.target_index_count
            {
                self.geometry_dirty = false;
            } else {
                log::info!(
                    "Geometry SSBO: pending data grew during the chunked rebuild \
                     ({} -> {} vertices, {} -> {} indices); leaving dirty for a follow-up \
                     rebuild (#3298)",
                    job.target_vertex_count,
                    self.pending_vertices.len(),
                    job.target_index_count,
                    self.pending_indices.len(),
                );
            }
        }

        Ok(())
    }

    /// Original atomic rebuild path (pre-#3298): idle-reclaim (or
    /// defer-destroy) the old SSBO, then build the replacement in one
    /// synchronous call. Kept as the fallback when there isn't enough
    /// device-local headroom to hold two full generations at once —
    /// [`Self::rebuild_geometry_ssbo`] tries the chunked path first and only
    /// reaches this when that allocation fails, or on any build with no
    /// prior generation to keep serving draws.
    fn rebuild_geometry_ssbo_atomic_fallback(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        rt_enabled: bool,
    ) -> Result<()> {
        let projected_bytes = (self.pending_vertices.len() * std::mem::size_of::<Vertex>()
            + self.pending_indices.len() * std::mem::size_of::<u32>())
            as u64;
        let has_existing_buffers =
            self.global_vertex_buffer.is_some() || self.global_index_buffer.is_some();
        let reclaim_before_rebuild =
            geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);

        // Defer destruction of old SSBOs instead of stalling with
        // device_wait_idle. The old buffers survive for MAX_FRAMES_IN_FLIGHT
        // frames, guaranteeing no in-flight command buffer references them
        // when they're finally destroyed.
        //
        // CRITICAL: this only covers *command-buffer* lifetime. The RT
        // descriptor bindings 8/9 (`GlobalVertices`/`GlobalIndices`) keep
        // naming the OLD `VkBuffer` until something re-points them — they are
        // NOT updated here. `draw_frame` re-points them for the current
        // frame-in-flight every frame (see the `write_geometry_buffers` call
        // right after `tick_deferred_destroy`), so by the time the deferred
        // free below executes (N+MAX_FRAMES_IN_FLIGHT) no descriptor names
        // the old buffer. Re-pointing here instead would be a
        // descriptor-update-while-in-use hazard (the previous frame's set is
        // still bound to an in-flight command buffer; bindings 8/9 are not
        // UPDATE_AFTER_BIND). A prior version of this comment claimed the
        // bindings were "updated in the same frame this is called" — they
        // were not, which caused a device-loss on cell-stream growth (the
        // WATAL §0 hunt).
        if reclaim_before_rebuild {
            log::warn!(
                "Large geometry SSBO rebuild ({:.1} MiB): idling once to reclaim prior \
                 generations before allocating the replacement (#2374)",
                projected_bytes as f64 / (1024.0 * 1024.0),
            );
            // SAFETY: this is the explicit synchronization boundary for the
            // low-headroom fallback. Once it returns, no submitted command
            // buffer or descriptor use can reference the old global or
            // deferred per-mesh buffers, so immediate destruction is legal.
            if let Err(error) = unsafe { device.device_wait_idle() } {
                bail!("device_wait_idle before large geometry rebuild: {error:?}");
            }
            self.drain_deferred_destroy(device, allocator);
            if let Some(mut buffer) = self.global_vertex_buffer.take() {
                buffer.destroy(device, allocator);
            }
            if let Some(mut buffer) = self.global_index_buffer.take() {
                buffer.destroy(device, allocator);
            }
        } else {
            let old_vb = self.global_vertex_buffer.take();
            let old_ib = self.global_index_buffer.take();
            if old_vb.is_some() || old_ib.is_some() {
                self.deferred_destroy
                    .push((old_vb, old_ib), DEFAULT_COUNTDOWN);
            }
        }

        log::info!(
            "Rebuilding geometry SSBO: {} → {} vertices",
            self.ssbo_vertex_count,
            self.pending_vertices.len(),
        );

        // Rebuild from all accumulated data. The internal
        // `geometry_staging_pool` (lazy-initialised in `build_geometry_ssbo`
        // on first call, then reused) keeps the staging-buffer churn bounded.
        self.build_geometry_ssbo(device, allocator, queue, command_pool, rt_enabled)
    }

    /// Returns true when new meshes have been loaded since the last SSBO
    /// build. The frame loop should call `rebuild_geometry_ssbo` to update.
    pub fn is_geometry_dirty(&self) -> bool {
        self.geometry_dirty
    }

    /// #2743 — monotonic counter bumped every time `global_vertex_buffer` /
    /// `global_index_buffer` get a fresh `vk::Buffer` pair. Fold into any
    /// cache keyed on the raw handle so a same-handle-recycled generation
    /// (destroy-then-immediately-reallocate, e.g. `rebuild_geometry_ssbo`'s
    /// `reclaim_before_rebuild` path) can't false-hit.
    pub fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }

    /// Whether `handle`'s scene-geometry range exists in the currently bound
    /// global SSBO generation. Streaming appends update the CPU pool and mesh
    /// offsets immediately, but the renderer may deliberately batch the GPU
    /// rebuild until the cell/LOD transaction settles. Commands for appended
    /// ranges must remain out of raster/TLAS until then or they index past the
    /// old buffer tail.
    pub fn is_geometry_resident(&self, handle: u32) -> bool {
        let Some(mesh) = self.get(handle) else {
            return false;
        };
        if !mesh.is_scene_mesh {
            return true;
        }
        if self.global_vertex_buffer.is_none() || self.global_index_buffer.is_none() {
            return false;
        }
        // #3372 — a compaction-bearing rebuild shrinks the pools, so a mesh
        // appended *after* the plan gets a compacted-layout offset that can
        // land inside the still-bound old buffer's extent. The length check
        // below would wave it through to read another mesh's bytes. Slots are
        // never reused (#372), so "index past the plan's snapshot" is an exact
        // test for those latecomers.
        let vertex_end = mesh.global_vertex_offset as usize + mesh.vertex_count as usize;
        let index_end = mesh.global_index_offset as usize + mesh.index_count as usize;
        scene_geometry_resident(
            handle as usize,
            vertex_end,
            index_end,
            self.ssbo_vertex_count,
            self.ssbo_index_count,
            self.deferred_compaction.as_ref().map(|p| p.mesh_count),
        )
    }
}

#[cfg(test)]
mod compaction_gate_tests {
    //! Regression tests for #2678 / PERF-D3-02 — `compact_pending_geometry`
    //! must actually skip when nothing has been dropped since the last pass.
    //!
    //! The bug was invisible in output: the old gate
    //! (`meshes.iter().any(|s| s.is_none())`) latched true on the first drop
    //! and never cleared, so every later rebuild re-ran a full compaction
    //! that produced a *byte-identical* layout. Correct pixels, redundant
    //! multi-hundred-MB copy per cell load.
    //!
    //! Comparing pool CONTENTS therefore cannot detect the regression — a
    //! redundant pass and a skipped pass agree on every element. These tests
    //! observe the allocation instead: compaction always installs freshly
    //! built `Vec`s, so `as_ptr()` moves iff the pass ran. Pools are kept
    //! non-empty so the pointers are real rather than dangling.
    use super::super::*;

    /// Upload two scene meshes through the device-free global-only path.
    fn two_scene_meshes(reg: &mut MeshRegistry) -> (u32, u32) {
        let (tv, ti) = triangle_vertices([1.0, 0.0, 0.0]);
        let (qv, qi) = quad_vertices();
        let a = reg.upload_scene_mesh_global_only(&tv, &ti).unwrap();
        let b = reg.upload_scene_mesh_global_only(&qv, &qi).unwrap();
        (a, b)
    }

    /// The core pin: a second compaction with no intervening drop must not
    /// touch the pools. Pre-fix this re-copied both of them.
    #[test]
    fn repeat_compaction_without_a_new_drop_does_not_recopy() {
        let mut reg = MeshRegistry::new();
        let (a, _b) = two_scene_meshes(&mut reg);

        assert!(reg.drop_mesh(a), "refcount 1 → drop frees the mesh");
        assert!(
            reg.geometry_has_holes,
            "dropping a scene mesh strands its span in the pending pools"
        );

        // First pass: real work, so the pools are rebuilt.
        reg.compact_pending_geometry();
        assert!(
            !reg.geometry_has_holes,
            "compaction must clear the flag it consumed"
        );
        assert!(
            !reg.pending_vertices.is_empty(),
            "survivor geometry remains"
        );

        let v_ptr = reg.pending_vertices.as_ptr();
        let i_ptr = reg.pending_indices.as_ptr();
        let v_len = reg.pending_vertices.len();
        let i_len = reg.pending_indices.len();

        // Second pass with nothing dropped in between: must be a no-op.
        reg.compact_pending_geometry();

        assert_eq!(
            reg.pending_vertices.as_ptr(),
            v_ptr,
            "vertex pool was reallocated by a compaction that had nothing to \
             compact — the #2678 redundant full-pool copy is back"
        );
        assert_eq!(
            reg.pending_indices.as_ptr(),
            i_ptr,
            "index pool was reallocated by a no-op compaction (#2678)"
        );
        assert_eq!(reg.pending_vertices.len(), v_len);
        assert_eq!(reg.pending_indices.len(), i_len);
    }

    /// The flag must not be derivable from the slot table: after a drop AND a
    /// compaction the `meshes` vec still contains a permanent `None`, which is
    /// precisely what made the old scan latch.
    #[test]
    fn dead_slots_persist_after_compaction_so_the_slot_scan_cannot_gate_it() {
        let mut reg = MeshRegistry::new();
        let (a, _b) = two_scene_meshes(&mut reg);
        assert!(reg.drop_mesh(a));
        reg.compact_pending_geometry();

        assert!(
            reg.meshes.iter().any(|slot| slot.is_none()),
            "dropped slots are None forever (handle stability, #372)"
        );
        assert!(
            !reg.geometry_has_holes,
            "…so the slot scan and the real hole state disagree — gating on \
             the scan is what made compaction unconditional"
        );
    }

    /// A fresh scene-mesh drop re-arms the gate.
    #[test]
    fn a_later_drop_rearms_compaction() {
        let mut reg = MeshRegistry::new();
        let (a, b) = two_scene_meshes(&mut reg);
        assert!(reg.drop_mesh(a));
        reg.compact_pending_geometry();
        assert!(!reg.geometry_has_holes);

        assert!(reg.drop_mesh(b), "second scene mesh dropped");
        assert!(
            reg.geometry_has_holes,
            "a new drop must re-arm the pass, or its span leaks in the pools"
        );
    }

    /// Appends alone must never arm compaction — `geometry_dirty` covers the
    /// rebuild trigger, and conflating the two is what the separate flag
    /// avoids.
    #[test]
    fn pure_appends_do_not_arm_compaction() {
        let mut reg = MeshRegistry::new();
        two_scene_meshes(&mut reg);
        assert!(
            !reg.geometry_has_holes,
            "uploads create no holes; only drops do"
        );
    }
}

#[cfg(test)]
mod deferred_compaction_tests {
    //! Regression tests for #3372 — a compaction whose upload is resumable
    //! must not publish its offsets while the *uncompacted* buffer is still
    //! bound.
    //!
    //! Pre-fix, `rebuild_geometry_ssbo` compacted (rewriting every survivor's
    //! `global_vertex_offset`/`global_index_offset`) and then handed the
    //! upload to a multi-frame state machine that leaves the old buffer
    //! serving every draw. For the 2..~15 frames in between, mesh offsets
    //! described the compacted layout while the bound buffer held the
    //! uncompacted bytes — so raster and every BLAS built in the window read
    //! another mesh's triangles. `is_geometry_resident` could not catch it: it
    //! compares the new (smaller) offsets against the old (larger) counts and
    //! answers `true` for everything.
    //!
    //! These tests exercise the CPU-side bookkeeping only; no Vulkan device is
    //! involved, which is exactly why the bug was invisible to the suite
    //! before.
    use super::super::*;

    fn three_scene_meshes(reg: &mut MeshRegistry) -> (u32, u32, u32) {
        let (tv, ti) = triangle_vertices([1.0, 0.0, 0.0]);
        let (qv, qi) = quad_vertices();
        let (tv2, ti2) = triangle_vertices([0.0, 1.0, 0.0]);
        let a = reg.upload_scene_mesh_global_only(&tv, &ti).unwrap();
        let b = reg.upload_scene_mesh_global_only(&qv, &qi).unwrap();
        let c = reg.upload_scene_mesh_global_only(&tv2, &ti2).unwrap();
        (a, b, c)
    }

    /// The core pin: planning compacts the pools but leaves every survivor's
    /// offset describing the OLD layout, so offsets stay in step with the
    /// still-bound old buffer.
    #[test]
    fn planning_compaction_does_not_publish_offsets() {
        let mut reg = MeshRegistry::new();
        let (a, _b, c) = three_scene_meshes(&mut reg);

        let c_v_before = reg.get(c).unwrap().global_vertex_offset;
        let c_i_before = reg.get(c).unwrap().global_index_offset;
        assert!(reg.drop_mesh(a), "refcount 1 → drop frees the mesh");

        let plan = reg
            .plan_geometry_compaction()
            .expect("a dropped scene mesh leaves a hole to compact");

        assert_eq!(
            reg.get(c).unwrap().global_vertex_offset,
            c_v_before,
            "planning published a compacted vertex offset while the \
             uncompacted buffer is still bound (#3372)"
        );
        assert_eq!(
            reg.get(c).unwrap().global_index_offset,
            c_i_before,
            "planning published a compacted index offset while the \
             uncompacted buffer is still bound (#3372)"
        );

        // ...and the plan really did carry a *different* (smaller) offset,
        // otherwise this test would pass vacuously.
        let (_idx, planned_v, _planned_i) = plan
            .offsets
            .iter()
            .copied()
            .find(|&(idx, _, _)| idx == c as usize)
            .expect("survivor must appear in the plan");
        assert!(
            planned_v < c_v_before,
            "compaction should move the survivor down; got {planned_v} vs {c_v_before}"
        );
    }

    /// Publishing is what moves the offsets — and it moves them to exactly
    /// what the plan computed.
    #[test]
    fn applying_the_plan_publishes_the_compacted_offsets() {
        let mut reg = MeshRegistry::new();
        let (a, _b, c) = three_scene_meshes(&mut reg);
        assert!(reg.drop_mesh(a));

        let plan = reg.plan_geometry_compaction().unwrap();
        let (_idx, planned_v, planned_i) = plan
            .offsets
            .iter()
            .copied()
            .find(|&(idx, _, _)| idx == c as usize)
            .unwrap();

        reg.apply_compaction_plan(&plan);

        assert_eq!(reg.get(c).unwrap().global_vertex_offset, planned_v);
        assert_eq!(reg.get(c).unwrap().global_index_offset, planned_i);
    }

    /// A slot vacated between plan and publish must be skipped, not panic and
    /// not resurrect: `drop_mesh` leaves `None` behind permanently (#372).
    #[test]
    fn publishing_skips_meshes_dropped_between_plan_and_swap_in() {
        let mut reg = MeshRegistry::new();
        let (a, b, _c) = three_scene_meshes(&mut reg);
        assert!(reg.drop_mesh(a));

        let plan = reg.plan_geometry_compaction().unwrap();
        assert!(reg.drop_mesh(b), "b dies mid-rebuild");

        reg.apply_compaction_plan(&plan);

        assert!(reg.get(b).is_none(), "a dropped slot stays empty");
    }

    /// The plan-and-publish wrapper still behaves exactly as the pre-#3372
    /// single-step compaction did — the synchronous paths depend on it.
    #[test]
    fn the_wrapper_still_compacts_and_publishes_in_one_step() {
        let mut reg = MeshRegistry::new();
        let (a, _b, c) = three_scene_meshes(&mut reg);
        let c_v_before = reg.get(c).unwrap().global_vertex_offset;
        assert!(reg.drop_mesh(a));

        reg.compact_pending_geometry();

        assert!(
            reg.get(c).unwrap().global_vertex_offset < c_v_before,
            "the synchronous wrapper must publish immediately"
        );
        assert!(!reg.geometry_has_holes);
    }

    /// Nothing dropped → no plan, and no offset churn.
    #[test]
    fn no_holes_yields_no_plan() {
        let mut reg = MeshRegistry::new();
        three_scene_meshes(&mut reg);
        assert!(
            reg.plan_geometry_compaction().is_none(),
            "uploads create no holes; only drops do (#2678)"
        );
    }

    /// A mesh appended *after* the plan carries a compacted-layout offset
    /// while the old buffer is still bound, so it must be held out of
    /// raster/TLAS until swap-in — the length check alone would wave it
    /// through into another mesh's bytes.
    #[test]
    fn a_mesh_appended_after_the_plan_is_not_resident_mid_rebuild() {
        let mut reg = MeshRegistry::new();
        let (a, _b, c) = three_scene_meshes(&mut reg);
        assert!(reg.drop_mesh(a));

        let plan = reg.plan_geometry_compaction().unwrap();
        let mesh_count_at_plan = plan.mesh_count;

        // Stand in for the in-flight chunked rebuild: old buffer still bound,
        // old counts still published, plan not yet applied.
        reg.ssbo_vertex_count = 10_000;
        reg.ssbo_index_count = 10_000;
        reg.deferred_compaction = Some(plan);

        let (lv, li) = triangle_vertices([0.0, 0.0, 1.0]);
        let late = reg.upload_scene_mesh_global_only(&lv, &li).unwrap();
        assert!(
            late as usize >= mesh_count_at_plan,
            "the latecomer must land past the plan's snapshot"
        );

        // Asserted through the pure predicate, not `is_geometry_resident`: a
        // device-free registry has no bound buffer, so the wrapper rejects on
        // that first and would pass this vacuously with the gate deleted.
        let lm = reg.get(late).unwrap();
        let late_v_end = lm.global_vertex_offset as usize + lm.vertex_count as usize;
        let late_i_end = lm.global_index_offset as usize + lm.index_count as usize;
        assert!(
            late_v_end <= 10_000 && late_i_end <= 10_000,
            "precondition: the latecomer's compacted offsets land INSIDE the \
             old buffer's extent, which is what makes the extent check unsafe"
        );
        assert!(
            !scene_geometry_resident(
                late as usize,
                late_v_end,
                late_i_end,
                10_000,
                10_000,
                Some(mesh_count_at_plan),
            ),
            "a mesh appended after the plan reads compacted coordinates out of \
             the uncompacted bound buffer — it must not be resident (#3372)"
        );
        // The survivor half is asserted through the pure predicate: a
        // device-free registry has no bound buffer, and `is_geometry_resident`
        // rejects on that first. What matters is that the #3372 gate does not
        // over-reach and blank the whole scene for the window.
        let cm = reg.get(c).unwrap();
        assert!(
            scene_geometry_resident(
                c as usize,
                cm.global_vertex_offset as usize + cm.vertex_count as usize,
                cm.global_index_offset as usize + cm.index_count as usize,
                10_000,
                10_000,
                Some(mesh_count_at_plan),
            ),
            "a survivor still on OLD offsets matches the bound old buffer and \
             must keep rendering — the gate must not blank the scene"
        );
    }

    /// The gate is scoped to the deferred window: with nothing deferred, a
    /// latecomer is judged purely on extent, exactly as before #3372.
    #[test]
    fn the_gate_is_inert_when_no_compaction_is_deferred() {
        assert!(
            scene_geometry_resident(99, 10, 10, 10_000, 10_000, None),
            "no deferred plan → plain extent check"
        );
        assert!(
            !scene_geometry_resident(99, 20_000, 10, 10_000, 10_000, None),
            "extent check still rejects a range past the bound tail"
        );
    }
}

#[cfg(test)]
mod geometry_rebuild_step_tests {
    //! Pure-logic regression tests for #3298's resumable geometry SSBO
    //! rebuild sequencing (`next_geometry_rebuild_chunk`). No Vulkan device
    //! is exercised — the actual copy/allocation path is validated live via
    //! `docs/smoke-tests/m-exteriors.sh boundary` (`grid-cross`), per this
    //! project's convention for GPU-touching code (see the module doc on
    //! `GeometryRebuildStep`). These tests pin the state machine's decisions
    //! against hand-picked progress/target/chunk-size combinations instead.
    use super::*;

    /// A fresh rebuild with nonzero work in both phases starts on vertices,
    /// not indices — the documented "vertex phase runs to completion first"
    /// ordering.
    #[test]
    fn starts_on_vertices_when_both_phases_have_work() {
        let step = next_geometry_rebuild_chunk(0, 100, 0, 300, 40, 40);
        assert_eq!(
            step,
            GeometryRebuildStep::CopyVertices { start: 0, end: 40 }
        );
    }

    /// A chunk that would overrun the target clamps to it exactly, rather
    /// than reading/copying past the end of `pending_vertices`.
    #[test]
    fn vertex_chunk_clamps_to_target_on_the_last_slice() {
        let step = next_geometry_rebuild_chunk(80, 100, 0, 300, 40, 40);
        assert_eq!(
            step,
            GeometryRebuildStep::CopyVertices {
                start: 80,
                end: 100
            },
            "80 + 40 overruns the 100-vertex target; must clamp to exactly 100"
        );
    }

    /// Once the vertex phase is fully copied, the index phase starts — even
    /// though `indices_copied` is still 0, vertices being done is what
    /// switches phases.
    #[test]
    fn switches_to_indices_once_vertices_are_fully_copied() {
        let step = next_geometry_rebuild_chunk(100, 100, 0, 300, 40, 90);
        assert_eq!(step, GeometryRebuildStep::CopyIndices { start: 0, end: 90 });
    }

    /// Both phases fully copied reports `Finished`, not another chunk of
    /// either — the completion signal `advance_geometry_rebuild` swaps on.
    #[test]
    fn both_phases_complete_reports_finished() {
        let step = next_geometry_rebuild_chunk(100, 100, 300, 300, 40, 90);
        assert_eq!(step, GeometryRebuildStep::Finished);
    }

    /// A target of exactly one chunk's width finishes that phase in a
    /// single step (`end` lands exactly on the target, not one short or one
    /// chunk past it) — the boundary case between "needs another chunk" and
    /// "done".
    #[test]
    fn chunk_exactly_covering_the_target_finishes_that_phase_in_one_step() {
        let step = next_geometry_rebuild_chunk(0, 40, 0, 300, 40, 90);
        assert_eq!(
            step,
            GeometryRebuildStep::CopyVertices { start: 0, end: 40 }
        );
        // The following call (as if this chunk just landed) must now switch
        // phases rather than emit a zero-length vertex chunk.
        let next = next_geometry_rebuild_chunk(40, 40, 0, 300, 40, 90);
        assert_eq!(next, GeometryRebuildStep::CopyIndices { start: 0, end: 90 });
    }

    /// A zero-sized chunk budget (degenerate `GEOMETRY_REBUILD_CHUNK_BYTES`
    /// misconfiguration, or an element wider than the whole configured
    /// budget) must still make forward progress — one element per call,
    /// never zero — so the rebuild cannot stall indefinitely. Mirrors
    /// `FrameTimeBudget`'s "first unit always admitted" guarantee
    /// (`work_budget.rs`).
    #[test]
    fn zero_chunk_size_still_advances_by_at_least_one_element() {
        let step = next_geometry_rebuild_chunk(0, 5, 0, 5, 0, 0);
        assert_eq!(
            step,
            GeometryRebuildStep::CopyVertices { start: 0, end: 1 },
            "a zero chunk size must still copy 1 element, or progress never happens"
        );
    }

    /// An empty target (nothing pending in one phase) skips straight past
    /// it — an already-satisfied phase (`copied == target == 0`) must not
    /// be mistaken for "has work".
    #[test]
    fn empty_vertex_target_skips_straight_to_indices() {
        let step = next_geometry_rebuild_chunk(0, 0, 0, 50, 40, 40);
        assert_eq!(step, GeometryRebuildStep::CopyIndices { start: 0, end: 40 });
    }

    /// Both targets empty (a rebuild started against no pending data at
    /// all) reports `Finished` immediately rather than looping.
    #[test]
    fn both_targets_empty_reports_finished() {
        let step = next_geometry_rebuild_chunk(0, 0, 0, 0, 40, 40);
        assert_eq!(step, GeometryRebuildStep::Finished);
    }
}
