//! Bloom pyramid pipeline (M58, Tier 8).
//!
//! Produces a blurred-bright-content texture that composite adds to
//! the scene HDR before tone-mapping. The single biggest "softness"
//! lever — bright emissives stop ending at their texel edges and
//! start spilling outward like real-world bright surfaces do.
//!
//! ## Architecture
//!
//! Two halves, both written as separable compute passes:
//!
//! 1. **Down-pyramid** — `BLOOM_MIP_COUNT` levels, each half the
//!    resolution of the previous. `down_mips[0]` is half-screen,
//!    `down_mips[N-1]` is the smallest. Uses a 4-tap bilinear box
//!    filter (`bloom_downsample.comp`).
//! 2. **Up-pyramid** — `BLOOM_MIP_COUNT - 1` levels. `up_mips[N-2]`
//!    is fed by `down_mips[N-1]` upsampled, summed with
//!    `down_mips[N-2]`. Each subsequent level reads the larger one
//!    above and the same-resolution down-mip, sums them. Final
//!    `up_mips[0]` is what composite samples
//!    (`bloom_upsample.comp`).
//!
//! ## Filter choice (and what we deliberately did NOT do)
//!
//! Plain box filters for both passes. Jimenez's 13-tap downsample +
//! 9-tap tent upsample (CoD: AW, SIGGRAPH 2014) is the better
//! reference, but its specific tap weights need to be lifted from
//! the talk slides verbatim — "make up reasonable weights" violates
//! the project's no-guessing rule. Box filter is mathematically
//! unambiguous, lands ~80% of the visual win, and can be upgraded
//! later when the reference is open. Tracked in M58 row of
//! ROADMAP.md.
//!
//! ## Resource layout
//!
//! ```text
//! frame F:
//!   down_mips[0..N]      (RGB11_F11_B10F, halving each level)
//!   up_mips[0..N-1]      (same format, same levels as down except top)
//!   sampler              (LINEAR, CLAMP_TO_EDGE)
//!   N down descriptor sets (input view rewritten per frame for
//!                           level 0 only — levels 1..N read previous
//!                           down level which is owned here)
//!   N-1 up descriptor sets
//! ```
//!
//! Both pyramids live in `VK_IMAGE_LAYOUT_GENERAL` for their entire
//! lifetime — same pattern as SVGF / volumetrics, simpler than
//! flipping layouts between passes.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_general_to_shader_read, image_barrier_shader_read_to_general,
    image_barrier_undef_to_general, write_combined_image_sampler, write_storage_image,
    write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const BLOOM_DOWNSAMPLE_COMP_SPV: &[u8] = include_bytes!("../../shaders/bloom_downsample.comp.spv");
const BLOOM_UPSAMPLE_COMP_SPV: &[u8] = include_bytes!("../../shaders/bloom_upsample.comp.spv");
/// #2796 / REN-D16-01 — reads composite's own assembled scene back as a
/// storage image and writes bloom into it in place. See `apply_to_scene`.
const BLOOM_APPLY_COMP_SPV: &[u8] = include_bytes!("../../shaders/bloom_apply.comp.spv");

/// Number of mip levels in the down-pyramid. The up-pyramid has
/// `BLOOM_MIP_COUNT - 1` levels (the smallest mip is the seed for
/// the upsample chain, not regenerated). 5 levels covers a 32×
/// blur at the final mip on a 1280×720 input — enough for soft glow
/// without losing too much fidelity.
pub const BLOOM_MIP_COUNT: usize = 5;

/// RGB11_G11_B10F — half the bandwidth of RGBA16F, alpha not used
/// for bloom. Same format SVGF uses for indirect history (see
/// `svgf.rs`'s `INDIRECT_HIST_FORMAT`) — universally supported as
/// a storage image format on RT-class GPUs (#275).
const BLOOM_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;

/// Default bloom intensity coefficient. 0.15 is a hand-tuned perceptual
/// constant — NOT tuned to cancel the upsample pyramid's own ~5× DC gain
/// (composes with it instead; see `shader_constants_data.rs` for the full
/// derivation and the effective ~0.75×, ~19×-Frostbite-reference math).
/// Chosen so Bethesda's LDR-authored emissive content (0–1 monitor-space
/// range, not HDR cd/m²) reads as obviously bloomed without flooding dim
/// surfaces; hand-tuned down from 0.20 on Prospector saloon (sun-lit
/// windows + chandelier globes had halos bleeding too far across walls).
/// Source of truth lives in `crate::shader_constants::BLOOM_INTENSITY`;
/// `build.rs` emits the matching `#define BLOOM_INTENSITY` into
/// `composite.frag`. The proper fix (HDR-boost emissives globally) is
/// tracked separately — see the "Color Space — Not sRGB" feedback memo.
pub const DEFAULT_BLOOM_INTENSITY: f32 = crate::shader_constants::BLOOM_INTENSITY;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct DownsampleParams {
    /// xy = 1 / src_resolution, zw = 1 / dst_resolution
    inv_resolutions: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct UpsampleParams {
    /// xy = 1 / smaller_resolution, zw = 1 / dst_resolution
    inv_resolutions: [f32; 4],
}

struct BloomMip {
    image: vk::Image,
    view: vk::ImageView,
    extent: vk::Extent2D,
    allocation: Option<vk_alloc::Allocation>,
}

struct BloomFrame {
    down_mips: Vec<BloomMip>, // BLOOM_MIP_COUNT
    up_mips: Vec<BloomMip>,   // BLOOM_MIP_COUNT - 1

    /// One per `down_mips[i]`. Binding 0 of `down_descriptor_sets[0]`
    /// is rewritten each frame from `dispatch()` because its source
    /// (the scene HDR view) is owned externally and not stable
    /// across pipeline construction.
    down_descriptor_sets: Vec<vk::DescriptorSet>, // BLOOM_MIP_COUNT
    up_descriptor_sets: Vec<vk::DescriptorSet>, // BLOOM_MIP_COUNT - 1

    down_param_buffers: Vec<GpuBuffer>, // BLOOM_MIP_COUNT
    up_param_buffers: Vec<GpuBuffer>,   // BLOOM_MIP_COUNT - 1

    /// #2796 / REN-D16-01 — one set for the bloom-apply step. Binding 0
    /// (the scene storage image) is rewritten each frame from
    /// `apply_to_scene`, same reason `down_descriptor_sets[0]` binding 0
    /// is: the scene image is owned externally (by `CompositePipeline`)
    /// and not stable across `BloomPipeline` construction. Binding 1
    /// (this pipeline's own final up_mips[0] view) is written once below,
    /// same as every other internally-owned binding.
    apply_descriptor_set: vk::DescriptorSet,
}

pub struct BloomPipeline {
    downsample_pipeline: vk::Pipeline,
    upsample_pipeline: vk::Pipeline,
    /// #2796 / REN-D16-01 — see `apply_to_scene`.
    apply_pipeline: vk::Pipeline,

    downsample_pipeline_layout: vk::PipelineLayout,
    upsample_pipeline_layout: vk::PipelineLayout,
    apply_pipeline_layout: vk::PipelineLayout,

    downsample_dsl: vk::DescriptorSetLayout,
    upsample_dsl: vk::DescriptorSetLayout,
    apply_dsl: vk::DescriptorSetLayout,

    descriptor_pool: vk::DescriptorPool,

    /// LINEAR + CLAMP_TO_EDGE — bloom relies on bilinear filtering
    /// for the cheap 4-tap box filters. No mipmap (we manage mips
    /// explicitly as separate images).
    sampler: vk::Sampler,

    frames: Vec<BloomFrame>,

    /// Top-level (mip 0) extent — tracked here so resize logic can
    /// know what we were sized for.
    pub extent: vk::Extent2D,
}

impl BloomPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        screen_extent: vk::Extent2D,
    ) -> Result<Self> {
        let result = Self::new_inner(device, allocator, pipeline_cache, screen_extent);
        if let Err(ref e) = result {
            log::debug!("Bloom pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        screen_extent: vk::Extent2D,
    ) -> Result<Self> {
        let mut partial = Self {
            downsample_pipeline: vk::Pipeline::null(),
            upsample_pipeline: vk::Pipeline::null(),
            apply_pipeline: vk::Pipeline::null(),
            downsample_pipeline_layout: vk::PipelineLayout::null(),
            upsample_pipeline_layout: vk::PipelineLayout::null(),
            apply_pipeline_layout: vk::PipelineLayout::null(),
            downsample_dsl: vk::DescriptorSetLayout::null(),
            upsample_dsl: vk::DescriptorSetLayout::null(),
            apply_dsl: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            sampler: vk::Sampler::null(),
            frames: Vec::new(),
            extent: screen_extent,
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `destroy` is an unsafe fn; on this error path
                        // the device is still live and no bloom command buffers
                        // are in flight (we are mid-construction), so tearing
                        // down the partially-built pipeline is sound.
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // ── 1. Sampler ────────────────────────────────────────────────
        // SAFETY: trivial ash create call; `device` is live and the
        // SamplerCreateInfo is fully initialized for the duration of the call.
        partial.sampler = try_or_cleanup!(unsafe {
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
                .context("bloom sampler")
        });

        // ── 2. Descriptor set layouts ─────────────────────────────────
        // Downsample: 0=src sampler, 1=dst storage, 2=params UBO.
        let down_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &down_bindings,
            &[ReflectedShader {
                name: "bloom_downsample.comp",
                spirv: BLOOM_DOWNSAMPLE_COMP_SPV,
            }],
            "bloom_downsample",
            &[],
        )
        .expect("bloom downsample layout drifted against bloom_downsample.comp (see #427)");
        // SAFETY: trivial ash create call; `device` is live and `down_bindings`
        // outlives the create info borrowed by this call.
        partial.downsample_dsl = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&down_bindings),
                    None,
                )
                .context("bloom downsample DSL")
        });

        // Upsample: 0=smaller sampler, 1=same sampler, 2=dst storage,
        // 3=params UBO.
        let up_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &up_bindings,
            &[ReflectedShader {
                name: "bloom_upsample.comp",
                spirv: BLOOM_UPSAMPLE_COMP_SPV,
            }],
            "bloom_upsample",
            &[],
        )
        .expect("bloom upsample layout drifted against bloom_upsample.comp (see #427)");
        // SAFETY: trivial ash create call; `device` is live and `up_bindings`
        // outlives the create info borrowed by this call.
        partial.upsample_dsl = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&up_bindings),
                    None,
                )
                .context("bloom upsample DSL")
        });

        // #2796 / REN-D16-01 — apply: 0=scene storage (read-modify-write),
        // 1=bloom output sampler.
        let apply_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &apply_bindings,
            &[ReflectedShader {
                name: "bloom_apply.comp",
                spirv: BLOOM_APPLY_COMP_SPV,
            }],
            "bloom_apply",
            &[],
        )
        .expect("bloom apply layout drifted against bloom_apply.comp (see #427)");
        // SAFETY: trivial ash create call; `device` is live and `apply_bindings`
        // outlives the create info borrowed by this call.
        partial.apply_dsl = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&apply_bindings),
                    None,
                )
                .context("bloom apply DSL")
        });

        // ── 3. Pipeline layouts ───────────────────────────────────────
        // SAFETY: trivial ash create call; `device` is live and
        // `partial.downsample_dsl` was just created by us and is still live.
        partial.downsample_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.downsample_dsl)),
                    None,
                )
                .context("bloom downsample pipeline layout")
        });
        // SAFETY: trivial ash create call; `device` is live and
        // `partial.upsample_dsl` was just created by us and is still live.
        partial.upsample_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.upsample_dsl)),
                    None,
                )
                .context("bloom upsample pipeline layout")
        });
        // SAFETY: trivial ash create call; `device` is live and
        // `partial.apply_dsl` was just created by us and is still live.
        partial.apply_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.apply_dsl)),
                    None,
                )
                .context("bloom apply pipeline layout")
        });

        // ── 4. Compute pipelines ──────────────────────────────────────
        partial.downsample_pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            BLOOM_DOWNSAMPLE_COMP_SPV,
            partial.downsample_pipeline_layout,
            "bloom downsample",
        ));
        partial.upsample_pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            BLOOM_UPSAMPLE_COMP_SPV,
            partial.upsample_pipeline_layout,
            "bloom upsample",
        ));
        partial.apply_pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            BLOOM_APPLY_COMP_SPV,
            partial.apply_pipeline_layout,
            "bloom apply",
        ));

        // ── 5. Descriptor pool ────────────────────────────────────────
        // One pool backs BLOOM_MIP_COUNT down-sets (down_bindings
        // layout) + (BLOOM_MIP_COUNT-1) up-sets (up_bindings layout) +
        // 1 apply-set (apply_bindings layout, #2796 / REN-D16-01) per
        // frame-in-flight. Pool sizes derived from each layout's
        // bindings via `add_layout_bindings` (#1030 / REN-D10-NEW-09).
        let down_sets = MAX_FRAMES_IN_FLIGHT * BLOOM_MIP_COUNT;
        let up_sets = MAX_FRAMES_IN_FLIGHT * (BLOOM_MIP_COUNT - 1);
        let apply_sets = MAX_FRAMES_IN_FLIGHT;
        let total_sets = down_sets + up_sets + apply_sets;
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &down_bindings,
            down_sets as u32,
        )
        .add_layout_bindings(&up_bindings, up_sets as u32)
        .add_layout_bindings(&apply_bindings, apply_sets as u32)
        .max_sets(total_sets as u32)
        .build(device, "bloom descriptor pool"));

        // ── 6. Per-frame mip pyramids + descriptor sets ───────────────
        for frame_idx in 0..MAX_FRAMES_IN_FLIGHT {
            let frame = try_or_cleanup!(BloomFrame::new(
                device,
                allocator,
                screen_extent,
                partial.descriptor_pool,
                partial.downsample_dsl,
                partial.upsample_dsl,
                partial.apply_dsl,
                partial.sampler,
                frame_idx,
            ));
            partial.frames.push(frame);
        }

        // #2037 / GPU-D5-01 — every down/upsample param UBO is a pure
        // function of `screen_extent` (via `partial.extent` and each
        // mip's own fixed extent), so it only needs writing once here
        // rather than every frame in `draw_frame`. A resize tears down
        // and rebuilds the whole `BloomPipeline` (see
        // `context/resize.rs`), which re-enters this constructor, so
        // this single write-at-construction covers both the initial
        // build and every resize — there is no separate "on resize"
        // path to keep in sync.
        for frame_idx in 0..MAX_FRAMES_IN_FLIGHT {
            try_or_cleanup!(partial.upload_params(device, frame_idx));
        }

        log::info!(
            "Bloom pipeline created: {} mip levels (down 0..{}, up 0..{}), top extent {}x{}",
            BLOOM_MIP_COUNT,
            BLOOM_MIP_COUNT - 1,
            BLOOM_MIP_COUNT - 2,
            partial.frames[0].down_mips[0].extent.width,
            partial.frames[0].down_mips[0].extent.height,
        );
        Ok(partial)
    }

    /// One-time UNDEFINED → GENERAL layout transition for every mip
    /// in every frame slot. Call once after `new()`. Same shape as
    /// `SvgfPipeline::initialize_layouts`.
    ///
    /// # Safety
    ///
    /// Caller must ensure all passed Vulkan handles (`device`, `cmd`) are
    /// valid and live, `cmd` is in the recording state, the device is not
    /// lost, and the bloom images are not concurrently accessed by another
    /// command buffer.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let total = MAX_FRAMES_IN_FLIGHT * (BLOOM_MIP_COUNT + (BLOOM_MIP_COUNT - 1));
            let mut barriers = Vec::with_capacity(total);
            for frame in &self.frames {
                for mip in frame.down_mips.iter().chain(frame.up_mips.iter()) {
                    barriers.push(image_barrier_undef_to_general(mip.image));
                }
            }
            // NONE as srcStageMask: UNDEFINED → GENERAL on the bloom
            // pyramid mips has no prior writes to expose; NONE is the
            // Vulkan 1.3 idiom post-#949 / #1100 / #1122.
            // SAFETY: `cmd` is the recording one-time command buffer supplied
            // by `with_one_time_commands`; every barrier image is a bloom mip
            // created above and not yet accessed, so the UNDEFINED->GENERAL
            // transition with srcStage=NONE has no prior writes to make visible
            // and no concurrent use.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
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

    /// Run the bloom pyramid for `frame`. Reads `input_view` (the
    /// scene HDR — typically composite's HDR view) and writes through
    /// the down chain then up chain, leaving the bloom result in
    /// `output_view(frame)`.
    ///
    /// The scene HDR view must already be in
    /// `SHADER_READ_ONLY_OPTIMAL`; the post-render-pass `final_layout`
    /// transition handles that for the composite HDR.
    /// Write per-mip UBOs into the mapped host-visible param buffers.
    ///
    /// #2037 / GPU-D5-01 — every down/upsample param is a pure function
    /// of `self.extent` and the (construction-time-fixed) per-mip
    /// extents, so this is called once per frame-in-flight from
    /// `new_inner` rather than every `draw_frame` call. A resize
    /// rebuilds the whole `BloomPipeline`, which re-enters `new_inner`
    /// and so re-runs this write — there is no separate steady-state
    /// caller. The descriptor update for `input_view` (HDR scene
    /// output) still happens inside `dispatch` because it depends on
    /// the render pass, unlike these UBOs.
    pub fn upload_params(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        let f = &mut self.frames[frame];
        for i in 0..BLOOM_MIP_COUNT {
            let src_extent = if i == 0 {
                self.extent
            } else {
                f.down_mips[i - 1].extent
            };
            let dst_extent = f.down_mips[i].extent;
            let p = DownsampleParams {
                inv_resolutions: [
                    1.0 / src_extent.width as f32,
                    1.0 / src_extent.height as f32,
                    1.0 / dst_extent.width as f32,
                    1.0 / dst_extent.height as f32,
                ],
            };
            f.down_param_buffers[i].write_mapped(device, std::slice::from_ref(&p))?;
        }
        for i in 0..(BLOOM_MIP_COUNT - 1) {
            let smaller_extent = if i + 1 < BLOOM_MIP_COUNT - 1 {
                f.up_mips[i + 1].extent
            } else {
                f.down_mips[BLOOM_MIP_COUNT - 1].extent
            };
            let dst_extent = f.up_mips[i].extent;
            let p = UpsampleParams {
                inv_resolutions: [
                    1.0 / smaller_extent.width as f32,
                    1.0 / smaller_extent.height as f32,
                    1.0 / dst_extent.width as f32,
                    1.0 / dst_extent.height as f32,
                ],
            };
            f.up_param_buffers[i].write_mapped(device, std::slice::from_ref(&p))?;
        }
        Ok(())
    }

    /// Dispatch bloom. [`Self::upload_params`] wrote the param UBOs once
    /// at construction (#2037 / GPU-D5-01) — no per-frame upload needed
    /// before this call.
    ///
    /// # Safety
    ///
    /// Caller must ensure all passed Vulkan handles (`device`, `cmd`) are
    /// valid and live, `cmd` is in the recording state, the device is not
    /// lost, and the bloom images/buffers are not concurrently accessed by
    /// another in-flight command buffer.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        input_view: vk::ImageView,
    ) -> Result<()> {
        // Rewrite binding 0 of the very first down set to point at
        // this frame's scene HDR. All other down sets (1..N) point
        // at our own internally-owned mip views, written once at
        // construction. This descriptor update depends on render-pass
        // output so it cannot move to the pre-barrier upload phase.
        let f = &mut self.frames[frame];

        let scene_info = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(input_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let scene_write = write_combined_image_sampler(f.down_descriptor_sets[0], 0, &scene_info);
        device.update_descriptor_sets(&[scene_write], &[]);

        let subresource = super::descriptors::color_subresource_single_mip();

        // ── Downsample chain ─────────────────────────────────────
        //
        // Barrier accounting (#931): only emit the *post*-barrier
        // on the mip just written. Pre-barriers on each iteration's
        // destination would be redundant — each `BloomFrame` slot
        // owns its own mip allocations (no cross-frame WAR hazard
        // beyond the per-frame fence, which sequences submissions),
        // and within this command buffer no in-frame access has
        // touched `mip[i]` before iteration i writes it. The prior
        // iteration's post-barrier (srcStage=COMPUTE →
        // dstStage=COMPUTE) acts as an execution barrier for the
        // next dispatch on its way to writing a *different* mip —
        // no memory dependency to publish since we're overwriting.
        //
        // Result: 19 → 10 barriers/frame. Audit's "~3 barriers"
        // target requires single-pass FidelityFX SPD (workgroup
        // atomic counters + LDS, several-hundred-LOC shader
        // rewrite), not a barrier coalesce.
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.downsample_pipeline,
        );
        for i in 0..BLOOM_MIP_COUNT {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.downsample_pipeline_layout,
                0,
                &[f.down_descriptor_sets[i]],
                &[],
            );
            let extent = f.down_mips[i].extent;
            let groups_x = extent.width.div_ceil(WORKGROUP_X);
            let groups_y = extent.height.div_ceil(WORKGROUP_Y);
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);

            // Make this mip's WRITE visible to the next iteration's
            // READ (downsample reads previous mip; upsample reads
            // same-resolution mip).
            let post = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(f.down_mips[i].image)
                .subresource_range(subresource);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[post],
            );
        }

        // ── Upsample chain ───────────────────────────────────────
        // Same barrier accounting as the down chain — see the
        // comment above. Per-frame `up_mips[i]` is exclusive to
        // this frame slot, so the only in-frame producer of its
        // contents is *this* iteration; no pre-barrier needed.
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.upsample_pipeline);
        // Iterate from N-2 down to 0 — each level reads the
        // previously-produced level (up_mips[i+1] or down_mips[N-1]
        // for the seed) and the same-resolution down_mip.
        for i in (0..(BLOOM_MIP_COUNT - 1)).rev() {
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.upsample_pipeline_layout,
                0,
                &[f.up_descriptor_sets[i]],
                &[],
            );
            let extent = f.up_mips[i].extent;
            let groups_x = extent.width.div_ceil(WORKGROUP_X);
            let groups_y = extent.height.div_ceil(WORKGROUP_Y);
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);

            let post = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(f.up_mips[i].image)
                .subresource_range(subresource);
            // #2796 / REN-D16-01 — mip 0's consumer used to be composite's
            // fragment read; it's now `apply_to_scene`'s compute dispatch
            // (composite no longer samples bloom directly), so every level
            // — including i==0 — only needs COMPUTE_SHADER visibility.
            let dst_stage = vk::PipelineStageFlags::COMPUTE_SHADER;
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[post],
            );
        }

        Ok(())
    }

    /// View into the bloom result for this frame. #2796 / REN-D16-01 —
    /// consumed internally by `apply_to_scene`'s own descriptor set
    /// (`combined_scene += bloom * BLOOM_INTENSITY`, same HDR-linear,
    /// pre-ACES math composite.frag used to do directly); no longer
    /// sampled by composite itself.
    pub fn output_view(&self, frame: usize) -> vk::ImageView {
        self.frames[frame].up_mips[0].view
    }

    pub fn output_views(&self) -> Vec<vk::ImageView> {
        self.frames.iter().map(|f| f.up_mips[0].view).collect()
    }

    /// Add this frame's already-built bloom pyramid onto `scene_view` in
    /// place — `scene_view + bloom * BLOOM_INTENSITY`, written back to the
    /// same image. #2796 / REN-D16-01.
    ///
    /// Must run AFTER [`Self::dispatch`] has built the pyramid for this
    /// frame from the SAME scene image (so the bloom pyramid and the
    /// value being added to are the same frame's content), and the scene
    /// image must currently be `SHADER_READ_ONLY_OPTIMAL` — composite's
    /// render pass `final_layout` contract, unchanged by `dispatch`
    /// sampling it. Leaves the scene image `SHADER_READ_ONLY_OPTIMAL`
    /// again on return, restoring `frame_upscaler.rs`'s documented
    /// "composition's output layout" entry contract.
    ///
    /// # Safety
    /// Caller must ensure `device` and `cmd` are valid and live, `cmd` is
    /// recording outside a render pass, the device is not lost, `scene_image`
    /// / `scene_view` are the exact pair `composite.scene_images[frame]` /
    /// `composite.scene_image_views[frame]` and are not concurrently
    /// accessed by another in-flight command buffer, and that image is
    /// currently `SHADER_READ_ONLY_OPTIMAL` (its state immediately after
    /// `record_composite_pass` + this frame's `dispatch` call, before
    /// `record_upscale_pass` runs).
    pub unsafe fn apply_to_scene(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        scene_image: vk::Image,
        scene_view: vk::ImageView,
    ) {
        let f = &mut self.frames[frame];

        // Rewrite binding 0 (the scene storage image) — externally owned,
        // not stable across `BloomPipeline` construction. Same reasoning
        // as `dispatch`'s per-frame rewrite of `down_descriptor_sets[0]`
        // binding 0.
        let scene_info = [vk::DescriptorImageInfo::default()
            .image_view(scene_view)
            .image_layout(vk::ImageLayout::GENERAL)];
        let scene_write = write_storage_image(f.apply_descriptor_set, 0, &scene_info);
        // SAFETY: `f.apply_descriptor_set` was allocated at construction and
        // is not in use by any pending command buffer at this point in the
        // frame (the caller's contract).
        unsafe { device.update_descriptor_sets(&[scene_write], &[]) };

        // SHADER_READ_ONLY_OPTIMAL -> GENERAL: `dispatch` above sampled
        // `scene_view` (COMPUTE_SHADER read) after composite's render pass
        // wrote it (COLOR_ATTACHMENT_OUTPUT); this transition's source
        // scope must cover both, mirroring `frame_upscaler.rs::
        // record_native_blit`'s identical three-stage source mask for the
        // same image's *next* transition out of SHADER_READ_ONLY_OPTIMAL.
        let to_general = image_barrier_shader_read_to_general(scene_image);
        // SAFETY: `cmd` is recording outside a render pass (caller's
        // contract); `scene_image` is live and currently
        // SHADER_READ_ONLY_OPTIMAL (caller's contract).
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_general],
            );
        }

        // SAFETY: `cmd` is recording; `self.apply_pipeline` and
        // `f.apply_descriptor_set` are live for this frame; `scene_image` is
        // now GENERAL (barrier above) and matches `imageSize`/coordinates the
        // shader assumes (same render-resolution extent bloom's own mips
        // were built against).
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.apply_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.apply_pipeline_layout,
                0,
                &[f.apply_descriptor_set],
                &[],
            );
            let groups_x = self.extent.width.div_ceil(WORKGROUP_X);
            let groups_y = self.extent.height.div_ceil(WORKGROUP_Y);
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);
        }

        // GENERAL -> SHADER_READ_ONLY_OPTIMAL: restore the layout
        // `frame_upscaler.rs` requires on entry. dst_access covers both
        // known downstream consumption patterns of `scene_color`
        // (`record_native_blit`'s TRANSFER_READ blit source and the FSR
        // SDK's own SHADER_READ sampling) since either may run next
        // depending on the active upscaler.
        let to_shader_read = image_barrier_general_to_shader_read(scene_image)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_READ);
        // SAFETY: `cmd` is recording outside a render pass; `scene_image` is
        // live and was just transitioned to GENERAL and written by the
        // dispatch above.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_shader_read],
            );
        }
    }

    /// Destroy all bloom images, views, and allocations.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that none of the bloom resources are still in
    /// use by an in-flight command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut frame in self.frames.drain(..) {
            for mip in frame.down_mips.drain(..).chain(frame.up_mips.drain(..)) {
                device.destroy_image_view(mip.view, None);
                device.destroy_image(mip.image, None);
                if let Some(a) = mip.allocation {
                    allocator.lock().expect("allocator lock").free(a).ok();
                }
            }
            for buf in frame
                .down_param_buffers
                .iter_mut()
                .chain(frame.up_param_buffers.iter_mut())
            {
                buf.destroy(device, allocator);
            }
            // #732 LIFE-N1 — drop GpuBuffer structs after their
            // GPU allocations are freed.
            frame.down_param_buffers.clear();
            frame.up_param_buffers.clear();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.downsample_pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.downsample_pipeline, None);
            self.downsample_pipeline = vk::Pipeline::null();
        }
        if self.upsample_pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.upsample_pipeline, None);
            self.upsample_pipeline = vk::Pipeline::null();
        }
        if self.apply_pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.apply_pipeline, None);
            self.apply_pipeline = vk::Pipeline::null();
        }
        if self.downsample_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.downsample_pipeline_layout, None);
            self.downsample_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.upsample_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.upsample_pipeline_layout, None);
            self.upsample_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.apply_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.apply_pipeline_layout, None);
            self.apply_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.downsample_dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.downsample_dsl, None);
            self.downsample_dsl = vk::DescriptorSetLayout::null();
        }
        if self.upsample_dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.upsample_dsl, None);
            self.upsample_dsl = vk::DescriptorSetLayout::null();
        }
        if self.apply_dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.apply_dsl, None);
            self.apply_dsl = vk::DescriptorSetLayout::null();
        }
        if self.sampler != vk::Sampler::null() {
            device.destroy_sampler(self.sampler, None);
            self.sampler = vk::Sampler::null();
        }
    }
}

impl BloomFrame {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        screen_extent: vk::Extent2D,
        descriptor_pool: vk::DescriptorPool,
        down_dsl: vk::DescriptorSetLayout,
        up_dsl: vk::DescriptorSetLayout,
        apply_dsl: vk::DescriptorSetLayout,
        sampler: vk::Sampler,
        frame_idx: usize,
    ) -> Result<Self> {
        // Down mip extents: half-size per level, starting from
        // half-screen (mip 0 of the bloom pyramid is half the screen
        // resolution — Frostbite convention).
        let mut down_mips: Vec<BloomMip> = Vec::with_capacity(BLOOM_MIP_COUNT);
        let mut prev_extent = vk::Extent2D {
            width: (screen_extent.width / 2).max(1),
            height: (screen_extent.height / 2).max(1),
        };
        for i in 0..BLOOM_MIP_COUNT {
            let mip = create_mip(
                device,
                allocator,
                prev_extent,
                &format!("bloom_down_f{frame_idx}_mip{i}"),
            )?;
            down_mips.push(mip);
            prev_extent = vk::Extent2D {
                width: (prev_extent.width / 2).max(1),
                height: (prev_extent.height / 2).max(1),
            };
        }

        // Up mips share extents with down_mips[0..N-1] (no top mip
        // since we seed the up chain from down_mips[N-1] directly).
        let mut up_mips: Vec<BloomMip> = Vec::with_capacity(BLOOM_MIP_COUNT - 1);
        for (i, down) in down_mips.iter().take(BLOOM_MIP_COUNT - 1).enumerate() {
            let mip = create_mip(
                device,
                allocator,
                down.extent,
                &format!("bloom_up_f{frame_idx}_mip{i}"),
            )?;
            up_mips.push(mip);
        }

        // Per-mip param UBOs.
        let down_param_size = std::mem::size_of::<DownsampleParams>() as vk::DeviceSize;
        let up_param_size = std::mem::size_of::<UpsampleParams>() as vk::DeviceSize;
        let mut down_param_buffers = Vec::with_capacity(BLOOM_MIP_COUNT);
        for _ in 0..BLOOM_MIP_COUNT {
            down_param_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                down_param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
        }
        let mut up_param_buffers = Vec::with_capacity(BLOOM_MIP_COUNT - 1);
        for _ in 0..(BLOOM_MIP_COUNT - 1) {
            up_param_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                up_param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?);
        }

        // Allocate descriptor sets.
        let down_layouts = vec![down_dsl; BLOOM_MIP_COUNT];
        let up_layouts = vec![up_dsl; BLOOM_MIP_COUNT - 1];
        // SAFETY: trivial ash call; `device` and `descriptor_pool` are live and
        // `down_layouts` (BLOOM_MIP_COUNT copies of `down_dsl`) outlives the
        // call.
        let down_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&down_layouts),
                )
                .context("bloom down descriptor sets")?
        };
        // SAFETY: trivial ash call; `device` and `descriptor_pool` are live and
        // `up_layouts` (BLOOM_MIP_COUNT-1 copies of `up_dsl`) outlives the
        // call.
        let up_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&up_layouts),
                )
                .context("bloom up descriptor sets")?
        };

        // Write the static descriptor bindings (everything except
        // down[0] binding 0, which is the scene HDR — written each
        // frame from `dispatch`).
        for i in 0..BLOOM_MIP_COUNT {
            let dst_info = [vk::DescriptorImageInfo::default()
                .image_view(down_mips[i].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: down_param_buffers[i].buffer,
                offset: 0,
                range: down_param_size,
            }];
            // Binding 0 (src sampler) — skip for i == 0; written
            // per-frame from `dispatch()` when the scene HDR view is
            // known. For i >= 1, src is down_mips[i-1].
            let mut writes: Vec<vk::WriteDescriptorSet> = Vec::with_capacity(3);
            let src_info = if i >= 1 {
                Some([vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(down_mips[i - 1].view)
                    .image_layout(vk::ImageLayout::GENERAL)])
            } else {
                None
            };
            if let Some(ref src) = src_info {
                writes.push(write_combined_image_sampler(
                    down_descriptor_sets[i],
                    0,
                    src.as_slice(),
                ));
            }
            writes.push(write_storage_image(down_descriptor_sets[i], 1, &dst_info));
            writes.push(write_uniform_buffer(down_descriptor_sets[i], 2, &ubo_info));
            // SAFETY: the written down set, its mip views, and param buffer are
            // all freshly created and not yet bound by any in-flight frame.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // Up sets: each level reads the smaller upsample result
        // (or down_mips[N-1] for the top of the up chain) and the
        // same-resolution down_mip.
        for i in 0..(BLOOM_MIP_COUNT - 1) {
            let smaller_view = if i + 1 < BLOOM_MIP_COUNT - 1 {
                up_mips[i + 1].view
            } else {
                down_mips[BLOOM_MIP_COUNT - 1].view
            };
            let smaller_info = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(smaller_view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let same_info = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(down_mips[i].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let dst_info = [vk::DescriptorImageInfo::default()
                .image_view(up_mips[i].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: up_param_buffers[i].buffer,
                offset: 0,
                range: up_param_size,
            }];
            let writes = [
                write_combined_image_sampler(up_descriptor_sets[i], 0, &smaller_info),
                write_combined_image_sampler(up_descriptor_sets[i], 1, &same_info),
                write_storage_image(up_descriptor_sets[i], 2, &dst_info),
                write_uniform_buffer(up_descriptor_sets[i], 3, &ubo_info),
            ];
            // SAFETY: the written up set, its mip/down views, and param buffer
            // are all freshly created and not yet bound by any in-flight frame.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // #2796 / REN-D16-01 — apply set. Binding 0 (scene storage image)
        // is externally owned (CompositePipeline) and rewritten per-frame
        // from `apply_to_scene`, same reason down[0]'s binding 0 is left
        // unwritten here. Binding 1 (this pyramid's own final up_mips[0]
        // result) is internally owned and stable, written once now.
        let apply_layouts = [apply_dsl];
        // SAFETY: trivial ash call; `device` and `descriptor_pool` are live and
        // `apply_layouts` outlives the call.
        let apply_descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&apply_layouts),
                )
                .context("bloom apply descriptor set")?[0]
        };
        let bloom_result_info = [vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(up_mips[0].view)
            .image_layout(vk::ImageLayout::GENERAL)];
        let apply_write = write_combined_image_sampler(apply_descriptor_set, 1, &bloom_result_info);
        // SAFETY: the written set and `up_mips[0]`'s view are both freshly
        // created and not yet bound by any in-flight frame.
        unsafe { device.update_descriptor_sets(&[apply_write], &[]) };

        Ok(Self {
            down_mips,
            up_mips,
            down_descriptor_sets,
            up_descriptor_sets,
            down_param_buffers,
            up_param_buffers,
            apply_descriptor_set,
        })
    }
}

fn create_mip(
    device: &ash::Device,
    allocator: &SharedAllocator,
    extent: vk::Extent2D,
    name: &str,
) -> Result<BloomMip> {
    let img_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(BLOOM_FORMAT)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    // SAFETY: trivial ash create call; `device` is live and `img_info` outlives
    // the borrow held during the create.
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
            // SAFETY: trivial ash get call; `image` was just created by us
            // above and is still live; `device` outlives this call.
            requirements: unsafe { device.get_image_memory_requirements(image) },
            location: gpu_allocator::MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
        })
        .with_context(|| format!("allocate {name}"))
    {
        Ok(a) => a,
        Err(e) => {
            // SAFETY: `image` was created by us just above and not yet
            // destroyed; device is live and the image was never submitted to
            // any queue.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }
    };
    // SAFETY: trivial ash bind call; `image` and `alloc`'s memory were both
    // created by us above and are still live; `device` outlives the call.
    if let Err(e) = unsafe {
        device
            .bind_image_memory(image, alloc.memory(), alloc.offset())
            .with_context(|| format!("bind {name}"))
    } {
        allocator.lock().expect("allocator lock").free(alloc).ok();
        // SAFETY: `image` was created by us above and not yet destroyed (its
        // allocation was just freed); device is live, image never submitted.
        unsafe { device.destroy_image(image, None) };
        return Err(e);
    }
    // SAFETY: trivial ash create call; `image` was just created and bound by us
    // and is still live; `device` outlives the borrow during create.
    let view = match unsafe {
        device
            .create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(BLOOM_FORMAT)
                    .subresource_range(super::descriptors::color_subresource_single_mip()),
                None,
            )
            .with_context(|| format!("view {name}"))
    } {
        Ok(v) => v,
        Err(e) => {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: `image` was created by us above and not yet destroyed
            // (its allocation was just freed); device is live, image never
            // submitted.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }
    };
    Ok(BloomMip {
        image,
        view,
        extent,
        allocation: Some(alloc),
    })
}

// Bloom's compute-pipeline builder was promoted to
// `pipeline::create_compute_pipeline` under #1751 (TD2-002) and is now shared
// with ssao / volumetrics / skin_compute.

// Bloom workgroup drift tests moved to shader_constants::tests after #1038
// folded all shared constants into the build.rs codegen path. Canonical checks:
//   shader_constants::tests::affected_shaders_include_constants_header
//   shader_constants::tests::generated_header_contains_all_defines

#[cfg(test)]
mod construction_invariant_upload_tests {
    //! Regression for #2037 / GPU-D5-01 — `upload_params` rewrote all 9
    //! down/upsample param UBOs every frame even though they're a pure
    //! function of `self.extent`, fixed at construction. `BloomPipeline`
    //! needs a live Vulkan device to exercise end-to-end, so this pins
    //! the fix at the source level — the same pattern already used by
    //! `light_overflow_tests` for another Vulkan-untestable invariant.

    /// `new_inner` must call `upload_params` once per frame-in-flight so
    /// every UBO is populated before the first `dispatch`, without
    /// `draw_frame` having to call it again every frame.
    #[test]
    fn new_inner_writes_params_once_per_frame_in_flight() {
        let src = include_str!("bloom.rs");
        let new_inner_start = src
            .find("fn new_inner(")
            .expect("new_inner must exist in bloom.rs");
        let upload_params_start = src
            .find("pub fn upload_params(")
            .expect("upload_params must exist in bloom.rs");
        assert!(
            new_inner_start < upload_params_start,
            "test assumes new_inner is declared before upload_params in bloom.rs"
        );
        let new_inner_body = &src[new_inner_start..upload_params_start];

        assert!(
            new_inner_body.contains("partial.upload_params(device, frame_idx)"),
            "new_inner must write every frame-in-flight's param UBOs once at \
             construction (#2037 / GPU-D5-01) — mirroring the per-frame-in-flight \
             loop already used for `BloomFrame::new`"
        );
    }

    /// `draw_frame`'s per-frame UBO section (composite/SVGF/TAA) must
    /// NOT call `bloom.upload_params` — the whole point of #2037 is that
    /// bloom's param UBOs are written once at construction, not every
    /// frame like its siblings.
    #[test]
    fn draw_frame_does_not_re_upload_bloom_params_every_frame() {
        let src = include_str!("context/draw.rs");
        assert!(
            !src.contains("bloom.upload_params"),
            "draw_frame must not call bloom.upload_params per-frame — bloom's \
             param UBOs are construction-invariant and written once in \
             BloomPipeline::new_inner (#2037 / GPU-D5-01). A per-frame call here \
             would silently reintroduce the redundant rewrite."
        );
    }
}
