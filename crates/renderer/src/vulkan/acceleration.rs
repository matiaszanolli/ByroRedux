//! Acceleration structure management for RT ray queries.
//!
//! Builds BLAS (bottom-level) per unique mesh and a single TLAS (top-level)
//! rebuilt each frame from all draw instances. The TLAS is bound as a
//! descriptor in the fragment shader for shadow ray queries.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::mesh::GpuMesh;
use crate::vertex::Vertex;
use crate::vulkan::context::DrawCommand;
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;

/// Dispatch helper: reuse the shared transfer fence when available,
/// otherwise fall back to per-call create/destroy (#302).
fn submit_one_time<F>(
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
            super::texture::with_one_time_commands_reuse_fence(device, queue, pool, f_mutex, f)
        }
        None => super::texture::with_one_time_commands(device, queue, pool, f),
    }
}

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

/// Manages BLAS and TLAS for RT ray queries.
///
/// TLAS state is double-buffered per frame-in-flight to avoid
/// synchronization hazards: each frame slot has its own accel structure,
/// instance buffer, and scratch buffer. The per-frame fence wait
/// guarantees the previous use of each slot is complete before reuse,
/// so no additional barriers or `device_wait_idle` calls are needed.
pub struct AccelerationManager {
    accel_loader: ash::khr::acceleration_structure::Device,
    /// One BLAS per mesh in MeshRegistry (indexed by mesh handle).
    blas_entries: Vec<Option<BlasEntry>>,
    /// Per-frame-in-flight TLAS state. Each slot is independently
    /// created/resized when that frame slot first needs it.
    pub tlas: [Option<TlasState>; MAX_FRAMES_IN_FLIGHT],
    /// Per-frame-in-flight TLAS scratch buffer. Grows to the high-water
    /// mark across full rebuilds (`need_new_tlas`); refit/update passes
    /// reuse the existing buffer. See #60 / #424 SIBLING — never a
    /// per-build allocation.
    scratch_buffers: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT],
    /// Shared BLAS scratch buffer (reused across builds, grows to the
    /// high-water mark across single and batched builds). BLAS builds
    /// use one-time command buffers with a fence wait, so a single
    /// shared buffer is safe (no overlapping BLAS builds). Fix #60,
    /// extended to the batched path in M31.
    blas_scratch_buffer: Option<GpuBuffer>,
    /// Reusable scratch buffer for TLAS instance data. Amortized across
    /// frames to avoid ~320KB/frame heap allocation for large scenes.
    tlas_instances_scratch: Vec<vk::AccelerationStructureInstanceKHR>,
    /// Monotonic frame counter for BLAS LRU tracking.
    frame_counter: u64,
    /// Total BLAS memory currently allocated (sum of all BlasEntry.size_bytes).
    total_blas_bytes: vk::DeviceSize,
    /// Maximum BLAS memory budget in bytes. Eviction triggers when exceeded.
    /// Derived at construction time from DEVICE_LOCAL heap size (VRAM / 3)
    /// with a 256 MB floor. On a 12 GB GPU this yields 4 GB (eviction
    /// virtually never fires); on a 6 GB GPU it yields 2 GB (eviction
    /// fires before OOM).
    blas_budget_bytes: vk::DeviceSize,
    /// Entries removed by [`drop_blas`] still referenced by an in-flight
    /// TLAS build. Each entry carries a countdown measured in
    /// `MAX_FRAMES_IN_FLIGHT` frames; when it hits zero the underlying
    /// `VkAccelerationStructureKHR` + buffer are finally destroyed. See #372.
    pending_destroy_blas: Vec<(BlasEntry, u32)>,
    /// Monotonic counter bumped whenever the `blas_entries` map mutates
    /// (add via [`build_blas`] / [`build_blas_batched`], remove via
    /// [`drop_blas`] / [`evict_unused_blas`]). Each [`TlasState`] caches
    /// the value seen at its last BUILD; when the counters disagree the
    /// next [`build_tlas`] knows the per-instance BLAS device addresses
    /// could have shifted and short-circuits to BUILD without paying
    /// the O(N) zip-compare against the cached address list. Steady-
    /// state frames where no BLAS lifecycle events fired keep paying
    /// the zip (it still has to run to detect frustum / draw-list
    /// composition changes), but cell load / unload / eviction frames
    /// — where the comparison is guaranteed to mismatch — skip it. See #300.
    blas_map_generation: u64,
    /// M29 Phase 2 — per-skinned-entity BLAS. One per animated
    /// instance, refit each frame against the SkinComputePipeline
    /// output buffer for that entity. 32 NPCs sharing a malebody mesh
    /// need 32 independent BLAS so each instance's pose feeds its own
    /// shadow / reflection / GI rays. Build flag `ALLOW_UPDATE` is
    /// always set so `cmd_build_acceleration_structures` with
    /// `mode = UPDATE` is legal each frame. Insert / remove bumps
    /// `blas_map_generation` so the TLAS-side cache invalidation
    /// tracks skinned BLAS alongside the static `blas_entries`.
    skinned_blas: std::collections::HashMap<EntityId, BlasEntry>,
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
fn decide_use_update(
    needs_full_rebuild: bool,
    tlas_last_gen: u64,
    current_gen: u64,
    cached_addresses: &[vk::DeviceAddress],
    current_addresses: &[vk::DeviceAddress],
) -> (bool, bool) {
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
/// order and `None` for every dropped one. See #419 — this is the
/// single source of truth the TLAS `instance_custom_index` and the
/// SSBO position must agree on; before it landed the two filter
/// predicates were independent and could silently diverge.
pub fn build_instance_map(len: usize, mut keep: impl FnMut(usize) -> bool) -> Vec<Option<u32>> {
    let mut out = Vec::with_capacity(len);
    let mut next: u32 = 0;
    for i in 0..len {
        if keep(i) {
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
fn scratch_needs_growth(current_capacity: Option<vk::DeviceSize>, required: vk::DeviceSize) -> bool {
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
fn scratch_should_shrink(current_capacity: vk::DeviceSize, peak_required: vk::DeviceSize) -> bool {
    const SLACK: vk::DeviceSize = 16 * 1024 * 1024;
    current_capacity > peak_required.saturating_mul(2)
        && current_capacity.saturating_sub(peak_required) > SLACK
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
pub(super) fn shrink_scratch_if_oversized<T>(vec: &mut Vec<T>, working_set: usize, floor: usize) {
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
fn should_evict_mid_batch(
    total_live_bytes: vk::DeviceSize,
    pending_bytes: vk::DeviceSize,
    budget_bytes: vk::DeviceSize,
) -> bool {
    let projected = total_live_bytes.saturating_add(pending_bytes);
    // projected >= budget * 0.9 without floats: multiply both sides by 10.
    projected.saturating_mul(10) >= budget_bytes.saturating_mul(9)
}

/// How often to check the eviction threshold inside the batched BLAS
/// build. Every N buffers created we test
/// [`should_evict_mid_batch`]; eviction runs only when needed, so the
/// idle cost is one add + one compare per N iterations.
const BATCH_EVICTION_CHECK_INTERVAL: usize = 64;

fn compute_blas_budget(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::DeviceSize {
    const MIN_BUDGET: vk::DeviceSize = 256 * 1024 * 1024; // 256 MB floor
    let device_local_bytes = super::device::total_device_local_bytes(instance, physical_device);
    (device_local_bytes / 3).max(MIN_BUDGET)
}

impl AccelerationManager {
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let accel_loader = ash::khr::acceleration_structure::Device::new(instance, device);
        let blas_budget_bytes = compute_blas_budget(instance, physical_device);
        log::info!(
            "BLAS memory budget: {} MB (derived from VRAM)",
            blas_budget_bytes / (1024 * 1024)
        );
        Self {
            accel_loader,
            blas_entries: Vec::new(),
            tlas: [None, None],
            scratch_buffers: [None, None],
            blas_scratch_buffer: None,
            tlas_instances_scratch: Vec::new(),
            frame_counter: 0,
            total_blas_bytes: 0,
            blas_budget_bytes,
            pending_destroy_blas: Vec::new(),
            blas_map_generation: 0,
            skinned_blas: std::collections::HashMap::new(),
        }
    }

    /// Queue a BLAS for deferred destruction.
    ///
    /// Unlike [`evict_unused_blas`](Self::evict_unused_blas) — which runs
    /// only on entries idle for `MAX_FRAMES_IN_FLIGHT` frames and so can
    /// destroy immediately — `drop_blas` is called by the cell loader on
    /// unload and the entry may still be in-flight. The entry moves to
    /// `pending_destroy_blas` and the actual `VkAccelerationStructureKHR`
    /// + buffer destruction is delayed until the countdown expires in
    /// [`tick_deferred_destroy`](Self::tick_deferred_destroy).
    ///
    /// Also forces a full TLAS rebuild on both frame slots so no
    /// subsequent `BUILD`/`UPDATE` references the dropped BLAS address.
    /// See #372. No-op if the handle is not a live BLAS.
    pub fn drop_blas(&mut self, handle: u32) {
        let idx = handle as usize;
        let Some(slot) = self.blas_entries.get_mut(idx) else {
            return;
        };
        let Some(entry) = slot.take() else {
            return;
        };
        self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
        // Two-frame countdown matches the existing SSBO rebuild pattern.
        self.pending_destroy_blas.push((entry, 2));
        // BLAS map mutated — bump generation so the next build_tlas
        // can short-circuit the per-instance zip-compare. #300.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        for tlas_slot in &mut self.tlas {
            if let Some(ref mut t) = tlas_slot {
                t.needs_full_rebuild = true;
            }
        }
    }

    /// Drain and destroy BLAS entries whose defer countdown has reached
    /// zero. Call once per frame alongside
    /// `MeshRegistry::tick_deferred_destroy`.
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.pending_destroy_blas.retain_mut(|(entry, countdown)| {
            if *countdown == 0 {
                // SAFETY: the countdown guarantees no in-flight command
                // buffer still references this acceleration structure.
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(entry.accel, None);
                }
                entry.buffer.destroy(device, allocator);
                false
            } else {
                *countdown -= 1;
                true
            }
        });
    }

    /// Build a BLAS for a mesh. Call after uploading the mesh to GPU.
    ///
    /// NOTE: This submits a one-time command buffer and blocks on a fence
    /// via `with_one_time_commands`. Acceptable during scene load; for
    /// streaming, batch BLAS builds into the frame's command buffer to
    /// avoid per-mesh GPU stalls. See #284 (C2-04).
    pub fn build_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        mesh_handle: u32,
        mesh: &GpuMesh,
        vertex_count: u32,
        index_count: u32,
    ) -> Result<()> {
        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;

        // SAFETY: get_buffer_device_address requires the buffer was created with
        // SHADER_DEVICE_ADDRESS. Our vertex/index buffers are created with this flag.
        // The returned u64 address is valid for the buffer's lifetime.
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(mesh.vertex_buffer.buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(mesh.index_buffer.buffer),
            )
        };

        // SAFETY: DeviceOrHostAddressConstKHR is a union — we initialize the
        // device_address field because we're using device-local buffers (not host pointers).
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
            .vertex_format(vk::Format::R32G32B32_SFLOAT)
            .vertex_data(vk::DeviceOrHostAddressConstKHR {
                device_address: vertex_address,
            })
            .vertex_stride(vertex_stride)
            .max_vertex(vertex_count.saturating_sub(1))
            .index_type(vk::IndexType::UINT32)
            .index_data(vk::DeviceOrHostAddressConstKHR {
                device_address: index_address,
            });

        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });

        let primitive_count = index_count / 3;

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(std::slice::from_ref(&geometry));

        // Query sizes.
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[primitive_count],
                &mut sizes,
            );
        };

        // Allocate result buffer in DEVICE_LOCAL memory (GPU-built, GPU-read only).
        let mut result_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )?;

        // Create the acceleration structure object.
        let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(result_buffer.buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

        let accel = unsafe {
            self.accel_loader
                .create_acceleration_structure(&accel_info, None)
                .context("Failed to create BLAS")?
        };

        // Wrap the remaining fallible operations in a closure so we can
        // clean up `accel` + `result_buffer` on any failure. Without this,
        // a scratch allocation or command submission error would leak both.
        let build_result = (|| -> Result<()> {
            // Reuse persisted BLAS scratch buffer; only reallocate if the current
            // one is too small for this build. Grow-only policy via shared
            // helper — see #60 / #424 SIBLING.
            let need_new_scratch = scratch_needs_growth(
                self.blas_scratch_buffer.as_ref().map(|b| b.size),
                sizes.build_scratch_size,
            );

            if need_new_scratch {
                if let Some(mut old) = self.blas_scratch_buffer.take() {
                    old.destroy(device, allocator);
                }
                self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    sizes.build_scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?);
            }

            // SAFETY: scratch buffer was just created with SHADER_DEVICE_ADDRESS flag.
            // NOTE: scratch device address should be aligned to
            // minAccelerationStructureScratchOffsetAlignment (typically
            // 128 or 256). gpu-allocator returns GpuOnly allocations at
            // 256+ alignment on all known desktop drivers, but this is not
            // explicitly guaranteed. A future hardening pass should query
            // the property at device selection and enforce alignment here.
            // See #260 (R-05).
            let scratch_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default()
                        .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
                )
            };

            // Build the BLAS via one-time command buffer.
            // SAFETY: DeviceOrHostAddressKHR union — device_address field used for device builds.
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .dst_acceleration_structure(accel)
                .geometries(std::slice::from_ref(&geometry))
                .scratch_data(vk::DeviceOrHostAddressKHR {
                    device_address: scratch_address,
                });

            let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count(primitive_count)
                .primitive_offset(0)
                .first_vertex(0);

            submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
                unsafe {
                    self.accel_loader.cmd_build_acceleration_structures(
                        cmd,
                        &[build_info],
                        &[std::slice::from_ref(&range_info)],
                    );
                }
                Ok(())
            })
        })();

        if let Err(e) = build_result {
            // Clean up the accel structure and result buffer that were
            // already created before the build failed.
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(accel, None);
            }
            result_buffer.destroy(device, allocator);
            return Err(e);
        }

        // Get the BLAS device address.
        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };

        // Store BLAS entry (scratch buffer is retained for next build).
        let handle = mesh_handle as usize;
        let blas_size = result_buffer.size;
        while self.blas_entries.len() <= handle {
            self.blas_entries.push(None);
        }
        self.total_blas_bytes += blas_size;
        self.blas_entries[handle] = Some(BlasEntry {
            accel,
            buffer: result_buffer,
            device_address,
            last_used_frame: self.frame_counter,
            size_bytes: blas_size,
            build_scratch_size: sizes.build_scratch_size,
        });
        // BLAS map mutated — see #300.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);

        Ok(())
    }

    /// M29 Phase 2: build a per-skinned-entity BLAS from the entity's
    /// SkinComputePipeline output buffer + the mesh's existing index
    /// buffer. Sets `ALLOW_UPDATE` on the build flags so subsequent
    /// per-frame `cmd_build_acceleration_structures(mode = UPDATE)`
    /// is legal (refit-in-place against the same vertex source —
    /// topology never changes for a skinned mesh).
    ///
    /// `vertex_buffer` is the SkinSlot's output_buffer and must have
    /// been created with `SHADER_DEVICE_ADDRESS +
    /// ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR + STORAGE_BUFFER`
    /// (skin_compute.rs already does this). `index_buffer` is reused
    /// from the bind-pose `GpuMesh.index_buffer` — topology stays
    /// identical across frames so we don't need a per-entity index
    /// buffer. Caller is responsible for inserting a
    /// COMPUTE_SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_INPUT_READ
    /// barrier on `vertex_buffer` before this build runs.
    ///
    /// Initial build copies bind-pose vertices through the SkinSlot's
    /// output (the caller dispatches the compute pass first), so the
    /// BLAS is correct from frame 0. Subsequent frames must call
    /// [`Self::refit_skinned_blas`] in `mode = UPDATE`.
    pub fn build_skinned_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        entity_id: EntityId,
        vertex_buffer: vk::Buffer,
        vertex_count: u32,
        index_buffer: vk::Buffer,
        index_count: u32,
    ) -> Result<()> {
        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(index_buffer),
            )
        };
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
            .vertex_format(vk::Format::R32G32B32_SFLOAT)
            .vertex_data(vk::DeviceOrHostAddressConstKHR {
                device_address: vertex_address,
            })
            .vertex_stride(vertex_stride)
            .max_vertex(vertex_count.saturating_sub(1))
            .index_type(vk::IndexType::UINT32)
            .index_data(vk::DeviceOrHostAddressConstKHR {
                device_address: index_address,
            });
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            // Skinned meshes are typically opaque; if any actor mesh
            // ever ships alpha-tested triangles, the per-instance
            // `two_sided` flag in the TLAS instance still toggles
            // backface-cull for that draw. Leaving OPAQUE here matches
            // the static `build_blas` path.
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });
        let primitive_count = index_count / 3;

        // Build flags: ALLOW_UPDATE makes `mode = UPDATE` legal each
        // frame; PREFER_FAST_BUILD trades a small ray-tracing perf hit
        // for cheaper per-frame refits (skinned BLAS rebuild every
        // frame, whereas static BLAS builds once and stays).
        let build_flags = vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
            | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD;

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(build_flags)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(std::slice::from_ref(&geometry));

        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[primitive_count],
                &mut sizes,
            );
        };

        let mut result_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )?;

        let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(result_buffer.buffer)
            .size(sizes.acceleration_structure_size)
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);
        let accel = unsafe {
            self.accel_loader
                .create_acceleration_structure(&accel_info, None)
                .context("create skinned BLAS")?
        };

        let build_result = (|| -> Result<()> {
            // Skinned BLAS uses the SAME shared scratch buffer as
            // static builds — the build still runs in a one-time
            // command buffer with a fence wait so there's no overlap.
            // `update_scratch_size` for refits is at most
            // `build_scratch_size` per Vulkan spec, so the existing
            // grow-only policy stays correct.
            let need_new_scratch = scratch_needs_growth(
                self.blas_scratch_buffer.as_ref().map(|b| b.size),
                sizes.build_scratch_size,
            );
            if need_new_scratch {
                if let Some(mut old) = self.blas_scratch_buffer.take() {
                    old.destroy(device, allocator);
                }
                self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    sizes.build_scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?);
            }
            let scratch_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default()
                        .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
                )
            };
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(build_flags)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .dst_acceleration_structure(accel)
                .geometries(std::slice::from_ref(&geometry))
                .scratch_data(vk::DeviceOrHostAddressKHR {
                    device_address: scratch_address,
                });
            let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                .primitive_count(primitive_count)
                .primitive_offset(0)
                .first_vertex(0);
            submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
                unsafe {
                    self.accel_loader.cmd_build_acceleration_structures(
                        cmd,
                        &[build_info],
                        &[std::slice::from_ref(&range_info)],
                    );
                }
                Ok(())
            })
        })();

        if let Err(e) = build_result {
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(accel, None);
            }
            result_buffer.destroy(device, allocator);
            return Err(e);
        }

        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };

        let blas_size = result_buffer.size;
        self.total_blas_bytes += blas_size;
        self.skinned_blas.insert(
            entity_id,
            BlasEntry {
                accel,
                buffer: result_buffer,
                device_address,
                last_used_frame: self.frame_counter,
                size_bytes: blas_size,
                build_scratch_size: sizes.build_scratch_size,
            },
        );
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);

        Ok(())
    }

    /// Refit an existing skinned BLAS in-place against an updated
    /// vertex buffer. Topology is unchanged; only the vertex positions
    /// shift. `cmd` must already have a
    /// COMPUTE_SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_INPUT_READ
    /// barrier on `vertex_buffer` recorded before this call.
    ///
    /// Refit cost is much lower than full rebuild but produces a BLAS
    /// with somewhat reduced ray-trace efficiency over time. Bethesda
    /// per-frame skin deltas are small so quality holds; if a session
    /// reveals visible degradation, the caller can periodically
    /// destroy the entry + call [`Self::build_skinned_blas`] again
    /// to start fresh (deferred to a follow-up).
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer; `vertex_buffer` must
    /// already have the COMPUTE_SHADER_WRITE → AS_BUILD_INPUT_READ
    /// barrier in place.
    pub unsafe fn refit_skinned_blas(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        entity_id: EntityId,
        vertex_buffer: vk::Buffer,
        vertex_count: u32,
        index_buffer: vk::Buffer,
        index_count: u32,
    ) -> Result<()> {
        let entry = self
            .skinned_blas
            .get_mut(&entity_id)
            .with_context(|| format!("no skinned BLAS for entity {entity_id}"))?;
        let scratch_buffer = self
            .blas_scratch_buffer
            .as_ref()
            .context("blas_scratch_buffer absent — must be allocated by build_skinned_blas first")?;

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;
        let vertex_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(vertex_buffer),
            )
        };
        let index_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(index_buffer),
            )
        };
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
            .vertex_format(vk::Format::R32G32B32_SFLOAT)
            .vertex_data(vk::DeviceOrHostAddressConstKHR {
                device_address: vertex_address,
            })
            .vertex_stride(vertex_stride)
            .max_vertex(vertex_count.saturating_sub(1))
            .index_type(vk::IndexType::UINT32)
            .index_data(vk::DeviceOrHostAddressConstKHR {
                device_address: index_address,
            });
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles });
        let primitive_count = index_count / 3;
        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(scratch_buffer.buffer),
            )
        };

        // mode = UPDATE: src == dst == this entity's BLAS. Vulkan
        // refits in-place against the new vertex data; topology must
        // stay identical to the original BUILD's geometry. The
        // ALLOW_UPDATE flag set on initial build is what makes this
        // legal.
        let build_flags = vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
            | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD;
        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
            .flags(build_flags)
            .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
            .src_acceleration_structure(entry.accel)
            .dst_acceleration_structure(entry.accel)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_address,
            });
        let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(primitive_count)
            .primitive_offset(0)
            .first_vertex(0);
        unsafe {
            self.accel_loader.cmd_build_acceleration_structures(
                cmd,
                &[build_info],
                &[std::slice::from_ref(&range_info)],
            );
        }
        entry.last_used_frame = self.frame_counter;
        Ok(())
    }

    /// Emit the inter-build scratch-buffer serialise barrier required
    /// when consecutive `cmd_build_acceleration_structures` calls share
    /// the same scratch region (as every BLAS build / refit in this
    /// manager does — `blas_scratch_buffer` is allocated once and
    /// reused). Vulkan spec
    /// (`VkAccelerationStructureBuildGeometryInfoKHR > scratchData`)
    /// requires `ACCELERATION_STRUCTURE_WRITE → ACCELERATION_STRUCTURE_WRITE`
    /// at `ACCELERATION_STRUCTURE_BUILD_KHR` stage between such calls.
    ///
    /// Stateless (the `&self` is for discoverability — the helper does
    /// not touch any field). Caller emits this between iterations of a
    /// build loop, **not** before the first iteration. See #642.
    pub fn record_scratch_serialize_barrier(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
            .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR);
        unsafe {
            // SAFETY: `cmd` is a recording command buffer the caller
            // owns; the barrier touches only AS-build state and has
            // no aliasing concerns.
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );
        }
    }

    /// Look up a per-skinned-entity BLAS entry. Used by the TLAS
    /// build path to override the per-mesh BLAS lookup when a
    /// `DrawCommand` carries a `skin_slot_id`.
    pub fn skinned_blas_entry(&self, entity_id: EntityId) -> Option<&BlasEntry> {
        self.skinned_blas.get(&entity_id)
    }

    /// Drop a per-skinned-entity BLAS. Caller must defer the destroy
    /// until any in-flight frame referencing it has completed (the
    /// renderer pairs this with `device_wait_idle` in the Drop chain
    /// + a per-frame deferred-destroy queue for cell unloads).
    pub fn drop_skinned_blas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        entity_id: EntityId,
    ) {
        if let Some(mut entry) = self.skinned_blas.remove(&entity_id) {
            self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(entry.accel, None);
            }
            entry.buffer.destroy(device, allocator);
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        }
    }

    /// Iterator over all skinned-entity IDs the manager currently
    /// holds a BLAS for. Used by the renderer's Drop chain to
    /// collect IDs for bulk teardown without holding a reference
    /// across mutation.
    pub fn skinned_blas_entities(&self) -> Vec<EntityId> {
        self.skinned_blas.keys().copied().collect()
    }

    /// Build BLAS for multiple meshes in a single command buffer submission.
    ///
    /// This eliminates the per-mesh fence stall from `build_blas` by recording
    /// all BLAS build commands into one command buffer with memory barriers
    /// between builds that share the scratch buffer. For 3000 meshes, this
    /// reduces scene load from 150-600ms (3000 fence round-trips) to ~5-15ms
    /// (single submission + one fence wait).
    ///
    /// Each build reuses the shared `blas_scratch_buffer` (grown to the max
    /// scratch size needed). Builds are serialized within the command buffer
    /// via `ACCELERATION_STRUCTURE_BUILD` → `ACCELERATION_STRUCTURE_BUILD`
    /// memory barriers since they share scratch memory.
    pub fn build_blas_batched(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        transfer_fence: Option<&std::sync::Mutex<vk::Fence>>,
        meshes: &[(u32, &GpuMesh, u32, u32)], // (mesh_handle, mesh, vertex_count, index_count)
    ) -> Result<usize> {
        if meshes.is_empty() {
            return Ok(0);
        }

        let vertex_stride = std::mem::size_of::<Vertex>() as vk::DeviceSize;

        // Phase 1: Query sizes and allocate result buffers for all meshes.
        struct PreparedBlas {
            mesh_handle: u32,
            accel: vk::AccelerationStructureKHR,
            buffer: GpuBuffer,
            /// `vk::AccelerationStructureGeometryKHR<'a>` carries a
            /// PHANTOM lifetime from ash's typed builder API — the
            /// compiler can't see that every union field used in the
            /// `BLAS-from-device-buffer` path is value-typed (`u64`
            /// device addresses + small enums), so without an
            /// annotation the borrow checker would tie the struct's
            /// lifetime to the local `triangles_data` Vec. We fill
            /// only `device_address: u64` (no host pointers, no Rust
            /// references) so the `'static` claim is sound.
            ///
            /// **Future-proof invariant**: every `.geometry()`-reachable
            /// field must remain value-typed. Adding a host-pointer
            /// variant or a `&[T]` body would make this UB with no
            /// compiler warning. See #580 / SAFE-21.
            geometry: vk::AccelerationStructureGeometryKHR<'static>,
            primitive_count: u32,
            /// Per-mesh scratch size from Phase 1 sizing — stored so the
            /// final `BlasEntry` can remember it for
            /// `shrink_blas_scratch_to_fit` (#495). Max across meshes is
            /// tracked separately in `max_scratch_size` for the single
            /// shared build scratch allocation.
            build_scratch_size: vk::DeviceSize,
        }

        let mut prepared: Vec<PreparedBlas> = Vec::with_capacity(meshes.len());
        let mut max_scratch_size: vk::DeviceSize = 0;

        // We need to keep the triangles data alive for the geometry references.
        // Store them in a parallel vec since the geometry structs reference them.
        let mut triangles_data: Vec<vk::AccelerationStructureGeometryTrianglesDataKHR> =
            Vec::with_capacity(meshes.len());

        for &(_mesh_handle, mesh, vertex_count, _index_count) in meshes {
            let vertex_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(mesh.vertex_buffer.buffer),
                )
            };
            let index_address = unsafe {
                device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(mesh.index_buffer.buffer),
                )
            };

            let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                .vertex_format(vk::Format::R32G32B32_SFLOAT)
                .vertex_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: vertex_address,
                })
                .vertex_stride(vertex_stride)
                .max_vertex(vertex_count.saturating_sub(1))
                .index_type(vk::IndexType::UINT32)
                .index_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: index_address,
                });

            triangles_data.push(triangles);
        }

        // Pre-batch eviction — release any previous-cell BLAS that is
        // safely past `MAX_FRAMES_IN_FLIGHT + 1` idle before we start
        // creating result buffers. Cheap when nothing qualifies
        // (`evict_unused_blas` early-returns under budget); helps cell
        // transitions where the outgoing cell's BLAS still holds live
        // memory that the incoming cell's batch is about to need. #510.
        unsafe {
            self.evict_unused_blas(device, allocator);
        }

        // Running sum of `acceleration_structure_size` across the Phase 1
        // buffers we've created for *this batch*. Combined with
        // `self.total_blas_bytes` it gives the projected footprint the
        // mid-batch eviction predicate tests. See [`should_evict_mid_batch`].
        let mut pending_bytes: vk::DeviceSize = 0;
        // Now build geometries referencing the stored triangles data.
        for (idx, &(mesh_handle, _mesh, _vertex_count, index_count)) in meshes.iter().enumerate() {
            // Mid-batch eviction check. Trigger only every N iterations
            // so the cost is amortized; the predicate itself is pure
            // arithmetic. #510.
            if idx > 0 && idx % BATCH_EVICTION_CHECK_INTERVAL == 0 {
                if should_evict_mid_batch(
                    self.total_blas_bytes,
                    pending_bytes,
                    self.blas_budget_bytes,
                ) {
                    // SAFETY: prepared buffers for this batch are local
                    // to `prepared` and not yet in `self.blas_entries`,
                    // so `evict_unused_blas` cannot touch them — it only
                    // frees entries in `blas_entries` that are past the
                    // idle threshold.
                    unsafe {
                        self.evict_unused_blas(device, allocator);
                    }
                }
            }

            let primitive_count = index_count / 3;

            // SAFETY: `vk::AccelerationStructureGeometryKHR<'a>` carries a
            // phantom lifetime from ash's typed builder API. We never
            // populate a Rust borrow into the geometry union — every
            // field in `vk::AccelerationStructureGeometryDataKHR.triangles`
            // we reach (vertex / index `device_address: u64`,
            // `vertex_format`, `index_type`, primitive count, etc.) is
            // value-typed; no host pointers, no `&[T]`. The `'static`
            // annotation on `PreparedBlas::geometry` (line ~610) is
            // therefore sound regardless of whether `triangles_data`
            // is still in scope.
            //
            // The geometry value itself is consumed inline below to
            // build the per-BLAS sizes query and stored on
            // `PreparedBlas::geometry` for the Phase 2 batched build
            // submission. No real cross-Vec borrow lives across that
            // boundary.
            //
            // Pre-#580 this comment claimed both "triangles_data lives
            // for the function" (which would imply a real borrow) and
            // "geometry holds a copy of the union data" (correct);
            // only the second half is the real invariant. See SAFE-21.
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    triangles: triangles_data[idx],
                });

            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                .flags(
                    vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                        | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
                )
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            unsafe {
                self.accel_loader.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &build_info,
                    &[primitive_count],
                    &mut sizes,
                );
            };

            max_scratch_size = max_scratch_size.max(sizes.build_scratch_size);
            pending_bytes = pending_bytes.saturating_add(sizes.acceleration_structure_size);

            let mut result_buffer = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?;

            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(result_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

            let accel = unsafe {
                match self
                    .accel_loader
                    .create_acceleration_structure(&accel_info, None)
                {
                    Ok(a) => a,
                    Err(e) => {
                        result_buffer.destroy(device, allocator);
                        anyhow::bail!("Failed to create BLAS for mesh {mesh_handle}: {e}");
                    }
                }
            };

            prepared.push(PreparedBlas {
                mesh_handle,
                accel,
                buffer: result_buffer,
                geometry,
                primitive_count,
                build_scratch_size: sizes.build_scratch_size,
            });
        }

        // Phase 2: Ensure scratch buffer is large enough. Grow-only
        // policy via shared helper — see #60 / #424 SIBLING.
        let need_new_scratch = scratch_needs_growth(
            self.blas_scratch_buffer.as_ref().map(|b| b.size),
            max_scratch_size,
        );

        if need_new_scratch {
            if let Some(mut old) = self.blas_scratch_buffer.take() {
                old.destroy(device, allocator);
            }
            self.blas_scratch_buffer = Some(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                max_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?);
        }

        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default()
                    .buffer(self.blas_scratch_buffer.as_ref().unwrap().buffer),
            )
        };

        // Phase 3: Create query pool for compacted size readback.
        let n = prepared.len() as u32;
        let query_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR)
            .query_count(n);
        let query_pool = unsafe {
            device
                .create_query_pool(&query_pool_info, None)
                .context("Failed to create compaction query pool")?
        };
        // Reset the query pool before use (required by Vulkan spec).
        unsafe {
            device.reset_query_pool(query_pool, 0, n);
        }

        // Phase 4: Record builds + compaction size queries into one command buffer.
        let build_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
            for (i, p) in prepared.iter().enumerate() {
                if i > 0 {
                    self.record_scratch_serialize_barrier(device, cmd);
                }

                let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                    .flags(
                        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                            | vk::BuildAccelerationStructureFlagsKHR::ALLOW_COMPACTION,
                    )
                    .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                    .dst_acceleration_structure(p.accel)
                    .geometries(std::slice::from_ref(&p.geometry))
                    .scratch_data(vk::DeviceOrHostAddressKHR {
                        device_address: scratch_address,
                    });

                let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
                    .primitive_count(p.primitive_count)
                    .primitive_offset(0)
                    .first_vertex(0);

                unsafe {
                    self.accel_loader.cmd_build_acceleration_structures(
                        cmd,
                        &[build_info],
                        &[std::slice::from_ref(&range_info)],
                    );
                }
            }

            // Barrier: all builds must complete before querying compacted sizes.
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::DependencyFlags::empty(),
                    &[barrier],
                    &[],
                    &[],
                );
            }

            // Query compacted sizes for all built BLAS.
            let accel_handles: Vec<vk::AccelerationStructureKHR> =
                prepared.iter().map(|p| p.accel).collect();
            unsafe {
                self.accel_loader
                    .cmd_write_acceleration_structures_properties(
                        cmd,
                        &accel_handles,
                        vk::QueryType::ACCELERATION_STRUCTURE_COMPACTED_SIZE_KHR,
                        query_pool,
                        0,
                    );
            }

            Ok(())
        });

        if let Err(e) = build_result {
            for mut p in prepared {
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
            unsafe {
                device.destroy_query_pool(query_pool, None);
            }
            return Err(e);
        }

        // Phases 5 + 6: Read back compacted sizes, then allocate compacted
        // destination buffers + acceleration structures. Wrapped in a
        // closure so that any mid-loop allocation failure can roll back
        // the partial compact-side state plus the still-owned `prepared`
        // originals and the `query_pool`. Pre-#316 these `?` exits leaked
        // every Vulkan handle whose Drop relies on the explicit `destroy`
        // calls in phase 7. Mirrors the build/copy-phase cleanup pattern
        // at lines 733-745 / 815-832.
        let alloc_compact = || -> Result<(
            Vec<(
                u32,
                vk::AccelerationStructureKHR,
                GpuBuffer,
                vk::DeviceSize,
            )>,
            u64,
            u64,
        )> {
            let mut compacted_sizes = vec![0u64; prepared.len()];
            unsafe {
                device
                    .get_query_pool_results(
                        query_pool,
                        0,
                        &mut compacted_sizes,
                        vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                    )
                    .context("Failed to read compaction query results")?;
            }

            let total_before: u64 = prepared.iter().map(|p| p.buffer.size).sum();
            let total_after: u64 = compacted_sizes.iter().sum();

            // Tuple: (mesh_handle, compacted accel, compacted buffer,
            // build_scratch_size). Scratch size is propagated from
            // `prepared` so the final `BlasEntry` can remember what
            // scratch this mesh consumed at build time (#495).
            let mut compact_accels: Vec<(
                u32,
                vk::AccelerationStructureKHR,
                GpuBuffer,
                vk::DeviceSize,
            )> = Vec::with_capacity(prepared.len());

            for (i, p) in prepared.iter().enumerate() {
                let compact_size = compacted_sizes[i];

                let compact_buffer = GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    compact_size,
                    vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                )?;

                let compact_accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(compact_buffer.buffer)
                    .size(compact_size)
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

                let compact_accel = unsafe {
                    match self
                        .accel_loader
                        .create_acceleration_structure(&compact_accel_info, None)
                    {
                        Ok(a) => a,
                        Err(e) => {
                            // Buffer was created in this iteration but not
                            // yet pushed into `compact_accels`, so the outer
                            // cleanup loop won't see it — destroy it locally
                            // before bubbling so the OOM path is leak-free.
                            let mut b = compact_buffer;
                            b.destroy(device, allocator);
                            anyhow::bail!("Failed to create compact BLAS: {e}");
                        }
                    }
                };

                compact_accels.push((
                    p.mesh_handle,
                    compact_accel,
                    compact_buffer,
                    p.build_scratch_size,
                ));
            }

            Ok((compact_accels, total_before, total_after))
        };

        let (compact_accels, total_before, total_after) = match alloc_compact() {
            Ok(v) => v,
            Err(e) => {
                // Roll back: destroy the originals (phase 7's job on the
                // happy path) and the query pool. Partial phase-6 compact
                // state was already cleaned up inside `alloc_compact`.
                for mut p in prepared {
                    unsafe {
                        self.accel_loader
                            .destroy_acceleration_structure(p.accel, None);
                    }
                    p.buffer.destroy(device, allocator);
                }
                unsafe {
                    device.destroy_query_pool(query_pool, None);
                }
                return Err(e);
            }
        };

        // Record compaction copies in a second command buffer.
        let copy_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
            for (i, (_, compact_accel, _, _)) in compact_accels.iter().enumerate() {
                let copy_info = vk::CopyAccelerationStructureInfoKHR::default()
                    .src(prepared[i].accel)
                    .dst(*compact_accel)
                    .mode(vk::CopyAccelerationStructureModeKHR::COMPACT);

                unsafe {
                    self.accel_loader
                        .cmd_copy_acceleration_structure(cmd, &copy_info);
                }
            }
            Ok(())
        });

        // Destroy the query pool — no longer needed.
        unsafe {
            device.destroy_query_pool(query_pool, None);
        }

        if let Err(e) = copy_result {
            // Clean up both original and compact structures on failure.
            for mut p in prepared {
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(p.accel, None);
                }
                p.buffer.destroy(device, allocator);
            }
            for (_, accel, mut buf, _) in compact_accels {
                unsafe {
                    self.accel_loader
                        .destroy_acceleration_structure(accel, None);
                }
                buf.destroy(device, allocator);
            }
            return Err(e);
        }

        // Phase 7: Destroy originals, store compacted entries.
        for mut p in prepared {
            unsafe {
                self.accel_loader
                    .destroy_acceleration_structure(p.accel, None);
            }
            p.buffer.destroy(device, allocator);
        }

        let count = compact_accels.len();
        for (mesh_handle, accel, buffer, build_scratch_size) in compact_accels {
            let device_address = unsafe {
                self.accel_loader.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(accel),
                )
            };

            let handle = mesh_handle as usize;
            let blas_size = buffer.size;
            while self.blas_entries.len() <= handle {
                self.blas_entries.push(None);
            }
            self.total_blas_bytes += blas_size;
            self.blas_entries[handle] = Some(BlasEntry {
                accel,
                buffer,
                device_address,
                last_used_frame: self.frame_counter,
                size_bytes: blas_size,
                build_scratch_size,
            });
        }
        // BLAS map mutated (one bump for the whole batch — generation is
        // a "did anything change" flag, not a count). See #300.
        if count > 0 {
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
        }

        let savings_pct = if total_before > 0 {
            100.0 * (1.0 - total_after as f64 / total_before as f64)
        } else {
            0.0
        };
        log::info!(
            "Batched BLAS build: {} meshes, compacted {:.1} KB → {:.1} KB ({:.0}% savings)",
            count,
            total_before as f64 / 1024.0,
            total_after as f64 / 1024.0,
            savings_pct,
        );
        Ok(count)
    }

    /// Build or rebuild the TLAS from draw commands for a specific frame-in-flight slot.
    ///
    /// Each frame slot has its own TLAS resources (accel structure, instance buffer,
    /// scratch buffer), so overlapping frames cannot interfere. The caller's fence
    /// wait guarantees the previous use of this slot is complete.
    ///
    /// `instance_map[i]` is `Some(ssbo_idx)` when `draw_commands[i]` is present
    /// in the compacted SSBO produced by the draw-frame builder, or `None`
    /// when the draw command was filtered out (e.g. the mesh handle no longer
    /// resolves). `instance_custom_index` is set from this map so the shader
    /// always indexes a valid SSBO entry regardless of which filter rejected
    /// a draw command. Before #419 the TLAS encoded the raw enumerate index
    /// here, which diverged from the SSBO's compacted index the moment any
    /// filter rejected anything — silent material/transform corruption on
    /// every RT hit downstream.
    ///
    /// Records commands into `cmd` — caller must ensure a memory barrier after.
    pub unsafe fn build_tlas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        draw_commands: &[DrawCommand],
        instance_map: &[Option<u32>],
        frame_index: usize,
    ) -> Result<()> {
        // Advance the frame counter for LRU tracking.
        self.frame_counter += 1;

        debug_assert_eq!(
            instance_map.len(),
            draw_commands.len(),
            "instance_map must be 1:1 with draw_commands (see #419)"
        );

        // Build instance array. `instance_custom_index` comes from the shared
        // `instance_map` so it matches the SSBO position exactly — the TLAS
        // can still be sparse (missing BLAS drop instances, particle / UI
        // draws with `in_tlas = false`), but the shader's
        // `rayQueryGetIntersectionInstanceCustomIndexEXT` is guaranteed to
        // land on the right SSBO entry. Pre-#516 `in_tlas` was also flipped
        // off for out-of-frustum entities; now frustum culling only gates
        // rasterization (`in_raster`) and off-screen occluders stay in
        // the TLAS so on-screen fragments' shadow / reflection / GI rays
        // hit them. See #419 + #516.
        let mut instances = std::mem::take(&mut self.tlas_instances_scratch);
        instances.clear();
        instances.reserve(draw_commands.len());
        for (i, draw_cmd) in draw_commands.iter().enumerate() {
            // Skip instances not flagged for TLAS inclusion (particles,
            // UI quad — small / transient / 2D). Frustum-culled geometry
            // still reaches this loop with `in_tlas = true` post-#516.
            if !draw_cmd.in_tlas {
                continue;
            }
            // M29 Phase 2 — skinned draws (`bone_offset != 0`) reference
            // a per-entity BLAS that's refit each frame against the
            // SkinComputePipeline output buffer. Look up by entity_id
            // first; rigid draws fall through to the per-mesh
            // `blas_entries` table. The skinned-BLAS path keeps the
            // same `last_used_frame` LRU bump as the static path so a
            // skinned NPC dropped from the draw list ages out
            // alongside its mesh.
            let blas_address: vk::DeviceAddress = if draw_cmd.bone_offset != 0 {
                let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) else {
                    // Skinned entity hasn't had its BLAS built yet
                    // (first sight is processed earlier in the same
                    // draw_frame; this gate is defensive — if we get
                    // here the entity will be invisible to RT this
                    // frame, but raster's inline-skinning path still
                    // renders it correctly).
                    continue;
                };
                entry.last_used_frame = self.frame_counter;
                entry.device_address
            } else {
                let mesh_handle = draw_cmd.mesh_handle as usize;
                let Some(Some(blas)) = self.blas_entries.get_mut(mesh_handle) else {
                    continue;
                };
                blas.last_used_frame = self.frame_counter;
                blas.device_address
            };
            // Skip commands that the SSBO builder also skipped. This
            // keeps the two filters in lockstep even when `blas_entries`
            // and `mesh_registry` diverge (e.g. a BLAS briefly survives
            // its source mesh during eviction).
            let Some(ssbo_idx) = instance_map.get(i).copied().flatten() else {
                continue;
            };

            // Convert column-major model_matrix [f32; 16] to VkTransformMatrixKHR (3x4 row-major).
            let m = &draw_cmd.model_matrix;
            let transform = vk::TransformMatrixKHR {
                matrix: [
                    m[0], m[4], m[8], m[12], m[1], m[5], m[9], m[13], m[2], m[6], m[10], m[14],
                ],
            };

            // SAFETY: AccelerationStructureReferenceKHR is a union — device_handle field
            // is used because our BLAS is on-device (not host-built). The address was
            // obtained from get_acceleration_structure_device_address after BLAS creation.
            //
            // Gate TRIANGLE_FACING_CULL_DISABLE on `draw_cmd.two_sided` so RT
            // traversal matches what the rasterizer renders. Pre-#416 every
            // instance disabled backface culling, so shadow / GI rays hit the
            // interior backfaces of closed single-sided meshes (rooms,
            // buildings) from outside — self-shadowing on far walls, ~2× ray
            // cost on closed meshes. The `two_sided` bit already rides on
            // `DrawCommand` (set from NiTriShape's NIF properties) and the
            // rasterizer pipeline cache keys on it via `PipelineKey`; the RT
            // path now honors the same bit.
            let instance_flags = if draw_cmd.two_sided {
                vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw()
            } else {
                0
            };
            instances.push(vk::AccelerationStructureInstanceKHR {
                transform,
                // #419 — SSBO-compacted index from the shared map, NOT
                // the raw enumerate index. The 24-bit field holds the
                // `instances[ssbo_idx]` position the shader reads via
                // `rayQueryGetIntersectionInstanceCustomIndexEXT`.
                instance_custom_index_and_mask: vk::Packed24_8::new(ssbo_idx, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    instance_flags as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas_address,
                },
            });
        }

        let instance_count = instances.len() as u32;
        let missing = draw_commands.len() - instances.len();
        if missing > 0 && frame_index == 0 {
            // Log once per second (at 60fps, frame_index 0 fires 30×/s — good enough).
            static LAST_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let prev = LAST_LOG.load(std::sync::atomic::Ordering::Relaxed);
            if now != prev {
                LAST_LOG.store(now, std::sync::atomic::Ordering::Relaxed);
                log::warn!(
                    "TLAS: {} instances from {} draw commands ({} lack BLAS — no RT shadows for those meshes)",
                    instance_count, draw_commands.len(), missing
                );
            }
        }

        // Even with 0 instances, we build a valid (empty) TLAS so the
        // descriptor set binding is always valid for the shader.

        // Create/resize instance buffer if needed for this frame slot.
        let need_new_tlas = self.tlas[frame_index].is_none()
            || self.tlas[frame_index].as_ref().unwrap().max_instances < instance_count;

        if need_new_tlas {
            // Destroy old TLAS for this frame slot. The fence wait in
            // draw_frame guarantees this slot's previous GPU work is
            // complete, so no device_wait_idle is needed.
            if let Some(mut old) = self.tlas[frame_index].take() {
                log::info!(
                    "TLAS[{frame_index}] resize: {} → {} instances",
                    old.max_instances,
                    instance_count,
                );
                self.accel_loader
                    .destroy_acceleration_structure(old.accel, None);
                old.buffer.destroy(device, allocator);
                old.instance_buffer.destroy(device, allocator);
                old.instance_buffer_device.destroy(device, allocator);
            }

            // Pre-size generously to avoid future resizes. 8192 covers
            // interior cells (~200-800) and large exterior cells (~3000-5000).
            // Growth: 2x current requirement, minimum 8192.
            let padded_count = ((instance_count as usize) * 2).max(8192);
            let padded_size = (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>()
                * padded_count) as vk::DeviceSize;

            // Host-visible staging buffer for CPU writes.
            let mut instance_buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                padded_size,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?;

            // Device-local buffer for GPU reads during AS build. On discrete
            // GPUs this avoids PCIe traversal (~10-30x faster). See #289.
            let mut instance_buffer_device = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                padded_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::TRANSFER_DST,
            )?;

            let instance_address = device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(instance_buffer_device.buffer),
            );

            // Query TLAS sizes.
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                        .array_of_pointers(false)
                        .data(vk::DeviceOrHostAddressConstKHR {
                            device_address: instance_address,
                        }),
                });

            // PREFER_FAST_TRACE + ALLOW_UPDATE: REFIT (#247) handles most
            // per-frame TLAS changes, so full rebuilds are rare and the
            // trace-time wins from a higher-quality BVH pay off on every
            // ray query (shadows, reflections, GI, caustics, window
            // portal). Matches the BLAS flag choice. See #307 / audit
            // AUDIT_PERFORMANCE_2026-04-13b P1-09.
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(
                    vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                        | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE,
                )
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[padded_count as u32],
                &mut sizes,
            );
            // Scratch is sized for BUILD which is >= UPDATE per Vulkan
            // spec, so the same buffer serves both modes.

            // DEVICE_LOCAL: GPU-built, GPU-read during ray queries.
            let mut tlas_buffer = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )
            .inspect_err(|_| {
                instance_buffer.destroy(device, allocator);
                instance_buffer_device.destroy(device, allocator);
            })?;

            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(tlas_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);

            let accel = self
                .accel_loader
                .create_acceleration_structure(&accel_info, None)
                .inspect_err(|_| {
                    tlas_buffer.destroy(device, allocator);
                    instance_buffer.destroy(device, allocator);
                    instance_buffer_device.destroy(device, allocator);
                })
                .context("Failed to create TLAS")?;

            // Grow-only per-frame scratch buffer (#424 SIBLING) — reuse
            // the existing allocation when it still fits the new build.
            // DEVICE_LOCAL: GPU-only scratch during TLAS build. Same
            // scratch alignment caveat as BLAS (#260 R-05).
            let needs_new_scratch = scratch_needs_growth(
                self.scratch_buffers[frame_index].as_ref().map(|b| b.size),
                sizes.build_scratch_size,
            );
            if needs_new_scratch {
                if let Some(mut old_scratch) = self.scratch_buffers[frame_index].take() {
                    old_scratch.destroy(device, allocator);
                }
                let scratch_result = GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    sizes.build_scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                );
                match scratch_result {
                    Ok(scratch) => self.scratch_buffers[frame_index] = Some(scratch),
                    Err(e) => {
                        self.accel_loader
                            .destroy_acceleration_structure(accel, None);
                        tlas_buffer.destroy(device, allocator);
                        instance_buffer.destroy(device, allocator);
                        instance_buffer_device.destroy(device, allocator);
                        return Err(e);
                    }
                }
            }

            self.tlas[frame_index] = Some(TlasState {
                accel,
                buffer: tlas_buffer,
                instance_buffer,
                instance_buffer_device,
                max_instances: padded_count as u32,
                last_blas_addresses: Vec::with_capacity(padded_count),
                // A freshly-created TLAS has no source to refit from —
                // the first frame after creation must do a full BUILD.
                needs_full_rebuild: true,
                // Sentinel that no real generation can match — forces
                // the first build_tlas after (re)creation to take the
                // gen-mismatch short-circuit, skipping the zip-compare
                // since `last_blas_addresses` is empty anyway.
                last_blas_map_gen: u64::MAX,
            });
        }

        let tlas = self.tlas[frame_index].as_mut().unwrap();

        // Decide BUILD vs UPDATE (#247). REFIT (UPDATE) is legal only
        // when the per-instance BLAS references are unchanged from the
        // last BUILD. Transforms, custom indices, SBT offsets, masks,
        // and flags can all change and still be refitted; only the
        // `acceleration_structure_reference` field is off-limits.
        //
        // Gate: if `needs_full_rebuild` is set (freshly created or
        // previous frame BUILT), or the BLAS map mutated since the
        // last build (#300 dirty flag), or the current BLAS address
        // sequence differs from the last BUILD, we BUILD. Otherwise
        // UPDATE. The dirty-flag short-circuit lets cell load /
        // unload / eviction frames skip the O(N) per-instance
        // zip-compare entirely — the gen mismatch already proves the
        // address sequence could have shifted.
        let map_gen = self.blas_map_generation;
        // Materialise the current address sequence as `&[u64]` so the
        // pure decision helper can compare it without re-reading the
        // union field — same invariant as the push loop below.
        // SAFETY: AccelerationStructureReferenceKHR is a union; our
        // BLAS entries are always device-built so `device_handle` is
        // the live variant on every push site in this manager.
        let mut current_addresses_scratch: Vec<vk::DeviceAddress> =
            Vec::with_capacity(instances.len());
        for inst in &instances {
            current_addresses_scratch
                .push(unsafe { inst.acceleration_structure_reference.device_handle });
        }
        let (use_update, _did_zip) = decide_use_update(
            tlas.needs_full_rebuild,
            tlas.last_blas_map_gen,
            map_gen,
            &tlas.last_blas_addresses,
            &current_addresses_scratch,
        );

        // Cache the current BLAS sequence so the next frame can compare.
        // Move-from-scratch avoids the second `union -> u64` round-trip
        // since `decide_use_update` already needed the materialised
        // sequence.
        tlas.last_blas_addresses = current_addresses_scratch;
        // After this BUILD/UPDATE completes, the next frame can refit
        // unless something invalidates it (resize, layout change).
        tlas.needs_full_rebuild = false;
        tlas.last_blas_map_gen = map_gen;

        // Mark referenced BLAS entries as used for LRU eviction.
        // Skinned draws ride the per-entity skinned_blas table; rigid
        // draws ride the per-mesh blas_entries table. The override
        // mirror in the build loop above kept the same discriminator.
        for draw_cmd in draw_commands {
            if !draw_cmd.in_tlas {
                continue;
            }
            if draw_cmd.bone_offset != 0 {
                if let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) {
                    entry.last_used_frame = self.frame_counter;
                }
            } else {
                let h = draw_cmd.mesh_handle as usize;
                if let Some(Some(ref mut blas)) = self.blas_entries.get_mut(h) {
                    blas.last_used_frame = self.frame_counter;
                }
            }
        }

        // Write instances to host-visible staging buffer.
        tlas.instance_buffer.write_mapped(device, &instances)?;

        let copy_size = (instances.len()
            * std::mem::size_of::<vk::AccelerationStructureInstanceKHR>())
            as vk::DeviceSize;

        // Skip the host→device staging copy entirely when there are no
        // instances this frame (#317 / audit D1-02). Vulkan's spec
        // rejects `VkBufferCopy.size` and `VkBufferMemoryBarrier.size`
        // of 0 (VUID-VkBufferCopy-size-01988 and
        // VUID-VkBufferMemoryBarrier-size-01188), and the spec leaves
        // `size = 0` driver-defined: NVIDIA treats it as a no-op,
        // AMD / Intel historically have not — this guard keeps the
        // path portable across vendors. Pre-fix we tripped four
        // validation errors per empty-TLAS frame (two barriers + one
        // copy per TLAS slot × two frame-in-flight slots). The empty-
        // instance TLAS BUILD below is still legal —
        // `primitiveCount = 0` produces an empty AS that ray queries
        // return "no hit" against.
        if copy_size > 0 {
            // Barrier 1: make host write visible to the transfer engine.
            let host_to_transfer = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .buffer(tlas.instance_buffer.buffer)
                .offset(0)
                .size(copy_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[host_to_transfer],
                &[],
            );

            // Copy staging → device-local. On discrete GPUs, the AS build
            // reads from VRAM instead of traversing PCIe. See #289.
            let copy_region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: copy_size,
            };
            device.cmd_copy_buffer(
                cmd,
                tlas.instance_buffer.buffer,
                tlas.instance_buffer_device.buffer,
                &[copy_region],
            );

            // Barrier 2: transfer write → AS build read on the device-local buffer.
            let transfer_to_as = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR)
                .buffer(tlas.instance_buffer_device.buffer)
                .offset(0)
                .size(copy_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[],
                &[transfer_to_as],
                &[],
            );
        }

        let instance_address = device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default().buffer(tlas.instance_buffer_device.buffer),
        );

        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                    .array_of_pointers(false)
                    .data(vk::DeviceOrHostAddressConstKHR {
                        device_address: instance_address,
                    }),
            });

        let scratch_address = device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default()
                .buffer(self.scratch_buffers[frame_index].as_ref().unwrap().buffer),
        );

        // Mirror the flags used at creation time so Vulkan's validation
        // layer matches source and dst flags. ALLOW_UPDATE must be set
        // on both BUILD and UPDATE submissions. PREFER_FAST_TRACE here
        // must stay in lockstep with the BUILD path above (#307).
        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(
                vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                    | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE,
            )
            .dst_acceleration_structure(tlas.accel)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_address,
            });

        if use_update {
            // REFIT path: reuse the existing accel as the source,
            // write the updated instance transforms into the same dst.
            build_info = build_info
                .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
                .src_acceleration_structure(tlas.accel);
        } else {
            build_info = build_info.mode(vk::BuildAccelerationStructureModeKHR::BUILD);
        }

        let range =
            vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(instance_count);

        self.accel_loader.cmd_build_acceleration_structures(
            cmd,
            &[build_info],
            &[std::slice::from_ref(&range)],
        );

        // Restore the scratch buffer so its capacity amortizes across
        // frames, then shrink it if a past peak (exterior open cell with
        // 10 k+ draw commands) left us holding 640 KB+ of unused
        // capacity long after the scene returned to a small interior.
        // `instance_count` is the number of entries we actually used
        // this frame; the 512 floor keeps the buffer usefully large
        // for common-case small scenes without reallocating on every
        // cell transition. See #504.
        self.tlas_instances_scratch = instances;
        shrink_scratch_if_oversized(
            &mut self.tlas_instances_scratch,
            instance_count as usize,
            512,
        );

        Ok(())
    }

    /// Get the TLAS acceleration structure handle for a frame slot (for descriptor binding).
    pub fn tlas_handle(&self, frame_index: usize) -> Option<vk::AccelerationStructureKHR> {
        self.tlas[frame_index].as_ref().map(|t| t.accel)
    }

    /// Evict unused BLAS entries when total BLAS memory exceeds the budget.
    ///
    /// Entries unused for more than `min_idle_frames` frames are candidates.
    /// Eviction is LRU — the least recently used entries are destroyed first.
    /// Only entries unused for >= MAX_FRAMES_IN_FLIGHT frames are safe to
    /// evict (guarantees no in-flight TLAS references them).
    pub unsafe fn evict_unused_blas(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if self.total_blas_bytes <= self.blas_budget_bytes {
            return;
        }

        let min_idle = MAX_FRAMES_IN_FLIGHT as u64 + 1;
        let current = self.frame_counter;

        // Collect eviction candidates: (index, last_used_frame, size).
        let mut candidates: Vec<(usize, u64, vk::DeviceSize)> = self
            .blas_entries
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.as_ref().and_then(|blas| {
                    let idle = current.saturating_sub(blas.last_used_frame);
                    if idle >= min_idle {
                        Some((i, blas.last_used_frame, blas.size_bytes))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Sort by oldest first (LRU).
        candidates.sort_unstable_by_key(|&(_, frame, _)| frame);

        let mut evicted = 0usize;
        let mut freed = 0u64;
        for (idx, _, _size) in candidates {
            if self.total_blas_bytes <= self.blas_budget_bytes {
                break;
            }
            if let Some(mut entry) = self.blas_entries[idx].take() {
                self.accel_loader
                    .destroy_acceleration_structure(entry.accel, None);
                entry.buffer.destroy(device, allocator);
                self.total_blas_bytes = self.total_blas_bytes.saturating_sub(entry.size_bytes);
                freed += entry.size_bytes;
                evicted += 1;
            }
        }

        if evicted > 0 {
            log::info!(
                "BLAS eviction: freed {} entries ({:.1} MB), budget: {:.1}/{:.1} MB",
                evicted,
                freed as f64 / (1024.0 * 1024.0),
                self.total_blas_bytes as f64 / (1024.0 * 1024.0),
                self.blas_budget_bytes as f64 / (1024.0 * 1024.0),
            );
            // BLAS map mutated — see #300.
            self.blas_map_generation = self.blas_map_generation.wrapping_add(1);
            // Force full TLAS rebuild next frame since BLAS addresses changed.
            for slot in &mut self.tlas {
                if let Some(ref mut t) = slot {
                    t.needs_full_rebuild = true;
                }
            }
        }
    }

    /// Shrink `blas_scratch_buffer` down to the size required by the
    /// current surviving BLAS set, if the high-water mark has grown
    /// disproportionately vs the current peak (see
    /// [`scratch_should_shrink`] for the threshold).
    ///
    /// Call at cell-unload boundaries — **not** from inside a BLAS
    /// build path. The method assumes no BLAS build is in flight (the
    /// shared scratch buffer is only referenced during one-time build
    /// command buffers that the per-build fence already waits on, but
    /// we also don't recreate it mid-build because that would invalidate
    /// a live device address).
    ///
    /// Per-frame TLAS `scratch_buffers[i]` are NOT touched here: they
    /// can be in flight on the GPU at this point and dropping them
    /// without the pending-destroy pattern would be a use-after-free.
    /// Shrinking TLAS scratch needs a follow-up that mirrors
    /// [`pending_destroy_blas`]. Issue #495 tracks this gap.
    ///
    /// # Safety
    ///
    /// - Caller must guarantee no BLAS build command buffer is
    ///   currently referencing `blas_scratch_buffer`. The two build
    ///   paths use one-time command buffers with synchronous fence
    ///   waits, so any call site that is NOT inside a BLAS build is
    ///   safe by construction.
    /// - The `device` and `allocator` must be the same ones that
    ///   allocated the current scratch buffer.
    pub unsafe fn shrink_blas_scratch_to_fit(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        let current = match self.blas_scratch_buffer.as_ref().map(|b| b.size) {
            Some(c) => c,
            None => return, // nothing to shrink
        };

        let peak: vk::DeviceSize = self
            .blas_entries
            .iter()
            .flatten()
            .map(|e| e.build_scratch_size)
            .max()
            .unwrap_or(0);

        if peak == 0 {
            // No BLAS survives — drop the scratch entirely. Next build
            // will allocate fresh (via `scratch_needs_growth`'s None
            // arm) at whatever the new build's peak is.
            if let Some(mut old) = self.blas_scratch_buffer.take() {
                old.destroy(device, allocator);
                log::debug!(
                    "BLAS scratch dropped: {:.1} MB → 0 (no BLAS survives)",
                    current as f64 / (1024.0 * 1024.0),
                );
            }
            return;
        }

        if !scratch_should_shrink(current, peak) {
            return;
        }

        // Reallocate to the current peak size. A future build that
        // exceeds the new capacity will grow via `scratch_needs_growth`.
        if let Some(mut old) = self.blas_scratch_buffer.take() {
            old.destroy(device, allocator);
        }
        match GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            peak,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ) {
            Ok(new_buf) => {
                log::debug!(
                    "BLAS scratch shrunk: {:.1} MB → {:.1} MB (peak survivor)",
                    current as f64 / (1024.0 * 1024.0),
                    peak as f64 / (1024.0 * 1024.0),
                );
                self.blas_scratch_buffer = Some(new_buf);
            }
            Err(e) => {
                // Allocation failed — leave `blas_scratch_buffer` as
                // `None` and let the next build allocate fresh. This is
                // a degraded but correct state.
                log::warn!(
                    "BLAS scratch shrink realloc failed: {e}; next build will re-allocate"
                );
            }
        }
    }

    /// Current total BLAS memory in bytes.
    pub fn total_blas_bytes(&self) -> vk::DeviceSize {
        self.total_blas_bytes
    }

    /// Current BLAS scratch buffer capacity in bytes. `None` if the
    /// scratch has never been allocated or was just shrunk to zero.
    /// Exposed for observability (#495 / PERF-D2-M1).
    pub fn blas_scratch_bytes(&self) -> Option<vk::DeviceSize> {
        self.blas_scratch_buffer.as_ref().map(|b| b.size)
    }

    /// CPU-side TLAS instance staging Vec — `(len, capacity)`. Element
    /// size is `size_of::<vk::AccelerationStructureInstanceKHR>()` (64
    /// bytes). Surfaced for the `ctx.scratch` console command (R6).
    pub fn tlas_instances_scratch_telemetry(&self) -> (usize, usize) {
        (
            self.tlas_instances_scratch.len(),
            self.tlas_instances_scratch.capacity(),
        )
    }

    /// Destroy all acceleration structures and buffers.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in self.blas_entries.drain(..) {
            if let Some(mut e) = entry {
                self.accel_loader
                    .destroy_acceleration_structure(e.accel, None);
                e.buffer.destroy(device, allocator);
            }
        }
        for slot in &mut self.tlas {
            if let Some(mut tlas) = slot.take() {
                self.accel_loader
                    .destroy_acceleration_structure(tlas.accel, None);
                tlas.buffer.destroy(device, allocator);
                tlas.instance_buffer.destroy(device, allocator);
                tlas.instance_buffer_device.destroy(device, allocator);
            }
        }
        for scratch in &mut self.scratch_buffers {
            if let Some(mut s) = scratch.take() {
                s.destroy(device, allocator);
            }
        }
        if let Some(mut scratch) = self.blas_scratch_buffer.take() {
            scratch.destroy(device, allocator);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for #504: the scratch-shrink helper must reclaim
    /// capacity after a past peak frame while leaving small working
    /// sets alone. Exercised on a plain `Vec<u8>` — the algorithm is
    /// size-agnostic, so `Vec<vk::AccelerationStructureInstanceKHR>`
    /// (the real caller) follows the same math.
    #[test]
    fn shrink_scratch_reclaims_capacity_after_peak() {
        // 10 000-entry peak, then a tiny steady-state restore.
        let mut v: Vec<u8> = Vec::with_capacity(10_000);
        shrink_scratch_if_oversized(&mut v, 50, 512);
        // Target = 2 × max(50, 512) = 1024. Capacity was 10 000 → shrink.
        assert!(
            v.capacity() <= 1024,
            "expected capacity <= 1024, got {}",
            v.capacity()
        );
        // Floor honoured — NOT shrunk to `working_set` alone (50).
        assert!(
            v.capacity() >= 512,
            "floor must keep capacity above working-set for small frames"
        );
    }

    /// Near-steady state: capacity just over the 2× band must not
    /// trigger a shrink (avoids thrashing when the working set
    /// oscillates around the peak).
    #[test]
    fn shrink_scratch_preserves_hysteresis_band() {
        // Working set 500, floor 512, target = 2 × max(500, 512) = 1024.
        // Capacity 1500 > target → shrink.
        let mut over: Vec<u8> = Vec::with_capacity(1500);
        shrink_scratch_if_oversized(&mut over, 500, 512);
        assert!(over.capacity() <= 1024);

        // Capacity 1024 == target → NO shrink (equality falls into
        // the "leave alone" branch).
        let mut at: Vec<u8> = Vec::with_capacity(1024);
        shrink_scratch_if_oversized(&mut at, 500, 512);
        assert_eq!(at.capacity(), 1024, "at-target capacity must not be touched");

        // Capacity below 2× — leave alone, we're already efficient.
        let mut under: Vec<u8> = Vec::with_capacity(800);
        shrink_scratch_if_oversized(&mut under, 500, 512);
        assert_eq!(under.capacity(), 800);
    }

    /// Zero working set must still honour the floor — don't shrink
    /// to zero just because the current frame emitted no draws.
    #[test]
    fn shrink_scratch_zero_working_set_keeps_floor() {
        let mut v: Vec<u8> = Vec::with_capacity(5000);
        shrink_scratch_if_oversized(&mut v, 0, 512);
        assert!(v.capacity() >= 512, "floor must survive zero working set");
        assert!(v.capacity() <= 1024, "shrink must still fire above 2 × floor");
    }

    /// Regression for #510: the mid-batch eviction predicate must
    /// fire at ≥ 90% of the configured budget and stay quiet below.
    /// Uses integer-only arithmetic so the threshold is consistent
    /// between 32- and 64-bit `DeviceSize` builds.
    #[test]
    fn should_evict_mid_batch_fires_at_ninety_percent() {
        let budget: vk::DeviceSize = 1_000_000_000; // 1 GB

        // Exactly 90%: projected == budget * 9 / 10 → fires.
        assert!(should_evict_mid_batch(700_000_000, 200_000_000, budget));

        // Exactly at the boundary: 900 MB projected, 900 MB trigger.
        assert!(should_evict_mid_batch(600_000_000, 300_000_000, budget));

        // One byte under 90%: must NOT fire.
        assert!(!should_evict_mid_batch(500_000_000, 399_999_999, budget));

        // Well under: empty live + small pending.
        assert!(!should_evict_mid_batch(0, 10_000_000, budget));

        // Saturating-add guards against overflow when a bogus caller
        // passes near-u64::MAX for pending. Must not panic.
        let _ = should_evict_mid_batch(u64::MAX / 2, u64::MAX / 2, budget);

        // Zero budget — eviction always fires (degenerate
        // configuration; `compute_blas_budget` floors at 256 MB so
        // this path can't hit in practice, but the predicate must
        // not panic or treat zero budget as "under").
        assert!(should_evict_mid_batch(1, 0, 0));
        assert!(should_evict_mid_batch(0, 0, 0));
    }

    /// Regression: #300 — when `needs_full_rebuild` is set, the
    /// per-instance address zip-compare must be skipped (the call is
    /// going to BUILD regardless, so paying O(N) is pure waste).
    #[test]
    fn decide_skips_zip_when_needs_full_rebuild() {
        // Even with identical address lists, needs_full_rebuild forces
        // BUILD and the zip is not run.
        let cached = vec![1u64, 2, 3];
        let current = vec![1u64, 2, 3];
        let (use_update, did_zip) = decide_use_update(true, 0, 0, &cached, &current);
        assert!(!use_update, "needs_full_rebuild forces BUILD");
        assert!(!did_zip, "comparison must be skipped — short-circuit");
    }

    /// Regression: #300 — when the BLAS map generation has bumped
    /// since the last build (cell load / unload / eviction frame),
    /// the per-instance address compare is also skipped because
    /// addresses might have shifted and we're going to BUILD anyway.
    #[test]
    fn decide_skips_zip_when_blas_map_dirty() {
        let cached = vec![1u64, 2, 3];
        let current = vec![1u64, 2, 3];
        // last_gen=5, current=7 → BLAS map changed since last build.
        let (use_update, did_zip) = decide_use_update(false, 5, 7, &cached, &current);
        assert!(!use_update, "blas_map_dirty forces BUILD");
        assert!(!did_zip, "comparison must be skipped — short-circuit");
    }

    /// Steady state — no rebuild needed, BLAS map unchanged. The zip
    /// runs to detect frustum / draw-list composition changes (which
    /// are invisible to the dirty flag).
    #[test]
    fn decide_runs_zip_when_steady_state_layout_matches() {
        let cached = vec![1u64, 2, 3];
        let current = vec![1u64, 2, 3];
        let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
        assert!(use_update, "matching steady state must use UPDATE");
        assert!(did_zip, "comparison must run to verify per-slot match");
    }

    /// Steady state but composition shifted (frustum culling brought
    /// a different mesh into a slot). The zip catches the mismatch
    /// and forces BUILD.
    #[test]
    fn decide_forces_build_when_layout_diverges() {
        let cached = vec![1u64, 2, 3];
        let current = vec![1u64, 2, 99]; // slot 2 now refers to a different BLAS
        let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
        assert!(!use_update, "diverging slot forces BUILD");
        assert!(did_zip, "comparison must run — that's how we noticed");
    }

    /// Length mismatch (entity spawned/despawned without the BLAS map
    /// noticing — e.g. an entity with an existing-mesh handle joined
    /// the in_tlas set). The zip-compare's length check catches this.
    #[test]
    fn decide_forces_build_when_lengths_differ() {
        let cached = vec![1u64, 2, 3];
        let current = vec![1u64, 2, 3, 4];
        let (use_update, did_zip) = decide_use_update(false, 7, 7, &cached, &current);
        assert!(!use_update);
        assert!(did_zip);
    }

    /// Sentinel from the freshly-created TlasState (`u64::MAX`) must
    /// never accidentally match a real generation. Forces BUILD on
    /// the very first frame after creation regardless of input
    /// identity.
    #[test]
    fn decide_first_frame_after_tlas_creation_builds() {
        let cached: Vec<u64> = Vec::new();
        let current = vec![1u64, 2, 3];
        let (use_update, did_zip) = decide_use_update(true, u64::MAX, 0, &cached, &current);
        assert!(!use_update);
        assert!(!did_zip);
    }

    // ── build_instance_map (#419) ──────────────────────────────────
    //
    // The shared `draw_idx → ssbo_idx` mapping is the single source of
    // truth the TLAS `instance_custom_index` and SSBO position must
    // agree on. Before #419 the TLAS used the raw enumerate index and
    // the SSBO used `gpu_instances.len()` (compacted) — identical only
    // when the filter in `draw.rs` never rejected a draw_cmd. A single
    // mesh eviction shifted every subsequent SSBO entry by one while
    // TLAS custom indices stayed put, silently corrupting material /
    // transform reads on every RT hit downstream.

    #[test]
    fn instance_map_empty_list_produces_empty_map() {
        let map = build_instance_map(0, |_| true);
        assert!(map.is_empty());
    }

    #[test]
    fn instance_map_all_kept_matches_iota() {
        // Happy path: every draw_cmd survives the filter. compacted
        // index equals the enumerate index, which is exactly the pre-fix
        // behaviour — so the mapping must be a no-op in this case.
        let map = build_instance_map(4, |_| true);
        assert_eq!(map, vec![Some(0), Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn instance_map_all_dropped_produces_all_none() {
        let map = build_instance_map(3, |_| false);
        assert_eq!(map, vec![None, None, None]);
    }

    #[test]
    fn instance_map_skips_compact_subsequent_indices() {
        // The failure mode from the audit: draw_cmds = [A, B, C, D, E]
        // where B and D are filtered out. Before #419 the TLAS would
        // encode custom_index = 2 for C but the SSBO compacted to
        // [A, C, E] at positions 0, 1, 2 — so the shader's ray hit on
        // C would read gpu_instances[2] = E. After #419 C's
        // custom_index is the compacted 1, which matches gpu_instances[1].
        let map = build_instance_map(5, |i| i != 1 && i != 3);
        assert_eq!(map, vec![Some(0), None, Some(1), None, Some(2)]);
    }

    #[test]
    fn instance_map_only_first_kept() {
        let map = build_instance_map(4, |i| i == 0);
        assert_eq!(map, vec![Some(0), None, None, None]);
    }

    #[test]
    fn instance_map_next_idx_never_overlaps_a_dropped_slot() {
        // Every `Some(x)` value must be unique and strictly increasing.
        // A regression that decremented or double-assigned `next` would
        // pass the "count matches" check but break SSBO indexing.
        let map = build_instance_map(10, |i| i % 2 == 0);
        let kept: Vec<u32> = map.iter().filter_map(|x| *x).collect();
        assert_eq!(kept, vec![0, 1, 2, 3, 4]);
        assert!(
            kept.windows(2).all(|w| w[0] < w[1]),
            "compacted indices must be strictly increasing"
        );
    }

    /// Regression: #60 + #424 SIBLING. Scratch pool growth policy is a
    /// pure `Option<size> + required -> bool` decision shared by both
    /// BLAS paths and the TLAS full-rebuild path. Must:
    ///   - grow on first use (no buffer yet)
    ///   - grow when the required size exceeds current capacity
    ///   - reuse when the existing buffer meets or exceeds the need
    ///     (including equality — the edge where pre-#424 TLAS code
    ///     would still destroy+recreate)
    #[test]
    fn scratch_pool_growth_policy() {
        // First use — no existing buffer.
        assert!(scratch_needs_growth(None, 1024));

        // Existing buffer too small — grow.
        assert!(scratch_needs_growth(Some(1024), 2048));

        // Existing buffer exactly the required size — REUSE.
        assert!(!scratch_needs_growth(Some(2048), 2048));

        // Existing buffer larger than required — REUSE (high-water mark).
        assert!(!scratch_needs_growth(Some(1 << 20), 1024));

        // Zero required (empty TLAS) — REUSE whatever's there.
        assert!(!scratch_needs_growth(Some(1), 0));
    }

    // ── scratch_should_shrink (#495) ─────────────────────────────────
    //
    // Shrink policy: current > 2× peak AND excess > 16 MB slack. Four
    // boundary cases pinned here so a future rewrite can't relax the
    // thresholds silently.
    const MB: vk::DeviceSize = 1024 * 1024;

    #[test]
    fn scratch_shrink_triggers_when_excess_is_large() {
        // Current = 100 MB, peak = 2 MB. Ratio = 50×, excess = 98 MB.
        // Both thresholds exceeded → shrink.
        assert!(scratch_should_shrink(100 * MB, 2 * MB));
    }

    #[test]
    fn scratch_shrink_skipped_below_2x_ratio() {
        // Current = 40 MB, peak = 30 MB. Ratio = 1.33×. Excess 10 MB.
        // Ratio check fails → don't shrink.
        assert!(!scratch_should_shrink(40 * MB, 30 * MB));
    }

    #[test]
    fn scratch_shrink_skipped_when_excess_under_slack() {
        // Current = 15 MB, peak = 2 MB. Ratio = 7.5×, but excess = 13 MB
        // < 16 MB slack → don't shrink (not worth the realloc churn).
        assert!(!scratch_should_shrink(15 * MB, 2 * MB));
    }

    #[test]
    fn scratch_shrink_triggers_at_zero_peak_with_large_current() {
        // No BLAS survives — peak = 0, current = 80 MB. Ratio check is
        // `current > 0 * 2 = 0` → true; excess = 80 MB > 16 MB → true.
        // Shrink (the caller's method drops the buffer entirely on zero
        // peak).
        assert!(scratch_should_shrink(80 * MB, 0));
    }

    #[test]
    fn scratch_shrink_skipped_at_zero_peak_under_slack() {
        // peak = 0 but current is tiny (8 MB) — excess 8 MB < 16 MB
        // slack → don't churn.
        assert!(!scratch_should_shrink(8 * MB, 0));
    }

    #[test]
    fn scratch_shrink_skipped_on_exactly_2x_ratio() {
        // current = 2× peak exactly — ratio check is strict `>`, so
        // equality does NOT trigger.
        assert!(!scratch_should_shrink(64 * MB, 32 * MB));
    }
}
