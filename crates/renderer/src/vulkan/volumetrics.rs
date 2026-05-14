//! Volumetric lighting pipeline (M55, Tier 8).
//!
//! Frostbite-style froxel volumetrics: a 3D texture indexed by
//! `(screenUV.x, screenUV.y, sliceZ)` where `sliceZ` is a non-linear
//! function of view-space depth (denser slices near the camera).
//! Each froxel stores RGB scattered radiance + alpha transmittance.
//! The composite fragment shader samples the integrated volume and
//! modulates the scene color: `final = scene * vol.a + vol.rgb`.
//!
//! ## Phase 1 — skeleton (this file)
//!
//! Allocates the 3D froxel images (per frame-in-flight), runs a
//! compute pass that clears them to (0 scattering, 1 transmittance).
//! No visual change yet — purpose is to validate the plumbing
//! (3D-image allocation, descriptor binding, dispatch shape, layout
//! transitions, post-dispatch barrier) without producing artifacts
//! on real content.
//!
//! ## Phase 2+ (planned, not yet implemented)
//!
//! - Density+lighting injection: read `GpuLight` SSBO, raymarch each
//!   froxel against the TLAS for shadow visibility, accumulate
//!   anisotropic single-scattering (Henyey-Greenstein phase function).
//! - Ray-march integration: marches Z from near to far, accumulating
//!   `scattering * transmittance` and updating cumulative
//!   transmittance. Writes a separate "integrated" 3D texture that
//!   composite samples.
//! - Composite modulation: replaces the existing fog mix in
//!   `composite.frag` (LIGHT-N2 site, line 346) with froxel sampling.
//! - Temporal reprojection: blends prior-frame integrated volume via
//!   `prev_view_proj`, suppresses ghosting on light-state changes.
//! - REGN-driven density per cell type.
//!
//! ## Resource layout (Phase 1)
//!
//! | Binding | Resource          | Type                            |
//! |---------|-------------------|---------------------------------|
//! | 0       | lighting froxel   | image3D (rgba16f, storage)      |
//!
//! Phase 2 extends with: TLAS, GpuLight SSBO, scene UBO (camera + jitter),
//! density UBO, integrated froxel output.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_undef_to_general, write_storage_image, write_uniform_buffer,
    DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const VOLUMETRICS_INJECT_COMP_SPV: &[u8] =
    include_bytes!("../../shaders/volumetrics_inject.comp.spv");
const VOLUMETRICS_INTEGRATE_COMP_SPV: &[u8] =
    include_bytes!("../../shaders/volumetrics_integrate.comp.spv");

/// Parameters uploaded to the volumetric injection shader as a UBO
/// each frame. Layout matches `VolumetricsParams` in
/// `volumetrics_inject.comp` — `validate_set_layout` enforces the
/// binding shape, but the std140 field layout is the host's
/// responsibility (each `vec4` is 16-byte aligned, `mat4` is
/// 4 × vec4 = 64 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VolumetricsParams {
    /// Inverse view-projection matrix; reconstructs world-space rays
    /// from screen-space (uv, NDC z = 1 = far) per froxel.
    pub inv_view_proj: [[f32; 4]; 4],
    /// xyz = camera world position (m), w = scattering coefficient
    /// (1 / m). The scattering coefficient also drives extinction in
    /// Phase 2 (single-scattering-albedo = 1).
    pub camera_pos: [f32; 4],
    /// xyz = directional light "from sun toward ground" (world space,
    /// matches the scene's directional-light convention), w = HG phase
    /// asymmetry parameter g in (-1, 1). Hazy sunlight ≈ 0.4–0.6.
    pub sun_dir: [f32; 4],
    /// rgb = sun radiance (already scaled by intensity), a = unused.
    pub sun_color: [f32; 4],
    /// x = volume far plane (m) — maps slice z = 1 to this distance
    /// along the view ray. y/z/w = unused.
    pub volume_extent: [f32; 4],
}

/// Default participating-medium scattering coefficient (1 / m). At
/// 0.005 a 200 m view distance has ~63% transmittance through the
/// fog — visible but not opaque, matches "clear haze" reference.
pub const DEFAULT_SCATTERING_COEF: f32 = 0.005;

/// Default Henyey-Greenstein asymmetry. 0.4 is mild forward
/// scattering — atmospheric haze; bumps the sun-side glow without
/// over-tinting the camera-side fog.
pub const DEFAULT_PHASE_G: f32 = 0.4;

/// Default volume extent (m). 200 m is a reasonable interior+near-
/// exterior reach; longer ranges would need exponential slice
/// distribution (Phase 5) to keep near-camera detail.
pub const DEFAULT_VOLUME_FAR: f32 = 200.0;

/// Single source of truth for whether the composite shader actually
/// consumes the integrated volumetric output. Pinned in lockstep with
/// `composite.frag`'s `combined += vol.rgb * 0.0` keep-alive at line
/// 362 — when M-LIGHT v2 lands and the per-froxel banding is fixed,
/// flip this `true` AND remove the `* 0.0` in the shader together.
///
/// While `false`, callers MUST gate `vol.dispatch()` behind this const.
/// The diagnostic that produced this gate (per-froxel single-shadow
/// ray banding on Prospector cup-and-lantern interior content) is
/// documented in commits `f62d4bd` and `33f48b5`.
///
/// **Why this is here, not in draw.rs**: keeping the flag adjacent to
/// the pipeline implementation it controls means a future contributor
/// editing `volumetrics.rs` for M-LIGHT v2 can't miss it. The
/// `composite.frag` shader can't `#include` Rust, so the lockstep is
/// documented via cross-comments rather than enforced by the compiler.
/// See #928.
pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = false;

/// Integration shader uniform — slab thickness `dt` shared across all
/// slices under linear distribution. Phase 5 will replace this with
/// an exponential per-slice `dt[]` array. std140 alignment: vec4 only.
#[repr(C)]
#[derive(Clone, Copy)]
struct IntegrationParams {
    /// x = dt (m) = `DEFAULT_VOLUME_FAR / FROXEL_DEPTH`. y/z/w = unused.
    step: [f32; 4],
}

/// Froxel volume dimensions. The (160, 90) horizontal grid follows
/// Frostbite SIGGRAPH 2015 (one froxel ≈ 12×12 pixels at 1920×1080,
/// 8×8 at 1280×720). 128 depth slices give ~5–10 m granularity in
/// the near field (where most scattering matters) under an
/// exponential slice distribution to be added in Phase 2.
///
/// Memory cost at RGBA16F: 160·90·128·8 = 14.06 MiB per slot,
/// ×2 frames-in-flight = 28.12 MiB total. Well under the per-pass
/// budget; fits in any RT-class GPU's L2.
pub const FROXEL_WIDTH: u32 = 160;
pub const FROXEL_HEIGHT: u32 = 90;
pub const FROXEL_DEPTH: u32 = 128;

/// RGB scattered radiance (HDR) + alpha transmittance. RGBA16F
/// matches Frostbite's reference layout — 8 bytes per froxel,
/// half-float precision is ample for both scattering ([0, ~10]) and
/// transmittance ([0, 1]). R11G11B10F was considered but its 10-bit
/// alpha-equivalent (the implicit 0.0 we'd reconstruct) loses the
/// transmittance channel entirely.
const FROXEL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Compute workgroup size — must match `volumetrics_clear.comp`.
const WORKGROUP_X: u32 = 8;
const WORKGROUP_Y: u32 = 8;
const WORKGROUP_Z: u32 = 8;

struct FroxelSlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct VolumetricsPipeline {
    // ── Injection pass ───────────────────────────────────────────────
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per frame-in-flight injection-pass output: per-froxel
    /// `(rgb=inscatter, a=extinction)`. Read by the integration pass.
    /// Phase 5 will additionally read the prior slot for temporal
    /// reprojection.
    lighting_volumes: Vec<FroxelSlot>,
    /// Per frame-in-flight host-mapped UBO carrying
    /// `VolumetricsParams`. Written each frame from `dispatch()`.
    param_buffers: Vec<GpuBuffer>,

    // ── Integration pass (Phase 3) ───────────────────────────────────
    integration_pipeline: vk::Pipeline,
    integration_pipeline_layout: vk::PipelineLayout,
    integration_descriptor_set_layout: vk::DescriptorSetLayout,
    integration_descriptor_pool: vk::DescriptorPool,
    integration_descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per frame-in-flight integration-pass output: per-slice
    /// cumulative `(rgb=∫inscatter, a=T_cum)`. Composite samples this
    /// once per fragment with a sampler3D.
    integrated_volumes: Vec<FroxelSlot>,
    /// Single-shot integration UBO holding `dt`. Written once in
    /// `new_inner` because dt is constant under linear slice
    /// distribution; Phase 5 will switch to per-frame exponential dt.
    integration_param_buffer: Option<GpuBuffer>,
}

impl VolumetricsPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<Self> {
        let result = Self::new_inner(device, allocator, pipeline_cache);
        if let Err(ref e) = result {
            log::debug!("Volumetrics pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<Self> {
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            lighting_volumes: Vec::new(),
            param_buffers: Vec::new(),
            integration_pipeline: vk::Pipeline::null(),
            integration_pipeline_layout: vk::PipelineLayout::null(),
            integration_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            integration_descriptor_pool: vk::DescriptorPool::null(),
            integration_descriptor_sets: Vec::new(),
            integrated_volumes: Vec::new(),
            integration_param_buffer: None,
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

        // ── 1. Allocate per-frame-in-flight froxel volumes ────────────
        // Two volumes per frame: lighting (injection output → integration
        // input) and integrated (integration output → composite read).
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_froxel_volume(
                device,
                allocator,
                &format!("volumetrics_lighting_{i}"),
            ));
            partial.lighting_volumes.push(slot);
            let integrated = try_or_cleanup!(Self::create_froxel_volume(
                device,
                allocator,
                &format!("volumetrics_integrated_{i}"),
            ));
            partial.integrated_volumes.push(integrated);
        }

        // ── 2. Per-frame parameter UBOs ───────────────────────────────
        let param_size = std::mem::size_of::<VolumetricsParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // ── 3. Descriptor set layout ──────────────────────────────────
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: scene TLAS (Phase 2c). Updated each frame via
            // `write_tlas` from draw.rs before dispatch — same flow
            // as `caustic.write_tlas` (caustic.rs:627). Used by the
            // injection shader's shadow visibility ray query.
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "volumetrics_inject.comp",
                spirv: VOLUMETRICS_INJECT_COMP_SPV,
            }],
            "volumetrics",
            &[],
        )
        .expect("volumetrics descriptor layout drifted against volumetrics_inject.comp (see #427)");
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("Volumetrics descriptor set layout")
        });

        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("Volumetrics pipeline layout")
        });

        // ── 4. Compute pipeline ───────────────────────────────────────
        let shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            VOLUMETRICS_INJECT_COMP_SPV
        ));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader_module)
            .name(c"main");
        partial.pipeline = match unsafe {
            device
                .create_compute_pipelines(
                    pipeline_cache,
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(stage)
                        .layout(partial.pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("Volumetrics clear compute pipeline")
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

        // ── 5. Descriptor pool + sets ─────────────────────────────────
        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "Volumetrics descriptor pool"));

        let layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("Volumetrics descriptor sets")
        });

        // ── 6. Write descriptor sets ──────────────────────────────────
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let lighting_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];
            let set = partial.descriptor_sets[f];
            let writes = [
                write_storage_image(set, 0, &lighting_info),
                write_uniform_buffer(set, 1, &params_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // ── 7. Integration pass UBO (single-shot, dt is constant) ─────
        let int_param_size = std::mem::size_of::<IntegrationParams>() as vk::DeviceSize;
        let mut int_param_buffer = try_or_cleanup!(GpuBuffer::create_host_visible(
            device,
            allocator,
            int_param_size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        ));
        let dt = DEFAULT_VOLUME_FAR / FROXEL_DEPTH as f32;
        let int_params = IntegrationParams {
            step: [dt, 0.0, 0.0, 0.0],
        };
        try_or_cleanup!(int_param_buffer
            .write_mapped(device, std::slice::from_ref(&int_params))
            .context("write integration params"));
        partial.integration_param_buffer = Some(int_param_buffer);

        // ── 8. Integration descriptor set layout ──────────────────────
        let int_bindings = [
            // 0: read-only injection volume
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1: write-only integrated volume
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: dt UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &int_bindings,
            &[ReflectedShader {
                name: "volumetrics_integrate.comp",
                spirv: VOLUMETRICS_INTEGRATE_COMP_SPV,
            }],
            "volumetrics_integrate",
            &[],
        )
        .expect(
            "volumetrics integration layout drifted against volumetrics_integrate.comp (see #427)",
        );
        partial.integration_descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&int_bindings),
                    None,
                )
                .context("Volumetrics integration descriptor set layout")
        });

        partial.integration_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(
                        &partial.integration_descriptor_set_layout,
                    )),
                    None,
                )
                .context("Volumetrics integration pipeline layout")
        });

        // ── 9. Integration compute pipeline ───────────────────────────
        let int_shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            VOLUMETRICS_INTEGRATE_COMP_SPV
        ));
        let int_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(int_shader_module)
            .name(c"main");
        partial.integration_pipeline = match unsafe {
            device
                .create_compute_pipelines(
                    pipeline_cache,
                    &[vk::ComputePipelineCreateInfo::default()
                        .stage(int_stage)
                        .layout(partial.integration_pipeline_layout)],
                    None,
                )
                .map_err(|(_, e)| e)
                .context("Volumetrics integration compute pipeline")
        } {
            Ok(pipelines) => {
                unsafe { device.destroy_shader_module(int_shader_module, None) };
                pipelines[0]
            }
            Err(e) => {
                unsafe { device.destroy_shader_module(int_shader_module, None) };
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // ── 10. Integration descriptor pool + sets ────────────────────
        // Pool sizes derived from `int_bindings` (#1030 / REN-D10-NEW-09).
        partial.integration_descriptor_pool =
            try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
                &int_bindings,
                MAX_FRAMES_IN_FLIGHT as u32,
            )
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
            .build(device, "Volumetrics integration descriptor pool"));

        let int_layouts = vec![partial.integration_descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        partial.integration_descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.integration_descriptor_pool)
                        .set_layouts(&int_layouts),
                )
                .context("Volumetrics integration descriptor sets")
        });

        // ── 11. Write integration descriptor sets ─────────────────────
        let int_param_buffer_handle = partial
            .integration_param_buffer
            .as_ref()
            .expect("integration param buffer was just constructed")
            .buffer;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let inj_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let int_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.integrated_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: int_param_buffer_handle,
                offset: 0,
                range: int_param_size,
            }];
            let set = partial.integration_descriptor_sets[f];
            let int_writes = [
                write_storage_image(set, 0, &inj_info),
                write_storage_image(set, 1, &int_info),
                write_uniform_buffer(set, 2, &ubo_info),
            ];
            unsafe { device.update_descriptor_sets(&int_writes, &[]) };
        }

        log::info!(
            "Volumetrics pipeline created: {}x{}x{} froxels, 2× {} MiB / slot (inject + integrated), dt={:.2}m",
            FROXEL_WIDTH,
            FROXEL_HEIGHT,
            FROXEL_DEPTH,
            (FROXEL_WIDTH as u64 * FROXEL_HEIGHT as u64 * FROXEL_DEPTH as u64 * 8) / (1024 * 1024),
            dt
        );

        Ok(partial)
    }

    fn create_froxel_volume(
        device: &ash::Device,
        allocator: &SharedAllocator,
        name: &str,
    ) -> Result<FroxelSlot> {
        let img_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_3D)
            .format(FROXEL_FORMAT)
            .extent(vk::Extent3D {
                width: FROXEL_WIDTH,
                height: FROXEL_HEIGHT,
                depth: FROXEL_DEPTH,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe {
            device
                .create_image(&img_info, None)
                .with_context(|| format!("create {name}"))?
        };

        let alloc = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name,
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(a) => a,
            Err(e) => {
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .with_context(|| format!("bind {name}"))
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let view = match unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_3D)
                        .format(FROXEL_FORMAT)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .with_context(|| format!("view {name}"))
        } {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(FroxelSlot {
            image,
            view,
            allocation: Some(alloc),
        })
    }

    /// One-time UNDEFINED → GENERAL transition for every froxel
    /// volume (both injection-output and integration-output) so the
    /// first dispatch and any subsequent sampling see a valid layout.
    /// Call once after `new()`. Mirrors `SvgfPipeline::initialize_layouts`.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT * 2);
            for slot in self
                .lighting_volumes
                .iter()
                .chain(self.integrated_volumes.iter())
            {
                barriers.push(image_barrier_undef_to_general(slot.image));
            }
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }
            Ok(())
        })
    }

    /// Dispatch both volumetric compute passes for this frame:
    ///   1. **Injection** writes per-froxel `(rgb=inscatter, a=extinction)`
    ///      from sun lighting + Henyey-Greenstein phase function into
    ///      the lighting volume.
    ///   2. **Integration** Z-scans the lighting volume per (x,y) column
    ///      and writes `(rgb=∫inscatter, a=T_cum)` into the integrated
    ///      volume — the result composite samples once per fragment.
    ///
    /// Must be called AFTER the main render pass ends (so caustic /
    /// SVGF have already scheduled their reads against the G-buffer)
    /// and BEFORE composite (so the integrated volume is ready to
    /// sample). Natural slot: between caustic and TAA in `draw.rs`.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        params: &VolumetricsParams,
    ) -> Result<()> {
        // ── Stage A: write injection-pass UBO ────────────────────────
        // The buffer is HOST_VISIBLE + HOST_COHERENT via
        // `GpuBuffer::create_host_visible`, but the execution
        // dependency (HOST → COMPUTE) is still required by the spec
        // to make the write visible to the compute stage.
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(params))?;
        let ubo_barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::HOST_WRITE)
            .dst_access_mask(vk::AccessFlags::UNIFORM_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[ubo_barrier],
            &[],
            &[],
        );

        let subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        // ── Stage B: pre-injection barrier on the lighting volume ────
        // Both volumes live in GENERAL across their lifetime (set by
        // `initialize_layouts`), so no layout transitions occur. The
        // barrier sequences last frame's integration READ of this
        // image against this frame's injection WRITE.
        let pre_inject = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.lighting_volumes[frame].image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[pre_inject],
        );

        // ── Stage C: dispatch injection ──────────────────────────────
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );
        let inj_groups_x = (FROXEL_WIDTH + WORKGROUP_X - 1) / WORKGROUP_X;
        let inj_groups_y = (FROXEL_HEIGHT + WORKGROUP_Y - 1) / WORKGROUP_Y;
        let inj_groups_z = (FROXEL_DEPTH + WORKGROUP_Z - 1) / WORKGROUP_Z;
        device.cmd_dispatch(cmd, inj_groups_x, inj_groups_y, inj_groups_z);

        // ── Stage D: barrier between injection and integration ──────
        // Sequence the injection WRITE on the lighting volume against
        // the integration READ of the same image. The integration
        // shader reads every froxel of the lighting volume; without
        // this barrier the recurrence reads stale (or partially-
        // written) data.
        let inj_to_int = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.lighting_volumes[frame].image)
            .subresource_range(subresource);
        // Plus a barrier on the integrated volume so last frame's
        // composite READ (sampler3D in fragment shader) is sequenced
        // against this frame's integration WRITE.
        let pre_int_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.integrated_volumes[frame].image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[inj_to_int, pre_int_write],
        );

        // ── Stage E: dispatch integration ────────────────────────────
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.integration_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.integration_pipeline_layout,
            0,
            &[self.integration_descriptor_sets[frame]],
            &[],
        );
        // 2D dispatch: one thread per (x, y) column; each thread Z-marches
        // all FROXEL_DEPTH slices internally.
        let int_groups_x = (FROXEL_WIDTH + WORKGROUP_X - 1) / WORKGROUP_X;
        let int_groups_y = (FROXEL_HEIGHT + WORKGROUP_Y - 1) / WORKGROUP_Y;
        device.cmd_dispatch(cmd, int_groups_x, int_groups_y, 1);

        // ── Stage F: post-integration barrier ────────────────────────
        // Make the integrated-volume WRITE visible to the composite
        // fragment shader's sampler3D READ.
        let post_int = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.integrated_volumes[frame].image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post_int],
        );

        Ok(())
    }

    /// All per-frame-in-flight integration-output views, in slot order.
    /// Composite consumes this at construction time to bind one view
    /// per frame-in-flight descriptor set. This is the volume composite
    /// SAMPLES — not the injection-output (which is internal to the
    /// volumetrics pipeline; integration consumes it).
    pub fn integrated_views(&self) -> Vec<vk::ImageView> {
        self.integrated_volumes.iter().map(|s| s.view).collect()
    }

    /// Update the injection descriptor set's binding 2 (TLAS) for
    /// `frame`. Mirrors `CausticPipeline::write_tlas` (caustic.rs:627)
    /// — the TLAS is rebuilt each frame, so this MUST be called every
    /// frame from `draw.rs` before `dispatch`. If the caller has no
    /// TLAS available for this frame (RT unsupported, scene not yet
    /// built), they should skip both `write_tlas` AND `dispatch`;
    /// composite will reuse the prior frame's integrated volume.
    pub fn write_tlas(
        &self,
        device: &ash::Device,
        frame: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame])
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for slot in self
            .lighting_volumes
            .drain(..)
            .chain(self.integrated_volumes.drain(..))
        {
            device.destroy_image_view(slot.view, None);
            device.destroy_image(slot.image, None);
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        // #732 LIFE-N1 pattern — drop the GpuBuffer structs after
        // their GPU allocations are freed so each one's
        // `Arc<Mutex<Allocator>>` clone releases now, not when
        // VulkanContext::Drop's `Arc::try_unwrap` has already given up.
        self.param_buffers.clear();
        if let Some(mut buf) = self.integration_param_buffer.take() {
            buf.destroy(device, allocator);
        }
        if self.integration_pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.integration_pipeline, None);
            self.integration_pipeline = vk::Pipeline::null();
        }
        if self.integration_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.integration_pipeline_layout, None);
            self.integration_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.integration_descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.integration_descriptor_pool, None);
            self.integration_descriptor_pool = vk::DescriptorPool::null();
        }
        if self.integration_descriptor_set_layout != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.integration_descriptor_set_layout, None);
            self.integration_descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
        if self.pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.pipeline, None);
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
    }
}

#[cfg(test)]
mod workgroup_shader_sync_tests {
    //! Drift-detection for `WORKGROUP_X` / `WORKGROUP_Y` / `WORKGROUP_Z`
    //! against the volumetrics compute shaders. `volumetrics_inject` is
    //! a full 3D dispatch (one thread per froxel); `volumetrics_integrate`
    //! is 2D (one thread per column, z-marches internally) so its shader
    //! pins `local_size_z = 1` as an intentional literal — only X/Y are
    //! checked there. See TD4-020 in
    //! `docs/audits/AUDIT_TECH_DEBT_2026-05-13.md`.
    use super::*;

    const INJECT_SRC: &str = include_str!("../../shaders/volumetrics_inject.comp");
    const INTEGRATE_SRC: &str = include_str!("../../shaders/volumetrics_integrate.comp");

    fn assert_contains(src: &str, expected: &str, shader: &str) {
        assert!(
            src.contains(expected),
            "{shader} must declare `{expected}` — bump the GLSL `layout(local_size_*)` in lockstep with the Rust const, or fold both through a build.rs codegen target (#1038).",
        );
    }

    #[test]
    fn volumetrics_inject_local_size_matches_rust_workgroup() {
        assert_contains(
            INJECT_SRC,
            &format!(
                "local_size_x = {}, local_size_y = {}, local_size_z = {}",
                WORKGROUP_X, WORKGROUP_Y, WORKGROUP_Z
            ),
            "volumetrics_inject.comp",
        );
    }

    #[test]
    fn volumetrics_integrate_local_size_xy_matches_rust_workgroup() {
        // `local_size_z = 1` is intentional (2D dispatch with internal
        // Z-march) — only X/Y are pinned here.
        assert_contains(
            INTEGRATE_SRC,
            &format!(
                "local_size_x = {}, local_size_y = {}",
                WORKGROUP_X, WORKGROUP_Y
            ),
            "volumetrics_integrate.comp",
        );
    }
}
