//! Top-level Vulkan context that owns the entire graphics state.

use super::acceleration::AccelerationManager;
use super::allocator::{self, SharedAllocator};
use super::caustic::CausticPipeline;
use super::composite::{CompositePipeline, HDR_FORMAT};
use super::compute::ClusterCullPipeline;
use super::debug;
use super::device::{self, QueueFamilyIndices};
use super::gbuffer::{
    GBuffer, ALBEDO_FORMAT, MESH_ID_FORMAT, MOTION_FORMAT, NORMAL_FORMAT, RAW_INDIRECT_FORMAT,
};
use super::instance;
use super::pipeline;
use super::scene_buffer;
use super::ssao::SsaoPipeline;
use super::surface;
use super::svgf::SvgfPipeline;
use super::swapchain::{self, SwapchainState};
use super::sync::{self, FrameSync, MAX_FRAMES_IN_FLIGHT};
use super::taa::TaaPipeline;
use super::texture::Texture;
use crate::mesh::MeshRegistry;
use crate::texture_registry::TextureRegistry;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A single draw command: which mesh to draw, with what texture, and what model matrix.
pub struct DrawCommand {
    pub mesh_handle: u32,
    pub texture_handle: u32,
    pub model_matrix: [f32; 16],
    pub alpha_blend: bool,
    /// Source blend factor (Gamebryo AlphaFunction enum). Only meaningful
    /// when `alpha_blend` is true. 6 = SRC_ALPHA (default).
    pub src_blend: u8,
    /// Destination blend factor (Gamebryo AlphaFunction enum). Only meaningful
    /// when `alpha_blend` is true. 7 = INV_SRC_ALPHA (default).
    pub dst_blend: u8,
    pub two_sided: bool,
    /// Decal geometry — renders on top of coplanar surfaces via depth bias.
    pub is_decal: bool,
    /// Base offset into the bone-palette SSBO for this draw, or 0 for rigid.
    pub bone_offset: u32,
    /// Bindless texture index for the normal map (0 = no normal map).
    pub normal_map_index: u32,
    /// Bindless texture index for the dark/lightmap (0 = no dark map). #264.
    pub dark_map_index: u32,
    /// Bindless texture index for the glow / self-illumination map
    /// (NiTexturingProperty slot 4). 0 = no glow map; the shader falls
    /// back to the inline `emissive_color` × `emissive_mult` constant.
    /// See #399.
    pub glow_map_index: u32,
    /// Bindless texture index for the detail overlay (NiTexturingProperty
    /// slot 2). Sampled at 2× UV scale and modulated into the base
    /// albedo. 0 = no detail map. See #399.
    pub detail_map_index: u32,
    /// Bindless texture index for the gloss / specular mask
    /// (NiTexturingProperty slot 3). Per-texel specular strength
    /// multiplier; the .r channel scales the inline
    /// `specular_strength`. 0 = no gloss map. See #399.
    pub gloss_map_index: u32,
    /// Bindless texture index for the parallax / height map
    /// (`BSShaderTextureSet` slot 3). 0 = no POM; fragment shader
    /// falls back to flat normal mapping. See #453.
    pub parallax_map_index: u32,
    /// POM height scale (`BSShaderPPLightingProperty.parallax_scale`
    /// or Skyrim `ShaderTypeData::ParallaxOcc.scale`). Typical
    /// range 0.02–0.08. Default 0.04. See #453.
    pub parallax_height_scale: f32,
    /// POM ray-march sample budget (typically 4–16). Default 4.0
    /// matches the Gamebryo PPLighting default. See #453.
    pub parallax_max_passes: f32,
    /// Bindless texture index for the environment reflection map
    /// (`BSShaderTextureSet` slot 4). Currently sampled as a 2D
    /// texture; cubemap support is deferred. 0 = no env map. See #453.
    pub env_map_index: u32,
    /// Bindless texture index for the env-reflection mask
    /// (`BSShaderTextureSet` slot 5). 0 = unmasked. See #453.
    pub env_mask_index: u32,
    /// Alpha test threshold in [0,1]. 0.0 when alpha test is disabled. #263.
    pub alpha_threshold: f32,
    /// Alpha test comparison function (Gamebryo TestFunction enum). #263.
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER, 5=NOTEQUAL,
    /// 6=GREATEREQUAL, 7=NEVER. Only meaningful when alpha_threshold > 0.
    pub alpha_test_func: u32,
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
    /// Offset into the global vertex SSBO (in vertices).
    pub vertex_offset: u32,
    /// Offset into the global index SSBO (in indices).
    pub index_offset: u32,
    /// Vertex count for this mesh.
    pub vertex_count: u32,
    /// Camera-space depth for draw order sorting. Opaque draws are sorted
    /// front-to-back (smaller depth first) for early-Z; transparent draws
    /// are sorted back-to-front (larger depth first) for correct blending.
    /// Encoded as `f32::to_bits()` for deterministic `sort_unstable_by_key`.
    pub sort_depth: u32,
    /// Include this instance in the TLAS for RT ray queries.
    pub in_tlas: bool,
    /// Pre-computed average albedo (RGB) for fast GI bounce approximation.
    /// Replaces per-hit UV lookup + texture sample in the GI ray hit shader.
    pub avg_albedo: [f32; 3],
    /// `BSLightingShaderProperty.shader_type` enum value (0–19) — fed
    /// to `GpuInstance.material_kind` for the fragment shader's
    /// per-variant dispatch (SkinTint / HairTint / EyeEnvmap / etc.).
    /// 0 = Default lit. Plumbing only — variant rendering branches
    /// are per-variant follow-up work. See #344.
    pub material_kind: u32,
    /// Depth test enabled (`NiZBufferProperty.z_test`). Forwarded into
    /// `vkCmdSetDepthTestEnable` per draw batch via Vulkan 1.3 core
    /// extended dynamic state. Default true. See #398 (OBL-D4-H1).
    pub z_test: bool,
    /// Depth write enabled (`NiZBufferProperty.z_write`). Forwarded
    /// into `vkCmdSetDepthWriteEnable`. Default true. `false` for sky
    /// domes / viewmodels / glow halos / billboarded particles.
    pub z_write: bool,
    /// Depth comparison function (Gamebryo `TestFunction` enum).
    /// 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL (default), 4=GREATER,
    /// 5=NOTEQUAL, 6=GREATEREQUAL, 7=NEVER. Mapped to
    /// `vk::CompareOp` and forwarded into `vkCmdSetDepthCompareOp`.
    pub z_function: u8,
    /// Terrain tile slot for LAND splat meshes. `None` on every non-
    /// terrain draw. When present, the draw assembler sets
    /// `INSTANCE_FLAG_TERRAIN_SPLAT` and packs the slot into the top
    /// 16 bits of `GpuInstance.flags` so the fragment shader can
    /// sample the 8 layer textures per `GpuTerrainTile`. See #470.
    pub terrain_tile_index: Option<u32>,
}

/// Sky rendering parameters passed per-frame to the composite shader.
/// Populated from WTHR records for exterior cells or a procedural fallback.
pub struct SkyParams {
    /// Zenith (top-of-sky) color, raw monitor-space per 0e8efc6.
    pub zenith_color: [f32; 3],
    /// Horizon color, raw monitor-space per 0e8efc6.
    pub horizon_color: [f32; 3],
    /// Sun direction (normalized, world-space Y-up).
    pub sun_direction: [f32; 3],
    /// Sun disc color, raw monitor-space per 0e8efc6.
    pub sun_color: [f32; 3],
    /// Angular size of the sun disc as cos(half-angle). ~0.9998 for real sun.
    pub sun_size: f32,
    /// Sun brightness multiplier.
    pub sun_intensity: f32,
    /// Whether sky rendering is enabled (true for exterior cells).
    pub is_exterior: bool,
    /// Cloud layer 0 scroll offset in UV space (accumulated by weather_system).
    pub cloud_scroll: [f32; 2],
    /// Cloud layer 0 UV tile scale. `0.0` disables the cloud sample in the shader.
    pub cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Ignored when
    /// `cloud_tile_scale == 0.0`; otherwise must be a valid TextureRegistry index.
    pub cloud_texture_index: u32,
}

impl Default for SkyParams {
    fn default() -> Self {
        Self {
            zenith_color: [0.15, 0.3, 0.6],
            horizon_color: [0.5, 0.5, 0.45],
            sun_direction: [-0.4, 0.8, -0.45],
            sun_color: [1.0, 0.95, 0.8],
            sun_size: 0.9994, // cos(~2°) — visible disc, larger than real sun
            sun_intensity: 5.0,
            is_exterior: false,
            cloud_scroll: [0.0, 0.0],
            cloud_tile_scale: 0.0, // disabled until WTHR supplies a cloud texture
            cloud_texture_index: 0,
        }
    }
}

/// Handle for requesting and retrieving screenshots from outside the render loop.
pub struct ScreenshotHandle {
    /// Set to `true` to request a screenshot on the next frame.
    pub requested: Arc<AtomicBool>,
    /// After capture, the PNG bytes are placed here for retrieval.
    pub result: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ScreenshotHandle {
    pub fn new() -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(None)),
        }
    }

    /// Request a screenshot. Returns immediately; check `result` later.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    /// Take the screenshot result if available. Returns None if not ready.
    pub fn take_result(&self) -> Option<Vec<u8>> {
        self.result.lock().unwrap().take()
    }
}

pub struct VulkanContext {
    // Ordered for drop safety — later fields are destroyed first.
    pub current_frame: usize,
    /// Monotonic frame counter for temporal effects (jitter seed, accumulation).
    pub frame_counter: u32,
    /// Previous frame's view-projection matrix (column-major [f32; 16]).
    /// Used to compute screen-space motion vectors in the vertex shader.
    /// On the very first frame, equals the current frame's viewProj (no motion).
    pub prev_view_proj: [f32; 16],
    /// Per-frame scratch buffer for the GPU instance SSBO payload. Held on
    /// the context so that capacity amortizes across frames instead of
    /// heap-allocating fresh each `draw_frame`. Cleared + reserved at the
    /// top of draw_frame. See issue #243.
    gpu_instances_scratch: Vec<scene_buffer::GpuInstance>,
    /// Per-frame scratch buffer for draw batch metadata. Same lifecycle
    /// as `gpu_instances_scratch`. See issue #243.
    batches_scratch: Vec<draw::DrawBatch>,

    // ── Screenshot capture ──────────────────────────────────────────
    screenshot_requested: Arc<AtomicBool>,
    screenshot_result: Arc<Mutex<Option<Vec<u8>>>>,
    /// Staging buffer for screenshot readback (allocated on first capture).
    screenshot_staging: Option<(vk::Buffer, vk_alloc::Allocation, vk::DeviceSize)>,
    /// True when the staging buffer contains valid data waiting for fence.
    screenshot_pending_readback: bool,

    frame_sync: FrameSync,
    command_buffers: Vec<vk::CommandBuffer>,
    command_pool: vk::CommandPool,
    /// Dedicated pool for one-time upload/transfer commands, separate from
    /// the per-frame draw pool. Vulkan requires external synchronization on
    /// VkCommandPool (VUID-vkAllocateCommandBuffers-commandPool-00044);
    /// keeping upload commands on a separate pool avoids contention with
    /// draw command buffer reset/recording.
    pub transfer_pool: vk::CommandPool,
    /// Persistent fence reused across one-time submits (texture upload,
    /// BLAS build, mesh staging copy). Saves per-call VkFence
    /// create/destroy overhead during cell load (#302). Mutex serializes
    /// concurrent callers — only one reset+wait cycle at a time.
    pub transfer_fence: Arc<Mutex<vk::Fence>>,
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
    pub composite: Option<CompositePipeline>,
    pub gbuffer: Option<GBuffer>,
    pub svgf: Option<SvgfPipeline>,
    /// TAA resolve pass — reprojects + clamps history to produce the final
    /// HDR image that composite samples. None when allocation fails; the
    /// fallback path feeds raw HDR directly into composite.
    pub taa: Option<TaaPipeline>,
    /// Caustic scatter pass (#321) — per-frame refracted-light accumulator
    /// sampled by the composite pass as a `usampler2D`. Created after SVGF
    /// and before composite so composite's binding 5 can point at its
    /// sampled views. Non-optional: the R32_UINT atomic storage image the
    /// pass needs is universally supported on desktop GPUs.
    pub caustic: Option<CausticPipeline>,
    pipeline_cache: vk::PipelineCache,
    /// Opaque pipeline (depth write on, no blend, BACK culling).
    pipeline: vk::Pipeline,
    /// Opaque two-sided pipeline (depth write on, no blend, no culling).
    pipeline_two_sided: vk::Pipeline,
    /// Lazy cache of blended pipelines, keyed by `(src, dst, two_sided)`
    /// from `NiAlphaProperty.flags` (Gamebryo `AlphaFunction` enum). Each
    /// entry has depth-write disabled, blend on with the exact factor
    /// pair the source NIF authored. See #392 for why this replaced the
    /// earlier 6-pipeline `(opaque|alpha|additive) × (one|two)-sided`
    /// scheme: collapsing 11×11 = 121 possible Gamebryo factor pairs
    /// down to two `Alpha`/`Additive` buckets dropped half the
    /// pipeline-state information for content that depends on it (glass
    /// modulation, premultiplied alpha, etc.).
    blend_pipeline_cache: HashMap<(u8, u8, bool), vk::Pipeline>,
    pipeline_ui: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    /// Mesh handle for the fullscreen quad used by UI overlay.
    pub ui_quad_handle: Option<u32>,
    /// Mesh handle for the unit XY quad used by the CPU particle billboard
    /// path (#401). Emitter entities push one DrawCommand per live particle
    /// referencing this handle, with the per-particle position + size baked
    /// into the model matrix and the camera-facing rotation precomputed
    /// CPU-side. The existing instanced batching from #272 collapses all
    /// per-frame particle draws into a single instanced cmd_draw_indexed.
    pub particle_quad_handle: Option<u32>,
    /// Cell-load-time registry of active terrain splat tiles. Parallel
    /// to the mesh / texture registries; maps a tile slot (0..1023) to
    /// its 8 bindless texture indices. Uploaded to the `GpuTerrainTile`
    /// SSBO once per cell load and referenced by fragment shaders via
    /// `(instance.flags >> 16) & 0xFFFF`. Vacant slots are tracked in
    /// a free list. See #470.
    terrain_tiles: Vec<Option<scene_buffer::GpuTerrainTile>>,
    /// LIFO free list of vacant terrain tile slots.
    terrain_tile_free_list: Vec<u32>,
    /// Countdown of how many more frames must reupload the terrain
    /// tile SSBO. Set to `MAX_FRAMES_IN_FLIGHT` on every mutation and
    /// decremented each draw after the per-frame upload runs — ensures
    /// every frame-in-flight observes the new slab before the flag
    /// stops triggering writes. A single per-frame upload is 32 KB.
    terrain_tiles_dirty_frames: u32,
    render_pass: vk::RenderPass,
    swapchain_state: SwapchainState,

    pub allocator: Option<SharedAllocator>,

    /// Graphics queue, wrapped in a Mutex for Vulkan-required external
    /// synchronization (VUID-vkQueueSubmit-queue-00893). All queue
    /// submissions (draw_frame, texture/buffer uploads) must lock this.
    pub graphics_queue: Arc<Mutex<vk::Queue>>,
    /// Present queue for vkQueuePresentKHR. When graphics and present
    /// queue families are the same (common on desktop GPUs), this is an
    /// `Arc::clone` of `graphics_queue` — a single Mutex protects the
    /// shared VkQueue handle. When they differ, it's an independent
    /// Mutex wrapping the separate present queue. See #284 (C2-03).
    pub present_queue: Arc<Mutex<vk::Queue>>,
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
        let graphics_queue = Arc::new(Mutex::new(raw_graphics_queue));
        // When graphics and present use the same queue family, share the
        // same Mutex to avoid two locks wrapping one VkQueue handle (#284).
        let present_queue = if queue_indices.graphics == queue_indices.present {
            Arc::clone(&graphics_queue)
        } else {
            Arc::new(Mutex::new(raw_present_queue))
        };

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

        // 10. Main render pass: 6 color attachments (HDR + G-buffer +
        // raw_indirect + albedo) + depth.
        let render_pass = create_render_pass(
            &device,
            HDR_FORMAT,
            NORMAL_FORMAT,
            MOTION_FORMAT,
            MESH_ID_FORMAT,
            RAW_INDIRECT_FORMAT,
            ALBEDO_FORMAT,
            depth_format,
        )?;

        // 10. Command pools: one for per-frame draw commands (RESET_COMMAND_BUFFER),
        //     one for one-time upload/transfer commands (separate pool to avoid
        //     contention — Vulkan requires external sync on VkCommandPool).
        let command_pool = create_command_pool(&device, queue_indices.graphics)?;
        let transfer_pool = create_transfer_pool(&device, queue_indices.graphics)?;

        // Persistent fence for one-time submits (#302). Created unsignaled;
        // every use calls reset_fences then wait_for_fences.
        let transfer_fence = Arc::new(Mutex::new(unsafe {
            device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .context("create transfer fence")?
        }));

        // 11. Texture registry with checkerboard fallback.
        // Bindless array size is driven by the device limit (query in
        // device.rs, clamped at the R16_UINT mesh_id ceiling) instead of a
        // hardcoded 1024 that large cells would silently overflow. See #425.
        let mut texture_registry = TextureRegistry::new(
            &device,
            &gpu_allocator,
            swapchain_state.images.len() as u32,
            device_caps.max_bindless_sampled_images,
            device_caps.max_sampler_anisotropy,
        )?;
        let checkerboard = super::texture::generate_checkerboard(256, 256, 32);
        // One-shot 256×256 fallback — `None` pool skips the overhead of
        // the first pool entry that would otherwise linger for the rest
        // of the session.
        let fallback_texture = Texture::from_rgba(
            &device,
            &gpu_allocator,
            &graphics_queue,
            transfer_pool,
            256,
            256,
            &checkerboard,
            texture_registry.shared_sampler,
            None,
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
            let mut accel = AccelerationManager::new(&vk_instance, &device, physical_device);
            // Build an empty TLAS per frame-in-flight slot via one-time command
            // buffers so all descriptor sets have a valid acceleration structure
            // from frame 0. Each build blocks until complete (fence wait inside
            // with_one_time_commands), so no overlap between builds.
            let empty_draws: Vec<DrawCommand> = Vec::new();
            let empty_map: Vec<Option<u32>> = Vec::new();
            for f in 0..MAX_FRAMES_IN_FLIGHT {
                super::texture::with_one_time_commands_reuse_fence(
                    &device,
                    &graphics_queue,
                    transfer_pool,
                    &transfer_fence,
                    |cmd| unsafe {
                        accel
                            .build_tlas(&device, &gpu_allocator, cmd, &empty_draws, &empty_map, f)
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

        // 12b. Pipeline cache (load from disk if available).
        // Created before ANY pipeline-create call so every compile
        // writes into the shared cache — warm-start second-launch
        // skips most driver IR compilation (#426).
        let pipeline_cache = load_or_create_pipeline_cache(&device)?;

        // 12c. Cluster cull compute pipeline (light culling)
        let cluster_cull = match ClusterCullPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
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
                log::warn!(
                    "Cluster cull pipeline creation failed: {e} — falling back to all-lights loop"
                );
                None
            }
        };

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
        let pipeline_ui = pipeline::create_ui_pipeline(
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
            pipeline_cache,
            depth_image_view,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(s) => {
                // Transition AO image from UNDEFINED to SHADER_READ_ONLY_OPTIMAL
                // so the first frame's fragment shader sees a valid layout (1.0 =
                // no occlusion). Without this, sampling UNDEFINED is UB.
                if let Err(e) =
                    unsafe { s.initialize_ao_images(&device, &graphics_queue, transfer_pool) }
                {
                    log::warn!("SSAO AO image init failed: {e}");
                }
                for f in 0..MAX_FRAMES_IN_FLIGHT {
                    scene_buffers.write_ao_texture(&device, f, s.ao_image_views[f], s.ao_sampler);
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

        // 14b. G-buffer: all auxiliary attachments (normal, motion, mesh_id,
        // raw_indirect, albedo). Created BEFORE composite because composite's
        // descriptor sets reference the raw_indirect + albedo views.
        let gbuffer = Some(GBuffer::new(
            &device,
            &gpu_allocator,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        )?);
        let gbuffer_ref = gbuffer.as_ref().expect("gbuffer must exist");

        // Transition all G-buffer images from UNDEFINED to
        // SHADER_READ_ONLY_OPTIMAL so the "previous frame" slot is in a
        // valid layout on the very first frame (SVGF temporal pass binds
        // the previous frame's mesh_id/motion/raw_indirect for sampling).
        if let Err(e) =
            unsafe { gbuffer_ref.initialize_layouts(&device, &graphics_queue, transfer_pool) }
        {
            log::warn!("G-buffer layout init failed: {e}");
        }

        // Collect G-buffer views up-front so svgf, composite, and main
        // framebuffer creation can reference them.
        let n_frames = MAX_FRAMES_IN_FLIGHT;
        let raw_indirect_views: Vec<vk::ImageView> = (0..n_frames)
            .map(|i| gbuffer_ref.raw_indirect_view(i))
            .collect();
        let motion_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        let albedo_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.albedo_view(i)).collect();

        // 14b2. SVGF temporal denoiser — reads raw_indirect + motion +
        // mesh_id from the G-buffer, writes accumulated_indirect images
        // that the composite pass will sample in place of raw_indirect.
        // Created before composite so composite's descriptor sets can
        // reference SVGF's indirect_history views.
        let mut svgf = match SvgfPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            &raw_indirect_views,
            &motion_views_seed,
            &mesh_id_views_seed,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("SVGF pipeline creation failed: {e} — falling back to raw indirect");
                None
            }
        };
        // Transition history images UNDEFINED → GENERAL so first dispatch
        // and first descriptor sampling see a valid layout.
        if let Some(ref s) = svgf {
            if let Err(e) = unsafe { s.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("SVGF layout init failed: {e} — disabling SVGF");
                // Destroy partially-initialized pipeline.
                if let Some(mut pipe) = svgf.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }

        // Composite samples SVGF's accumulated indirect (GENERAL layout)
        // when SVGF is available, else falls back to raw G-buffer indirect
        // (SHADER_READ_ONLY_OPTIMAL layout).
        let (composite_indirect_views, indirect_is_general): (Vec<vk::ImageView>, bool) =
            if let Some(ref s) = svgf {
                ((0..n_frames).map(|i| s.indirect_view(i)).collect(), true)
            } else {
                (raw_indirect_views.clone(), false)
            };

        // 14b-bis. Caustic scatter pass (#321). Sits between SVGF and
        // composite so composite's binding 5 can sample its R32_UINT
        // accumulator. The compute shader fires ray queries against the
        // TLAS and uses the full set of per-FIF scene buffers, so all of
        // those need to exist (they do — this runs after SceneBuffers and
        // AccelerationManager are built).
        let normal_views_seed: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let mut caustic: Option<CausticPipeline> = match CausticPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            depth_image_view,
            &normal_views_seed,
            &mesh_id_views_seed,
            scene_buffers.light_buffers(),
            scene_buffers.light_buffer_size(),
            scene_buffers.camera_buffers(),
            scene_buffers.camera_buffer_size(),
            scene_buffers.instance_buffers(),
            scene_buffers.instance_buffer_size(),
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(anyhow::anyhow!("Caustic pipeline creation failed: {e}"));
            }
        };
        if let Some(ref c) = caustic {
            if let Err(e) = unsafe { c.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("Caustic layout init failed: {e} — disabling caustic");
                if let Some(mut pipe) = caustic.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }
        // Build caustic view list for composite. When caustic is disabled
        // we reuse the mesh_id views as a harmless placeholder (composite
        // samples with texelFetch as usampler2D; R16_UINT is narrower than
        // R32_UINT but SPIR-V's usampler2D reads undefined-for-bits-above-
        // format anyway, yielding small values and ~zero caustic). This
        // avoids a dedicated dummy image while keeping the descriptor slot
        // populated.
        let caustic_views: Vec<vk::ImageView> = match caustic {
            Some(ref c) => (0..n_frames).map(|i| c.sampled_view(i)).collect(),
            None => mesh_id_views_seed.clone(),
        };

        // 14c. Composite pipeline: owns HDR intermediates + tone-map pass.
        // Its descriptor sets sample HDR (owned by composite), indirect
        // (from SVGF or raw G-buffer), and albedo (G-buffer).
        let mut composite = match CompositePipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            swapchain_state.format.format,
            &swapchain_state.image_views,
            &composite_indirect_views,
            indirect_is_general,
            &albedo_views,
            depth_image_view,
            &caustic_views,
            texture_registry.descriptor_set_layout,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(c) => Some(c),
            Err(e) => {
                return Err(anyhow::anyhow!("Composite pipeline creation failed: {e}"));
            }
        };
        // Snapshot composite's HDR image views into an owned Vec so the
        // subsequent &mut borrow of `composite` (for TAA rewire) doesn't
        // conflict with the main-framebuffer creation below.
        let hdr_views_owned: Vec<vk::ImageView> = composite
            .as_ref()
            .expect("composite must exist after construction")
            .hdr_image_views
            .clone();

        // 14d. TAA resolve pass — needs the composite's HDR views (created
        // above) as its "current HDR" input, plus per-FIF motion + mesh_id.
        // If creation succeeds, composite's HDR descriptor is rewired to
        // sample TAA's output; otherwise we keep the raw HDR path.
        let mut taa = match TaaPipeline::new(
            &device,
            &gpu_allocator,
            pipeline_cache,
            &hdr_views_owned,
            &motion_views_seed,
            &mesh_id_views_seed,
            swapchain_state.extent.width,
            swapchain_state.extent.height,
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("TAA pipeline creation failed: {e} — falling back to raw HDR");
                None
            }
        };
        if let Some(ref t) = taa {
            if let Err(e) = unsafe { t.initialize_layouts(&device, &graphics_queue, transfer_pool) }
            {
                log::warn!("TAA layout init failed: {e} — disabling TAA");
                if let Some(mut pipe) = taa.take() {
                    unsafe { pipe.destroy(&device, &gpu_allocator) };
                }
            }
        }
        // Swap composite's HDR binding to TAA output so tone-map samples
        // the anti-aliased image. When TAA is disabled composite keeps its
        // original raw-HDR descriptors.
        if let (Some(ref t), Some(ref mut c)) = (taa.as_ref(), composite.as_mut()) {
            let taa_views: Vec<vk::ImageView> = (0..n_frames).map(|i| t.output_view(i)).collect();
            c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL);
        }

        // 15. Main framebuffers: one per frame-in-flight slot, binding that
        // slot's HDR + normal + motion + mesh_id + raw_indirect + albedo
        // views + shared depth view.
        let hdr_views: &[vk::ImageView] = &hdr_views_owned;
        let normal_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.normal_view(i)).collect();
        let motion_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.motion_view(i)).collect();
        let mesh_id_views: Vec<vk::ImageView> =
            (0..n_frames).map(|i| gbuffer_ref.mesh_id_view(i)).collect();
        let framebuffers = create_main_framebuffers(
            &device,
            render_pass,
            hdr_views,
            &normal_views,
            &motion_views,
            &mesh_id_views,
            &raw_indirect_views,
            &albedo_views,
            depth_image_view,
            swapchain_state.extent,
        )?;

        // 16. Command buffers — one per frame-in-flight (NOT per swapchain
        // image). The in_flight fence is per-frame, so tying command buffer
        // reuse to the same index makes the fence → cmd-buf relationship
        // direct and obvious. See #259.
        let command_buffers =
            allocate_command_buffers(&device, command_pool, sync::MAX_FRAMES_IN_FLIGHT)?;

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
            pipeline_two_sided: pipelines.opaque_two_sided,
            blend_pipeline_cache: HashMap::new(),
            pipeline_ui,
            pipeline_layout: pipelines.layout,
            ui_quad_handle: None,
            particle_quad_handle: None,
            terrain_tiles: vec![None; scene_buffer::MAX_TERRAIN_TILES],
            // Free list seeded with every slot in reverse order so
            // `pop()` returns slots in ascending order (deterministic
            // test behaviour).
            terrain_tile_free_list: (0..scene_buffer::MAX_TERRAIN_TILES as u32).rev().collect(),
            terrain_tiles_dirty_frames: 0,
            mesh_registry,
            texture_registry,
            scene_buffers,
            accel_manager,
            cluster_cull,
            ssao,
            composite,
            gbuffer,
            svgf,
            taa,
            caustic,
            depth_allocation: Some(depth_allocation),
            depth_image,
            depth_image_view,
            framebuffers,
            command_pool,
            transfer_pool,
            transfer_fence,
            command_buffers,
            frame_sync,
            current_frame: 0,
            frame_counter: 0,
            // Initialize to identity; first frame will overwrite with current
            // viewProj so motion vector is zero on the first frame.
            prev_view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            gpu_instances_scratch: Vec::new(),
            batches_scratch: Vec::new(),
            screenshot_requested: Arc::new(AtomicBool::new(false)),
            screenshot_result: Arc::new(Mutex::new(None)),
            screenshot_staging: None,
            screenshot_pending_readback: false,
        })
    }

    /// Run a closure in a one-time-submit command buffer, reusing the
    /// persistent transfer fence (#302). Prefer this over the free-function
    /// `with_one_time_commands` to avoid per-call fence create/destroy.
    pub fn with_transfer_commands<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(vk::CommandBuffer) -> Result<()>,
    {
        super::texture::with_one_time_commands_reuse_fence(
            &self.device,
            &self.graphics_queue,
            self.transfer_pool,
            &self.transfer_fence,
            f,
        )
    }

    /// Look up the cached blended pipeline for a given Gamebryo
    /// `(src, dst)` factor pair (with two-sided rasterizer flag), or
    /// create + cache it on first use. The cache is keyed by the raw
    /// `NiAlphaProperty.flags` nibbles, so identical factor pairs across
    /// different materials share one pipeline.
    ///
    /// Returns the cached pipeline on cache hit (no allocation, no
    /// device call). On cache miss, creates a pipeline through
    /// [`pipeline::create_blend_pipeline`] and inserts it.
    ///
    /// Pipelines created here are tied to the current render pass and
    /// must be destroyed and re-created on swapchain recreate
    /// ([`recreate_swapchain`](Self::recreate_swapchain)).
    pub fn get_or_create_blend_pipeline(
        &mut self,
        src: u8,
        dst: u8,
        two_sided: bool,
    ) -> Result<vk::Pipeline> {
        let key = (src, dst, two_sided);
        if let Some(&pipe) = self.blend_pipeline_cache.get(&key) {
            return Ok(pipe);
        }
        let pipe = pipeline::create_blend_pipeline(
            &self.device,
            self.render_pass,
            self.swapchain_state.extent,
            self.pipeline_cache,
            self.pipeline_layout,
            src,
            dst,
            two_sided,
        )?;
        self.blend_pipeline_cache.insert(key, pipe);
        Ok(pipe)
    }

    /// Get a handle for requesting screenshots from outside the render loop.
    pub fn screenshot_handle(&self) -> ScreenshotHandle {
        ScreenshotHandle {
            requested: Arc::clone(&self.screenshot_requested),
            result: Arc::clone(&self.screenshot_result),
        }
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
mod screenshot;

impl Drop for VulkanContext {
    fn drop(&mut self) {
        // SAFETY: device_wait_idle ensures all GPU work is complete before
        // destroying resources. Destruction follows reverse-creation order
        // to satisfy Vulkan object lifetime requirements.
        unsafe {
            let _ = self.device.device_wait_idle();

            self.destroy_screenshot_staging();

            self.frame_sync.destroy(&self.device);
            // Destroy persistent transfer fence (#302). device_wait_idle
            // above ensures it's not signaled in-flight.
            {
                let fence = *self
                    .transfer_fence
                    .lock()
                    .expect("transfer fence lock poisoned");
                self.device.destroy_fence(fence, None);
            }
            self.device.destroy_command_pool(self.transfer_pool, None);
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
                if let Some(ref mut composite) = self.composite {
                    composite.destroy(&self.device, alloc);
                }
                if let Some(ref mut caustic) = self.caustic {
                    caustic.destroy(&self.device, alloc);
                }
                if let Some(ref mut svgf) = self.svgf {
                    svgf.destroy(&self.device, alloc);
                }
                if let Some(ref mut taa) = self.taa {
                    taa.destroy(&self.device, alloc);
                }
                if let Some(ref mut gbuffer) = self.gbuffer {
                    gbuffer.destroy(&self.device, alloc);
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
            self.device.destroy_pipeline(self.pipeline_two_sided, None);
            for &pipe in self.blend_pipeline_cache.values() {
                self.device.destroy_pipeline(pipe, None);
            }
            self.blend_pipeline_cache.clear();
            self.device.destroy_pipeline(self.pipeline_ui, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
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
    allocate_command_buffers, create_command_pool, create_depth_resources,
    create_main_framebuffers, create_render_pass, create_transfer_pool, find_depth_format,
    load_or_create_pipeline_cache, save_pipeline_cache,
};
