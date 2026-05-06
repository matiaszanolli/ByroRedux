//! Mesh registry — maps MeshHandle IDs to GPU buffers.

use crate::vertex::{UiVertex, Vertex};
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::{GpuBuffer, StagingPool};
use anyhow::Result;
use ash::vk;
use std::collections::HashMap;

/// Cache key for the refcounted scene-mesh dedup layer (#879). The
/// `path` is the lowercased model path (matches
/// `cell_loader::nif_import_registry`'s key); `sub_mesh_index` indexes
/// into a multi-mesh NIF so two `chair.nif` placements share the same
/// handle while a `corpse.nif`'s body + helmet sub-meshes get distinct
/// entries.
pub type MeshCacheKey = (String, u32);

/// A mesh stored on the GPU: vertex + index buffers and index count.
pub struct GpuMesh {
    pub vertex_buffer: GpuBuffer,
    pub index_buffer: GpuBuffer,
    pub index_count: u32,
    /// Offset into the global vertex SSBO (in vertices). Set after build_geometry_ssbo.
    pub global_vertex_offset: u32,
    /// Offset into the global index SSBO (in indices). Set after build_geometry_ssbo.
    pub global_index_offset: u32,
    /// Number of vertices in this mesh.
    pub vertex_count: u32,
    /// `true` when this mesh's data lives in `pending_vertices` /
    /// `pending_indices` and must be retained during SSBO compaction.
    /// UI overlays (uploaded via plain [`MeshRegistry::upload`]) are
    /// `false`; scene meshes (terrain, NIF, clutter) are `true`.
    pub is_scene_mesh: bool,
}

impl GpuMesh {
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.vertex_buffer.destroy(device, allocator);
        self.index_buffer.destroy(device, allocator);
    }
}

/// Registry mapping mesh handle IDs to GPU-side geometry.
///
/// Handles are stable — dropping a mesh leaves a `None` in its slot
/// rather than shifting subsequent handles. This keeps `GpuInstance`
/// and cached handle lookups valid across cell transitions (#372).
pub struct MeshRegistry {
    meshes: Vec<Option<GpuMesh>>,
    /// Accumulated vertex data for building the global geometry SSBO.
    /// Kept alive after `build_geometry_ssbo()` so late-loaded meshes
    /// can append and trigger a rebuild. See #258.
    pending_vertices: Vec<Vertex>,
    /// Accumulated index data for building the global geometry SSBO.
    pending_indices: Vec<u32>,
    /// Global geometry SSBO (vertices). Built by `build_geometry_ssbo()`.
    pub global_vertex_buffer: Option<GpuBuffer>,
    /// Global geometry SSBO (indices). Built by `build_geometry_ssbo()`.
    pub global_index_buffer: Option<GpuBuffer>,
    /// Set when `upload_scene_mesh` is called after the initial SSBO
    /// build — signals the frame loop to call `rebuild_geometry_ssbo`.
    geometry_dirty: bool,
    /// Number of vertices in the SSBO at last build. Used to detect
    /// whether a rebuild is needed vs. the current pending state.
    ssbo_vertex_count: usize,
    /// Old SSBOs awaiting deferred destruction. Each entry is a pair of
    /// (vertex, index) buffers and a countdown of frames before they can
    /// be safely destroyed (must survive MAX_FRAMES_IN_FLIGHT frames to
    /// guarantee no in-flight command buffer references them).
    deferred_destroy: Vec<(Option<GpuBuffer>, Option<GpuBuffer>, u32)>,
    /// Refcounted scene-mesh dedup keyed by `(model_path,
    /// sub_mesh_index)` — populated by
    /// [`Self::register_scene_mesh_keyed`] and consulted by
    /// [`Self::acquire_cached`]. Mirror of
    /// `TextureRegistry.path_map` (#524). Pre-#879 every REFR
    /// placement re-uploaded its NIF's vertex/index buffers as a
    /// fresh GPU pair even when the underlying `Arc<CachedNifImport>`
    /// was already shared on the CPU side: 40 chairs in Megaton →
    /// 80 fence-waits per cell load. With this cache, those 40
    /// placements share one upload + one BLAS build, and unloads
    /// only free the GPU resources when the last placement releases.
    mesh_cache: HashMap<MeshCacheKey, u32>,
    /// Live reference counts, parallel-indexed by mesh handle (slot
    /// `i` of `mesh_ref_counts` holds the refcount for the entry at
    /// `meshes[i]`). Each placement holding a mesh through
    /// `MeshHandle` contributes 1; `drop_mesh` decrements once per
    /// holder and only queues the GPU buffers for deferred
    /// destruction when the count reaches 0. Single-owner uploads
    /// (terrain tiles, CLI single-NIF view, UI overlays) start at
    /// 1 so the legacy "drop once → free" path is preserved.
    /// Refcounted dedup
    /// (`acquire_cached` / `register_scene_mesh_keyed`) bumps the
    /// count per placement.
    ///
    /// Stored as a parallel vec rather than a field on `GpuMesh` so
    /// the `#[cfg(test)] mod refcount_tests` block can exercise the
    /// bookkeeping without synthesising a `GpuMesh` (which contains
    /// `ash::Device` Arc fields whose validity invariants forbid
    /// zero-initialisation). See #879 / CELL-PERF-01.
    mesh_ref_counts: Vec<u32>,
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            pending_vertices: Vec::new(),
            pending_indices: Vec::new(),
            global_vertex_buffer: None,
            global_index_buffer: None,
            geometry_dirty: false,
            ssbo_vertex_count: 0,
            deferred_destroy: Vec::new(),
            mesh_cache: HashMap::new(),
            mesh_ref_counts: Vec::new(),
        }
    }

    /// Tick the deferred-destroy list. Call once per frame. Destroys old
    /// SSBOs whose countdown has reached zero (safe because all in-flight
    /// command buffers referencing them have completed).
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.deferred_destroy.retain_mut(|(vb, ib, countdown)| {
            if *countdown == 0 {
                if let Some(mut b) = vb.take() {
                    b.destroy(device, allocator);
                }
                if let Some(mut b) = ib.take() {
                    b.destroy(device, allocator);
                }
                false // remove from list
            } else {
                *countdown -= 1;
                true // keep
            }
        });
    }

    /// Drain `deferred_destroy` synchronously regardless of countdown.
    /// Counterpart of [`Self::tick_deferred_destroy`] for the shutdown
    /// path where no future frames will tick the countdown. Caller must
    /// have already called `device_wait_idle` so the queued buffers
    /// can't be in-flight. See #732 / LIFE-H2.
    pub fn drain_deferred_destroy(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        for (mut vb, mut ib, _countdown) in self.deferred_destroy.drain(..) {
            if let Some(ref mut b) = vb {
                b.destroy(device, allocator);
            }
            if let Some(ref mut b) = ib {
                b.destroy(device, allocator);
            }
        }
    }

    /// Number of pairs currently waiting in `deferred_destroy`. Surfaced
    /// for the [`drain_deferred_destroy`] regression test and shutdown
    /// telemetry. See #732.
    pub fn deferred_destroy_count(&self) -> usize {
        self.deferred_destroy.len()
    }

    /// Upload a mesh to the GPU and return its handle ID.
    ///
    /// Uses a staging buffer to place geometry in DEVICE_LOCAL memory.
    /// The vertex type is generic (`Vertex` for scene meshes, `UiVertex`
    /// for UI overlays) — the GPU buffer is format-agnostic.
    pub fn upload<V: Copy>(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[V],
        indices: &[u32],
        rt_enabled: bool,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        let vertex_buffer = GpuBuffer::create_vertex_buffer(
            device,
            allocator,
            queue,
            command_pool,
            vertices,
            rt_enabled,
            staging_pool.as_deref_mut(),
        )?;
        let index_buffer = GpuBuffer::create_index_buffer(
            device,
            allocator,
            queue,
            command_pool,
            indices,
            rt_enabled,
            staging_pool,
        )?;
        let index_count = indices.len() as u32;

        let id = self.meshes.len() as u32;
        self.meshes.push(Some(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count,
            global_vertex_offset: 0,
            global_index_offset: 0,
            vertex_count: vertices.len() as u32,
            is_scene_mesh: false,
        }));
        // Parallel-indexed refcount; lockstep with `meshes` push.
        self.mesh_ref_counts.push(1);

        Ok(id)
    }

    /// Upload a scene mesh (Vertex type) and track its geometry for the
    /// global SSBO used by RT reflection ray UV lookups.
    pub fn upload_scene_mesh(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        // Record offsets before appending.
        let v_offset = self.pending_vertices.len() as u32;
        let i_offset = self.pending_indices.len() as u32;

        // Accumulate for global SSBO.
        self.pending_vertices.extend_from_slice(vertices);
        self.pending_indices.extend_from_slice(indices);

        // Upload to per-mesh buffers.
        let id = self.upload(
            device,
            allocator,
            queue,
            command_pool,
            vertices,
            indices,
            rt_enabled,
            staging_pool,
        )?;

        // Store offsets.
        let mesh = self.meshes[id as usize]
            .as_mut()
            .expect("upload just pushed this slot");
        mesh.global_vertex_offset = v_offset;
        mesh.global_index_offset = i_offset;
        mesh.is_scene_mesh = true;

        // If the SSBO has already been built, mark dirty so the frame
        // loop knows to call rebuild_geometry_ssbo. See #258.
        if self.global_vertex_buffer.is_some()
            && self.pending_vertices.len() > self.ssbo_vertex_count
        {
            self.geometry_dirty = true;
        }

        Ok(id)
    }

    /// Acquire a previously-cached scene mesh by `(model_path,
    /// sub_mesh_index)`. Bumps the entry's refcount on hit, returning
    /// the handle so the caller can attach it to a new placement
    /// without re-uploading. Returns `None` when the key has never
    /// been registered or the entry has already been freed (last
    /// holder released it). Mirror of
    /// [`crate::texture_registry::TextureRegistry::acquire_by_path`]
    /// (#524). See #879 / CELL-PERF-01.
    pub fn acquire_cached(&mut self, model_path: &str, sub_mesh_index: u32) -> Option<u32> {
        let key = (model_path.to_string(), sub_mesh_index);
        let &handle = self.mesh_cache.get(&key)?;
        let rc = self.mesh_ref_counts.get_mut(handle as usize)?;
        if *rc == 0 {
            // Stale cache entry pointing at a freed slot; treat as
            // miss so the caller falls through to a fresh upload.
            return None;
        }
        *rc = rc.saturating_add(1);
        Some(handle)
    }

    /// Upload a scene mesh AND register it in the refcounted dedup
    /// cache under `(model_path, sub_mesh_index)`. The first placement
    /// of a NIF takes this path; subsequent placements of the same
    /// NIF should hit [`Self::acquire_cached`] instead and skip the
    /// upload entirely. Initial refcount is `1` so the caller's
    /// matching `drop_mesh` (paired with the first placement's
    /// despawn) leaves the entry at zero unless other placements have
    /// since acquired it. See #879 / CELL-PERF-01.
    pub fn register_scene_mesh_keyed(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
        model_path: &str,
        sub_mesh_index: u32,
    ) -> Result<u32> {
        let handle = self.upload_scene_mesh(
            device,
            allocator,
            queue,
            command_pool,
            vertices,
            indices,
            rt_enabled,
            staging_pool,
        )?;
        self.mesh_cache
            .insert((model_path.to_string(), sub_mesh_index), handle);
        Ok(handle)
    }

    /// Live refcount for `handle`, or `None` if the slot is empty
    /// (never allocated or already freed — refcount == 0). Read-only
    /// — used by the cell-unload pre-pass (#879) to decide whether
    /// dropping all holders in this cell will actually free the GPU
    /// buffer (so it can run BLAS detach exactly once for those
    /// handles, preserving the BLAS-before-mesh ordering invariant
    /// from #372).
    pub fn refcount(&self, handle: u32) -> Option<u32> {
        self.mesh_ref_counts
            .get(handle as usize)
            .copied()
            .filter(|&rc| rc > 0)
    }

    /// Decrement a holder's reference. Returns `true` iff this call
    /// took the refcount from 1 → 0 and queued the GPU buffers for
    /// deferred destruction. Returns `false` when other holders still
    /// reference the mesh (refcount stayed positive) or the handle is
    /// already dropped / never allocated.
    ///
    /// Per-mesh vertex/index buffers are queued for deferred
    /// destruction (2 frames, matching `MAX_FRAMES_IN_FLIGHT`) on the
    /// last release so no in-flight command buffer that still
    /// references them can use-after-free. Scene meshes additionally
    /// mark the global SSBO dirty — the next `rebuild_geometry_ssbo`
    /// call will compact the dead mesh's range out of
    /// `pending_vertices`/`pending_indices` and rewrite live meshes'
    /// offsets. See #372 (handle stability) and #879 (refcount).
    ///
    /// Handles stay stable: the dropped slot holds `None` forever.
    /// Re-using a handle would re-enter the same `GpuInstance.mesh_id`
    /// for a different mesh and produce silent data corruption.
    pub fn drop_mesh(&mut self, handle: u32) -> bool {
        let idx = handle as usize;
        let rc = match self.mesh_ref_counts.get_mut(idx) {
            Some(rc) => rc,
            None => return false,
        };
        if *rc == 0 {
            log::warn!(
                "drop_mesh({}) on already-released handle (ref_count was 0)",
                handle,
            );
            return false;
        }
        *rc -= 1;
        if *rc > 0 {
            return false;
        }

        // Last holder released — perform the GPU-side drop. Take the
        // owned buffers (if present) and queue for 2-frame deferred
        // destruction. The `meshes` slot may be empty in test-only
        // synthetic scenarios that populate `mesh_ref_counts` /
        // `mesh_cache` directly without uploading real GPU buffers;
        // the production `upload` paths always push a paired entry.
        let mut was_scene_mesh = false;
        if let Some(slot) = self.meshes.get_mut(idx) {
            if let Some(mesh) = slot.take() {
                was_scene_mesh = mesh.is_scene_mesh;
                self.deferred_destroy
                    .push((Some(mesh.vertex_buffer), Some(mesh.index_buffer), 2));
            }
        }
        if was_scene_mesh {
            self.geometry_dirty = true;
        }

        // Purge any cache entries pointing at this freed handle so a
        // subsequent `acquire_cached` for the same path doesn't return
        // a dangling slot. Linear scan is fine: cell unloads are rare
        // relative to per-frame draws. Mirrors
        // `TextureRegistry::release_ref`'s `path_map.retain`.
        self.mesh_cache.retain(|_, &mut h| h != handle);

        true
    }

    /// Compact `pending_vertices`/`pending_indices` to contain only live
    /// scene meshes' data, and rewrite each survivor's
    /// `global_vertex_offset`/`global_index_offset` to its new position.
    ///
    /// Called implicitly by [`rebuild_geometry_ssbo`](Self::rebuild_geometry_ssbo)
    /// when any scene mesh has been dropped. Safe to call with no drops
    /// — it exits early if no dead slots are present.
    fn compact_pending_geometry(&mut self) {
        // Fast path: no holes → nothing to compact.
        let any_dead = self.meshes.iter().any(|slot| slot.is_none());
        if !any_dead {
            return;
        }

        let mut new_vertices: Vec<Vertex> = Vec::with_capacity(self.pending_vertices.len());
        let mut new_indices: Vec<u32> = Vec::with_capacity(self.pending_indices.len());

        // Snapshot old offsets so we can rewrite them without aliasing.
        for slot in self.meshes.iter_mut() {
            let Some(mesh) = slot.as_mut() else { continue };
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

            mesh.global_vertex_offset = new_v_offset;
            mesh.global_index_offset = new_i_offset;
        }

        self.pending_vertices = new_vertices;
        self.pending_indices = new_indices;
    }

    /// Build the global geometry SSBO from accumulated vertex/index data.
    /// Call once after all scene meshes are loaded.
    ///
    /// When `staging_pool` is `Some`, the staging buffer is reused from the
    /// pool instead of allocating a fresh one. This avoids a large
    /// fire-and-forget staging allocation on cell loads. See #242.
    pub fn build_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<()> {
        if self.pending_vertices.is_empty() {
            return Ok(());
        }

        let vertex_size =
            (std::mem::size_of::<Vertex>() * self.pending_vertices.len()) as vk::DeviceSize;
        let index_size =
            (std::mem::size_of::<u32>() * self.pending_indices.len()) as vk::DeviceSize;

        // Create with STORAGE_BUFFER (RT reflection UV lookups) plus
        // VERTEX_BUFFER / INDEX_BUFFER so the draw loop can bind this
        // single global buffer instead of per-mesh rebinding. See #294.
        let mut pool = staging_pool;
        self.global_vertex_buffer = Some(GpuBuffer::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            vertex_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER,
            &self.pending_vertices,
            pool.as_deref_mut(),
        )?);
        self.global_index_buffer = Some(GpuBuffer::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER,
            &self.pending_indices,
            pool.as_deref_mut(),
        )?);

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
        self.geometry_dirty = false;

        Ok(())
    }

    /// Rebuild the global geometry SSBO after new meshes have been loaded.
    /// Destroys the old SSBO and creates a new one from all accumulated
    /// vertex/index data. Only call when `is_geometry_dirty()` returns true.
    ///
    /// This is the simple "full rebuild" path — acceptable for infrequent
    /// cell transitions. A future streaming optimization could append
    /// in-place with buffer resize. See #258.
    pub fn rebuild_geometry_ssbo(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<()> {
        // If any scene meshes were dropped since the last build, compact
        // the pending buffers and rewrite every live mesh's offsets. Pure
        // appends (no drops) skip this pass.
        self.compact_pending_geometry();

        // Defer destruction of old SSBOs instead of stalling with
        // device_wait_idle. The old buffers survive for MAX_FRAMES_IN_FLIGHT
        // frames, guaranteeing no in-flight command buffer references them
        // when they're finally destroyed. The descriptor set bindings 8/9
        // will be updated to the new SSBO in the same frame this is called.
        let old_vb = self.global_vertex_buffer.take();
        let old_ib = self.global_index_buffer.take();
        if old_vb.is_some() || old_ib.is_some() {
            // Countdown of 2 frames (MAX_FRAMES_IN_FLIGHT) ensures safety.
            self.deferred_destroy.push((old_vb, old_ib, 2));
        }

        log::info!(
            "Rebuilding geometry SSBO: {} → {} vertices",
            self.ssbo_vertex_count,
            self.pending_vertices.len(),
        );

        // Rebuild from all accumulated data.
        self.build_geometry_ssbo(device, allocator, queue, command_pool, staging_pool)
    }

    /// Returns true when new meshes have been loaded since the last SSBO
    /// build. The frame loop should call `rebuild_geometry_ssbo` to update.
    pub fn is_geometry_dirty(&self) -> bool {
        self.geometry_dirty
    }

    pub fn get(&self, id: u32) -> Option<&GpuMesh> {
        self.meshes.get(id as usize).and_then(|slot| slot.as_ref())
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn destroy_all(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for slot in &mut self.meshes {
            if let Some(mesh) = slot.as_mut() {
                mesh.destroy(device, allocator);
            }
        }
        self.meshes.clear();
        // Refcount table is parallel-indexed; clear in lockstep with
        // `meshes`. Leaving the counts populated would let a stale
        // cache lookup post-shutdown bump a refcount on a freed slot.
        self.mesh_ref_counts.clear();
        if let Some(ref mut vb) = self.global_vertex_buffer {
            vb.destroy(device, allocator);
        }
        if let Some(ref mut ib) = self.global_index_buffer {
            ib.destroy(device, allocator);
        }
        self.global_vertex_buffer = None;
        self.global_index_buffer = None;
        // The shared mesh-cache map only holds handle indices; the
        // backing GPU buffers were already torn down by the per-slot
        // `mesh.destroy` loop above. Clear the map so a post-shutdown
        // `acquire_cached` can't hand out a dangling handle. See #879.
        self.mesh_cache.clear();
        // Drain deferred-destroy list. #732 factored the body into
        // `drain_deferred_destroy` so the App-level shutdown sweep can
        // call the same drain explicitly before `Drop`.
        self.drain_deferred_destroy(device, allocator);
    }
}

/// Colored cube geometry: 24 vertices (4 per face), 36 indices, with UVs and normals.
pub fn cube_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        // Front face (red-ish), normal = +Z
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 0.3, 0.3],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 0.5],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
        // Back face (blue-ish), normal = -Z
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 0.3, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0],
        ),
        // Top face (green-ish), normal = +Y
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 0.3],
            [0.0, 1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [0.5, 1.0, 0.5],
            [0.0, 1.0, 0.0],
            [0.0, 0.0],
        ),
        // Bottom face (yellow-ish), normal = -Y
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, -0.5],
            [1.0, 1.0, 0.3],
            [0.0, -1.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 1.0, 0.5],
            [0.0, -1.0, 0.0],
            [0.0, 1.0],
        ),
        // Right face (cyan-ish), normal = +X
        Vertex::new(
            [0.5, -0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, -0.5],
            [0.3, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.5],
            [0.5, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        // Left face (magenta-ish), normal = -X
        Vertex::new(
            [-0.5, -0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [-0.5, 0.5, -0.5],
            [1.0, 0.3, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        Vertex::new(
            [-0.5, -0.5, 0.5],
            [1.0, 0.5, 1.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0],
        ),
    ];

    let indices = vec![
        0, 1, 2, 2, 3, 0, // front
        4, 6, 5, 6, 4, 7, // back
        8, 9, 10, 10, 11, 8, // top
        12, 14, 13, 14, 12, 15, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 22, 21, 22, 20, 23, // left
    ];

    (vertices, indices)
}

/// A single colored triangle in the XY plane at Z=0, with UVs and normals.
pub fn triangle_vertices(color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new([0.0, 0.5, 0.0], color, [0.0, 0.0, 1.0], [0.5, 0.0]),
        Vertex::new([-0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [0.0, 1.0]),
        Vertex::new([0.5, -0.5, 0.0], color, [0.0, 0.0, 1.0], [1.0, 1.0]),
    ];
    let indices = vec![0, 1, 2];
    (vertices, indices)
}

/// A textured quad in the XY plane at Z=0, with normals.
pub fn quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new(
            [-0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [0.5, -0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-0.5, 0.5, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Fullscreen quad in NDC (clip space [-1,1]), for UI overlay compositing.
/// No transforms needed — vertices pass through directly to clip space.
pub fn fullscreen_quad_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let vertices = vec![
        Vertex::new(
            [-1.0, -1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0],
        ),
        Vertex::new(
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0],
        ),
        Vertex::new(
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0],
        ),
        Vertex::new(
            [-1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

/// Lightweight fullscreen quad for UI overlay — position + UV only (20 B/vertex).
pub fn fullscreen_quad_ui_vertices() -> (Vec<UiVertex>, Vec<u32>) {
    let vertices = vec![
        UiVertex::new([-1.0, -1.0, 0.0], [0.0, 1.0]),
        UiVertex::new([1.0, -1.0, 0.0], [1.0, 1.0]),
        UiVertex::new([1.0, 1.0, 0.0], [1.0, 0.0]),
        UiVertex::new([-1.0, 1.0, 0.0], [0.0, 0.0]),
    ];
    let indices = vec![0, 1, 2, 2, 3, 0];
    (vertices, indices)
}

#[cfg(test)]
mod drain_tests {
    use super::*;

    /// Regression for #732 / LIFE-H2: `deferred_destroy_count` must
    /// reflect every queued row (regardless of per-row countdown) so
    /// the shutdown sweep can assert "zero pending after drain"
    /// without paying the integration-test setup of a live Vulkan
    /// device. Real `drain_deferred_destroy` invocation is exercised
    /// by the integration path in
    /// `byroredux::main::WindowEvent::CloseRequested`; this is the
    /// pure-Rust pin against the counter accessor's accuracy.
    #[test]
    fn deferred_destroy_count_pins_to_queue_length() {
        let mut reg = MeshRegistry::new();
        assert_eq!(reg.deferred_destroy_count(), 0);
        // Push three placeholder rows with countdowns 2 / 1 / 0.
        // `(None, None, n)` is the legitimate row shape for a mesh
        // whose vertex/index buffers were already taken — the queue
        // still tracks the row's countdown until the next tick or
        // drain.
        reg.deferred_destroy.push((None, None, 2));
        reg.deferred_destroy.push((None, None, 1));
        reg.deferred_destroy.push((None, None, 0));
        assert_eq!(reg.deferred_destroy_count(), 3);

        // Simulate the drain's post-condition (real drain calls
        // `destroy(device, allocator)` on each Some payload, which
        // needs Vulkan; the queue-clear half of the drain is the
        // shutdown-correctness invariant).
        reg.deferred_destroy.clear();
        assert_eq!(reg.deferred_destroy_count(), 0);
    }
}

#[cfg(test)]
mod refcount_tests {
    //! Regression tests for #879 / CELL-PERF-01: the refcounted
    //! GPU-mesh dedup layer (`acquire_cached` /
    //! `register_scene_mesh_keyed` / `drop_mesh` returning bool).
    //!
    //! Real `register_scene_mesh_keyed` requires a live
    //! `VkDevice` + `SharedAllocator`; these tests bypass the GPU
    //! storage entirely by populating only the parallel
    //! `mesh_ref_counts` vec + `mesh_cache` map. Because `ref_count`
    //! lives in its own vec (rather than as a field on `GpuMesh`),
    //! the bookkeeping is exercisable without synthesising a
    //! `GpuMesh` (whose `ash::Device` Arc fields can't be safely
    //! zero-initialised). The end-to-end integration is covered by
    //! the live cell-load path (`spawn_placed_instances`) every
    //! time the engine loads a real cell.
    use super::*;

    /// Install a synthetic refcount slot for `(model_path,
    /// sub_mesh_index)`. Returns the assigned handle. The
    /// corresponding `meshes` slot is left absent (None) — production
    /// `drop_mesh` handles the missing-buffer case gracefully so the
    /// pure-Rust refcount path still exercises end-to-end.
    fn install_synthetic_slot(
        reg: &mut MeshRegistry,
        model_path: &str,
        sub_mesh_index: u32,
        initial_ref_count: u32,
    ) -> u32 {
        let handle = reg.mesh_ref_counts.len() as u32;
        reg.mesh_ref_counts.push(initial_ref_count);
        reg.mesh_cache
            .insert((model_path.to_string(), sub_mesh_index), handle);
        handle
    }

    /// Empty registry: every probe returns the no-op.
    #[test]
    fn empty_registry_returns_none_for_all_probes() {
        let mut reg = MeshRegistry::new();
        assert_eq!(reg.acquire_cached("chair.nif", 0), None);
        assert_eq!(reg.refcount(0), None);
        assert!(!reg.drop_mesh(0), "drop on unknown handle is a no-op");
    }

    /// 40 chairs sharing one `chair.nif` cache entry: the first
    /// `register_scene_mesh_keyed` (simulated via direct slot
    /// install at refcount 1) is followed by 39 `acquire_cached`
    /// hits that bump the count to 40 without re-uploading. Each
    /// placement's `drop_mesh` decrements once; the 40th finally
    /// frees and returns `true` so the unload path runs `drop_blas`
    /// for that handle exactly once.
    #[test]
    fn shared_cache_hits_bump_refcount_and_only_last_drop_frees() {
        let mut reg = MeshRegistry::new();
        // First placement: ref_count = 1.
        let handle = install_synthetic_slot(&mut reg, "chair.nif", 0, 1);
        assert_eq!(reg.refcount(handle), Some(1));

        // 39 subsequent placements share the cached handle.
        for expected in 2..=40u32 {
            let h = reg
                .acquire_cached("chair.nif", 0)
                .expect("cache hit must return the same handle");
            assert_eq!(h, handle, "shared placements must dedup to one handle");
            assert_eq!(reg.refcount(handle), Some(expected));
        }

        // First 39 drops decrement but DO NOT free. `drop_mesh`
        // returns false so the unload path skips `drop_blas` for
        // these calls — preserving the BLAS for the 40th holder.
        for expected in (1..40u32).rev() {
            assert!(
                !reg.drop_mesh(handle),
                "intermediate drop must not free (refcount > 0)",
            );
            assert_eq!(reg.refcount(handle), Some(expected));
        }

        // 40th drop hits zero. Returns true → unload signals
        // `drop_blas` exactly once. The cache entry is purged so a
        // future `acquire_cached` for the same path can never
        // return this freed handle.
        assert!(reg.drop_mesh(handle), "last drop must free");
        assert_eq!(reg.refcount(handle), None);
        assert_eq!(reg.acquire_cached("chair.nif", 0), None);
    }

    /// Multi-mesh NIF (`(path, sub_mesh_index)` pairs): two distinct
    /// sub-meshes get distinct handles even when they share a path.
    /// Pins that the cache key disambiguates sub-meshes correctly.
    #[test]
    fn distinct_sub_mesh_indices_get_distinct_handles() {
        let mut reg = MeshRegistry::new();
        let body = install_synthetic_slot(&mut reg, "corpse.nif", 0, 1);
        let helmet = install_synthetic_slot(&mut reg, "corpse.nif", 1, 1);
        assert_ne!(body, helmet, "different sub_mesh_index → different handle");

        // Acquiring sub_mesh 0 must not affect sub_mesh 1.
        let body2 = reg
            .acquire_cached("corpse.nif", 0)
            .expect("sub_mesh 0 cache hit");
        assert_eq!(body2, body);
        assert_eq!(reg.refcount(body), Some(2));
        assert_eq!(reg.refcount(helmet), Some(1));

        // Drop body twice (initial install + acquire) → freed.
        assert!(!reg.drop_mesh(body));
        assert!(reg.drop_mesh(body));
        assert_eq!(reg.refcount(body), None);
        // Helmet untouched.
        assert_eq!(reg.refcount(helmet), Some(1));
    }

    /// `drop_mesh` past zero is a logged no-op (returns false), not
    /// a panic. Pre-fix `drop_mesh` had no refcount and panicked on
    /// `slot.take()` when called twice on the same handle; the new
    /// path returns false on a 0-refcount probe.
    #[test]
    fn drop_past_zero_is_a_warning_not_a_panic() {
        let mut reg = MeshRegistry::new();
        let handle = install_synthetic_slot(&mut reg, "stub.nif", 0, 1);
        assert!(reg.drop_mesh(handle));
        // Second call: refcount already 0, slot already empty.
        assert!(!reg.drop_mesh(handle));
    }

    /// After the last release, an attempt to `acquire_cached` on
    /// the same key must NOT bump the count back from zero — that
    /// would resurrect a freed handle. The path is treated as a
    /// miss so the caller falls through to a fresh upload. The
    /// purge in `drop_mesh` removes the cache entry, but this also
    /// pins the secondary defence: a stale lookup that races with
    /// the purge still observes refcount == 0 and bails.
    #[test]
    fn stale_cache_lookup_does_not_resurrect_freed_handle() {
        let mut reg = MeshRegistry::new();
        let handle = install_synthetic_slot(&mut reg, "stale.nif", 0, 1);
        assert!(reg.drop_mesh(handle), "last release frees the slot");

        // Re-insert a stale cache entry (simulating a hypothetical
        // race where the cache map outlived the purge). The 0-rc
        // gate must reject it.
        reg.mesh_cache
            .insert(("stale.nif".to_string(), 0), handle);
        assert_eq!(reg.acquire_cached("stale.nif", 0), None);
        assert_eq!(reg.refcount(handle), None);
    }
}
