//! Compute pipeline infrastructure for light culling and post-processing.
//!
//! The cluster culling pipeline runs before the render pass each frame,
//! building per-cluster light lists that the fragment shader reads instead
//! of looping over all lights.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

/// Cluster grid dimensions — must match cluster_cull.comp constants.
pub const TILES_X: u32 = 16;
pub const TILES_Y: u32 = 9;
pub const SLICES_Z: u32 = 24;
pub const TOTAL_CLUSTERS: u32 = TILES_X * TILES_Y * SLICES_Z; // 3456
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 32;

/// Per-cluster entry: offset into the flat light index list + count.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ClusterEntry {
    offset: u32,
    count: u32,
}

/// Manages the cluster culling compute pipeline and its buffers.
pub struct ClusterCullPipeline {
    /// Compute pipeline.
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    /// Descriptor set layout for the compute shader:
    /// binding 0 = lights SSBO (read), binding 1 = camera UBO (read),
    /// binding 2 = cluster grid SSBO (write), binding 3 = light indices SSBO (write).
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// One descriptor set per frame-in-flight.
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame cluster grid SSBOs (offset + count per cluster).
    cluster_grid_buffers: Vec<GpuBuffer>,
    /// Per-frame light index list SSBOs.
    light_index_buffers: Vec<GpuBuffer>,
    /// Descriptor set layout for fragment shader access to cluster data.
    /// Bindings 5+6 in the scene descriptor set.
    pub scene_cluster_grid_buffers: Vec<vk::Buffer>,
    pub scene_light_index_buffers: Vec<vk::Buffer>,
}

impl ClusterCullPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        light_buffers: &[GpuBuffer],
        camera_buffers: &[GpuBuffer],
        light_buf_size: vk::DeviceSize,
        camera_buf_size: vk::DeviceSize,
    ) -> Result<Self> {
        // Buffer sizes.
        let grid_size =
            (std::mem::size_of::<ClusterEntry>() * TOTAL_CLUSTERS as usize) as vk::DeviceSize;
        let index_list_size = (std::mem::size_of::<u32>()
            * TOTAL_CLUSTERS as usize
            * MAX_LIGHTS_PER_CLUSTER as usize) as vk::DeviceSize;

        // Create per-frame buffers.
        let mut cluster_grid_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut light_index_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            cluster_grid_buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                grid_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            light_index_buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                index_list_size,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }

        // Compute descriptor set layout.
        let bindings = [
            // binding 0: lights SSBO (read)
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // binding 1: camera UBO (read)
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // binding 2: cluster grid SSBO (write)
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // binding 3: light indices SSBO (write)
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .context("Failed to create cluster cull descriptor set layout")?
        };

        // No push constants — screen dimensions are in the camera UBO.
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .context("Failed to create cluster cull pipeline layout")?
        };

        // Load compute shader.
        let comp_spv = include_bytes!("../../shaders/cluster_cull.comp.spv");
        let shader_module = super::pipeline::load_shader_module(device, comp_spv)?;
        let entry_point = c"main";

        let stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(entry_point);

        let compute_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(pipeline_layout);

        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[compute_info], None)
                .map_err(|(_, e)| e)
                .context("Failed to create cluster cull compute pipeline")?[0]
        };

        // Shader module no longer needed after pipeline creation.
        unsafe {
            device.destroy_shader_module(shader_module, None);
        }

        // Descriptor pool + sets.
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                // 3 SSBOs per frame × 2 frames (lights, grid, indices).
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 3) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .context("Failed to create cluster cull descriptor pool")?
        };

        let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .context("Failed to allocate cluster cull descriptor sets")?
        };

        // Write descriptor sets.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let light_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[i].buffer,
                offset: 0,
                range: light_buf_size,
            }];
            let camera_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[i].buffer,
                offset: 0,
                range: camera_buf_size,
            }];
            let grid_info = [vk::DescriptorBufferInfo {
                buffer: cluster_grid_buffers[i].buffer,
                offset: 0,
                range: grid_size,
            }];
            let index_info = [vk::DescriptorBufferInfo {
                buffer: light_index_buffers[i].buffer,
                offset: 0,
                range: index_list_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&light_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&camera_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&grid_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&index_info),
            ];
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        let scene_cluster_grid_buffers = cluster_grid_buffers
            .iter()
            .map(|b| b.buffer)
            .collect();
        let scene_light_index_buffers = light_index_buffers
            .iter()
            .map(|b| b.buffer)
            .collect();

        log::info!(
            "Cluster cull pipeline created: {}×{}×{} = {} clusters, {} max lights/cluster",
            TILES_X,
            TILES_Y,
            SLICES_Z,
            TOTAL_CLUSTERS,
            MAX_LIGHTS_PER_CLUSTER,
        );

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
            cluster_grid_buffers,
            light_index_buffers,
            scene_cluster_grid_buffers,
            scene_light_index_buffers,
        })
    }

    /// Record the cluster culling dispatch into a command buffer.
    ///
    /// Must be called AFTER light + camera uploads and BEFORE the render pass.
    /// The caller must insert a COMPUTE→FRAGMENT barrier after this returns.
    pub unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );

        device.cmd_dispatch(cmd, TILES_X, TILES_Y, SLICES_Z);
    }

    /// Get the cluster grid buffer handle for a frame (for scene descriptor set writes).
    pub fn grid_buffer(&self, frame: usize) -> vk::Buffer {
        self.scene_cluster_grid_buffers[frame]
    }

    /// Get the light index buffer handle for a frame.
    pub fn index_buffer(&self, frame: usize) -> vk::Buffer {
        self.scene_light_index_buffers[frame]
    }

    /// Grid buffer size in bytes.
    pub fn grid_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<ClusterEntry>() * TOTAL_CLUSTERS as usize) as vk::DeviceSize
    }

    /// Light index buffer size in bytes.
    pub fn index_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<u32>() * TOTAL_CLUSTERS as usize * MAX_LIGHTS_PER_CLUSTER as usize)
            as vk::DeviceSize
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.cluster_grid_buffers {
            buf.destroy(device, allocator);
        }
        for buf in &mut self.light_index_buffers {
            buf.destroy(device, allocator);
        }
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}
