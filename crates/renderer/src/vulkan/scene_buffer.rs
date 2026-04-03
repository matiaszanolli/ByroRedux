//! Per-frame scene data buffers for multi-light rendering.
//!
//! Manages an SSBO for the light array and a UBO for camera data,
//! with per-frame-in-flight double-buffering to avoid write-after-read hazards.
//! Bound as descriptor set 1 in the pipeline layout.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

/// Maximum lights we can upload per frame. The SSBO is pre-allocated to this size.
/// 512 lights × 48 bytes = 24 KB per frame — trivial.
const MAX_LIGHTS: usize = 512;

/// GPU-side light struct (48 bytes, std430 layout).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuLight {
    /// xyz = world position, w = radius (Bethesda units).
    pub position_radius: [f32; 4],
    /// rgb = color (0–1), w = type (0=point, 1=spot, 2=directional).
    pub color_type: [f32; 4],
    /// xyz = direction (spot/directional), w = spot outer angle cosine.
    pub direction_angle: [f32; 4],
}

/// GPU-side camera data (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GpuCamera {
    /// xyz = world position, w = unused.
    pub position: [f32; 4],
    /// x = RT enabled (1.0), y/z/w = ambient light color (RGB).
    pub flags: [f32; 4],
}

/// SSBO header: lightCount + padding to 16-byte alignment (std430).
#[repr(C)]
#[derive(Clone, Copy)]
struct LightHeader {
    count: u32,
    _pad: [u32; 3],
}

/// Per-frame scene buffers and their descriptor sets.
pub struct SceneBuffers {
    /// One SSBO per frame-in-flight (header + light array).
    light_buffers: Vec<GpuBuffer>,
    /// One UBO per frame-in-flight (camera data).
    camera_buffers: Vec<GpuBuffer>,
    /// Descriptor pool for scene descriptor sets.
    descriptor_pool: vk::DescriptorPool,
    /// Layout for set 1: binding 0 = SSBO (lights), binding 1 = UBO (camera).
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One descriptor set per frame-in-flight.
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Tracks whether the TLAS binding has been written for each frame.
    pub tlas_written: Vec<bool>,
}

impl SceneBuffers {
    /// Create scene buffers and descriptor infrastructure.
    pub fn new(device: &ash::Device, allocator: &SharedAllocator, rt_enabled: bool) -> Result<Self> {
        // Calculate buffer sizes.
        let light_buf_size = (std::mem::size_of::<LightHeader>()
            + std::mem::size_of::<GpuLight>() * MAX_LIGHTS) as vk::DeviceSize;
        let camera_buf_size = std::mem::size_of::<GpuCamera>() as vk::DeviceSize;

        // Create per-frame buffers.
        let mut light_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            light_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                light_buf_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            camera_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                camera_buf_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
        }

        // Descriptor set layout: set 1.
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        if rt_enabled {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create scene descriptor set layout")?
        };

        // Descriptor pool.
        let mut pool_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        if rt_enabled {
            pool_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            });
        }
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create scene descriptor pool")?
        };

        // Allocate descriptor sets.
        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate scene descriptor sets")?
        };

        // Write descriptor sets to point at the buffers.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let light_buf_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[i].buffer,
                offset: 0,
                range: light_buf_size,
            }];
            let camera_buf_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[i].buffer,
                offset: 0,
                range: camera_buf_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&light_buf_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&camera_buf_info),
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        log::info!(
            "Scene buffers created: {} frames, {} max lights ({} bytes/frame)",
            MAX_FRAMES_IN_FLIGHT,
            MAX_LIGHTS,
            light_buf_size,
        );

        Ok(Self {
            light_buffers,
            camera_buffers,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
        })
    }

    /// Upload light data for the current frame-in-flight.
    pub fn upload_lights(&mut self, frame_index: usize, lights: &[GpuLight]) -> Result<()> {
        let count = lights.len().min(MAX_LIGHTS);
        let header = LightHeader {
            count: count as u32,
            _pad: [0; 3],
        };

        // Build combined byte buffer: header + light array.
        let header_size = std::mem::size_of::<LightHeader>();
        let light_size = std::mem::size_of::<GpuLight>();
        let total_size = header_size + light_size * count;
        let mut data = vec![0u8; total_size];

        // Write header.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &header as *const LightHeader as *const u8,
                data.as_mut_ptr(),
                header_size,
            );
        }
        // Write lights.
        if count > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    lights.as_ptr() as *const u8,
                    data.as_mut_ptr().add(header_size),
                    light_size * count,
                );
            }
        }

        self.light_buffers[frame_index].write_mapped(&data)
    }

    /// Upload camera data for the current frame-in-flight.
    pub fn upload_camera(&mut self, frame_index: usize, camera: &GpuCamera) -> Result<()> {
        self.camera_buffers[frame_index].write_mapped(std::slice::from_ref(camera))
    }

    /// Get the descriptor set for the current frame-in-flight.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.descriptor_sets[frame_index]
    }

    /// Update the TLAS acceleration structure in the descriptor set for a given frame.
    pub fn write_tlas(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        self.tlas_written[frame_index] = true;
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);

        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Destroy all resources.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}
