//! Mesh registry — maps MeshHandle IDs to GPU buffers.

use crate::deferred_destroy::{DeferredDestroyQueue, DEFAULT_COUNTDOWN};
use crate::vertex::{UiVertex, Vertex};
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::{GpuBuffer, StagingPool};
use anyhow::{bail, Result};
use ash::vk;
use std::collections::HashMap;
use std::sync::Once;

/// Defence-in-depth cap on the global vertex pool size. The pool grows
/// monotonically until `drop_mesh` (refcount → 0) lets `compact_pending_geometry`
/// rewrite it. A correct streaming session sees `pending_vertices` track the
/// resident scene's geometry; a broken cell-unload path leaks placements and
/// grows the pool unbounded.
///
/// Soft cap (~400 MB at `Vertex` = 100 B post-M-NORMALS) fires a one-shot
/// `warn!` so a regression in cell unload becomes visible without crashing
/// the engine. Hard cap (~1.6 GB) returns `Err` from `upload_scene_mesh` so
/// the caller can skip the placement and continue, rather than letting the
/// allocator OOM-panic mid-frame.
///
/// See REN-D2-005 / #1016. Mirrors the `MAX_INDIRECT_DRAWS` defence-in-depth
/// cap at `scene_buffer.rs:1326+` — these are not perf knobs, they are
/// safety guards against unbounded-growth bugs.
pub const VERTEX_POOL_SOFT_CAP: usize = 4_000_000;
pub const VERTEX_POOL_HARD_CAP: usize = 16_000_000;
/// Index pool caps — typical mesh ratio is ~3 indices per vertex, so the
/// caps here track the vertex caps proportionally.
pub const INDEX_POOL_SOFT_CAP: usize = 16_000_000;
pub const INDEX_POOL_HARD_CAP: usize = 64_000_000;

static VERTEX_POOL_SOFT_WARNED: Once = Once::new();
static INDEX_POOL_SOFT_WARNED: Once = Once::new();

/// Pure-function check — given the current pool length and the new
/// length after the proposed `extend_from_slice`, decide whether to
/// allow the growth (`Ok(soft_warn_needed)`), or reject it (`Err`).
///
/// Returns `true` in the `Ok` case when the soft cap was crossed by
/// this growth (caller should fire a one-shot warn). Returns `Err`
/// when the hard cap would be exceeded.
///
/// Pulled out of `upload_scene_mesh` so it can be unit-tested with
/// arbitrary cap values without allocating gigabytes of vertex data.
pub(crate) fn check_pool_growth(
    current_len: usize,
    new_len: usize,
    soft_cap: usize,
    hard_cap: usize,
    label: &'static str,
) -> Result<bool> {
    if new_len > hard_cap {
        bail!(
            "{label} pool hard cap exceeded: would grow from {current_len} to {new_len} \
             (cap {hard_cap}). Likely a leaked cell unload — placements were uploaded \
             without a matching `drop_mesh`. See REN-D2-005 / #1016.",
        );
    }
    let crossed_soft = current_len <= soft_cap && new_len > soft_cap;
    Ok(crossed_soft)
}

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
    /// Old per-mesh GPU buffers awaiting deferred destruction. Each
    /// entry is a `(vertex, index)` pair (both `Option` because some
    /// drop paths take only one buffer at a time). The countdown is
    /// owned by the queue primitive — it survives
    /// MAX_FRAMES_IN_FLIGHT frames before destruction so no in-flight
    /// command buffer can reference the freed memory.
    deferred_destroy: DeferredDestroyQueue<(Option<GpuBuffer>, Option<GpuBuffer>)>,
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
    /// Staging pool reused across global-geometry-SSBO builds and
    /// rebuilds. Lazy-initialised on the first `build_geometry_ssbo`
    /// call because `MeshRegistry::new()` runs before the device is
    /// available; once created, the pool's retained capacity is
    /// recycled per #242's hit-rate target. Pre-#1055 both
    /// `build_geometry_ssbo` and `rebuild_geometry_ssbo` accepted
    /// `Option<&mut StagingPool>` and the two consumer sites always
    /// passed `None`, leaving the whole large-scene rebuild path on
    /// the per-call create/destroy fallback. Mirrors
    /// `TextureRegistry::staging_pool`.
    geometry_staging_pool: Option<StagingPool>,
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
            deferred_destroy: DeferredDestroyQueue::new(),
            mesh_cache: HashMap::new(),
            mesh_ref_counts: Vec::new(),
            geometry_staging_pool: None,
        }
    }

    /// Tick the deferred-destroy list. Call once per frame. Destroys old
    /// SSBOs whose countdown has reached zero (safe because all in-flight
    /// command buffers referencing them have completed).
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.deferred_destroy.tick(|(vb, ib)| {
            if let Some(mut b) = vb {
                b.destroy(device, allocator);
            }
            if let Some(mut b) = ib {
                b.destroy(device, allocator);
            }
        });
    }

    /// Drain `deferred_destroy` synchronously regardless of countdown.
    /// Counterpart of [`Self::tick_deferred_destroy`] for the shutdown
    /// path where no future frames will tick the countdown. Caller must
    /// have already called `device_wait_idle` so the queued buffers
    /// can't be in-flight. See #732 / LIFE-H2.
    pub fn drain_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.deferred_destroy.drain(|(vb, ib)| {
            if let Some(mut b) = vb {
                b.destroy(device, allocator);
            }
            if let Some(mut b) = ib {
                b.destroy(device, allocator);
            }
        });
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
    ///
    /// `rt_enabled = false` skips the
    /// `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` usage flag on
    /// the vertex/index buffers, which prevents the caller from ever
    /// building a BLAS over this mesh. Water plane meshes are uploaded
    /// with `rt_enabled = false` (see
    /// `byroredux::cell_loader::water::spawn_water_plane`) so they
    /// never enter the BLAS pool — the mesh-side half of the water
    /// TLAS-exclusion contract documented on
    /// `crates/renderer/src/vulkan/context/mod.rs::DrawCommand::is_water`
    /// (#1024 / F-WAT-03). The TLAS-build path enforces the same
    /// contract from the draw side via the `is_water` flag, so a
    /// future code path adding water to BLAS can't silently
    /// reintroduce ray self-hits.
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

        // Defence-in-depth growth caps (#1016 / REN-D2-005). A correct
        // streaming session keeps `pending_vertices`/`pending_indices`
        // bounded via the cell-unload `drop_mesh` path; these caps catch
        // a regression in that path before the allocator OOMs.
        let new_v_len = self.pending_vertices.len() + vertices.len();
        let new_i_len = self.pending_indices.len() + indices.len();
        let v_warn = check_pool_growth(
            self.pending_vertices.len(),
            new_v_len,
            VERTEX_POOL_SOFT_CAP,
            VERTEX_POOL_HARD_CAP,
            "vertex",
        )?;
        let i_warn = check_pool_growth(
            self.pending_indices.len(),
            new_i_len,
            INDEX_POOL_SOFT_CAP,
            INDEX_POOL_HARD_CAP,
            "index",
        )?;
        if v_warn {
            VERTEX_POOL_SOFT_WARNED.call_once(|| {
                log::warn!(
                    "Global vertex pool crossed soft cap ({VERTEX_POOL_SOFT_CAP} verts \
                     ≈ {} MB). A correct cell-unload flow keeps this bounded; this warn \
                     is a one-shot heads-up that the resident scene grew larger than \
                     expected. Hard cap {VERTEX_POOL_HARD_CAP} returns Err. \
                     See REN-D2-005 / #1016.",
                    VERTEX_POOL_SOFT_CAP * std::mem::size_of::<Vertex>() / 1_000_000,
                );
            });
        }
        if i_warn {
            INDEX_POOL_SOFT_WARNED.call_once(|| {
                log::warn!(
                    "Global index pool crossed soft cap ({INDEX_POOL_SOFT_CAP} indices \
                     ≈ {} MB). See REN-D2-005 / #1016.",
                    INDEX_POOL_SOFT_CAP * 4 / 1_000_000,
                );
            });
        }

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
                self.deferred_destroy.push(
                    (Some(mesh.vertex_buffer), Some(mesh.index_buffer)),
                    DEFAULT_COUNTDOWN,
                );
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
    ) -> Result<()> {
        if self.pending_vertices.is_empty() {
            return Ok(());
        }

        let vertex_size =
            (std::mem::size_of::<Vertex>() * self.pending_vertices.len()) as vk::DeviceSize;
        let index_size =
            (std::mem::size_of::<u32>() * self.pending_indices.len()) as vk::DeviceSize;

        if self.geometry_staging_pool.is_none() {
            self.geometry_staging_pool =
                Some(StagingPool::new(device.clone(), allocator.clone()));
        }

        // Create with STORAGE_BUFFER (RT reflection UV lookups) plus
        // VERTEX_BUFFER / INDEX_BUFFER so the draw loop can bind this
        // single global buffer instead of per-mesh rebinding. See #294.
        self.global_vertex_buffer = Some(GpuBuffer::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            vertex_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER,
            &self.pending_vertices,
            self.geometry_staging_pool.as_mut(),
        )?);
        self.global_index_buffer = Some(GpuBuffer::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER,
            &self.pending_indices,
            self.geometry_staging_pool.as_mut(),
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
            self.deferred_destroy
                .push((old_vb, old_ib), DEFAULT_COUNTDOWN);
        }

        log::info!(
            "Rebuilding geometry SSBO: {} → {} vertices",
            self.ssbo_vertex_count,
            self.pending_vertices.len(),
        );

        // Rebuild from all accumulated data. The internal
        // `geometry_staging_pool` (lazy-initialised in `build_geometry_ssbo`
        // on first call, then reused) keeps the staging-buffer churn bounded.
        self.build_geometry_ssbo(device, allocator, queue, command_pool)
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
        // #1055 — release the geometry-build StagingPool's retained
        // buffer + the pool's `Arc<Mutex<Allocator>>` clone. Same
        // shape as `TextureRegistry::destroy`'s pool teardown — the
        // `take()` form (not `as_mut()`) drops the clone so
        // `Arc::try_unwrap` on the parent `VulkanContext::Drop` can
        // finally release the allocator.
        if let Some(mut pool) = self.geometry_staging_pool.take() {
            pool.destroy();
        }
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
mod pool_growth_cap_tests {
    //! Regression tests for #1016 / REN-D2-005: defence-in-depth caps
    //! on `pending_vertices` / `pending_indices` growth. The pure-
    //! function `check_pool_growth` is exercised here with mock cap
    //! values so the test doesn't need to allocate gigabytes.
    use super::*;

    #[test]
    fn growth_below_soft_cap_is_clean() {
        let warned = check_pool_growth(0, 100, 1000, 2000, "vertex").unwrap();
        assert!(!warned, "growth fully under soft cap must not warn");
    }

    #[test]
    fn growth_crossing_soft_cap_signals_warn() {
        // 900 → 1100 crosses soft cap 1000.
        let warned = check_pool_growth(900, 1100, 1000, 2000, "vertex").unwrap();
        assert!(warned, "growth that crosses soft cap must signal warn");
    }

    #[test]
    fn second_growth_beyond_soft_cap_does_not_re_signal() {
        // 1500 → 1600 is fully past soft cap 1000; the warn was already
        // signalled on the crossing growth, this growth should be silent.
        let warned = check_pool_growth(1500, 1600, 1000, 2000, "vertex").unwrap();
        assert!(
            !warned,
            "growth fully above soft cap (already warned) must NOT re-signal"
        );
    }

    #[test]
    fn growth_exceeding_hard_cap_returns_err() {
        // 1500 → 2100 exceeds hard cap 2000.
        let result = check_pool_growth(1500, 2100, 1000, 2000, "vertex");
        assert!(result.is_err(), "growth past hard cap must return Err");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("hard cap"),
            "err message should mention hard cap; got: {err_msg}",
        );
        assert!(
            err_msg.contains("REN-D2-005"),
            "err message should reference the issue id for grep-ability; got: {err_msg}",
        );
    }

    #[test]
    fn growth_landing_exactly_at_hard_cap_is_allowed() {
        // 1500 → 2000 is exactly at hard cap — within bounds.
        let result = check_pool_growth(1500, 2000, 1000, 2000, "vertex");
        assert!(
            result.is_ok(),
            "growth landing exactly at hard cap must be allowed"
        );
    }

    #[test]
    fn shipping_caps_have_sane_relative_sizing() {
        // The hard caps must be strictly greater than the soft caps,
        // and both must fit in usize comfortably (defence against a
        // future edit accidentally setting hard < soft).
        assert!(VERTEX_POOL_HARD_CAP > VERTEX_POOL_SOFT_CAP);
        assert!(INDEX_POOL_HARD_CAP > INDEX_POOL_SOFT_CAP);
        // At Vertex = 100 B, hard cap 16M = 1.6 GB. At u32 indices,
        // hard cap 64M = 256 MB. Sanity-check: vertex cap is the bigger
        // memory commitment of the two.
        let vertex_bytes = VERTEX_POOL_HARD_CAP * std::mem::size_of::<Vertex>();
        let index_bytes = INDEX_POOL_HARD_CAP * 4;
        assert!(
            vertex_bytes > index_bytes,
            "vertex cap should be larger memory budget than index cap (got {} vs {} bytes)",
            vertex_bytes,
            index_bytes,
        );
    }
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
    ///
    /// Generic queue mechanics (tick / drain semantics across mixed
    /// countdowns) are exercised by `deferred_destroy::tests` since
    /// the consolidation into `DeferredDestroyQueue<T>`. This test
    /// pins that `MeshRegistry`'s `deferred_destroy_count()` accessor
    /// stays in lockstep with the underlying queue's `len()` —
    /// shutdown telemetry consumes it.
    #[test]
    fn deferred_destroy_count_pins_to_queue_length() {
        let mut reg = MeshRegistry::new();
        assert_eq!(reg.deferred_destroy_count(), 0);
        // Push three placeholder rows with mixed countdowns.
        // `(None, None)` is the legitimate row shape for a mesh
        // whose vertex/index buffers were already taken — the queue
        // still tracks the row until the next tick or drain.
        reg.deferred_destroy.push((None, None), 2);
        reg.deferred_destroy.push((None, None), 1);
        reg.deferred_destroy.push((None, None), 0);
        assert_eq!(reg.deferred_destroy_count(), 3);

        // Drain via the primitive's `drain` (no destroyer side
        // effects needed here — the rows hold no GPU resources).
        reg.deferred_destroy.drain(|_| ());
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
        reg.mesh_cache.insert(("stale.nif".to_string(), 0), handle);
        assert_eq!(reg.acquire_cached("stale.nif", 0), None);
        assert_eq!(reg.refcount(handle), None);
    }
}
