//! Mesh registry — maps MeshHandle IDs to GPU buffers.

use crate::deferred_destroy::{DeferredDestroyQueue, DEFAULT_COUNTDOWN};
use crate::vertex::{UiVertex, Vertex};
use crate::vulkan::allocator::SharedAllocator;
use crate::vulkan::buffer::{DeviceLocalBufferUpload, GpuBuffer, StagingPool};
use crate::vulkan::GpuUploadCtx;
use anyhow::{bail, Context, Result};
use ash::vk;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Once;

/// Defence-in-depth cap on the global vertex pool size. The pool grows
/// monotonically until `drop_mesh` (refcount → 0) lets `compact_pending_geometry`
/// rewrite it. A correct streaming session sees `pending_vertices` track the
/// resident scene's geometry; a broken cell-unload path leaks placements and
/// grows the pool unbounded.
///
/// Soft cap (~416 MB at `Vertex` = 104 B) fires a one-shot
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

/// Large global-geometry rebuilds cannot safely keep two prior SSBO
/// generations alive while allocating the replacement on mid-range GPUs.
/// Above 256 MiB, prefer a one-time device-idle reclamation over a recoverable
/// allocation failure escalating into `VK_ERROR_DEVICE_LOST` (FO4 boundary
/// traversal, #2374). EX-07 tracks replacing this safety path with a
/// capacity-managed append/update buffer that remains fully asynchronous.
pub const GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES: u64 = 256 * 1024 * 1024;

/// Per-`advance_geometry_rebuild`-call byte budget for a resumable global
/// geometry SSBO copy (#3298). Chosen conservatively pending live
/// `grid-cross` tuning against real FO4/Skyrim/FNV data — the 1.50 s
/// worst-frame figure this replaces came from one atomic ~600 MiB copy, so
/// this value trades total elapsed time (unchanged) for a bounded
/// per-frame slice of it. Not a `FrameTimeBudget`-style wall-clock deadline:
/// a submitted `vkCmdCopyBuffer` cannot be paused mid-flight, so the unit of
/// pacing here is bytes-per-call, converted to a whole-element count per
/// phase (`Vertex` for the vertex phase, `u32` for the index phase) so
/// every chunk's offset and size stay 4-byte aligned as `vkCmdCopyBuffer`
/// requires.
pub const GEOMETRY_REBUILD_CHUNK_BYTES: usize = 64 * 1024 * 1024;

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
/// rebuild's duration. This is an accepted trade-off (#3298): it smooths a
/// multi-hundred-ms atomic stall into several bounded per-frame chunks, at
/// the cost of a temporarily higher VRAM high-water mark. If the up-front
/// allocation for the second generation fails (no headroom), the caller
/// (`MeshRegistry::rebuild_geometry_ssbo`) falls back to the original
/// atomic idle-reclaim-then-build path unchanged — #2374's device-loss
/// protection stays intact as a fallback, not the common case.
/// A computed-but-unpublished geometry compaction (#3372).
///
/// Produced by `plan_geometry_compaction`, applied by `apply_compaction_plan`.
/// The chunked rebuild carries one of these for the whole multi-frame copy so
/// the compacted offsets become visible in the same step that binds the
/// compacted buffer.
#[derive(Debug, Clone)]
struct CompactionPlan {
    /// `(mesh slot index, new global_vertex_offset, new global_index_offset)`
    /// for every scene mesh live at plan time.
    offsets: Vec<(usize, u32, u32)>,
    /// `meshes.len()` at plan time. Slots at or past this index were appended
    /// *after* the plan, so they already carry compacted-layout offsets and
    /// must stay out of raster/TLAS until swap-in — see `is_geometry_resident`.
    mesh_count: usize,
}

struct GeometryRebuildInProgress {
    new_vertex_buffer: GpuBuffer,
    new_index_buffer: GpuBuffer,
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

/// Whether a scene mesh's range is safe to draw against the currently bound
/// global geometry generation. Pure — no `self` — so the rule is unit-testable
/// without a live device, mirroring [`next_geometry_rebuild_chunk`] and the
/// `acceleration/predicates.rs` pattern.
///
/// `deferred_plan_mesh_count` is `Some(n)` while a compaction is computed but
/// unpublished (#3372): the pools have already shrunk, so anything uploaded at
/// or past slot `n` carries a compacted-layout offset that can land *inside*
/// the still-bound uncompacted buffer. The extent check alone would wave it
/// through to read another mesh's bytes, so those latecomers are held out of
/// raster/TLAS until swap-in. Slots are never reused (#372), which is what
/// makes the index an exact test.
fn scene_geometry_resident(
    handle: usize,
    vertex_end: usize,
    index_end: usize,
    ssbo_vertex_count: usize,
    ssbo_index_count: usize,
    deferred_plan_mesh_count: Option<usize>,
) -> bool {
    if deferred_plan_mesh_count.is_some_and(|n| handle >= n) {
        return false;
    }
    vertex_end <= ssbo_vertex_count && index_end <= ssbo_index_count
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
        let end =
            (vertices_copied + vertex_chunk_elems.max(1)).min(target_vertex_count);
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

static VERTEX_POOL_SOFT_WARNED: Once = Once::new();
static INDEX_POOL_SOFT_WARNED: Once = Once::new();

/// Hard cap on the number of mesh handle slots. Slot IDs are cast to
/// `u32`; this constant keeps that cast safe.
///
/// #2035 / MEM-D3-02 — slots are grow-only and never reused: a dropped
/// slot's entry holds `None` forever ([`MeshRegistry::drop_mesh`]).
/// Reusing a handle would re-enter the same `GpuInstance.mesh_id` for a
/// different mesh and produce silent data corruption, so the 16 M
/// ceiling here — not slot reuse — is what keeps a long streaming
/// session safe: every mesh ever loaded across the session's lifetime
/// gets its own permanent slot, and 16 M is far past any realistic
/// per-session unique-mesh count.
pub const MAX_MESH_SLOTS: u32 = 1 << 24; // 16 M

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

pub(crate) fn geometry_rebuild_needs_idle(
    projected_bytes: u64,
    has_existing_buffers: bool,
) -> bool {
    has_existing_buffers && projected_bytes >= GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES
}

/// Cache key for the refcounted scene-mesh dedup layer (#879). The
/// `path` is the lowercased model path (matches
/// `cell_loader::nif_import_registry`'s key); `sub_mesh_index` indexes
/// into a multi-mesh NIF so two `chair.nif` placements share the same
/// handle while a `corpse.nif`'s body + helmet sub-meshes get distinct
/// entries.
pub type MeshCacheKey = (String, u32);

/// One scene mesh participating in a shared upload submission.
///
/// The destination buffers remain per-mesh (BLAS and lifetime ownership are
/// unchanged); only their staging copies share a command buffer and fence.
pub struct SceneMeshUpload<'a> {
    pub vertices: &'a [Vertex],
    pub indices: &'a [u32],
    pub rt_enabled: bool,
    pub cache_key: Option<(&'a str, u32)>,
}

/// A mesh stored on the GPU: vertex + index buffers and index count.
pub struct GpuMesh {
    /// `None` for global-SSBO-only scene meshes (distant terrain/object LOD,
    /// #1370): they rasterize from the shared vertex/index buffers and carry
    /// no per-mesh allocations. Nearby LOD structure may still build a static
    /// BLAS from its subrange of those shared buffers; the per-mesh draw
    /// fallback continues to skip it. Every other upload path sets `Some`.
    pub vertex_buffer: Option<GpuBuffer>,
    pub index_buffer: Option<GpuBuffer>,
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
    /// Mirrors the `rt_enabled` argument this mesh was uploaded with —
    /// i.e. whether its per-mesh vertex/index buffers carry
    /// `SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`.
    ///
    /// **Load-bearing**: a BLAS may only be built over a mesh with this
    /// set. Taking the buffer device address of a mesh without it — or
    /// naming it as AS build-input geometry — violates
    /// VUID-VkBufferDeviceAddressInfo-buffer-02601 and
    /// VUID-vkCmdBuildAccelerationStructuresKHR-geometry-03673.
    ///
    /// The upload side deliberately clears this for effect-shader proxy
    /// volumes, decals, water, and global-only LOD because this flag describes
    /// dedicated-buffer eligibility. The bounded LOD shadow path validates and
    /// uses the RT-capable shared buffers separately. The static
    /// BLAS path has always honoured that via its own `for_rt` gate; the
    /// skinned path now reads this flag instead of assuming every
    /// skinned mesh is RT-capable.
    pub rt_capable: bool,
}

impl GpuMesh {
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if let Some(vb) = self.vertex_buffer.as_mut() {
            vb.destroy(device, allocator);
        }
        if let Some(ib) = self.index_buffer.as_mut() {
            ib.destroy(device, allocator);
        }
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
    /// #2743 — monotonic counter bumped every time `build_geometry_ssbo`
    /// allocates a new `global_vertex_buffer` / `global_index_buffer` pair
    /// (fresh build or `rebuild_geometry_ssbo`'s destroy-then-recreate).
    /// Vulkan does not guarantee non-dispatchable handle values are
    /// unique or non-recycled — `rebuild_geometry_ssbo`'s low-headroom
    /// `reclaim_before_rebuild` path destroys the old SSBO and allocates
    /// the replacement inside the same call, the max-probability recycle
    /// window. `SkinComputePipeline::dispatch`'s per-FIF descriptor cache
    /// keys on the raw `vk::Buffer` handle alone; folding this generation
    /// into that key lets a same-handle-different-generation rebuild be
    /// told apart from an unchanged buffer. See `Self::geometry_generation`.
    geometry_generation: u64,
    /// Set when `upload_scene_mesh` is called after the initial SSBO
    /// build — signals the frame loop to call `rebuild_geometry_ssbo`.
    geometry_dirty: bool,
    /// Set when a *scene* mesh is dropped, leaving its span stranded inside
    /// `pending_vertices`/`pending_indices`; cleared once
    /// `compact_pending_geometry` has squeezed those spans out.
    ///
    /// Deliberately NOT derivable from `meshes.iter().any(|s| s.is_none())`:
    /// slots are `None` *forever* by design (handle stability, #372), so that
    /// scan latches true on the first drop of any mesh — scene or not — and
    /// never clears, making compaction unconditional from then on (#2678).
    /// Also distinct from `geometry_dirty`, which appends set too and which
    /// therefore cannot express "a hole exists".
    geometry_has_holes: bool,
    /// Number of vertices in the SSBO at last build. Used to detect
    /// whether a rebuild is needed vs. the current pending state.
    ssbo_vertex_count: usize,
    /// Index-side companion to `ssbo_vertex_count`, used to keep newly
    /// appended meshes out of draws while a streaming transaction batches
    /// one coherent global-geometry rebuild.
    ssbo_index_count: usize,
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
    /// In-flight multi-frame global geometry SSBO rebuild (#3298). `None`
    /// when no rebuild is running. See [`GeometryRebuildInProgress`].
    geometry_rebuild: Option<GeometryRebuildInProgress>,
    /// A compaction whose offsets are computed but **not yet published**,
    /// because the chunked rebuild that will bind the compacted buffer is
    /// still copying. Published — and cleared — at swap-in. #3372.
    deferred_compaction: Option<CompactionPlan>,
}

impl Default for MeshRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            pending_vertices: Vec::new(),
            pending_indices: Vec::new(),
            global_vertex_buffer: None,
            global_index_buffer: None,
            geometry_generation: 0,
            geometry_dirty: false,
            geometry_has_holes: false,
            ssbo_vertex_count: 0,
            ssbo_index_count: 0,
            deferred_destroy: DeferredDestroyQueue::new(),
            mesh_cache: HashMap::new(),
            mesh_ref_counts: Vec::new(),
            geometry_staging_pool: None,
            geometry_rebuild: None,
            deferred_compaction: None,
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
        ctx: GpuUploadCtx,
        vertices: &[V],
        indices: &[u32],
        rt_enabled: bool,
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        let vertex_buffer = GpuBuffer::create_vertex_buffer(
            ctx.device,
            ctx.allocator,
            ctx.queue,
            ctx.command_pool,
            vertices,
            rt_enabled,
            staging_pool.as_deref_mut(),
        )?;
        let index_buffer = GpuBuffer::create_index_buffer(
            ctx.device,
            ctx.allocator,
            ctx.queue,
            ctx.command_pool,
            indices,
            rt_enabled,
            staging_pool,
        )?;
        let index_count = indices.len() as u32;

        if self.meshes.len() >= MAX_MESH_SLOTS as usize {
            bail!(
                "MeshRegistry slot overflow: {} slots used (cap {}). \
                 Likely a cell-unload leak — meshes are uploaded without matching drop_mesh calls.",
                self.meshes.len(),
                MAX_MESH_SLOTS,
            );
        }
        let id = self.meshes.len() as u32;
        self.meshes.push(Some(GpuMesh {
            vertex_buffer: Some(vertex_buffer),
            index_buffer: Some(index_buffer),
            index_count,
            global_vertex_offset: 0,
            global_index_offset: 0,
            vertex_count: vertices.len() as u32,
            is_scene_mesh: false,
            rt_capable: rt_enabled,
        }));
        // Parallel-indexed refcount; lockstep with `meshes` push.
        self.mesh_ref_counts.push(1);

        Ok(id)
    }

    /// Clamp any local index that overshoots this mesh's own vertex block
    /// to the last valid vertex, returning a borrowed slice (no allocation)
    /// when the geometry is already consistent.
    ///
    /// #1532 / #markarth-fragments — at draw time `cmd_draw_indexed` adds
    /// the per-mesh `global_vertex_offset` to each local index; an index
    /// `>= vertex_count` therefore reads PAST this mesh into the next mesh's
    /// vertices in the shared global pool (the "exploding spike" artifact),
    /// and with `robustBufferAccess` off a pool-tail overshoot is an OOB GPU
    /// fetch (UB / potential DEVICE_LOST). The same out-of-range index is
    /// also an invalid BLAS build input (it exceeds the declared
    /// `max_vertex`). The original guard only `log::error!`d and uploaded
    /// the inconsistent geometry anyway; this hard-gates by clamping so the
    /// uploaded (index, vertex) pair stays self-consistent — a degenerate
    /// (collapsed) triangle at worst — for the raster pool AND the per-mesh
    /// BLAS input (both consume the sanitized slice). The error log is
    /// retained for the decode-vs-compaction bisect the diagnostic was added
    /// for: if it fires, the NIF decode emitted a self-inconsistent
    /// (index, vertex) pair; if it never fires, look at pool compaction.
    ///
    /// `vertex_count == 0` can't clamp to a valid vertex, so indices map to
    /// `0`; such a mesh is already degenerate (no vertices of its own) and
    /// the clamp at least keeps the index count consistent and in-range of
    /// the global pool origin rather than producing an OOB fetch.
    fn sanitize_scene_indices(vertex_count: usize, indices: &[u32]) -> Cow<'_, [u32]> {
        let Some(&max_idx) = indices.iter().max() else {
            return Cow::Borrowed(indices);
        };
        if (max_idx as usize) < vertex_count {
            return Cow::Borrowed(indices);
        }
        log::error!(
            "GEOMETRY CORRUPTION (#markarth-fragments): local index {} >= \
             vertex_count {} (overshoot {}, idx_count {}). Clamping to the last \
             valid vertex (degenerate triangle) instead of reading past this \
             mesh's vertex block. NIF decode index/vertex-count mismatch.",
            max_idx,
            vertex_count,
            max_idx as usize + 1 - vertex_count,
            indices.len(),
        );
        let max_valid = vertex_count.saturating_sub(1) as u32;
        Cow::Owned(indices.iter().map(|&i| i.min(max_valid)).collect())
    }

    /// Accumulate scene geometry into the global SSBO pools, returning the
    /// `(vertex, index)` offsets recorded *before* the append and marking the
    /// SSBO dirty when it has already been built. Shared by
    /// [`Self::upload_scene_mesh`] (per-mesh buffers + global) and
    /// [`Self::upload_scene_mesh_global_only`] (global-only, #1370) so the
    /// growth caps + dirty bookkeeping stay in one place.
    fn accumulate_global_geometry(
        &mut self,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Result<(u32, u32)> {
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

        // Index/vertex consistency is enforced by `sanitize_scene_indices`
        // at the `upload_scene_mesh{,_global_only}` entry points (#1532),
        // so any index reaching here is already in-range for `vertices`.
        self.pending_vertices.extend_from_slice(vertices);
        self.pending_indices.extend_from_slice(indices);

        // If the SSBO has already been built, mark dirty so the frame
        // loop knows to call rebuild_geometry_ssbo. See #258.
        if self.global_vertex_buffer.is_some()
            && self.pending_vertices.len() > self.ssbo_vertex_count
        {
            self.geometry_dirty = true;
        }

        Ok((v_offset, i_offset))
    }

    pub fn upload_scene_mesh(
        &mut self,
        ctx: GpuUploadCtx,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<u32> {
        // Clamp any vertex-block overshoot ONCE (#1532), then feed the same
        // sanitized indices to both the global pool and the per-mesh / BLAS
        // upload so neither consumes an out-of-range index.
        let indices = Self::sanitize_scene_indices(vertices.len(), indices);
        let (v_offset, i_offset) = self.accumulate_global_geometry(vertices, &indices)?;

        // Upload to per-mesh buffers (also the BLAS build input when
        // `rt_enabled`).
        let id = self.upload(ctx, vertices, &indices, rt_enabled, staging_pool)?;

        // Store offsets.
        let mesh = self.meshes[id as usize]
            .as_mut()
            .expect("upload just pushed this slot");
        mesh.global_vertex_offset = v_offset;
        mesh.global_index_offset = i_offset;
        mesh.is_scene_mesh = true;

        Ok(id)
    }

    /// Upload a scene mesh that draws only from the global geometry SSBO —
    /// no per-mesh vertex/index allocations (#1370). Used by distant terrain
    /// and object LOD blocks: they rasterize from the global buffer with
    /// `rt_enabled = false`, so the per-mesh buffers
    /// [`Self::upload_scene_mesh`] would create are pure boot-time waste
    /// (~2 synchronous fence-waits + 2 tiny device-local sub-allocations per
    /// block × hundreds of blocks = a multi-hundred-ms boot stall).
    ///
    /// The returned mesh carries `None` buffers, so it MUST be drawn via the
    /// global indirect path (`global_bound == true`); the per-mesh draw
    /// fallback skips it (it renders once `rebuild_geometry_ssbo` runs — a
    /// ≤1-frame distant pop-in). A bounded nearby subset may be BLAS-built
    /// directly from its offsets in the RT-capable shared buffers.
    pub fn upload_scene_mesh_global_only(
        &mut self,
        vertices: &[Vertex],
        indices: &[u32],
    ) -> Result<u32> {
        let indices = Self::sanitize_scene_indices(vertices.len(), indices);
        let (v_offset, i_offset) = self.accumulate_global_geometry(vertices, &indices)?;

        if self.meshes.len() >= MAX_MESH_SLOTS as usize {
            bail!(
                "MeshRegistry slot overflow: {} slots used (cap {}). \
                 Likely a cell-unload leak — meshes are uploaded without matching drop_mesh calls.",
                self.meshes.len(),
                MAX_MESH_SLOTS,
            );
        }
        let id = self.meshes.len() as u32;
        self.meshes.push(Some(GpuMesh {
            vertex_buffer: None,
            index_buffer: None,
            index_count: indices.len() as u32,
            global_vertex_offset: v_offset,
            global_index_offset: i_offset,
            vertex_count: vertices.len() as u32,
            is_scene_mesh: true,
            // No dedicated buffers: the ordinary per-mesh BLAS path must skip
            // this handle. The bounded LOD path uses the shared pool instead.
            rt_capable: false,
        }));
        // Parallel-indexed refcount; lockstep with `meshes` push (mirrors
        // `upload`).
        self.mesh_ref_counts.push(1);

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
        ctx: GpuUploadCtx,
        vertices: &[Vertex],
        indices: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
        cache_key: (&str, u32),
    ) -> Result<u32> {
        let (model_path, sub_mesh_index) = cache_key;
        let handle = self.upload_scene_mesh(ctx, vertices, indices, rt_enabled, staging_pool)?;
        self.mesh_cache
            .insert((model_path.to_string(), sub_mesh_index), handle);
        Ok(handle)
    }

    /// Upload a set of fresh scene meshes with one transfer submission.
    ///
    /// This is the bulk counterpart of [`Self::upload_scene_mesh`] and
    /// [`Self::register_scene_mesh_keyed`]. Global-pool offsets, stable handle
    /// slots, refcounts, cache keys, and per-mesh vertex/index buffers keep the
    /// exact same representation; only the staging copies are packed together.
    pub fn upload_scene_meshes_batched(
        &mut self,
        ctx: GpuUploadCtx,
        uploads: &[SceneMeshUpload<'_>],
        transfer_fence: &std::sync::Mutex<vk::Fence>,
    ) -> Result<Vec<u32>> {
        if uploads.is_empty() {
            return Ok(Vec::new());
        }
        let projected_slots = self
            .meshes
            .len()
            .checked_add(uploads.len())
            .context("MeshRegistry slot count overflow")?;
        if projected_slots > MAX_MESH_SLOTS as usize {
            bail!(
                "MeshRegistry slot overflow: {} existing + {} uploads (cap {}). \
                 Likely a cell-unload leak — meshes are uploaded without matching drop_mesh calls.",
                self.meshes.len(),
                uploads.len(),
                MAX_MESH_SLOTS,
            );
        }

        let sanitized = uploads
            .iter()
            .map(|upload| Self::sanitize_scene_indices(upload.vertices.len(), upload.indices))
            .collect::<Vec<_>>();
        let old_vertex_len = self.pending_vertices.len();
        let old_index_len = self.pending_indices.len();
        let old_dirty = self.geometry_dirty;
        let mut global_offsets = Vec::with_capacity(uploads.len());
        for (upload, indices) in uploads.iter().zip(&sanitized) {
            match self.accumulate_global_geometry(upload.vertices, indices) {
                Ok(offsets) => global_offsets.push(offsets),
                Err(error) => {
                    self.pending_vertices.truncate(old_vertex_len);
                    self.pending_indices.truncate(old_index_len);
                    self.geometry_dirty = old_dirty;
                    return Err(error).context("accumulate batched scene geometry");
                }
            }
        }

        let mut buffer_uploads = Vec::with_capacity(uploads.len() * 2);
        for (upload, indices) in uploads.iter().zip(&sanitized) {
            let rt_usage = if upload.rt_enabled {
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            } else {
                vk::BufferUsageFlags::empty()
            };
            buffer_uploads.push(DeviceLocalBufferUpload {
                bytes: vertex_slice_bytes(upload.vertices),
                usage: vk::BufferUsageFlags::VERTEX_BUFFER | rt_usage,
            });
            buffer_uploads.push(DeviceLocalBufferUpload {
                bytes: index_slice_bytes(indices),
                usage: vk::BufferUsageFlags::INDEX_BUFFER | rt_usage,
            });
        }

        if self.geometry_staging_pool.is_none() {
            self.geometry_staging_pool =
                Some(StagingPool::new(ctx.device.clone(), ctx.allocator.clone()));
        }
        let buffers = match GpuBuffer::create_device_local_buffers_batched(
            ctx,
            &buffer_uploads,
            self.geometry_staging_pool.as_mut(),
            transfer_fence,
        ) {
            Ok(buffers) => buffers,
            Err(error) => {
                self.pending_vertices.truncate(old_vertex_len);
                self.pending_indices.truncate(old_index_len);
                self.geometry_dirty = old_dirty;
                return Err(error).context("upload batched scene geometry");
            }
        };

        debug_assert_eq!(buffers.len(), uploads.len() * 2);
        let mut buffers = buffers.into_iter();
        let mut handles = Vec::with_capacity(uploads.len());
        for ((upload, indices), (global_vertex_offset, global_index_offset)) in
            uploads.iter().zip(&sanitized).zip(global_offsets)
        {
            let id = self.meshes.len() as u32;
            self.meshes.push(Some(GpuMesh {
                vertex_buffer: Some(buffers.next().expect("batched vertex buffer missing")),
                index_buffer: Some(buffers.next().expect("batched index buffer missing")),
                index_count: indices.len() as u32,
                global_vertex_offset,
                global_index_offset,
                vertex_count: upload.vertices.len() as u32,
                is_scene_mesh: true,
                rt_capable: upload.rt_enabled,
            }));
            self.mesh_ref_counts.push(1);
            if let Some((model_path, sub_mesh_index)) = upload.cache_key {
                self.mesh_cache
                    .insert((model_path.to_string(), sub_mesh_index), id);
            }
            handles.push(id);
        }
        Ok(handles)
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
        if !self.release_mesh_ref(handle) {
            return false;
        }
        self.mesh_cache.retain(|_, &mut h| h != handle);
        true
    }

    /// Drop a holder-counted mesh batch and purge freed cache entries once.
    ///
    /// `handles` deliberately retains duplicates: every placement contributes
    /// one refcount decrement. Exterior unloads release thousands of unique
    /// meshes; scanning the entire cache after every last release made that
    /// path O(freed × cached). This keeps the same per-handle lifetime rules
    /// and performs one O(cached) purge after all decrements.
    pub fn drop_meshes(&mut self, handles: &[u32]) -> usize {
        let mut freed = HashSet::new();
        for &handle in handles {
            if self.release_mesh_ref(handle) {
                freed.insert(handle);
            }
        }
        if !freed.is_empty() {
            self.mesh_cache.retain(|_, h| !freed.contains(h));
        }
        freed.len()
    }

    fn release_mesh_ref(&mut self, handle: u32) -> bool {
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
                    .push((mesh.vertex_buffer, mesh.index_buffer), DEFAULT_COUNTDOWN);
            }
        }
        if was_scene_mesh {
            self.geometry_dirty = true;
            // #2678 — this drop stranded the mesh's span inside the pending
            // pools. Record it explicitly; the `meshes` slot it just vacated
            // stays `None` permanently, so the slot table cannot answer
            // "is there a hole *right now*".
            self.geometry_has_holes = true;
        }

        true
    }

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
    /// slices. If the up-front allocation for the second (temporarily
    /// duplicate) generation fails, this falls back unchanged to the
    /// original atomic idle-reclaim-then-build path
    /// ([`Self::rebuild_geometry_ssbo_atomic_fallback`]) — #2374's
    /// device-loss protection stays intact as the low-headroom recovery
    /// path, not the common case.
    pub fn rebuild_geometry_ssbo(
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
        // it always goes straight through the chunked path below.
        if has_existing_buffers {
            let rt_usage = if rt_enabled {
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            } else {
                vk::BufferUsageFlags::empty()
            };
            match Self::try_allocate_empty_geometry_buffers(
                device, allocator, vertex_size, index_size, rt_usage,
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
        self.rebuild_geometry_ssbo_atomic_fallback(device, allocator, queue, command_pool, rt_enabled)
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
                        self.geometry_staging_pool.as_mut().expect("just initialised above"),
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
                        self.geometry_staging_pool.as_mut().expect("just initialised above"),
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

    pub fn get(&self, id: u32) -> Option<&GpuMesh> {
        self.meshes.get(id as usize).and_then(|slot| slot.as_ref())
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    /// Number of *occupied* mesh slots.
    ///
    /// [`Self::len`] is the slot-vector length, which never shrinks: dropped
    /// meshes leave a placeholder behind so a dangling `GpuInstance.mesh_id`
    /// can never resolve to a different mesh (#372). That makes `len()` a
    /// monotonic watermark rather than a residency figure. This counts the
    /// slots that actually hold a mesh, which is what the EX-08 ownership soak
    /// (#2374) holds to an exact return across a load/unload cycle — and the
    /// mesh-side counterpart of `TextureRegistry::live_slot_count`.
    pub fn live_slot_count(&self) -> usize {
        self.meshes.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
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
        self.ssbo_vertex_count = 0;
        self.ssbo_index_count = 0;
        // #3298 — an in-flight chunked rebuild's target buffers are real,
        // live allocations that never made it into `global_vertex_buffer`/
        // `global_index_buffer` above. `GpuBuffer::Drop` would eventually
        // self-free them, but that's a leak-prevention safety net, not the
        // canonical path (#927 — relying on it during shutdown let the
        // allocator's `Arc` outlive `Arc::try_unwrap`'s window). Destroy
        // explicitly here, same as every other buffer in this function.
        if let Some(mut job) = self.geometry_rebuild.take() {
            job.new_vertex_buffer.destroy(device, allocator);
            job.new_index_buffer.destroy(device, allocator);
        }
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

fn vertex_slice_bytes(values: &[Vertex]) -> &[u8] {
    unsafe {
        // SAFETY: `Vertex` is `#[repr(C)]` and its scalar/array fields occupy
        // the complete 104-byte shader contract without padding. Every byte
        // is initialized by construction, and the returned view cannot
        // outlive the source slice.
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

fn index_slice_bytes(values: &[u32]) -> &[u8] {
    unsafe {
        // SAFETY: every bit pattern in a `u32` is initialized and valid; the
        // returned byte view covers exactly the borrowed slice and cannot
        // outlive it.
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
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

/// Parametric axis-aligned box with arbitrary half-extents and a single
/// flat vertex color. Outward-facing normals, per-face UVs. Unlike the
/// uniform-scale `Transform`, the half-extents let a single mesh model a
/// non-cubic shape (thin slabs for walls, tall blocks). Used by the
/// Cornell-box test harness (`byroredux::cornell`) and available to any
/// caller needing a quick colored box primitive.
pub fn box_vertices_colored(half: [f32; 3], color: [f32; 3]) -> (Vec<Vertex>, Vec<u32>) {
    let [hx, hy, hz] = half;
    let v = |p: [f32; 3], n: [f32; 3], uv: [f32; 2]| Vertex::new(p, color, n, uv);
    let vertices = vec![
        // +Z front
        v([-hx, -hy, hz], [0.0, 0.0, 1.0], [0.0, 1.0]),
        v([hx, -hy, hz], [0.0, 0.0, 1.0], [1.0, 1.0]),
        v([hx, hy, hz], [0.0, 0.0, 1.0], [1.0, 0.0]),
        v([-hx, hy, hz], [0.0, 0.0, 1.0], [0.0, 0.0]),
        // -Z back
        v([-hx, -hy, -hz], [0.0, 0.0, -1.0], [1.0, 1.0]),
        v([hx, -hy, -hz], [0.0, 0.0, -1.0], [0.0, 1.0]),
        v([hx, hy, -hz], [0.0, 0.0, -1.0], [0.0, 0.0]),
        v([-hx, hy, -hz], [0.0, 0.0, -1.0], [1.0, 0.0]),
        // +Y top
        v([-hx, hy, -hz], [0.0, 1.0, 0.0], [0.0, 1.0]),
        v([hx, hy, -hz], [0.0, 1.0, 0.0], [1.0, 1.0]),
        v([hx, hy, hz], [0.0, 1.0, 0.0], [1.0, 0.0]),
        v([-hx, hy, hz], [0.0, 1.0, 0.0], [0.0, 0.0]),
        // -Y bottom
        v([-hx, -hy, -hz], [0.0, -1.0, 0.0], [0.0, 0.0]),
        v([hx, -hy, -hz], [0.0, -1.0, 0.0], [1.0, 0.0]),
        v([hx, -hy, hz], [0.0, -1.0, 0.0], [1.0, 1.0]),
        v([-hx, -hy, hz], [0.0, -1.0, 0.0], [0.0, 1.0]),
        // +X right
        v([hx, -hy, -hz], [1.0, 0.0, 0.0], [0.0, 1.0]),
        v([hx, hy, -hz], [1.0, 0.0, 0.0], [0.0, 0.0]),
        v([hx, hy, hz], [1.0, 0.0, 0.0], [1.0, 0.0]),
        v([hx, -hy, hz], [1.0, 0.0, 0.0], [1.0, 1.0]),
        // -X left
        v([-hx, -hy, -hz], [-1.0, 0.0, 0.0], [1.0, 1.0]),
        v([-hx, hy, -hz], [-1.0, 0.0, 0.0], [1.0, 0.0]),
        v([-hx, hy, hz], [-1.0, 0.0, 0.0], [0.0, 0.0]),
        v([-hx, -hy, hz], [-1.0, 0.0, 0.0], [0.0, 1.0]),
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

/// A flat-colored UV sphere centered at the origin. `rings` is the number
/// of latitude bands, `segments` the longitude divisions. Smooth (radial)
/// normals, equirectangular UVs. Outward winding matches the engine's
/// front-face convention. Used by the Cornell-box test harness to probe
/// curved-surface RT behaviour (GGX highlight shape, reflection/refraction
/// across the full normal range) that flat primitives can't.
pub fn uv_sphere(
    radius: f32,
    color: [f32; 3],
    rings: u32,
    segments: u32,
) -> (Vec<Vertex>, Vec<u32>) {
    let rings = rings.max(2);
    let segments = segments.max(3);
    let mut vertices = Vec::with_capacity(((rings + 1) * (segments + 1)) as usize);
    for r in 0..=rings {
        // theta: 0 (north pole, +Y) .. PI (south pole, -Y)
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        let (st, ct) = theta.sin_cos();
        for s in 0..=segments {
            let phi = std::f32::consts::TAU * s as f32 / segments as f32;
            let (sp, cp) = phi.sin_cos();
            let n = [st * cp, ct, st * sp];
            let pos = [n[0] * radius, n[1] * radius, n[2] * radius];
            let uv = [s as f32 / segments as f32, r as f32 / rings as f32];
            vertices.push(Vertex::new(pos, color, n, uv));
        }
    }
    let stride = segments + 1;
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);
    for r in 0..rings {
        for s in 0..segments {
            let a = r * stride + s;
            let b = a + stride;
            // Wind so the front face points outward (+normal).
            indices.extend_from_slice(&[a, a + 1, b, b, a + 1, b + 1]);
        }
    }
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
        const {
            assert!(VERTEX_POOL_HARD_CAP > VERTEX_POOL_SOFT_CAP);
            assert!(INDEX_POOL_HARD_CAP > INDEX_POOL_SOFT_CAP);
        }
        // At Vertex = 104 B, hard cap 16M = 1.66 GB. At u32 indices,
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

    #[test]
    fn large_rebuilds_idle_only_when_replacing_existing_buffers() {
        let threshold = GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES;
        assert!(!geometry_rebuild_needs_idle(threshold - 1, true));
        assert!(geometry_rebuild_needs_idle(threshold, true));
        assert!(geometry_rebuild_needs_idle(threshold + 1, true));
        assert!(
            !geometry_rebuild_needs_idle(threshold * 2, false),
            "initial build has no old generation to reclaim",
        );
    }
}

#[cfg(test)]
mod sanitize_index_tests {
    //! Regression for #1532 / #markarth-fragments: the vertex-block
    //! overshoot guard was log-only and uploaded the inconsistent geometry
    //! anyway (raster reads into other meshes' vertices, OOB GPU fetch with
    //! robustness off, invalid BLAS build input). `sanitize_scene_indices`
    //! now hard-gates by clamping; these pin the clamp without a GPU device.
    use super::*;

    /// Consistent geometry passes through borrowed — no allocation, bytes
    /// unchanged.
    #[test]
    fn in_range_indices_pass_through_borrowed() {
        let idx = [0u32, 1, 2, 2, 1, 0];
        let out = MeshRegistry::sanitize_scene_indices(3, &idx);
        assert!(matches!(out, Cow::Borrowed(_)), "no clamp ⇒ no allocation");
        assert_eq!(&*out, &idx);
    }

    /// `max_idx == vertex_count - 1` is the last valid index and must NOT
    /// trip the clamp (the bug was a `>=` vs `>` off-by-one risk).
    #[test]
    fn last_valid_index_is_not_clamped() {
        let idx = [2u32, 2, 2];
        let out = MeshRegistry::sanitize_scene_indices(3, &idx);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    /// An overshoot index is clamped to the last valid vertex; in-range
    /// indices are untouched and the count is preserved (so the caller's
    /// `index_count` stays consistent with the appended slice).
    #[test]
    fn overshoot_index_is_clamped_to_last_valid_vertex() {
        // vertex_count 3 ⇒ valid indices 0..=2; index 5 overshoots.
        let idx = [0u32, 1, 5];
        let out = MeshRegistry::sanitize_scene_indices(3, &idx);
        assert!(matches!(out, Cow::Owned(_)), "overshoot ⇒ owned clamp");
        assert_eq!(&*out, &[0, 1, 2], "5 clamped to 2; rest unchanged");
        assert_eq!(out.len(), idx.len(), "index count preserved");
        // Every emitted index is now a valid vertex reference.
        assert!(out.iter().all(|&i| (i as usize) < 3));
    }

    /// Empty index list is a borrowed no-op (no `max`, no clamp).
    #[test]
    fn empty_indices_pass_through() {
        let out = MeshRegistry::sanitize_scene_indices(0, &[]);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert!(out.is_empty());
    }

    /// `vertex_count == 0` can't clamp to a valid vertex; indices map to 0
    /// (in-range of the global pool origin) rather than panicking on the
    /// `len - 1` underflow.
    #[test]
    fn zero_vertex_count_clamps_to_zero_without_underflow() {
        let idx = [0u32, 3, 7];
        let out = MeshRegistry::sanitize_scene_indices(0, &idx);
        assert_eq!(&*out, &[0, 0, 0]);
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

    /// Global-SSBO-only meshes carry no per-mesh buffers at all, so no BLAS
    /// can legally be built over them. `rt_capable` must report that, since
    /// it is the flag every BLAS path now gates on — the skinned path used
    /// to assume "skinned ⇒ RT-capable" and would take the device address of
    /// an index buffer created without `SHADER_DEVICE_ADDRESS`, tripping
    /// VUID-VkBufferDeviceAddressInfo-buffer-02601 (and then
    /// -geometry-03673 on the build itself).
    ///
    /// This path is device-free, so it is the one upload route the pure-Rust
    /// tests can drive end to end; the `rt_enabled`-mirroring behaviour of
    /// the buffer-backed `upload` is exercised by every live cell load.
    #[test]
    fn global_only_meshes_are_never_rt_capable() {
        let mut reg = MeshRegistry::new();
        let vertices = [
            Vertex::new(
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0],
            ),
            Vertex::new(
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0],
            ),
            Vertex::new(
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0],
            ),
        ];
        let handle = reg
            .upload_scene_mesh_global_only(&vertices, &[0, 1, 2])
            .expect("global-only upload needs no device");

        let mesh = reg.get(handle).expect("handle just returned by upload");
        assert!(
            !mesh.rt_capable,
            "a mesh with no per-mesh buffers must never be reported RT-capable",
        );
        assert!(
            mesh.index_buffer.is_none() && mesh.vertex_buffer.is_none(),
            "global-only meshes intentionally carry no per-mesh buffers",
        );
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

    #[test]
    fn batch_drop_preserves_holder_counts_and_purges_cache_once() {
        let mut reg = MeshRegistry::new();
        let chair = install_synthetic_slot(&mut reg, "chair.nif", 0, 1);
        let table = install_synthetic_slot(&mut reg, "table.nif", 0, 2);
        let survivor = install_synthetic_slot(&mut reg, "lamp.nif", 0, 1);

        assert_eq!(reg.drop_meshes(&[table, chair, table]), 2);
        assert_eq!(reg.refcount(chair), None);
        assert_eq!(reg.refcount(table), None);
        assert_eq!(reg.refcount(survivor), Some(1));
        assert_eq!(reg.acquire_cached("chair.nif", 0), None);
        assert_eq!(reg.acquire_cached("table.nif", 0), None);
        assert_eq!(reg.acquire_cached("lamp.nif", 0), Some(survivor));
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
    use super::*;

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
    use super::*;

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
        assert_eq!(step, GeometryRebuildStep::CopyVertices { start: 0, end: 40 });
    }

    /// A chunk that would overrun the target clamps to it exactly, rather
    /// than reading/copying past the end of `pending_vertices`.
    #[test]
    fn vertex_chunk_clamps_to_target_on_the_last_slice() {
        let step = next_geometry_rebuild_chunk(80, 100, 0, 300, 40, 40);
        assert_eq!(
            step,
            GeometryRebuildStep::CopyVertices { start: 80, end: 100 },
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
        assert_eq!(step, GeometryRebuildStep::CopyVertices { start: 0, end: 40 });
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
