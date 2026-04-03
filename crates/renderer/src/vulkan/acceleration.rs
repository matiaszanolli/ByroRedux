//! Acceleration structure management for RT ray queries.
//!
//! Builds BLAS (bottom-level) per unique mesh and a single TLAS (top-level)
//! rebuilt each frame from all draw instances. The TLAS is bound as a
//! descriptor in the fragment shader for shadow ray queries.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
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
}

/// Manages BLAS and TLAS for RT ray queries.
pub struct AccelerationManager {
    accel_loader: ash::khr::acceleration_structure::Device,
    /// One BLAS per mesh in MeshRegistry (indexed by mesh handle).
    blas_entries: Vec<Option<BlasEntry>>,
    pub tlas: Option<TlasState>,
    scratch_buffer: Option<GpuBuffer>,
}

impl AccelerationManager {
    pub fn new(instance: &ash::Instance, device: &ash::Device) -> Self {
        let accel_loader = ash::khr::acceleration_structure::Device::new(instance, device);
        Self {
            accel_loader,
            blas_entries: Vec::new(),
            tlas: None,
            scratch_buffer: None,
        }
    }

    /// Build a BLAS for a mesh. Call after uploading the mesh to GPU.
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

        // Get buffer device addresses.
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

        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                triangles,
            });

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

        // Allocate result buffer.
        let result_buffer = GpuBuffer::create_host_visible(
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

        // Allocate scratch buffer.
        let mut scratch = GpuBuffer::create_host_visible(
            device,
            allocator,
            sizes.build_scratch_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )?;

        let scratch_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(scratch.buffer),
            )
        };

        // Build the BLAS via one-time command buffer.
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

        super::texture::with_one_time_commands(device, queue, command_pool, |cmd| unsafe {
            self.accel_loader.cmd_build_acceleration_structures(
                cmd,
                &[build_info],
                &[std::slice::from_ref(&range_info)],
            );
        })?;

        // Get the BLAS device address.
        let device_address = unsafe {
            self.accel_loader.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                    .acceleration_structure(accel),
            )
        };

        // Free scratch.
        scratch.destroy(device, allocator);

        // Store BLAS entry.
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

    /// Build or rebuild the TLAS from draw commands.
    /// Records commands into `cmd` — caller must ensure a memory barrier after.
    pub unsafe fn build_tlas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        draw_commands: &[DrawCommand],
    ) -> Result<()> {
        // Build instance array.
        let mut instances: Vec<vk::AccelerationStructureInstanceKHR> = Vec::new();
        for draw_cmd in draw_commands {
            let mesh_handle = draw_cmd.mesh_handle as usize;
            let Some(Some(blas)) = self.blas_entries.get(mesh_handle) else {
                continue;
            };

            // Convert column-major model_matrix [f32; 16] to VkTransformMatrixKHR (3x4 row-major).
            let m = &draw_cmd.model_matrix;
            let transform = vk::TransformMatrixKHR {
                matrix: [
                    m[0], m[4], m[8],  m[12],
                    m[1], m[5], m[9],  m[13],
                    m[2], m[6], m[10], m[14],
                ],
            };

            instances.push(vk::AccelerationStructureInstanceKHR {
                transform,
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas.device_address,
                },
            });
        }

        if instances.is_empty() {
            return Ok(());
        }

        let instance_count = instances.len() as u32;

        // Create/resize instance buffer if needed.
        let need_new_tlas = self.tlas.is_none()
            || self.tlas.as_ref().unwrap().max_instances < instance_count;

        if need_new_tlas {
            // Destroy old TLAS.
            if let Some(mut old) = self.tlas.take() {
                device.device_wait_idle().ok();
                self.accel_loader
                    .destroy_acceleration_structure(old.accel, None);
                old.buffer.destroy(device, allocator);
                old.instance_buffer.destroy(device, allocator);
            }

            // Create instance buffer.
            let padded_count = (instance_count as usize).next_power_of_two().max(64);
            let padded_size = (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>()
                * padded_count) as vk::DeviceSize;

            let instance_buffer = GpuBuffer::create_host_visible(
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

            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[padded_count as u32],
                &mut sizes,
            );

            let tlas_buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?;

            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(tlas_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);

            let accel = self
                .accel_loader
                .create_acceleration_structure(&accel_info, None)
                .context("Failed to create TLAS")?;

            // Allocate scratch for TLAS.
            if let Some(mut old_scratch) = self.scratch_buffer.take() {
                old_scratch.destroy(device, allocator);
            }
            self.scratch_buffer = Some(GpuBuffer::create_host_visible(
                device,
                allocator,
                sizes.build_scratch_size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )?);

            self.tlas = Some(TlasState {
                accel,
                buffer: tlas_buffer,
                instance_buffer,
                max_instances: padded_count as u32,
            });
        }

        let tlas = self.tlas.as_mut().unwrap();

        // Write instances to buffer.
        tlas.instance_buffer.write_mapped(&instances)?;

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
                .buffer(self.scratch_buffer.as_ref().unwrap().buffer),
        );

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .dst_acceleration_structure(tlas.accel)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_address,
            });

        let range = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(instance_count);

        self.accel_loader.cmd_build_acceleration_structures(
            cmd,
            &[build_info],
            &[std::slice::from_ref(&range)],
        );

        Ok(())
    }

    /// Get the TLAS acceleration structure handle (for descriptor binding).
    pub fn tlas_handle(&self) -> Option<vk::AccelerationStructureKHR> {
        self.tlas.as_ref().map(|t| t.accel)
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
        if let Some(mut tlas) = self.tlas.take() {
            self.accel_loader
                .destroy_acceleration_structure(tlas.accel, None);
            tlas.buffer.destroy(device, allocator);
            tlas.instance_buffer.destroy(device, allocator);
        }
        if let Some(mut scratch) = self.scratch_buffer.take() {
            scratch.destroy(device, allocator);
        }
    }
}
