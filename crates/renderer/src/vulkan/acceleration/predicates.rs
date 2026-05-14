//! Pure predicates and free helpers shared across the acceleration
//! submodules.
//!
//! Every function here is unit-testable without a live Vulkan device.
//! Lifted out of the monolithic `acceleration` module so the tuning
//! decisions live next to their tests.

use super::constants::{
    BLAS_REBUILD_SLACK_BYTES, MIN_BLAS_BUDGET_BYTES, SKINNED_BLAS_REFIT_THRESHOLD,
    TLAS_REBUILD_SLACK_BYTES,
};
use crate::vulkan::context::DrawCommand;
use anyhow::Result;
use ash::vk;

/// Convert a column-major `[f32; 16]` model matrix (glam / shader
/// convention) into the row-major 3×4 layout `VkTransformMatrixKHR`
/// expects. The bottom row of an affine model matrix is always
/// `(0, 0, 0, 1)` and Vulkan's TLAS instance struct drops it; we
/// emit only the upper three rows in row-major order.
///
/// Pinned by `column_major_to_vk_transform_*` tests so a future
/// refactor that touches glam's storage convention or accidentally
/// re-transposes the matrix is caught at build time. Pre-#926 this
/// conversion was inline-spelt at the TLAS rebuild site, with no
/// unit test — REN-D8-NEW-11.
#[inline]
pub(super) fn column_major_to_vk_transform(m: &[f32; 16]) -> vk::TransformMatrixKHR {
    vk::TransformMatrixKHR {
        matrix: [
            m[0], m[4], m[8], m[12], // row 0: X axis
            m[1], m[5], m[9], m[13], // row 1: Y axis
            m[2], m[6], m[10], m[14], // row 2: Z axis
        ],
    }
}

/// Dispatch helper: reuse the shared transfer fence when available,
/// otherwise fall back to per-call create/destroy (#302).
pub(super) fn submit_one_time<F>(
    device: &ash::Device,
    queue: &std::sync::Mutex<vk::Queue>,
    pool: vk::CommandPool,
    fence: Option<&std::sync::Mutex<vk::Fence>>,
    f: F,
) -> Result<()>
where
    F: FnOnce(vk::CommandBuffer) -> Result<()>,
{
    match fence {
        Some(f_mutex) => {
            super::super::texture::with_one_time_commands_reuse_fence(device, queue, pool, f_mutex, f)
        }
        None => super::super::texture::with_one_time_commands(device, queue, pool, f),
    }
}

/// Pure predicate: does a skinned BLAS with `refit_count` refits
/// since its last fresh BUILD warrant a drop+rebuild this frame?
/// Pulled out so the threshold logic is unit-testable without a
/// Vulkan context.
#[inline]
pub(super) fn should_rebuild_skinned_blas_after(refit_count: u32) -> bool {
    refit_count >= SKINNED_BLAS_REFIT_THRESHOLD
}

/// #907 — pure check that the caller-supplied vertex / index counts
/// for `refit_skinned_blas` match the counts the original fresh
/// BUILD was sized for. Returns `Ok` when they agree and a
/// human-readable mismatch description otherwise.
///
/// Vulkan
/// `VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667` requires
/// `primitiveCount` (== `index_count / 3` here) at UPDATE-mode
/// builds to match the source BUILD's value exactly. `vertex_count`
/// also feeds `max_vertex` on the geometry — changing it shifts the
/// valid-index range and trips related VUIDs around index bounds.
/// Both are pinned defensively even though `primitiveCount` is the
/// only spec-strict one.
///
/// Split out so the check is unit-testable without a Vulkan context
/// (the rest of `refit_skinned_blas` needs an `ash::Device` and a
/// recording command buffer to exercise).
#[inline]
pub(super) fn validate_refit_counts(
    built_vertex_count: u32,
    built_index_count: u32,
    refit_vertex_count: u32,
    refit_index_count: u32,
) -> Result<(), String> {
    if built_vertex_count != refit_vertex_count || built_index_count != refit_index_count {
        return Err(format!(
            "BUILD-time counts (v={built_vertex_count}, i={built_index_count}) \
             differ from refit-time counts (v={refit_vertex_count}, i={refit_index_count}); \
             VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 \
             requires `primitiveCount` at UPDATE to equal the source BUILD's"
        ));
    }
    Ok(())
}

/// Decide whether the next TLAS build can `UPDATE` (refit) or must
/// `BUILD` from scratch. Pulled out as a pure function so the dirty-
/// flag short-circuit logic introduced in #300 can be unit-tested
/// without a Vulkan context.
///
/// Returns `(use_update, did_zip)`:
///   - `use_update` is the gate fed to `build_geometry_info.mode`.
///   - `did_zip` reports whether the per-instance address comparison
///     actually ran. Used by tests to assert the short-circuit fires
///     in the cases the audit cares about.
pub(super) fn decide_use_update(
    needs_full_rebuild: bool,
    tlas_last_gen: u64,
    current_gen: u64,
    cached_addresses: &[vk::DeviceAddress],
    current_addresses: &[vk::DeviceAddress],
) -> (bool, bool) {
    // Empty current frame → must BUILD (#657). The naive zip-compare
    // would treat two empty lists as identical and pick UPDATE, then
    // submit `mode = UPDATE, src == dst, primitiveCount = 0` against
    // a TLAS that may have been built with a non-empty primitive list
    // last frame. Today this is masked by `needs_full_rebuild = true`
    // at TLAS creation, but the helper's contract should not depend
    // on the caller setting that flag — any future refactor that
    // resets needs_full_rebuild after a successful BUILD without
    // checking instance_count > 0 would otherwise hit this. Cost:
    // one extra BUILD on transition between empty-scene frames (zero
    // measurable impact since `primitiveCount = 0`).
    //
    // The mirror case — empty frame followed by a non-empty frame —
    // also runs a BUILD on both sides (frame N: empty BUILD because
    // the helper returned early here; frame N+1: BUILD because the
    // BLAS-map gen differs from the cached non-empty list). The
    // empty BUILD costs nothing (`primitiveCount = 0`), the non-empty
    // BUILD would have happened on first-frame regardless. Verified
    // correct — no double-work to remove. See REN-D8-NEW-13 (audit
    // 2026-05-09).
    if current_addresses.is_empty() {
        return (false, false);
    }
    let blas_map_dirty = tlas_last_gen != current_gen;
    if needs_full_rebuild || blas_map_dirty {
        // Headed to BUILD regardless — skip the O(N) comparison.
        return (false, false);
    }
    let layout_matches = cached_addresses.len() == current_addresses.len()
        && cached_addresses
            .iter()
            .zip(current_addresses.iter())
            .all(|(a, b)| a == b);
    (layout_matches, true)
}

/// Compute the BLAS memory budget as `VRAM / 3` with a 256 MB floor.
///
/// The budget must bound BLAS memory so smaller-VRAM GPUs evict before
/// hitting an out-of-memory condition, while leaving the bulk of the
/// device-local heap available for textures, vertex/index buffers, and
/// the framebuffer. See #387.
/// Build the shared `draw_idx → ssbo_idx` mapping that
/// [`AccelerationManager::build_tlas`] and the SSBO builder in
/// `draw_frame` both honour. `keep(draw_idx)` returns true when the
/// corresponding draw command survives the SSBO filter (typically
/// `mesh_registry.get(handle).is_some()` in the caller). The returned
/// map is `Some(compacted_idx)` for every kept command in enumeration
/// order and `None` for every dropped or capacity-clipped one. See
/// #419 — this is the single source of truth the TLAS
/// `instance_custom_index` and the SSBO position must agree on; before
/// it landed the two filter predicates were independent and could
/// silently diverge.
///
/// `max_kept` enforces the SSBO cap (`scene_buffer::MAX_INSTANCES`).
/// Beyond it `keep` returns are forced to `None` so the TLAS doesn't
/// emit instances whose `instance_custom_index` would point past the
/// SSBO upload — that would produce garbage reads on every shadow /
/// reflection / GI ray hit against an over-cap instance. Caller is
/// expected to have sorted the draw list so cap-clipped tail entries
/// are the lowest-priority ones (RT-only off-frustum occluders per
/// the `!in_raster` prefix in `byroredux::render::draw_sort_key`),
/// so the dropped contribution is bounded to off-screen RT bounces.
pub fn build_instance_map(
    len: usize,
    max_kept: usize,
    mut keep: impl FnMut(usize) -> bool,
) -> Vec<Option<u32>> {
    let mut out = Vec::with_capacity(len);
    let mut next: u32 = 0;
    for i in 0..len {
        if keep(i) && (next as usize) < max_kept {
            out.push(Some(next));
            next += 1;
        } else {
            out.push(None);
        }
    }
    out
}

/// Grow-only policy for BLAS / TLAS scratch buffers. Returns `true`
/// when the current buffer is absent or its capacity is strictly less
/// than the required size — so a cell whose scratch footprint is
/// smaller than the high-water mark reuses the existing allocation
/// instead of churning through `gpu-allocator`. Pulled out as a pure
/// function so the BLAS single-build, BLAS batched-build, and TLAS
/// rebuild call sites can share one decision rule and a unit test can
/// guard against drift. See #60 (BLAS pool) + #424 SIBLING (TLAS pool).
pub(super) fn scratch_needs_growth(
    current_capacity: Option<vk::DeviceSize>,
    required: vk::DeviceSize,
) -> bool {
    match current_capacity {
        Some(cap) => cap < required,
        None => true,
    }
}

/// Decide whether a persisted scratch buffer is disproportionately
/// large and should be shrunk to match the post-eviction peak.
///
/// Returns `true` only when BOTH conditions hold:
/// - Current capacity is more than 2× the new peak requirement, AND
/// - The absolute excess is larger than a 16 MB slack margin.
///
/// The 2× factor absorbs normal build-to-build variance (adjacent cell
/// loads typically have similar high-water marks); the slack margin
/// keeps us from churning through reallocation for small wins. See
/// issue #495 for the failure mode — a single 80–200 MB BLAS build
/// pinning VRAM for the rest of the process lifetime.
pub(super) fn scratch_should_shrink(current_capacity: vk::DeviceSize, peak_required: vk::DeviceSize) -> bool {
    current_capacity > peak_required.saturating_mul(2)
        && current_capacity.saturating_sub(peak_required) > BLAS_REBUILD_SLACK_BYTES
}

/// Hysteresis decision for the TLAS instance buffer pair (`#645` /
/// MEM-2-3). Mirrors [`scratch_should_shrink`]'s `2× + slack` shape
/// but with a TLAS-calibrated slack: instance buffers are 64 B/entry
/// and live at MB scale, so the BLAS-scratch's 16 MB slack would
/// effectively never trigger here (a 32 K-instance peak buffer is
/// only 2 MB). 1 MB ≈ 16 K instances — wide enough to absorb
/// adjacent-cell-load variance, narrow enough to actually fire when a
/// big exterior peak settles back into a small interior working set.
///
/// `current_capacity_bytes` is `max_instances × sizeof(VkASInstance)`;
/// `working_set_bytes` is `working_count × sizeof(VkASInstance)`. Pure
/// function so the unit test can pin the threshold math without a
/// live Vulkan device.
pub(super) fn tlas_instance_should_shrink(
    current_capacity_bytes: vk::DeviceSize,
    working_set_bytes: vk::DeviceSize,
) -> bool {
    current_capacity_bytes > working_set_bytes.saturating_mul(2)
        && current_capacity_bytes.saturating_sub(working_set_bytes) > TLAS_REBUILD_SLACK_BYTES
}

/// Shrink a per-frame scratch `Vec` back toward its working set when a
/// past peak frame left it holding far more capacity than the steady
/// state needs. Called at the end of the restore path so scratch
/// vectors behave as "grow fast, shrink on pressure" buffers.
///
/// Shrinks only when current capacity exceeds `2 × max(working_set, floor)` —
/// the 2× hysteresis band prevents thrashing on frame-to-frame variance
/// around the peak, and the `floor` keeps the buffer large enough for
/// the common-case small scenes without repeated reallocation.
///
/// Pulled out as a pure function so the unit test can pin the threshold
/// math without needing a live allocator. See #504 (CPU-side mirror of
/// #495's GPU-side scratch shrink).
pub fn shrink_scratch_if_oversized<T>(vec: &mut Vec<T>, working_set: usize, floor: usize) {
    let target = 2 * working_set.max(floor);
    if vec.capacity() > target {
        vec.shrink_to(target);
    }
}

/// Mid-batch BLAS eviction trigger (#510).
///
/// Returns `true` when the *projected* live BLAS footprint
/// (`already_live + pending_this_batch`) is at or above 90% of the
/// configured budget, so the batched-build Phase 1 should pause and
/// evict previous-cell BLAS before creating more result buffers. The
/// 90% threshold leaves headroom for the batch's final few allocations
/// + the scratch buffer; the budget itself is VRAM/3 so a breach
/// represents genuine residency pressure, not just a hot spike.
///
/// Pulled out as a pure function so the unit test can pin the
/// threshold math without needing a live Vulkan device.
pub(super) fn should_evict_mid_batch(
    total_live_bytes: vk::DeviceSize,
    pending_bytes: vk::DeviceSize,
    budget_bytes: vk::DeviceSize,
) -> bool {
    let projected = total_live_bytes.saturating_add(pending_bytes);
    // projected >= budget * 0.9 without floats: multiply both sides by 10.
    projected.saturating_mul(10) >= budget_bytes.saturating_mul(9)
}

/// Decide whether a `DrawCommand` should emit a TLAS instance.
///
/// Two-axis gate (#516 + #1024):
/// - `in_tlas == false` excludes particles / UI quads / other
///   draws that are by design rasterized-only.
/// - `is_water == true` excludes water surfaces unconditionally
///   (F-WAT-03). Water reflection / refraction rays must hit opaque
///   geometry under the water, not the water plane itself; even
///   though the cell loader uploads water meshes with
///   `for_rt = false` (no BLAS allocated), this predicate keeps the
///   exclusion explicit on the consumer side so a future BLAS-add
///   on the same mesh handle can't silently reintroduce self-hits.
///
/// Pure function so the unit test can pin the contract without a
/// live Vulkan device.
#[inline]
pub(super) fn draw_command_eligible_for_tlas(draw_cmd: &DrawCommand) -> bool {
    draw_cmd.in_tlas && !draw_cmd.is_water
}

/// Vulkan-spec compliance check for AS-build scratch addresses.
///
/// Every `scratch_data.device_address` passed to
/// `cmd_build_acceleration_structures` must be a multiple of
/// `VkPhysicalDeviceAccelerationStructurePropertiesKHR::minAccelerationStructureScratchOffsetAlignment`.
/// Pulled out as a pure function so the unit test can pin the math
/// without a live Vulkan device. See #659 / #260 R-05.
///
/// Trivially true when `align <= 1` — used as the no-op path on
/// RT-disabled GPUs (the caller in `device::pick_physical_device`
/// clamps to 1 when the property isn't queryable).
#[inline]
pub(super) fn is_scratch_aligned(scratch_address: vk::DeviceAddress, align: u32) -> bool {
    if align <= 1 {
        return true;
    }
    scratch_address % align as vk::DeviceAddress == 0
}

pub(super) fn compute_blas_budget(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::DeviceSize {
    let device_local_bytes = super::super::device::total_device_local_bytes(instance, physical_device);
    (device_local_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)
}
