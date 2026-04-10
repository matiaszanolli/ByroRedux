//! Top-level Vulkan context that owns the entire graphics state.

use super::acceleration::AccelerationManager;
use super::compute::ClusterCullPipeline;
use super::ssao::SsaoPipeline;
use super::allocator::{self, SharedAllocator};
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::instance;
use super::pipeline;
use super::scene_buffer;
use super::surface;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use super::texture::Texture;
use crate::mesh::MeshRegistry;
use crate::texture_registry::TextureRegistry;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::sync::Mutex;

/// A single draw command: which mesh to draw, with what texture, and what model matrix.
pub struct DrawCommand {
    pub mesh_handle: u32,
    pub texture_handle: u32,
    pub model_matrix: [f32; 16],
    pub alpha_blend: bool,
    pub two_sided: bool,
    /// Decal geometry — renders on top of coplanar surfaces via depth bias.
    pub is_decal: bool,
    /// Base offset into the bone-palette SSBO for this draw, or 0 for rigid.
    pub bone_offset: u32,
    /// Bindless texture index for the normal map (0 = no normal map).
    pub normal_map_index: u32,
    /// PBR roughness [0.05..0.95].
    pub roughness: f32,
    /// PBR metalness [0..1].
    pub metalness: f32,
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Emissive color (RGB).
    pub emissive_color: [f32; 3],
    /// Specular intensity multiplier.
    pub specular_strength: f32,
    /// Specular color (RGB).
    pub specular_color: [f32; 3],
}

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,
    /// Monotonic frame counter for temporal effects (jitter seed, accumulation).
    pub frame_counter: u32,

    frame_sync: FrameSync,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: vk::CommandPool,
    /// Dedicated pool for one-time upload/transfer commands, separate from
    /// the per-frame draw pool. Vulkan requires external synchronization on
    /// VkCommandPool (VUID-vkAllocateCommandBuffers-commandPool-00044);
    /// keeping upload commands on a separate pool avoids contention with
    /// draw command buffer reset/recording.
    pub transfer_pool: vk::CommandPool,
    framebuffers: Vec<vk::Framebuffer>,
    depth_image_view: vk::ImageView,
    depth_image: vk::Image,
    depth_allocation: Option<vk_alloc::Allocation>,
    pub mesh_registry: MeshRegistry,
    pub texture_registry: TextureRegistry,
    pub scene_buffers: scene_buffer::SceneBuffers,
    pub accel_manager: Option<AccelerationManager>,
    pub cluster_cull: Option<ClusterCullPipeline>,
    pub ssao: Option<SsaoPipeline>,
    pipeline_cache: vk::PipelineCache,
    pipeline: vk::Pipeline,
    pipeline_alpha: vk::Pipeline,
    pipeline_two_sided: vk::Pipeline,
    pipeline_alpha_two_sided: vk::Pipeline,
    pipeline_ui: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    ui_vert_module: vk::ShaderModule,
    ui_frag_module: vk::ShaderModule,
    /// Mesh handle for the fullscreen quad used by UI overlay.
    pub ui_quad_handle: Option<u32>,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    pub allocator: Option<SharedAllocator>,

    /// Graphics queue, wrapped in a Mutex for Vulkan-required external
    /// synchronization (VUID-vkQueueSubmit-queue-00893). All queue
    /// submissions (draw_frame, texture/buffer uploads) must lock this.
    pub graphics_queue: Mutex<vk::Queue>,
    /// Present queue, also Mutex-wrapped for Vulkan external synchronization
    /// (VUID-vkQueuePresentKHR-pPresentInfo-06329). May alias the graphics
    /// queue when both use the same queue family (common on desktop GPUs).
    pub present_queue: Mutex<vk::Queue>,
    pub queue_indices: QueueFamilyIndices,
    pub device: ash::Device,
    pub device_caps: device::DeviceCapabilities,
    pub physical_device: vk::PhysicalDevice,
    depth_format: vk::Format,

    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,

    debug_messenger: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,

    pub instance: ash::Instance,
    pub entry: ash::Entry,
}

impl VulkanContext {
    /// Full Vulkan initialization chain:
    /// 1. Load Vulkan entry points
    /// 2. Create instance + validation layers
    /// 3. Set up debug messenger
    /// 4. Create surface
    /// 5. Pick physical device
    /// 6. Create logical device + queues
    /// 7. Create swapchain
    /// 8. Create render pass
    /// 9. Create framebuffers
    /// 10. Create command pool + command buffers
    /// 11. Create synchronization objects
    pub fn new(
        display_handle: RawDisplayHandle,
        window_handle: RawWindowHandle,
        window_size: [u32; 2],
    ) -> Result<Self> {
        // 1. Entry
        // SAFETY: Loads the Vulkan shared library (libvulkan.so / vulkan-1.dll).
        // Must be called before any other Vulkan function. The Entry must
        // outlive all objects created through it (guaranteed by struct field order).
        let entry = unsafe { ash::Entry::load().context("Failed to load Vulkan loader")? };
        log::info!("Vulkan loader ready");

        // 2. Instance
        let vk_instance = instance::create_instance(&entry, display_handle)?;

        // 3. Debug messenger
        let debug_messenger = if cfg!(debug_assertions) {
            Some(debug::create_debug_messenger(&vk_instance, &entry)?)
        } else {
            None
        };

        // 4. Surface
        let surface_loader = ash::khr::surface::Instance::new(&entry, &vk_instance);
        let vk_surface =
            surface::create_surface(&entry, &vk_instance, display_handle, window_handle)?;

        // 5. Physical device + capability probe
        let (physical_device, queue_indices, device_caps) =
            device::pick_physical_device(&vk_instance, &surface_loader, vk_surface)?;

        // 6. Query supported depth format
        let depth_format = find_depth_format(&vk_instance, physical_device)?;

        // 7. Logical device + queues (enables RT extensions when available)
        let (device, raw_graphics_queue, raw_present_queue) = device::create_logical_device(
            &vk_instance,
            physical_device,
            queue_indices,
            &device_caps,
        )?;
        let graphics_queue = Mutex::new(raw_graphics_queue);
        let present_queue = Mutex::new(raw_present_queue);

        // 7. GPU allocator (buffer_device_address required for RT acceleration structures)
        let gpu_allocator = allocator::create_allocator(
            &vk_instance,
            &device,
            physical_device,
            device_caps.ray_query_supported,
        )?;

        // 8. Swapchain
        let swapchain_state = swapchain::create_swapchain(
            &vk_instance,
            &device,
            physical_device,
            &surface_loader,
            vk_surface,
            queue_indices,
            window_size,
            vk::SwapchainKHR::null(), // no old swapchain on initial creation
        )?;

        // 9. Depth resources
        let (depth_image, depth_image_view, depth_allocation) = create_depth_resources(
            &device,
            &gpu_allocator,
            swapchain_state.extent,
            depth_format,
        )?;

        // 10. Render pass (color + depth)
        let render_pass = create_render_pass(&device, swapchain_state.format.format, depth_format)?;

        // 10. Command pools: one for per-frame draw commands (RESET_COMMAND_BUFFER),
        //     one for one-time upload/transfer commands (separate pool to avoid
        //     contention — Vulkan requires external sync on VkCommandPool).
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;
        let transfer_pool = create_transfer_pool(&device, queue_indices.graphics)?;

        // 11. Texture registry with checkerboard fallback
        let mut texture_registry = TextureRegistry::new(
            &device,
            swapchain_state.images.len() as u32,
            1024,
            device_caps.max_sampler_anisotropy,
        )?;
        let checkerboard = super::texture::generate_checkerboard(256, 256, 32);
        let fallback_texture = Texture::from_rgba(
            &device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
            256,
            256,
            &checkerboard,
            texture_registry.shared_sampler,
        )?;
        texture_registry.set_fallback(&device, fallback_texture)?;

        // 12. Scene buffers (light SSBO + camera UBO + optional TLAS, descriptor set 1)
        let scene_buffers = scene_buffer::SceneBuffers::new(
            &device,
            &gpu_allocator,
            device_caps.ray_query_supported,
        )?;

        // 12b. Acceleration manager (RT only) — build empty TLAS so descriptors are valid
        let mut scene_buffers = scene_buffers;
        let accel_manager = if device_caps.ray_query_supported {
            let mut accel = AccelerationManager::new(&vk_instance, &device);
            // Build an empty TLAS per frame-in-flight slot via one-time command
            // buffers so all descriptor sets have a valid acceleration structure
            // from frame 0. Each build blocks until complete (fence wait inside
            // with_one_time_commands), so no overlap between builds.
            let empty_draws: Vec<DrawCommand> = Vec::new();
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                super::texture::with_one_time_commands(
                    &device,
                    &graphics_queue,
                    transfer_pool,
                    |cmd| unsafe {
                        accel
                            .build_tlas(&device, &gpu_allocator, cmd, &empty_draws, f)
                            .context("initial empty TLAS build")
                    },
                )?;
                if let Some(tlas_handle) = accel.tlas_handle(f) {
                    scene_buffers.write_tlas(&device, f, tlas_handle);
                }
            }
            Some(accel)
        } else {
            None
        };

        // 12c. Cluster cull compute pipeline (light culling)
        let cluster_cull = match ClusterCullPipeline::new(
            &device,
            &gpu_allocator,
            scene_buffers.light_buffers(),
            scene_buffers.camera_buffers(),
            scene_buffers.light_buffer_size(),
            scene_buffers.camera_buffer_size(),
        ) {
            Ok(cc) => {
                // Write cluster buffer references into scene descriptor sets.
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_cluster_buffers(
                        &device,
                        f,
                        cc.grid_buffer(f),
                        cc.grid_buffer_size(),
                        cc.index_buffer(f),
                        cc.index_buffer_size(),
                    );
                }
                Some(cc)
            }
            Err(e) => {
                log::warn!("Cluster cull pipeline creation failed: {e} — falling back to all-lights loop");
                None
            }
        };

        // 13. Pipeline cache (load from disk if available)
        let pipeline_cache = load_or_create_pipeline_cache(&device)?;

        // 14. Graphics pipeline (with depth test + descriptor set layouts for set 0 + set 1)
        let pipelines = pipeline::create_triangle_pipeline(
            &device,
            render_pass,
            swapchain_state.extent,
            texture_registry.descriptor_set_layout,
            scene_buffers.descriptor_set_layout,
            pipeline_cache,
        )?;

        // 15. UI overlay pipeline (no depth, alpha blend, passthrough shaders)
        let (pipeline_ui, ui_vert_module, ui_frag_module) = pipeline::create_ui_pipeline(
            &device,
            render_pass,
            swapchain_state.extent,
            pipelines.layout,
            pipeline_cache,
        )?;

        // 14a. SSAO pipeline (reads depth buffer after render pass)
        let ssao = match SsaoPipeline::new(
            &device,
            &gpu_allocator,
            depth_image_view,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(s) => {
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_ao_texture(
                        &device, f, s.ao_image_view, s.ao_sampler,
                    );
                }
                Some(s)
            }
            Err(e) => {
                log::warn!("SSAO pipeline creation failed: {e} — no ambient occlusion");
                None
            }
        };

        // 14. Mesh registry (empty — meshes uploaded by the application)
        let mesh_registry = MeshRegistry::new();

        // 15. Framebuffers (color + depth attachments)
        let framebuffers =
            create_framebuffers(&device, render_pass, &swapchain_state, depth_image_view)?;

        // 16. Command buffers
        let command_buffers =
            allocate_command_buffers(&device, command_pool, swapchain_state.images.len())?;

        // 17. Sync objects
        let frame_sync = sync::create_sync_objects(&device, swapchain_state.images.len())?;

        log::info!("Vulkan context fully initialized");

        Ok(Self {
            entry,
            instance: vk_instance,
            debug_messenger,
            surface_loader,
            surface: vk_surface,
            physical_device,
            depth_format,
            device,
            device_caps,
            queue_indices,
            graphics_queue,
            present_queue,
            swapchain_state,
            allocator: Some(gpu_allocator),
            render_pass,
            pipeline_cache,
            pipeline: pipelines.opaque,
            pipeline_alpha: pipelines.alpha,
            pipeline_two_sided: pipelines.opaque_two_sided,
            pipeline_alpha_two_sided: pipelines.alpha_two_sided,
            pipeline_ui,
            pipeline_layout: pipelines.layout,
            vert_module: pipelines.vert_module,
            frag_module: pipelines.frag_module,
            ui_vert_module,
            ui_frag_module,
            ui_quad_handle: None,
            mesh_registry,
            texture_registry,
            scene_buffers,
            accel_manager,
            cluster_cull,
            ssao,
            depth_allocation: Some(depth_allocation),
            depth_image,
            depth_image_view,
            framebuffers,
            command_pool,
            transfer_pool,
            command_buffers,
            frame_sync,
            current_frame: 0,
            frame_counter: 0,
        })
    }

    // draw_frame is in draw.rs
    // build_blas_for_mesh, register_ui_quad, swapchain_extent, log_memory_usage are in resources.rs
    // recreate_swapchain is in resize.rs
}

// Method implementations split across submodules:
mod draw;
mod helpers;
mod resize;
mod resources;

impl Drop for VulkanContext {
    fn drop(&mut self) {
        // SAFETY: device_wait_idle ensures all GPU work is complete before
        // destroying resources. Destruction follows reverse-creation order
        // to satisfy Vulkan object lifetime requirements.
        unsafe {
            let _ = self.device.device_wait_idle();

            self.frame_sync.destroy(&self.device);
            self.device
                .destroy_command_pool(self.transfer_pool, None);
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            // Destroy texture registry, scene buffers, and acceleration structures.
            if let Some(ref alloc) = self.allocator {
                self.texture_registry.destroy(&self.device, alloc);
                self.scene_buffers.destroy(&self.device, alloc);
                if let Some(ref mut accel) = self.accel_manager {
                    accel.destroy(&self.device, alloc);
                }
                if let Some(ref mut cc) = self.cluster_cull {
                    cc.destroy(&self.device, alloc);
                }
                if let Some(ref mut ssao) = self.ssao {
                    ssao.destroy(&self.device, alloc);
                }
            }

            // Destroy depth resources before the allocator.
            // Order: view → image → free allocation. The image must be
            // destroyed while its bound memory is still valid (Vulkan spec
            // VUID-vkFreeMemory-memory-00677).
            self.device.destroy_image_view(self.depth_image_view, None);
            self.device.destroy_image(self.depth_image, None);
            if let Some(alloc) = self.depth_allocation.take() {
                if let Some(ref allocator) = self.allocator {
                    allocator
                        .lock()
                        .expect("allocator lock poisoned")
                        .free(alloc)
                        .expect("Failed to free depth allocation");
                }
            }

            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline(self.pipeline_alpha, None);
            self.device.destroy_pipeline(self.pipeline_two_sided, None);
            self.device
                .destroy_pipeline(self.pipeline_alpha_two_sided, None);
            self.device.destroy_pipeline(self.pipeline_ui, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_shader_module(self.vert_module, None);
            self.device.destroy_shader_module(self.frag_module, None);
            self.device.destroy_shader_module(self.ui_vert_module, None);
            self.device.destroy_shader_module(self.ui_frag_module, None);
            // Meshes after pipelines: pipelines consume meshes at draw time,
            // so meshes should outlive the pipelines that reference them.
            if let Some(ref alloc) = self.allocator {
                self.mesh_registry.destroy_all(&self.device, alloc);
            }
            // Save pipeline cache to disk before destroying.
            save_pipeline_cache(&self.device, self.pipeline_cache);
            self.device
                .destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.swapchain_state.destroy(&self.device);
            // Drop the allocator before destroying the device.
            // take() extracts from Option, then try_unwrap gets the inner
            // Mutex if we hold the last Arc, then into_inner gives us the
            // Allocator which we drop — running its cleanup while the device
            // is still alive.
            if let Some(alloc_arc) = self.allocator.take() {
                match std::sync::Arc::try_unwrap(alloc_arc) {
                    Ok(mutex) => drop(mutex.into_inner().expect("allocator lock poisoned")),
                    Err(arc) => {
                        log::error!(
                            "GPU allocator has {} outstanding references — \
                             leaking allocator to avoid use-after-free on device destroy",
                            std::sync::Arc::strong_count(&arc),
                        );
                        debug_assert!(false, "GPU allocator leaked: outstanding Arc references");
                    }
                }
            }
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((ref utils, messenger)) = self.debug_messenger {
                utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
        log::info!("Vulkan context destroyed cleanly");
    }
}

// Helper functions are in helpers.rs — use helpers:: prefix.
use helpers::{
    allocate_command_buffers, create_command_pool, create_depth_resources, create_framebuffers, create_transfer_pool,
    create_render_pass, find_depth_format, load_or_create_pipeline_cache, save_pipeline_cache,
};
