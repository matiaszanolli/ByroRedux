//! HDR composite + tone-mapping pass.
//!
//! Owns the HDR color intermediate images that the main render pass writes
//! to, and the fullscreen composite pipeline that samples those images,
//! applies ACES tone mapping, and writes the result to the sRGB swapchain.
//!
//! This is the last pass of the frame. Runs as a dedicated render pass
//! after the main render pass ends:
//!
//!   main render pass → (HDR image in SHADER_READ_ONLY layout)
//!   composite render pass → (swapchain image in PRESENT_SRC_KHR layout)
//!   SSAO dispatch (reads depth, unchanged)
//!   submit + present
//!
//! ## Per-frame HDR images
//!
//! With MAX_FRAMES_IN_FLIGHT in flight simultaneously, a single HDR image
//! would create a read-after-write hazard: frame N's composite reads HDR
//! while frame N+1's main render pass writes it. We use one HDR image
//! per frame-in-flight slot. Memory cost: ~16 MB at 1080p (2 × RGBA16F).
//!
//! ## Per-swapchain-image composite framebuffers
//!
//! The composite render pass writes to the swapchain, which has its own
//! image per swapchain slot (typically 3). We create one composite
//! framebuffer per swapchain image, binding just the swapchain view (no
//! depth needed — fullscreen triangle, depth test disabled).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const COMPOSITE_VERT_SPV: &[u8] = include_bytes!("../../shaders/composite.vert.spv");
const COMPOSITE_FRAG_SPV: &[u8] = include_bytes!("../../shaders/composite.frag.spv");

/// Composite parameter UBO — fog state + sky rendering parameters.
///
/// Layout must match `CompositeParams` in `composite.frag` exactly.
/// The struct is uploaded once per frame before the composite dispatch.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeParams {
    /// xyz = RGB, w = fog enabled (1.0 = yes, 0.0 = no).
    pub fog_color: [f32; 4],
    /// x = fog near, y = fog far, z/w = unused.
    pub fog_params: [f32; 4],
    /// x = is_exterior (1.0 = sky enabled), y = exposure (default 0.85), z/w = reserved.
    pub depth_params: [f32; 4],
    /// xyz = zenith (top-of-sky) color in linear RGB, w = sun angular size (cos threshold).
    pub sky_zenith: [f32; 4],
    /// xyz = horizon color in linear RGB, w = unused.
    pub sky_horizon: [f32; 4],
    /// `xyz` = below-horizon ground colour from WTHR's `SKY_LOWER`
    /// group (real `Sky-Lower` per nif.xml NAM0 schema, slot 7
    /// post-#729). `w` = unused. Drives the `compute_sky` shader
    /// branch when `dir.y < 0`. Pre-#541 the shader faked this as
    /// `sky_horizon * 0.3` and the authored colour was discarded.
    pub sky_lower: [f32; 4],
    /// xyz = sun direction (normalized, world-space Y-up), w = sun intensity.
    pub sun_dir: [f32; 4],
    /// xyz = sun disc color in linear RGB, w = bindless texture index
    /// for the CLMT FNAM sun sprite stored via `f32::from_bits(idx)`;
    /// `0` = procedural disc (pre-#478 behaviour). Reinterpreted as
    /// `uint` in the shader.
    pub sun_color: [f32; 4],
    /// Cloud layer 0 parameters.
    ///
    /// - `x` = scroll U (accumulated over time × wind speed)
    /// - `y` = scroll V
    /// - `z` = tile scale (0.0 disables clouds — shader skips the sample)
    /// - `w` = bindless texture index for cloud_textures[0], stored via
    ///         `f32::from_bits(idx as u32)`; reinterpreted as uint in the shader.
    pub cloud_params: [f32; 4],
    /// Cloud layer 1 parameters (WTHR CNAM). Same packing as cloud_params.
    /// Drifts in the opposite U direction at 1.35× speed for parallax.
    /// `z` = 0.0 disables the layer when no CNAM texture is available.
    pub cloud_params_1: [f32; 4],
    /// Cloud layer 2 parameters (WTHR ANAM). Same packing as cloud_params (M33.1).
    /// Drifts in the same direction as layer 0 for layered parallax.
    /// `z` = 0.0 disables the layer when no ANAM texture is available.
    pub cloud_params_2: [f32; 4],
    /// Cloud layer 3 parameters (WTHR BNAM). Same packing as cloud_params (M33.1).
    /// Drifts in the opposite U direction (mirrors layer 1).
    /// `z` = 0.0 disables the layer when no BNAM texture is available.
    pub cloud_params_3: [f32; 4],
    /// `xyz` = camera world position; `w` unused. Needed for per-pixel
    /// world-space fog distance in composite (#428). Before this field
    /// landed, fog was computed in `triangle.frag` and baked into the
    /// G-buffer indirect-light attachment — which leaked into SVGF
    /// history and produced multi-frame ghosting on fog transitions.
    pub camera_pos: [f32; 4],
    /// Inverse view-projection matrix for reconstructing world-space ray direction
    /// (and, with `camera_pos`, world-space fragment positions for fog) from
    /// screen UV in the composite shader.
    pub inv_view_proj: [[f32; 4]; 4],
}

/// HDR color format. RGBA16F = 8 bytes/pixel, sufficient dynamic range
/// for all real-world scene brightness, supports alpha for glass blending.
pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Owns the HDR intermediates + composite pipeline + composite render pass.
pub struct CompositePipeline {
    /// HDR color images (one per frame-in-flight slot).
    pub hdr_images: Vec<vk::Image>,
    /// HDR color image views (parallel to hdr_images).
    pub hdr_image_views: Vec<vk::ImageView>,
    /// GPU-local allocations backing hdr_images.
    hdr_allocations: Vec<Option<vk_alloc::Allocation>>,

    /// Dedicated render pass for the composite step. Single color attachment
    /// = swapchain format, no depth.
    pub composite_render_pass: vk::RenderPass,
    /// Per-swapchain-image composite framebuffer (binds just swapchain view).
    composite_framebuffers: Vec<vk::Framebuffer>,

    /// Graphics pipeline: fullscreen triangle + ACES tone map fragment shader.
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// One descriptor set per frame-in-flight. Each references that frame's
    /// HDR + raw_indirect + albedo views + the per-frame params UBO.
    descriptor_sets: Vec<vk::DescriptorSet>,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    /// Sampler for reading all the input textures (HDR, indirect, albedo).
    hdr_sampler: vk::Sampler,
    /// Separate NEAREST sampler for integer-format attachments (R32_UINT
    /// caustic accumulator). `hdr_sampler` uses `VK_FILTER_LINEAR` which
    /// is illegal on UINT formats under VUID-vkCmdDraw-magFilter-04553
    /// (integer formats don't expose `SAMPLED_IMAGE_FILTER_LINEAR_BIT`).
    /// The fragment shader texelFetches this texture — filter mode is
    /// cosmetically irrelevant, but the validation layer checks it
    /// regardless.
    caustic_sampler: vk::Sampler,
    /// Per-frame parameter UBOs.
    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,
}

impl CompositePipeline {
    /// Create all HDR intermediate images, the composite render pass +
    /// pipeline, and the per-swapchain-image composite framebuffers.
    ///
    /// `indirect_views` comes from the SVGF denoiser (Phase 3+) in layout
    /// GENERAL, or from the raw G-buffer output (Phase 2) in layout
    /// SHADER_READ_ONLY_OPTIMAL. `albedo_views` comes from the G-buffer.
    /// Which layout the indirect is in is baked into the descriptor sets
    /// at write time — callers must pass `indirect_is_general=true` when
    /// wiring up the SVGF output.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        indirect_views: &[vk::ImageView],
        indirect_is_general: bool,
        albedo_views: &[vk::ImageView],
        depth_view: vk::ImageView,
        caustic_views: &[vk::ImageView],
        volumetric_views: &[vk::ImageView],
        bloom_views: &[vk::ImageView],
        bindless_layout: vk::DescriptorSetLayout,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            swapchain_format,
            swapchain_views,
            indirect_views,
            indirect_is_general,
            albedo_views,
            depth_view,
            caustic_views,
            volumetric_views,
            bloom_views,
            bindless_layout,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("Composite pipeline creation failed at: {e}");
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        swapchain_format: vk::Format,
        swapchain_views: &[vk::ImageView],
        indirect_views: &[vk::ImageView],
        indirect_is_general: bool,
        albedo_views: &[vk::ImageView],
        depth_view: vk::ImageView,
        caustic_views: &[vk::ImageView],
        volumetric_views: &[vk::ImageView],
        bloom_views: &[vk::ImageView],
        bindless_layout: vk::DescriptorSetLayout,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(indirect_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(albedo_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(caustic_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(volumetric_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(bloom_views.len(), MAX_FRAMES_IN_FLIGHT);
        // Build a partially-valid Self so we can use destroy() for cleanup
        // on any error. Fields that haven't been created yet use null
        // handles — destroy() calls vkDestroy* on null (always a no-op).
        let mut partial = Self {
            hdr_images: Vec::new(),
            hdr_image_views: Vec::new(),
            hdr_allocations: Vec::new(),
            composite_render_pass: vk::RenderPass::null(),
            composite_framebuffers: Vec::new(),
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            vert_module: vk::ShaderModule::null(),
            frag_module: vk::ShaderModule::null(),
            hdr_sampler: vk::Sampler::null(),
            caustic_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
        };

        // Macro to clean up partial state on any fallible call.
        // SAFETY (inside macro): `partial` is local to this fn and not
        // yet referenced by any command buffer / descriptor set;
        // cleanup-on-error closes the partial state before returning.
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

        // ── 1. Create HDR images (one per frame-in-flight) ───────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let img_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HDR_FORMAT)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let img = try_or_cleanup!(unsafe {
                device
                    .create_image(&img_info, None)
                    .context("Failed to create HDR color image")
            });
            partial.hdr_images.push(img);
            partial.hdr_allocations.push(None);

            let alloc = try_or_cleanup!(allocator
                .lock()
                .expect("allocator lock")
                .allocate(&vk_alloc::AllocationCreateDesc {
                    name: &format!("hdr_color_{}", i),
                    requirements: unsafe { device.get_image_memory_requirements(img) },
                    location: gpu_allocator::MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                })
                .context("Failed to allocate HDR image memory"));
            try_or_cleanup!(unsafe {
                device
                    .bind_image_memory(img, alloc.memory(), alloc.offset())
                    .context("bind HDR image memory")
            });
            partial.hdr_allocations[i] = Some(alloc);

            let view = try_or_cleanup!(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(HDR_FORMAT)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )
                    .context("HDR image view")
            });
            partial.hdr_image_views.push(view);
        }

        // ── 2. HDR sampler (linear filter for slight bilinear smoothing) ──
        partial.hdr_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("HDR sampler")
        });

        // Separate NEAREST sampler for the R32_UINT caustic accumulator.
        // Integer-format views don't expose FILTER_LINEAR, and binding an
        // LINEAR sampler to a `usampler2D` trips
        // VUID-vkCmdDraw-magFilter-04553 even though the fragment shader
        // only ever uses `texelFetch` against this view.
        partial.caustic_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("Caustic (R32_UINT) sampler")
        });

        // ── 3. Composite render pass ─────────────────────────────────
        // Single color attachment = swapchain. Load DONT_CARE (fullscreen
        // triangle covers every pixel). Final layout PRESENT_SRC_KHR.
        let composite_color = vk::AttachmentDescription::default()
            .format(swapchain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let composite_color_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };
        let composite_color_refs = [composite_color_ref];

        let composite_subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&composite_color_refs);

        // Incoming dependency: wait for upstream producers to finish
        // writing the attachments / storage images that composite.frag
        // samples.
        //
        // Two producer stages feed this subpass:
        //   - the main render pass fragment-shader path writes the HDR
        //     color attachment (COLOR_ATTACHMENT_OUTPUT / COLOR_ATTACHMENT_WRITE).
        //   - several compute passes — SVGF temporal + spatial accumulation,
        //     TAA history, caustic splat, SSAO — write storage images
        //     sampled by composite.frag. In practice every one of these
        //     passes emits an explicit manual `vkCmdPipelineBarrier` after
        //     dispatch, so the execution dependency is already covered and
        //     validation layers don't fire. But if a future compute pass
        //     is added that relies on the render-pass-level dependency
        //     instead of its own barrier, it would race composite without
        //     the COMPUTE_SHADER bit here. See #572 / AUDIT_RENDERER
        //     2026-04-22 Dim 1 SY-1.
        let composite_dep_in = vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
            )
            .src_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE,
            )
            .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);

        // Outgoing dependency: ensure swapchain write finishes before present.
        let composite_dep_out = vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags::empty());

        let attachments = [composite_color];
        let subpasses = [composite_subpass];
        let dependencies = [composite_dep_in, composite_dep_out];
        let rp_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        partial.composite_render_pass = try_or_cleanup!(unsafe {
            device
                .create_render_pass(&rp_info, None)
                .context("composite render pass")
        });

        // ── 4. Composite framebuffers (one per swapchain image) ─────
        for &view in swapchain_views {
            let attachments = [view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(partial.composite_render_pass)
                .attachments(&attachments)
                .width(width)
                .height(height)
                .layers(1);
            let fb = try_or_cleanup!(unsafe {
                device
                    .create_framebuffer(&fb_info, None)
                    .context("composite framebuffer")
            });
            partial.composite_framebuffers.push(fb);
        }

        // ── 5. Per-frame parameter UBOs ──────────────────────────────
        let param_size = std::mem::size_of::<CompositeParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // ── 6. Descriptor set layout + pipeline layout ───────────────
        // 7 bindings — HDR, indirect, albedo, params UBO, depth,
        // caustic, volumetric (M55 Phase 4).
        let ds_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // 5: caustic accumulator as usampler2D — R32_UINT sampled view.
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // 6: volumetric froxel volume as sampler3D (M55 Phase 4).
            // Shader ray-marches this volume per fragment to add
            // inscattered light + extinction modulation.
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            // 7: bloom output mip 0 (M58). HDR-linear, half-screen
            // resolution; sampled with bilinear filter to upscale to
            // full screen. Shader does `combined += bloom * intensity`
            // before tone-map.
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        validate_set_layout(
            0,
            &ds_bindings,
            &[
                ReflectedShader {
                    name: "composite.vert",
                    spirv: COMPOSITE_VERT_SPV,
                },
                ReflectedShader {
                    name: "composite.frag",
                    spirv: COMPOSITE_FRAG_SPV,
                },
            ],
            "composite",
            &[],
        )
        .expect("composite descriptor layout drifted against composite.vert/frag (see #427)");
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&ds_bindings),
                    None,
                )
                .context("composite descriptor set layout")
        });

        // Pipeline layout uses set 0 = composite's own bindings,
        // set 1 = bindless texture array from TextureRegistry. The composite
        // shader samples cloud textures through the bindless array at an
        // index provided in CompositeParams.cloud_params.w.
        let set_layouts = [partial.descriptor_set_layout, bindless_layout];
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                    None,
                )
                .context("composite pipeline layout")
        });

        // ── 7. Descriptor pool + per-frame descriptor sets ───────────
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                // 7 sampler bindings per set: HDR, indirect, albedo,
                // depth, caustic, volumetric, bloom.
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 7) as u32,
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
                .context("composite descriptor pool")
        });

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("composite descriptor sets")
        });

        // Write each descriptor set to sample its own frame's HDR + indirect +
        // albedo views, bind the param UBO, and the shared depth image.
        let indirect_layout = if indirect_is_general {
            vk::ImageLayout::GENERAL
        } else {
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        };
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let hdr_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(partial.hdr_image_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let indirect_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(indirect_views[i])
                .image_layout(indirect_layout)];
            let albedo_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(albedo_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            // Depth is shared (not per-frame-in-flight) — the main render pass
            // has finished and transitioned depth to SHADER_READ_ONLY by the
            // time the composite pass runs.
            let depth_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(depth_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let caustic_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.caustic_sampler)
                .image_view(caustic_views[i])
                .image_layout(vk::ImageLayout::GENERAL)];
            let volumetric_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(volumetric_views[i])
                .image_layout(vk::ImageLayout::GENERAL)];
            let bloom_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.hdr_sampler)
                .image_view(bloom_views[i])
                .image_layout(vk::ImageLayout::GENERAL)];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&hdr_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&indirect_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&albedo_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&params_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(4)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&depth_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(5)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&caustic_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(6)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&volumetric_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(partial.descriptor_sets[i])
                    .dst_binding(7)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&bloom_info),
            ];
            // SAFETY: descriptor sets owned by `partial`; writes reference
            // HDR / depth / indirect / albedo / caustic / volumetric /
            // bloom image views — all owned by `partial` or caller-borrowed
            // for the duration of this `new()` call.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // ── 8. Shader modules ────────────────────────────────────────
        partial.vert_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            COMPOSITE_VERT_SPV
        ));
        partial.frag_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            COMPOSITE_FRAG_SPV
        ));

        // ── 9. Graphics pipeline ─────────────────────────────────────
        let entry_point = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(partial.vert_module)
                .name(entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(partial.frag_module)
                .name(entry_point),
        ];

        // No vertex input — the fullscreen triangle is generated in the
        // vertex shader from gl_VertexIndex.
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        // Both VIEWPORT and SCISSOR are declared dynamic below (and
        // the caller's `dispatch()` sets them every frame via
        // `cmd_set_viewport` / `cmd_set_scissor`), so the `viewports`
        // / `scissors` arrays in `PipelineViewportStateCreateInfo`
        // are ignored at pipeline-create time per the Vulkan spec.
        // Only the counts matter. See audit PIPE-1 / #578.
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        // No depth test/write — fullscreen triangle covers everything.
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(false)
            .depth_write_enable(false);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false)];
        let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(partial.pipeline_layout)
            .render_pass(partial.composite_render_pass)
            .subpass(0);

        // SAFETY: pipeline_info references partial.{vert_module,
        // frag_module, pipeline_layout, composite_render_pass} all
        // created above. pipeline_cache is caller-provided.
        partial.pipeline = match unsafe {
            device.create_graphics_pipelines(pipeline_cache, &[pipeline_info], None)
        } {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                // SAFETY: cleanup-on-error.
                unsafe { partial.destroy(device, allocator) };
                return Err(anyhow::anyhow!("composite graphics pipeline: {e}"));
            }
        };

        log::info!("Composite pipeline created: {}x{} HDR", width, height);

        Ok(partial)
    }

    /// Begin composite render pass + draw fullscreen triangle + end.
    /// Call after the main render pass ends and before submit.
    ///
    /// Safety: `cmd` must be a valid recording command buffer. Frame index
    /// must be < MAX_FRAMES_IN_FLIGHT. Swapchain image index must be valid.
    pub unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        swapchain_image_index: usize,
        bindless_set: vk::DescriptorSet,
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        }];
        let rp_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.composite_render_pass)
            .framebuffer(self.composite_framebuffers[swapchain_image_index])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            })
            .clear_values(&clear_values);
        // SAFETY: caller of `dispatch` (unsafe fn) guarantees `cmd` is a
        // recording command buffer, `frame < MAX_FRAMES_IN_FLIGHT`, and
        // `swapchain_image_index` is a valid swapchain index. The render
        // pass + pipeline + descriptor sets are owned by `self`.
        unsafe {
            device.cmd_begin_render_pass(cmd, &rp_begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.width as f32,
                height: self.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            };
            device.cmd_set_scissor(cmd, 0, &[scissor]);

            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_sets[frame], bindless_set],
                &[],
            );

            // Fullscreen triangle: 3 vertices, no bound vertex buffer.
            device.cmd_draw(cmd, 3, 1, 0, 0);

            device.cmd_end_render_pass(cmd);
        }
    }

    /// Recreate framebuffers and pipeline viewport-dependent state on
    /// swapchain resize. The HDR images themselves are recreated because
    /// their size matches the swapchain. Caller must also pass the new
    /// indirect + albedo views (SVGF/GBuffer just recreated them).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        swapchain_views: &[vk::ImageView],
        indirect_views: &[vk::ImageView],
        indirect_is_general: bool,
        albedo_views: &[vk::ImageView],
        depth_view: vk::ImageView,
        caustic_views: &[vk::ImageView],
        volumetric_views: &[vk::ImageView],
        bloom_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        // Destroy old framebuffers
        // SAFETY (the three destroy loops below): `recreate_on_resize`
        // runs from the fenced swapchain-resize path
        // (`VulkanContext::recreate_swapchain` waits both frames-in-flight
        // first). Old framebuffer / view / image handles are unreferenced
        // by any in-flight command.
        for &fb in &self.composite_framebuffers {
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        self.composite_framebuffers.clear();

        // Destroy old HDR images
        for &view in &self.hdr_image_views {
            unsafe { device.destroy_image_view(view, None) };
        }
        self.hdr_image_views.clear();
        for &img in &self.hdr_images {
            unsafe { device.destroy_image(img, None) };
        }
        self.hdr_images.clear();
        for alloc in self.hdr_allocations.drain(..) {
            if let Some(a) = alloc {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }

        self.width = width;
        self.height = height;

        // Recreate HDR images. On partial failure, clean up any
        // already-allocated new resources. See #283.
        let result = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                let img_info = vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(HDR_FORMAT)
                    .extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED);
                // SAFETY: `img_info` fully populated above; image owned
                // by `self.hdr_images` on Ok. On Err the `?` bubbles up
                // before any subsequent allocation runs.
                let img = unsafe { device.create_image(&img_info, None)? };
                self.hdr_images.push(img);

                let alloc = allocator.lock().expect("allocator lock").allocate(
                    &vk_alloc::AllocationCreateDesc {
                        name: &format!("hdr_color_{}", i),
                        // SAFETY: `img` just created above.
                        requirements: unsafe { device.get_image_memory_requirements(img) },
                        location: gpu_allocator::MemoryLocation::GpuOnly,
                        linear: false,
                        allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
                    },
                )?;
                // SAFETY: `img` matches the memory requirements that
                // produced `alloc`; bound once per image.
                unsafe { device.bind_image_memory(img, alloc.memory(), alloc.offset())? };
                self.hdr_allocations.push(Some(alloc));

                // SAFETY: `img` is bound (line above); view owned by
                // `self.hdr_image_views` on Ok.
                let view = unsafe {
                    device.create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(HDR_FORMAT)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            }),
                        None,
                    )?
                };
                self.hdr_image_views.push(view);
            }

            // Rewrite descriptor sets to point at the new HDR, indirect,
            // albedo views, and updated depth view. Params UBO buffers are unchanged.
            let param_size = std::mem::size_of::<CompositeParams>() as vk::DeviceSize;
            let indirect_layout = if indirect_is_general {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            };
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                let hdr_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(self.hdr_image_views[i])
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let indirect_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(indirect_views[i])
                    .image_layout(indirect_layout)];
                let albedo_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(albedo_views[i])
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
                let params_info = [vk::DescriptorBufferInfo {
                    buffer: self.param_buffers[i].buffer,
                    offset: 0,
                    range: param_size,
                }];
                let depth_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(depth_view)
                    .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
                let caustic_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.caustic_sampler)
                    .image_view(caustic_views[i])
                    .image_layout(vk::ImageLayout::GENERAL)];
                let volumetric_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(volumetric_views[i])
                    .image_layout(vk::ImageLayout::GENERAL)];
                let bloom_info = [vk::DescriptorImageInfo::default()
                    .sampler(self.hdr_sampler)
                    .image_view(bloom_views[i])
                    .image_layout(vk::ImageLayout::GENERAL)];
                // Typed [_; 8] array — compile catches divergence from
                // the 8-binding layout (#905). Init path mirrors this
                // exact shape at lines 633-674.
                let writes: [vk::WriteDescriptorSet; 8] = [
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&hdr_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&indirect_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&albedo_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(&params_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(4)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&depth_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(5)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&caustic_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(6)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&volumetric_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.descriptor_sets[i])
                        .dst_binding(7)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&bloom_info),
                ];
                // SAFETY: descriptor sets owned by `self`; writes
                // reference freshly-recreated HDR views (owned by `self`)
                // and caller-borrowed indirect / albedo / depth /
                // caustic / volumetric / bloom views.
                unsafe { device.update_descriptor_sets(&writes, &[]) };
            }

            // Recreate composite framebuffers (bound to swapchain views).
            for &view in swapchain_views {
                let attachments = [view];
                let fb_info = vk::FramebufferCreateInfo::default()
                    .render_pass(self.composite_render_pass)
                    .attachments(&attachments)
                    .width(width)
                    .height(height)
                    .layers(1);
                // SAFETY: `fb_info` references `self.composite_render_pass`
                // (live) and the caller-borrowed swapchain `view` (live
                // until the next swapchain recreate, which we are inside).
                let fb = unsafe { device.create_framebuffer(&fb_info, None)? };
                self.composite_framebuffers.push(fb);
            }

            Ok(())
        })(); // end of closure
        if let Err(ref e) = result {
            log::error!("Composite recreate partial failure: {e} — cleaning up");
            // Clean up only the recreatable resources (HDR images +
            // framebuffers), NOT the pipeline/render-pass/descriptors.
            // SAFETY (the four destroy loops below): fenced-resize path —
            // `VulkanContext::recreate_swapchain` waits both
            // frames-in-flight before reaching this branch, so no
            // in-flight command references any of the partially-allocated
            // recreate state.
            for &fb in &self.composite_framebuffers {
                unsafe { device.destroy_framebuffer(fb, None) };
            }
            self.composite_framebuffers.clear();
            for &view in &self.hdr_image_views {
                unsafe { device.destroy_image_view(view, None) };
            }
            self.hdr_image_views.clear();
            for &img in &self.hdr_images {
                unsafe { device.destroy_image(img, None) };
            }
            self.hdr_images.clear();
            for alloc in self.hdr_allocations.drain(..) {
                if let Some(a) = alloc {
                    allocator.lock().expect("allocator lock").free(a).ok();
                }
            }
        }
        result
    }

    /// Rewrite binding 0 (HDR sampler) across every per-frame descriptor
    /// set to point at a different set of views. Called from init
    /// (`context/mod.rs:792`) and resize (`context/resize.rs:347`) to
    /// switch composite between raw HDR (from the render pass) and the
    /// TAA storage-image output.
    ///
    /// `hdr_layout` must match the new views' current image layout:
    ///   - `SHADER_READ_ONLY_OPTIMAL` for raw HDR (render-pass final layout)
    ///   - `GENERAL` for TAA storage-image output (current default)
    ///
    /// Current usage: both callers pass the TAA output views at
    /// `GENERAL` layout — the raw-HDR path is dormant but kept available
    /// for diagnostic A/B testing or a future TAA-disable flag.
    /// Permanent-failure escape hatch for the TAA pass. When TAA
    /// dispatch fails (lost device, descriptor pool exhaustion, driver
    /// crash) the composite's binding 0 still points at the TAA
    /// output — which holds whatever TAA wrote on its last successful
    /// dispatch, so the screen freezes on a stale HDR frame with no
    /// user-facing signal. Pointing composite back at its own raw HDR
    /// views restores a live image (no temporal AA) so the pipeline
    /// stays visibly alive while the driver failure gets diagnosed.
    /// See #479.
    pub fn fall_back_to_raw_hdr(&mut self, device: &ash::Device) {
        // `clone` the view handles (not the images) so the existing
        // `rebind_hdr_views` contract (single source-of-views arg)
        // doesn't need to grow a borrow-self variant. Views are `Copy`-
        // like Vulkan handles — no actual allocation beyond the Vec.
        let views = self.hdr_image_views.clone();
        self.rebind_hdr_views(device, &views, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    }

    pub fn rebind_hdr_views(
        &mut self,
        device: &ash::Device,
        hdr_views: &[vk::ImageView],
        hdr_layout: vk::ImageLayout,
    ) {
        debug_assert_eq!(hdr_views.len(), MAX_FRAMES_IN_FLIGHT);
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let info = [vk::DescriptorImageInfo::default()
                .sampler(self.hdr_sampler)
                .image_view(hdr_views[i])
                .image_layout(hdr_layout)];
            let write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[i])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info);
            // SAFETY: descriptor set `i` owned by `self`; `info` references
            // caller-borrowed `hdr_views[i]` (live for this call) and
            // `self.hdr_sampler` (live for `self`).
            unsafe { device.update_descriptor_sets(&[write], &[]) };
        }
    }

    /// Upload per-frame composite parameters (fog state, etc.) to the
    /// frame's UBO. Call once per frame before `dispatch`.
    pub fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        params: &CompositeParams,
    ) -> Result<()> {
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(params))
    }

    /// Destroy all Vulkan objects. Must be called before the device/allocator
    /// are dropped. Safe to call on partially-initialized state.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY (whole function): caller of `destroy` (unsafe fn)
        // guarantees no in-flight command buffer references any object
        // owned by `self`. Per-handle `if != null()` guards make this
        // safe to call on partially-initialised state from
        // `try_or_cleanup`.
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.pipeline != vk::Pipeline::null() {
            unsafe { device.destroy_pipeline(self.pipeline, None) };
        }
        if self.vert_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.vert_module, None) };
        }
        if self.frag_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.frag_module, None) };
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            unsafe { device.destroy_pipeline_layout(self.pipeline_layout, None) };
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            unsafe { device.destroy_descriptor_set_layout(self.descriptor_set_layout, None) };
        }
        for &fb in &self.composite_framebuffers {
            unsafe { device.destroy_framebuffer(fb, None) };
        }
        self.composite_framebuffers.clear();
        if self.composite_render_pass != vk::RenderPass::null() {
            unsafe { device.destroy_render_pass(self.composite_render_pass, None) };
        }
        if self.hdr_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.hdr_sampler, None) };
        }
        if self.caustic_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.caustic_sampler, None) };
        }
        for &view in &self.hdr_image_views {
            unsafe { device.destroy_image_view(view, None) };
        }
        self.hdr_image_views.clear();
        for &img in &self.hdr_images {
            unsafe { device.destroy_image(img, None) };
        }
        self.hdr_images.clear();
        for alloc in self.hdr_allocations.drain(..) {
            if let Some(a) = alloc {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

#[cfg(test)]
mod composite_params_layout_tests {
    //! Guards against drift between the Rust `CompositeParams` struct and
    //! the `CompositeParams` block declared in `composite.frag`. The
    //! descriptor-set reflection from #427 cross-checks bindings at
    //! startup, but the field offsets inside the UBO block are invisible
    //! to Vulkan — a silent reorder would miscompute fog distance etc.
    //! until someone notices visually. Offsets follow std140 layout,
    //! which for an all-vec4/mat4 block is just the sum of the field
    //! sizes in declaration order.

    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn composite_params_is_16_byte_aligned_std140_shape() {
        // Every field is vec4 (16 B) or mat4 (64 B = 4 × vec4). std140
        // requires vec4 alignment on both, so offsets are trivially
        // 16-byte-aligned sums.
        assert_eq!(offset_of!(CompositeParams, fog_color), 0);
        assert_eq!(offset_of!(CompositeParams, fog_params), 16);
        assert_eq!(offset_of!(CompositeParams, depth_params), 32);
        assert_eq!(offset_of!(CompositeParams, sky_zenith), 48);
        assert_eq!(offset_of!(CompositeParams, sky_horizon), 64);
        // #541 — `sky_lower` slotted between `sky_horizon` and
        // `sun_dir`. Every subsequent field shifts by 16 B; the GLSL
        // declaration in `composite.frag` is updated in lockstep.
        assert_eq!(offset_of!(CompositeParams, sky_lower), 80);
        assert_eq!(offset_of!(CompositeParams, sun_dir), 96);
        assert_eq!(offset_of!(CompositeParams, sun_color), 112);
        assert_eq!(offset_of!(CompositeParams, cloud_params), 128);
        assert_eq!(offset_of!(CompositeParams, cloud_params_1), 144);
        // M33.1 — cloud layers 2/3 inserted between cloud_params_1 and
        // camera_pos so the new vec4s slot in cleanly without disturbing
        // the trailing camera_pos + inv_view_proj layout shape.
        assert_eq!(offset_of!(CompositeParams, cloud_params_2), 160);
        assert_eq!(offset_of!(CompositeParams, cloud_params_3), 176);
        // #428 — `camera_pos` was added after `cloud_params` and before
        // `inv_view_proj`. Fixing the offset here prevents a future
        // reorder from silently corrupting the fog-distance origin.
        assert_eq!(offset_of!(CompositeParams, camera_pos), 192);
        assert_eq!(offset_of!(CompositeParams, inv_view_proj), 208);
        assert_eq!(
            size_of::<CompositeParams>(),
            208 + 64,
            "CompositeParams must be 272 bytes (13 × vec4 + mat4)"
        );
    }
}
