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
    image_barrier_undef_to_general, memory_barrier, write_combined_image_sampler,
    write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const BLOOM_DOWNSAMPLE_COMP_SPV: &[u8] = include_bytes!("../../shaders/bloom_downsample.comp.spv");
const BLOOM_UPSAMPLE_COMP_SPV: &[u8] = include_bytes!("../../shaders/bloom_upsample.comp.spv");

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

/// Default bloom intensity coefficient. 0.15 — ≈4× the Frostbite
/// SIGGRAPH 2015 default of 0.04. The 4× compensates for Bethesda
/// content being LDR-authored (emissive surfaces in 0–1 monitor
/// space rather than HDR cd/m²); at the Frostbite default bloom
/// reads as essentially-invisible on real cells. Hand-tuned down
/// from 0.20 on Prospector saloon (sun-lit windows + chandelier
/// globes had halos bleeding too far across walls); 0.15 keeps
/// emissives obviously bloomed without flooding dim surfaces.
/// Pinned in lockstep with `composite.frag`'s `BLOOM_INTENSITY`
/// constant; update both at once. The proper fix (HDR-boost
/// emissives globally) is tracked separately — see the "Color
/// Space — Not sRGB" feedback memo.
pub const DEFAULT_BLOOM_INTENSITY: f32 = 0.15;

#[repr(C)]
#[derive(Clone, Copy)]
struct DownsampleParams {
    /// xy = 1 / src_resolution, zw = 1 / dst_resolution
    inv_resolutions: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UpsampleParams {
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
}

pub struct BloomPipeline {
    downsample_pipeline: vk::Pipeline,
    upsample_pipeline: vk::Pipeline,

    downsample_pipeline_layout: vk::PipelineLayout,
    upsample_pipeline_layout: vk::PipelineLayout,

    downsample_dsl: vk::DescriptorSetLayout,
    upsample_dsl: vk::DescriptorSetLayout,

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
            downsample_pipeline_layout: vk::PipelineLayout::null(),
            upsample_pipeline_layout: vk::PipelineLayout::null(),
            downsample_dsl: vk::DescriptorSetLayout::null(),
            upsample_dsl: vk::DescriptorSetLayout::null(),
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
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // ── 1. Sampler ────────────────────────────────────────────────
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
        partial.upsample_dsl = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&up_bindings),
                    None,
                )
                .context("bloom upsample DSL")
        });

        // ── 3. Pipeline layouts ───────────────────────────────────────
        partial.downsample_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.downsample_dsl)),
                    None,
                )
                .context("bloom downsample pipeline layout")
        });
        partial.upsample_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.upsample_dsl)),
                    None,
                )
                .context("bloom upsample pipeline layout")
        });

        // ── 4. Compute pipelines ──────────────────────────────────────
        partial.downsample_pipeline = try_or_cleanup!(create_compute_pipeline(
            device,
            pipeline_cache,
            BLOOM_DOWNSAMPLE_COMP_SPV,
            partial.downsample_pipeline_layout,
            "bloom downsample",
        ));
        partial.upsample_pipeline = try_or_cleanup!(create_compute_pipeline(
            device,
            pipeline_cache,
            BLOOM_UPSAMPLE_COMP_SPV,
            partial.upsample_pipeline_layout,
            "bloom upsample",
        ));

        // ── 5. Descriptor pool ────────────────────────────────────────
        // One pool backs BLOOM_MIP_COUNT down-sets (down_bindings
        // layout) + (BLOOM_MIP_COUNT-1) up-sets (up_bindings layout)
        // per frame-in-flight. Pool sizes derived from each layout's
        // bindings via `add_layout_bindings` (#1030 / REN-D10-NEW-09).
        let down_sets = MAX_FRAMES_IN_FLIGHT * BLOOM_MIP_COUNT;
        let up_sets = MAX_FRAMES_IN_FLIGHT * (BLOOM_MIP_COUNT - 1);
        let total_sets = down_sets + up_sets;
        partial.descriptor_pool =
            try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
                &down_bindings,
                down_sets as u32,
            )
            .add_layout_bindings(&up_bindings, up_sets as u32)
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
                partial.sampler,
                frame_idx,
            ));
            partial.frames.push(frame);
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

    /// Run the bloom pyramid for `frame`. Reads `input_view` (the
    /// scene HDR — typically composite's HDR view) and writes through
    /// the down chain then up chain, leaving the bloom result in
    /// `output_view(frame)`.
    ///
    /// The scene HDR view must already be in
    /// `SHADER_READ_ONLY_OPTIMAL`; the post-render-pass `final_layout`
    /// transition handles that for the composite HDR.
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
        // construction.
        let f = &mut self.frames[frame];

        let scene_info = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(input_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let scene_write = write_combined_image_sampler(f.down_descriptor_sets[0], 0, &scene_info);
        device.update_descriptor_sets(&[scene_write], &[]);

        // Compute and upload the per-mip params for both chains.
        // dt is constant under linear distribution (matches the
        // injection / integration shaders' `dt` shape — no per-frame
        // change), but the resolutions differ per mip.
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
            // up_mips[i] = upsample(smaller) + same.
            // smaller = up_mips[i+1] for i < N-2; for i == N-2,
            // smaller = down_mips[N-1] (the seed of the up chain).
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

        // HOST → COMPUTE_SHADER (UBO flush before dispatch).
        memory_barrier(
            device, cmd,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::UNIFORM_READ,
        );

        let subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

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
            let groups_x = (extent.width + WORKGROUP_X - 1) / WORKGROUP_X;
            let groups_y = (extent.height + WORKGROUP_Y - 1) / WORKGROUP_Y;
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
            let groups_x = (extent.width + WORKGROUP_X - 1) / WORKGROUP_X;
            let groups_y = (extent.height + WORKGROUP_Y - 1) / WORKGROUP_Y;
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);

            let post = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(f.up_mips[i].image)
                .subresource_range(subresource);
            // Last upsample (i==0) feeds composite's fragment read,
            // so dst stage includes FRAGMENT_SHADER.
            let dst_stage = if i == 0 {
                vk::PipelineStageFlags::FRAGMENT_SHADER
            } else {
                vk::PipelineStageFlags::COMPUTE_SHADER
            };
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

    /// View into the bloom result for this frame — sampled by
    /// composite via `combined += vol_inscatter + bloom * intensity`
    /// in HDR-linear space before the ACES tone-map.
    pub fn output_view(&self, frame: usize) -> vk::ImageView {
        self.frames[frame].up_mips[0].view
    }

    pub fn output_views(&self) -> Vec<vk::ImageView> {
        self.frames.iter().map(|f| f.up_mips[0].view).collect()
    }

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
        if self.downsample_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.downsample_pipeline_layout, None);
            self.downsample_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.upsample_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.upsample_pipeline_layout, None);
            self.upsample_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.downsample_dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.downsample_dsl, None);
            self.downsample_dsl = vk::DescriptorSetLayout::null();
        }
        if self.upsample_dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.upsample_dsl, None);
            self.upsample_dsl = vk::DescriptorSetLayout::null();
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
        for i in 0..(BLOOM_MIP_COUNT - 1) {
            let mip = create_mip(
                device,
                allocator,
                down_mips[i].extent,
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
        let down_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&down_layouts),
                )
                .context("bloom down descriptor sets")?
        };
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
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        Ok(Self {
            down_mips,
            up_mips,
            down_descriptor_sets,
            up_descriptor_sets,
            down_param_buffers,
            up_param_buffers,
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
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(BLOOM_FORMAT)
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
    Ok(BloomMip {
        image,
        view,
        extent,
        allocation: Some(alloc),
    })
}

fn create_compute_pipeline(
    device: &ash::Device,
    pipeline_cache: vk::PipelineCache,
    spv: &[u8],
    layout: vk::PipelineLayout,
    name: &str,
) -> Result<vk::Pipeline> {
    let shader_module = super::pipeline::load_shader_module(device, spv)?;
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(shader_module)
        .name(c"main");
    let result = unsafe {
        device
            .create_compute_pipelines(
                pipeline_cache,
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(stage)
                    .layout(layout)],
                None,
            )
            .map_err(|(_, e)| e)
            .with_context(|| format!("{name} compute pipeline"))
    };
    let pipeline = match result {
        Ok(pipelines) => {
            unsafe { device.destroy_shader_module(shader_module, None) };
            pipelines[0]
        }
        Err(e) => {
            unsafe { device.destroy_shader_module(shader_module, None) };
            return Err(e);
        }
    };
    Ok(pipeline)
}

// Bloom workgroup drift tests moved to shader_constants::tests after #1038
// folded all shared constants into the build.rs codegen path. Canonical checks:
//   shader_constants::tests::affected_shaders_include_constants_header
//   shader_constants::tests::generated_header_contains_all_defines
