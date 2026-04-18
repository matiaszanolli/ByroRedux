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
    /// Per-frame-in-flight TLAS scratch buffers (reused across rebuilds).
    scratch_buffers: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT],
    /// Persisted BLAS scratch buffer (reused across builds, grows to high-water mark).
    /// BLAS builds use one-time command buffers with a fence wait, so a single
    /// shared buffer is safe (no overlapping BLAS builds).
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
fn compute_blas_budget(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::DeviceSize {
    const MIN_BUDGET: vk::DeviceSize = 256 * 1024 * 1024; // 256 MB floor

    // SAFETY: get_physical_device_memory_properties is a simple query with
    // no preconditions beyond a valid physical device handle, which the
    // caller already holds through the VulkanContext construction chain.
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let device_local_bytes: vk::DeviceSize = mem_props.memory_heaps
        [..mem_props.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .sum();

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
            // one is too small for this build.
            let need_new_scratch = self
                .blas_scratch_buffer
                .as_ref()
                .map_or(true, |b| b.size < sizes.build_scratch_size);

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
        });
        // BLAS map mutated — see #300.
        self.blas_map_generation = self.blas_map_generation.wrapping_add(1);

        Ok(())
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
            geometry: vk::AccelerationStructureGeometryKHR<'static>,
            primitive_count: u32,
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

        // Now build geometries referencing the stored triangles data.
        for (idx, &(mesh_handle, _mesh, _vertex_count, index_count)) in meshes.iter().enumerate() {
            let primitive_count = index_count / 3;

            // SAFETY: We transmute the lifetime to 'static because the triangles_data
            // vec lives for the duration of this function. The geometry struct just
            // holds a copy of the union data, not a reference.
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
            });
        }

        // Phase 2: Ensure scratch buffer is large enough.
        let need_new_scratch = self
            .blas_scratch_buffer
            .as_ref()
            .map_or(true, |b| b.size < max_scratch_size);

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
                    let barrier = vk::MemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                        .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR);
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
            Vec<(u32, vk::AccelerationStructureKHR, GpuBuffer)>,
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

            let mut compact_accels: Vec<(u32, vk::AccelerationStructureKHR, GpuBuffer)> =
                Vec::with_capacity(prepared.len());

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

                compact_accels.push((p.mesh_handle, compact_accel, compact_buffer));
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
            for (i, (_, compact_accel, _)) in compact_accels.iter().enumerate() {
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
            for (_, accel, mut buf) in compact_accels {
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
        for (mesh_handle, accel, buffer) in compact_accels {
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
    /// Records commands into `cmd` — caller must ensure a memory barrier after.
    pub unsafe fn build_tlas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        draw_commands: &[DrawCommand],
        frame_index: usize,
    ) -> Result<()> {
        // Advance the frame counter for LRU tracking.
        self.frame_counter += 1;

        // Build instance array. The enumeration index `i` matches the SSBO
        // instance index in draw_frame (both iterate draw_commands in order and
        // the mesh_registry.get() guard in draw.rs always succeeds for submitted
        // draw commands). We encode `i` as instance_custom_index so the shader
        // can map a TLAS hit back to the correct SSBO entry — the TLAS may be
        // sparse (some draw commands lack a BLAS), so InstanceId != SSBO index.
        let mut instances = std::mem::take(&mut self.tlas_instances_scratch);
        instances.clear();
        instances.reserve(draw_commands.len());
        for (i, draw_cmd) in draw_commands.iter().enumerate() {
            // Skip instances not flagged for TLAS inclusion (frustum-culled).
            // The enumeration index `i` is preserved as instance_custom_index
            // so it always matches the SSBO layout regardless of filtering.
            if !draw_cmd.in_tlas {
                continue;
            }
            let mesh_handle = draw_cmd.mesh_handle as usize;
            let Some(Some(blas)) = self.blas_entries.get(mesh_handle) else {
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
            instances.push(vk::AccelerationStructureInstanceKHR {
                transform,
                instance_custom_index_and_mask: vk::Packed24_8::new(i as u32, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas.device_address,
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

            // Allocate scratch for this frame slot.
            if let Some(mut old_scratch) = self.scratch_buffers[frame_index].take() {
                old_scratch.destroy(device, allocator);
            }
            // DEVICE_LOCAL: GPU-only scratch space during TLAS build.
            // NOTE: same scratch alignment caveat as BLAS — see #260 (R-05).
            let scratch_result = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.build_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
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
        for draw_cmd in draw_commands {
            if !draw_cmd.in_tlas {
                continue;
            }
            let h = draw_cmd.mesh_handle as usize;
            if let Some(Some(ref mut blas)) = self.blas_entries.get_mut(h) {
                blas.last_used_frame = self.frame_counter;
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

        // Restore the scratch buffer so its capacity amortizes across frames.
        self.tlas_instances_scratch = instances;

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

    /// Current total BLAS memory in bytes.
    pub fn total_blas_bytes(&self) -> vk::DeviceSize {
        self.total_blas_bytes
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
}
