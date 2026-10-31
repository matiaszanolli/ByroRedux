//! The geometry pool's ceilings and admission policy: the vertex / index caps, the
//! three admission tiers (`GeometryAdmissionClass`) and the reserves between them
//! (`PoolReserve`, `PoolLimits`), the pure growth check (`check_pool_growth`) and the
//! rebuild-vs-idle decision (`geometry_rebuild_needs_idle`). Split out of `mesh.rs`
//! (#6520); `MeshRegistry` applies this policy, it does not define it.

use anyhow::{bail, Result};

/// Defence-in-depth cap on the global vertex pool size. The pool grows
/// monotonically until `drop_mesh` (refcount → 0) lets `compact_pending_geometry`
/// rewrite it. A correct streaming session sees `pending_vertices` track the
/// resident scene's geometry; a broken cell-unload path leaks placements and
/// grows the pool unbounded.
///
/// The 7 M-vertex / 28 M-index residency ceiling (~840 MB total source
/// geometry) returns `Err` from `upload_scene_mesh` so the caller can skip a
/// placement and continue. It is deliberately the same as the historical soft
/// threshold: each scene mesh also has upload and ray-tracing allocations, so
/// allowing the previous 1.9 GB global-pool ceiling could multiply into
/// enough process memory for Linux to invoke the OOM killer.
///
/// See REN-D2-005 / #1016. Mirrors the `MAX_INDIRECT_DRAWS` defence-in-depth
/// cap (`MAX_INDIRECT_DRAWS`, `vulkan/scene_buffer/constants.rs`) — these are not perf knobs, they are
/// safety guards against unbounded-growth bugs.
pub const VERTEX_POOL_SOFT_CAP: usize = 7_000_000;
pub const VERTEX_POOL_HARD_CAP: usize = VERTEX_POOL_SOFT_CAP;
/// Index pool caps — typical mesh ratio is ~3 indices per vertex, so the
/// caps here track the vertex caps proportionally.
pub const INDEX_POOL_SOFT_CAP: usize = 28_000_000;
pub const INDEX_POOL_HARD_CAP: usize = INDEX_POOL_SOFT_CAP;

/// Default share of each pool kept back from lower tiers, in percent, used
/// until the host sizes the reserve for the real exterior radius
/// ([`MeshRegistry::set_pool_reserve`]). 3% for the ground, 2% for actors:
/// props get 95%, as the first version of the reserve did.
pub const DEFAULT_GROUND_RESERVE_PERCENT: usize = 3;
pub const DEFAULT_ACTOR_RESERVE_PERCENT: usize = 2;

/// Who is asking for pool space. The class decides which ceiling and which
/// refusal latch an upload meets. Priority is ground > actors > props: when
/// the pool is full the horizon's props go first, then people, and the ground
/// and water last.
///
/// Placed props are the pool's bulk (FO4 radius 4: 3.1 M of 6.4 M vertices,
/// plus 2.7 M of distant object LOD), so without tiers the first prop to hit
/// the cap latched admission closed for everyone: at FO4's default radius 12
/// that refused 178 terrain cells, the water planes and the whole LOD terrain
/// ring — a void with no ground or water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryAdmissionClass {
    /// Terrain, water, terrain LOD: the full cap, the global latch.
    Essential,
    /// Actors (the player and NPC bodies): the cap less the ground's reserve.
    Actor,
    /// Placed statics, object / tree / placement LOD: the cap less the
    /// ground's and the actors' reserves.
    Prop,
}

/// Pool space kept back for the higher tiers, in vertices and indices.
/// `ground_*` is what terrain, water and terrain LOD may count on beyond what
/// actors and props can touch; `actor_*` is the extra room actors have over
/// props.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolReserve {
    pub ground_vertices: usize,
    pub ground_indices: usize,
    pub actor_vertices: usize,
    pub actor_indices: usize,
}

/// The pool ceilings a registry enforces. A field (defaulting to the
/// constants above) so tests can use small numbers instead of allocating
/// millions of vertices.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PoolLimits {
    pub(crate) vertex_hard: usize,
    pub(crate) index_hard: usize,
    pub(crate) reserve: PoolReserve,
}

impl PoolLimits {
    /// The default reserve for ceilings `vertex_hard` / `index_hard`.
    pub(crate) const fn default_reserve(vertex_hard: usize, index_hard: usize) -> PoolReserve {
        PoolReserve {
            ground_vertices: vertex_hard / 100 * DEFAULT_GROUND_RESERVE_PERCENT,
            ground_indices: index_hard / 100 * DEFAULT_GROUND_RESERVE_PERCENT,
            actor_vertices: vertex_hard / 100 * DEFAULT_ACTOR_RESERVE_PERCENT,
            actor_indices: index_hard / 100 * DEFAULT_ACTOR_RESERVE_PERCENT,
        }
    }

    /// A limit with the default reserve.
    pub(crate) const fn with_default_reserve(vertex_hard: usize, index_hard: usize) -> Self {
        Self {
            vertex_hard,
            index_hard,
            reserve: Self::default_reserve(vertex_hard, index_hard),
        }
    }

    pub(crate) const DEFAULT: Self =
        Self::with_default_reserve(VERTEX_POOL_HARD_CAP, INDEX_POOL_HARD_CAP);

    /// Hard ceilings for `class`: each tier below the ground stops short by the
    /// reserves of the tiers above it.
    pub(crate) fn for_class(self, class: GeometryAdmissionClass) -> (usize, usize) {
        let (ground, actor) = (self.reserve.ground_vertices, self.reserve.actor_vertices);
        let (ground_i, actor_i) = (self.reserve.ground_indices, self.reserve.actor_indices);
        match class {
            GeometryAdmissionClass::Essential => (self.vertex_hard, self.index_hard),
            GeometryAdmissionClass::Actor => (
                self.vertex_hard.saturating_sub(ground),
                self.index_hard.saturating_sub(ground_i),
            ),
            GeometryAdmissionClass::Prop => (
                self.vertex_hard.saturating_sub(ground + actor),
                self.index_hard.saturating_sub(ground_i + actor_i),
            ),
        }
    }
}

/// Fixed duplication ceiling for a global-geometry rebuild on a device
/// without `VK_EXT_memory_budget`: large rebuilds cannot safely keep two SSBO
/// generations alive while allocating the replacement on mid-range GPUs.
/// Above 256 MiB, prefer a one-time device-idle reclamation over a recoverable
/// allocation failure escalating into `VK_ERROR_DEVICE_LOST` (FO4 boundary
/// traversal, #2374). With the extension, [`geometry_rebuild_needs_idle`]
/// measures the headroom instead. EX-07 tracks replacing this safety path
/// with a capacity-managed append/update buffer that remains fully
/// asynchronous.
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

/// Whether a rebuild must idle-reclaim the old generation before building the
/// replacement, instead of holding both through a resumable copy.
/// `live_budget` is `(usage, budget)` summed over the DEVICE_LOCAL heaps by
/// `VK_EXT_memory_budget` ([`crate::vulkan::allocator::query_live_memory_budget`]).
///
/// With a live reading, duplication is allowed only while the new
/// generation keeps usage at or under the engine's approaching-OOM line, 80%
/// of the live budget ([`crate::vulkan::allocator::approaching_oom_line`]).
/// The budget is the extension's estimate of what the process can allocate
/// before allocations "may fail or cause performance degradation", which is
/// the spill #2374 / #3443 guard against. `heapUsage` already counts every
/// generation still allocated, the bound one and any awaiting deferred
/// destroy, so the stacked generations of repeated rebuilds (#2374) are
/// refused as usage climbs. The remaining 20% absorbs what streaming
/// allocates during the multi-frame copy. On a 12 GB card an FO4 crossing's
/// ~400 MiB rebuild now streams in chunks instead of idling the device; a
/// small rebuild on a nearly full card now idles where it used to duplicate.
///
/// Without a reading, the fixed [`GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`]
/// rule applies unchanged. A first build has no old generation to reclaim.
pub(crate) fn geometry_rebuild_needs_idle(
    projected_bytes: u64,
    has_existing_buffers: bool,
    live_budget: Option<(u64, u64)>,
) -> bool {
    if !has_existing_buffers {
        return false;
    }
    match live_budget {
        Some((usage, budget)) => {
            usage.saturating_add(projected_bytes)
                > crate::vulkan::allocator::approaching_oom_line(budget)
        }
        None => projected_bytes >= GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES,
    }
}
