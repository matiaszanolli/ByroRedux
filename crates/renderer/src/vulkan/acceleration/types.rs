//! BLAS / TLAS data types.
//!
//! Split out from the monolithic `acceleration` module. These are
//! pure data — no behaviour — referenced by every other submodule
//! (lifecycle, builds, eviction, telemetry).

use super::super::buffer::GpuBuffer;
use ash::vk;

/// A bottom-level acceleration structure for one mesh.
pub struct BlasEntry {
    pub accel: vk::AccelerationStructureKHR,
    pub buffer: GpuBuffer,
    pub device_address: vk::DeviceAddress,
    /// Frame counter when this BLAS was last referenced by a TLAS build.
    /// Used for LRU eviction of unused BLAS entries.
    pub last_used_frame: u64,
    /// Size of the acceleration structure buffer in bytes.
    pub size_bytes: vk::DeviceSize,
    /// Scratch-buffer capacity that this BLAS required at build time.
    /// `shrink_blas_scratch_to_fit` takes the max across surviving
    /// entries to decide the minimum scratch needed post-eviction. See
    /// issue #495.
    pub build_scratch_size: vk::DeviceSize,
    /// Number of [`AccelerationManager::refit_skinned_blas`] calls
    /// against this entry since the last fresh BUILD. Bumped each
    /// frame for skinned BLAS; stays at 0 for static (mesh-keyed)
    /// BLAS that never refit.
    ///
    /// Vulkan REFIT-only updates progressively degrade BVH traversal
    /// quality as vertex motion exceeds the original BUILD bounds —
    /// a long animation cycle (an NPC walking 30s) eventually has
    /// the refit BLAS noticeably slower to traverse than a fresh
    /// BUILD. The renderer compares this counter against
    /// [`SKINNED_BLAS_REFIT_THRESHOLD`] each frame and triggers a
    /// drop+rebuild when the threshold is reached. See #679 / AS-8-9.
    pub refit_count: u32,
    /// Vertex / index counts the original fresh BUILD was sized for.
    /// `refit_skinned_blas` validates the caller-supplied counts
    /// against these before issuing `mode = UPDATE` — a mismatch
    /// would trip
    /// `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667`
    /// ("the corresponding `pBuildRangeInfos[i][j].primitiveCount`
    /// must equal the `primitiveCount` used to build the source
    /// acceleration structure") and silently corrupt the BVH on
    /// NVIDIA. Today no in-engine path remaps `entity_id → mesh`
    /// between frames, but a future mod swap, LOD switch, or mesh
    /// hot-reload would; pinning the BUILD-time counts here turns
    /// that future regression into a logged error + safe fresh-BUILD
    /// fallback instead of silent corruption. Static BLAS use the
    /// same fields purely for symmetry — they never refit, so the
    /// stored values are read-only telemetry there. See #907 /
    /// REN-D12-NEW-01.
    pub built_vertex_count: u32,
    pub built_index_count: u32,
}

/// Top-level acceleration structure state.
pub struct TlasState {
    pub accel: vk::AccelerationStructureKHR,
    pub buffer: GpuBuffer,
    /// Host-visible staging buffer for CPU writes of instance data.
    pub instance_buffer: GpuBuffer,
    /// Device-local copy for GPU reads during AS build. On discrete GPUs,
    /// reads from VRAM avoid PCIe traversal (~10-30x faster). See #289.
    pub instance_buffer_device: GpuBuffer,
    /// Max instances the instance_buffer can hold.
    pub max_instances: u32,
    /// BLAS device addresses submitted on the most recent BUILD, in
    /// submission order. Used by `build_tlas` to decide whether the
    /// next frame can refit (`UPDATE` mode) or must full-rebuild
    /// (`BUILD` mode). REFIT is only legal when the per-instance BLAS
    /// references are unchanged from the last build — Vulkan's UPDATE
    /// mode permits changes to transforms, custom indices, SBT offsets,
    /// mask, and flags, but NOT to `acceleration_structure_reference`.
    /// See #247.
    pub last_blas_addresses: Vec<vk::DeviceAddress>,
    /// `true` when the next build must be a full BUILD (either the
    /// TLAS was just (re)created, or the instance layout changed).
    /// Reset to `false` after each successful BUILD.
    pub needs_full_rebuild: bool,
    /// Value of `AccelerationManager.blas_map_generation` the last
    /// time this TLAS was BUILT. When the manager's counter is ahead
    /// of this one, the BLAS map mutated since the last build (a
    /// BLAS was added, dropped, rebuilt, or evicted) and we can
    /// short-circuit straight to BUILD without running the O(N)
    /// per-instance BLAS-address zip-compare that gates UPDATE
    /// eligibility otherwise. See #300.
    pub last_blas_map_gen: u64,
}
