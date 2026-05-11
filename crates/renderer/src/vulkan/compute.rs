//! Compute pipeline infrastructure for light culling and post-processing.
//!
//! The cluster culling pipeline runs before the render pass each frame,
//! building per-cluster light lists that the fragment shader reads instead
//! of looping over all lights.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

const CLUSTER_CULL_COMP_SPV: &[u8] = include_bytes!("../../shaders/cluster_cull.comp.spv");

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
        pipeline_cache: vk::PipelineCache,
        light_buffers: &[GpuBuffer],
        camera_buffers: &[GpuBuffer],
        light_buf_size: vk::DeviceSize,
        camera_buf_size: vk::DeviceSize,
    ) -> Result<Self> {
        let grid_size =
            (std::mem::size_of::<ClusterEntry>() * TOTAL_CLUSTERS as usize) as vk::DeviceSize;
        let index_list_size = (std::mem::size_of::<u32>()
            * TOTAL_CLUSTERS as usize
            * MAX_LIGHTS_PER_CLUSTER as usize) as vk::DeviceSize;

        // Build a partially-initialized struct so we can use destroy() for
        // cleanup on error. Null handles are safe — vkDestroy* is a no-op
        // on VK_NULL_HANDLE, and GpuBuffer::destroy skips null buffers.
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            cluster_grid_buffers: Vec::new(),
            light_index_buffers: Vec::new(),
            scene_cluster_grid_buffers: Vec::new(),
            scene_light_index_buffers: Vec::new(),
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // Create per-frame buffers.
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            partial.cluster_grid_buffers.push(try_or_cleanup!(
                GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    grid_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )
            ));
            partial.light_index_buffers.push(try_or_cleanup!(
                GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    index_list_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )
            ));
        }

        // Compute descriptor set layout.
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "cluster_cull.comp",
                spirv: CLUSTER_CULL_COMP_SPV,
            }],
            "cluster_cull",
            &[],
        )
        .expect("cluster_cull layout drifted against cluster_cull.comp (see #427)");
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("Failed to create cluster cull descriptor set layout")
        });

        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("Failed to create cluster cull pipeline layout")
        });

        // Load compute shader.
        let shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            CLUSTER_CULL_COMP_SPV
        ));

        partial.pipeline = match unsafe {
            device
                .create_compute_pipelines(
                    pipeline_cache,
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(
                            vk::PipelineShaderStageCreateInfo::default()
                                .stage(vk::ShaderStageFlags::COMPUTE)
                                .module(shader_module)
                                .name(c"main"),
                        )
                        .layout(partial.pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("Failed to create cluster cull compute pipeline")
        } {
            Ok(pipelines) => {
                unsafe { device.destroy_shader_module(shader_module, None) };
                pipelines[0]
            }
            Err(e) => {
                unsafe { device.destroy_shader_module(shader_module, None) };
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // Descriptor pool + sets.
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 3) as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        partial.descriptor_pool = try_or_cleanup!(unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&pool_sizes)
                        .max_sets(MAX_FRAMES_IN_FLIGHT as u32),
                    None,
                )
                .context("Failed to create cluster cull descriptor pool")
        });

        let layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("Failed to allocate cluster cull descriptor sets")
        });

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
                buffer: partial.cluster_grid_buffers[i].buffer,
                offset: 0,
                range: grid_size,
            }];
            let index_info = [vk::DescriptorBufferInfo {
                buffer: partial.light_index_buffers[i].buffer,
                offset: 0,
                range: index_list_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&light_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&camera_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&grid_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&index_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        partial.scene_cluster_grid_buffers = partial
            .cluster_grid_buffers
            .iter()
            .map(|b| b.buffer)
            .collect();
        partial.scene_light_index_buffers = partial
            .light_index_buffers
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

        Ok(partial)
    }

    /// Record the cluster culling dispatch into a command buffer.
    ///
    /// Must be called AFTER light + camera uploads and BEFORE the render pass.
    /// The caller must insert a COMPUTE→FRAGMENT barrier after this returns.
    ///
    /// One workgroup per cluster — the shader's `local_size_x = 32`
    /// fans the per-cluster light scan out across one warp / wavefront
    /// (#652). Total threads = `TILES_X × TILES_Y × SLICES_Z × 32 =
    /// 3456 × 32 = 110_592` per dispatch.
    pub unsafe fn dispatch(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
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
        // #927 — drop the GpuBuffer structs after `destroy()` has
        // freed their GPU allocations + released their per-buffer
        // allocator Arc clones. Matches the `param_buffers.clear()`
        // pattern used by every other compute / graphics pipeline
        // (Bloom, SSAO, Volumetrics, TAA, Caustic, SVGF, Composite).
        // Without this the GpuBuffer structs lingered in the Vec
        // until ClusterCullPipeline naturally dropped, which on
        // shutdown happens after `VulkanContext::Drop` — the same
        // class of late-Drop bug fixed at the per-buffer level on
        // the Option<SharedAllocator> field.
        self.cluster_grid_buffers.clear();
        for buf in &mut self.light_index_buffers {
            buf.destroy(device, allocator);
        }
        self.light_index_buffers.clear();
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}
