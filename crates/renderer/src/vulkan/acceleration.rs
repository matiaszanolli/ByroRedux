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

/// A bottom-level acceleration structure for one mesh.
pub struct BlasEntry {
    pub accel: vk::AccelerationStructureKHR,
    pub buffer: GpuBuffer,
    pub device_address: vk::DeviceAddress,
}

/// Top-level acceleration structure state.
pub struct TlasState {
    pub accel: vk::AccelerationStructureKHR,
    pub buffer: GpuBuffer,
    pub instance_buffer: GpuBuffer,
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
}

impl AccelerationManager {
    pub fn new(instance: &ash::Instance, device: &ash::Device) -> Self {
        let accel_loader = ash::khr::acceleration_structure::Device::new(instance, device);
        Self {
            accel_loader,
            blas_entries: Vec::new(),
            tlas: [None, None],
            scratch_buffers: [None, None],
            blas_scratch_buffer: None,
        }
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

            super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
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
        while self.blas_entries.len() <= handle {
            self.blas_entries.push(None);
        }
        self.blas_entries[handle] = Some(BlasEntry {
            accel,
            buffer: result_buffer,
            device_address,
        });

        Ok(())
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
        // Build instance array. The enumeration index `i` matches the SSBO
        // instance index in draw_frame (both iterate draw_commands in order and
        // the mesh_registry.get() guard in draw.rs always succeeds for submitted
        // draw commands). We encode `i` as instance_custom_index so the shader
        // can map a TLAS hit back to the correct SSBO entry — the TLAS may be
        // sparse (some draw commands lack a BLAS), so InstanceId != SSBO index.
        let mut instances: Vec<vk::AccelerationStructureInstanceKHR> = Vec::new();
        for (i, draw_cmd) in draw_commands.iter().enumerate() {
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
            || self.tlas[frame_index]
                .as_ref()
                .unwrap()
                .max_instances
                < instance_count;

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
            }

            // Pre-size generously to avoid future resizes. 4096 covers
            // most interior cells (~200-800) and exterior cells (~1000-3000).
            // Growth: 2x current requirement, minimum 4096.
            let padded_count = ((instance_count as usize) * 2).max(4096);
            let padded_size = (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>()
                * padded_count) as vk::DeviceSize;

            let mut instance_buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                padded_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?;

            let instance_address = device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(instance_buffer.buffer),
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

            // ALLOW_UPDATE enables REFIT on subsequent frames (#247).
            // PREFER_FAST_BUILD is retained — we rebuild often enough
            // that trace-time wins from PREFER_FAST_TRACE don't pay back
            // the slower build; REFIT is the actual optimization for
            // static content.
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(
                    vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD
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
            .inspect_err(|_| instance_buffer.destroy(device, allocator))?;

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
                    return Err(e);
                }
            }

            self.tlas[frame_index] = Some(TlasState {
                accel,
                buffer: tlas_buffer,
                instance_buffer,
                max_instances: padded_count as u32,
                last_blas_addresses: Vec::with_capacity(padded_count),
                // A freshly-created TLAS has no source to refit from —
                // the first frame after creation must do a full BUILD.
                needs_full_rebuild: true,
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
        // previous frame BUILT), or the current BLAS address sequence
        // differs from the last BUILD, we BUILD. Otherwise UPDATE.
        let blas_layout_matches = tlas.last_blas_addresses.len() == instances.len()
            && tlas
                .last_blas_addresses
                .iter()
                .zip(instances.iter())
                .all(|(prev, inst)| unsafe {
                    // SAFETY: AccelerationStructureReferenceKHR is a
                    // union. Our BLAS entries are always device-built,
                    // so `device_handle` is the live variant — same
                    // invariant as the push site above.
                    *prev == inst.acceleration_structure_reference.device_handle
                });
        let use_update = !tlas.needs_full_rebuild && blas_layout_matches;

        // Cache the current BLAS sequence so the next frame can compare.
        // Do this regardless of mode so a BUILD-after-UPDATE transition
        // sees the up-to-date baseline.
        tlas.last_blas_addresses.clear();
        for inst in &instances {
            tlas.last_blas_addresses
                .push(inst.acceleration_structure_reference.device_handle);
        }
        // After this BUILD/UPDATE completes, the next frame can refit
        // unless something invalidates it (resize, layout change).
        tlas.needs_full_rebuild = false;

        // Write instances to buffer (host write).
        tlas.instance_buffer.write_mapped(device, &instances)?;

        // Barrier: ensure host write to instance buffer is visible to the
        // AS build command that reads it. Required by Vulkan spec —
        // host writes are not automatically visible to device commands.
        let barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::HOST_WRITE)
            .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR)
            .buffer(tlas.instance_buffer.buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE);

        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::DependencyFlags::empty(),
            &[],
            &[barrier],
            &[],
        );

        let instance_address = device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default().buffer(tlas.instance_buffer.buffer),
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
        // on both BUILD and UPDATE submissions.
        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(
                vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD
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

        Ok(())
    }

    /// Get the TLAS acceleration structure handle for a frame slot (for descriptor binding).
    pub fn tlas_handle(&self, frame_index: usize) -> Option<vk::AccelerationStructureKHR> {
        self.tlas[frame_index].as_ref().map(|t| t.accel)
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
