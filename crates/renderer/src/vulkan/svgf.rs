//! SVGF (spatiotemporal variance-guided filtering) denoiser for 1-SPP GI.
//!
//! Phase 3 only implements the temporal accumulation pass: reproject the
//! previous frame's accumulated indirect via motion vectors, test
//! consistency against the previous frame's mesh_id, and blend with the
//! current 1-SPP indirect at α=0.2. The moments image tracks luminance
//! μ₁/μ₂ for Phase 4's variance estimation.
//!
//! ## Resource layout
//!
//! - **indirect_history[frame]** (RGBA16F, STORAGE|SAMPLED, per frame-in-flight):
//!   the accumulated demodulated indirect light. Read as history by the
//!   NEXT frame, written by the CURRENT frame, sampled by composite.
//! - **moments_history[frame]** (RGBA16F, STORAGE|SAMPLED, per frame-in-flight):
//!   rg = running (μ₁, μ₂) luminance moments, b = history length.
//!
//! Both live in VK_IMAGE_LAYOUT_GENERAL throughout their lifetime — simpler
//! than flipping layouts between passes, and sampling in GENERAL is legal
//! per the Vulkan spec for combined image samplers.
//!
//! ## Ping-pong scheme
//!
//! With MAX_FRAMES_IN_FLIGHT = 2, each frame slot owns its own pair of
//! history images. Frame N writes to slot N and reads slot (N+1) mod 2 as
//! its history input. Submission ordering + in-flight fence guarantee that
//! the read target was fully written by the previous frame before the new
//! dispatch begins.
//!
//! ## Descriptor set (binding layout)
//!
//! | Binding | Resource            | Type                       |
//! |---------|---------------------|----------------------------|
//! | 0       | curr raw indirect   | sampler2D (G-buffer)       |
//! | 1       | motion vector       | sampler2D (G-buffer)       |
//! | 2       | curr mesh_id        | usampler2D (G-buffer)      |
//! | 3       | prev mesh_id        | usampler2D (G-buffer prev) |
//! | 4       | prev indirect hist  | sampler2D (this module)    |
//! | 5       | prev moments hist   | sampler2D (this module)    |
//! | 6       | out indirect        | image2D (rgba16f, storage) |
//! | 7       | out moments         | image2D (rgba16f, storage) |
//! | 8       | SvgfTemporalParams  | uniform buffer             |
//! | 9       | curr normal         | sampler2D (RG16_SNORM oct) |
//! | 10      | prev normal         | sampler2D (RG16_SNORM oct) |
//!
//! Bindings 9/10 land #650 / SH-5 — the 2×2 bilinear consistency loop
//! also rejects taps whose previous-frame normal disagrees with the
//! current-frame normal by more than ~25°. mesh_id alone wasn't enough
//! to catch same-mesh disocclusions on long static walls (camera orbit
//! revealing a previously self-occluded part of the same mesh would
//! pick up the wrong tap and ghost-streak the lighting integration).
//! Schied 2017 §4.2 specifies depth + normal rejection in addition
//! to mesh_id; this lands the normal half of that.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_undef_to_general, write_combined_image_sampler, write_storage_image,
    write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

// #918 / REN-D10-NEW-04 — SVGF's read-previous / write-current
// ping-pong (`prev = (f + 1) % MAX_FRAMES_IN_FLIGHT`) silently aliases
// to the same slot if the constant is ever lowered to 1
// (single-frame-in-flight CPU-bound mode). Compile-time gate so a
// future sync-tier change that touches the constant fails the build
// here rather than producing a degenerate history-recovery boundary
// at runtime.
const _: () = assert!(
    MAX_FRAMES_IN_FLIGHT >= 2,
    "SVGF ping-pong arithmetic requires MAX_FRAMES_IN_FLIGHT >= 2 — \
     lowering it aliases the read-previous and write-current slots to the same index"
);

const SVGF_TEMPORAL_COMP_SPV: &[u8] = include_bytes!("../../shaders/svgf_temporal.comp.spv");

/// Accumulated indirect light format. R11G11B10F saves 50% vs RGBA16F
/// (4B vs 8B/pixel). Alpha is always 1.0 and never read. Storage image
/// support for R11G11B10 is required on all desktop GPUs since 2014
/// (Maxwell/GCN/Gen9). See #275.
const INDIRECT_HIST_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// Moments format (μ1, μ2, history_length, unused). Kept as RGBA16F for
/// precision — luminance² values up to 100+ need 10+ bit mantissa.
const MOMENTS_HIST_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Steady-state SVGF temporal blend α (Schied 2017 §4 floor). One-fifth
/// weight on the current frame, four-fifths on the temporally-clamped
/// history. Per-pixel age modulation in-shader recovers from
/// reset / disocclusion automatically; the host-side α drives the
/// coarse-grained recovery window. See #674.
pub const SVGF_ALPHA_STEADY_STATE: f32 = 0.2;
/// Elevated α used during a discontinuity-recovery window (cell load,
/// weather flip, fast camera turn). Half weight on the current frame
/// for `svgf_recovery_frames` upcoming frames.
pub const SVGF_ALPHA_RECOVERY: f32 = 0.5;

/// Pure-fn state machine for the SVGF temporal-α recovery window.
/// Returns `(alpha_color, alpha_moments, next_recovery_frames)`.
/// Extracted from the dispatch site so it can be unit-tested without
/// a Vulkan device. See #674 / DEN-4.
pub fn next_svgf_temporal_alpha(recovery_frames: u32) -> (f32, f32, u32) {
    if recovery_frames > 0 {
        (
            SVGF_ALPHA_RECOVERY,
            SVGF_ALPHA_RECOVERY,
            recovery_frames - 1,
        )
    } else {
        (SVGF_ALPHA_STEADY_STATE, SVGF_ALPHA_STEADY_STATE, 0)
    }
}

/// Should the temporal pass force a full history reset on this frame?
///
/// `true` when the SVGF state has fewer than `MAX_FRAMES_IN_FLIGHT`
/// successful dispatches under its belt — either because the slot
/// pair was just created, or because `recreate_on_resize` zeroed
/// `frames_since_creation` and the new G-buffer / history images
/// haven't been written enough times to host a useful prior. The
/// dispatch maps the result onto `SvgfTemporalParams.params.z`
/// (`1.0 = reset history`, see `svgf_temporal.comp:81`); the shader
/// short-circuits the bilinear-tap reprojection and writes the
/// current frame's indirect+moments without any history blend.
///
/// Pinned as a pure helper (#648 / RP-2) so a future change to the
/// MAX_FRAMES_IN_FLIGHT boundary or to the resize-zero policy
/// surfaces as a unit-test failure. Pre-#648 the audit flagged
/// "G-buffer images sampled by SVGF temporal before any color write
/// on first 2-3 frames after resize" — the existing
/// `frames_since_creation` reset path already addresses it; this
/// extraction is the regression guard the audit asked for.
pub(super) fn should_force_history_reset(frames_since_creation: u32) -> bool {
    frames_since_creation < MAX_FRAMES_IN_FLIGHT as u32
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SvgfTemporalParams {
    /// xy = screen size (pixels), zw = 1/screen size.
    pub screen: [f32; 4],
    /// x = α color blend, y = α moments blend, z = first_frame flag
    /// (1.0 = reset history), w = unused.
    pub params: [f32; 4],
}

struct HistorySlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

impl Drop for HistorySlot {
    /// Safety net mirroring `Attachment::Drop` in `gbuffer.rs` and
    /// `GpuBuffer::Drop` (#656). `HistorySlot` doesn't stash device
    /// or allocator handles internally — the parent
    /// `SvgfPipeline::destroy` passes them in — so this Drop can't
    /// clean up; it can only scream so a leak surfaces in tests +
    /// release-log error stream.
    ///
    /// Gate on `allocation.is_some()` because the canonical destroy
    /// path moves slots out of the parent Vec via `drain(..)`, calls
    /// `destroy_image*` on the bare Vulkan handles, and consumes
    /// `allocation` via `if let Some(a) = slot.allocation`. The
    /// `vk::Image` / `vk::ImageView` handles stay non-null on the
    /// dropped slot (Vulkan handles are integers; their value
    /// doesn't change post-destroy), so checking those would
    /// false-positive on every clean shutdown. The
    /// `gpu_allocator::Allocation` is the load-bearing leak
    /// indicator — its `Drop` is what releases the slab, and the
    /// canonical path consumes it before the slot's Drop fires.
    /// See REN-D2-NEW-01 (audit 2026-05-09).
    fn drop(&mut self) {
        if self.allocation.is_none() {
            return;
        }
        log::error!(
            "HistorySlot leaked into Drop: image={:?} view={:?} \
             — SvgfPipeline::destroy(device, allocator) was not \
             called and the gpu_allocator slab will leak. See REN-D2-NEW-01.",
            self.image,
            self.view,
        );
        debug_assert!(false, "HistorySlot dropped without destroy()");
    }
}

pub struct SvgfPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    shader_module: vk::ShaderModule,

    indirect_history: Vec<HistorySlot>,
    moments_history: Vec<HistorySlot>,

    /// Linear sampler used to sample history + raw indirect (for fetches).
    /// All lookups in the shader are texelFetch so filtering does not
    /// matter, but we still need a sampler object.
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    /// True until the first frame has written both slots — forces the
    /// shader to take the "no history" branch and reset moments.
    frames_since_creation: u32,
    /// Set to `true` in [`Self::dispatch`] after the compute dispatch
    /// has been recorded into the per-frame command buffer; consumed +
    /// cleared by [`Self::mark_frame_completed`] after the host-side
    /// `queue_submit` returns success. Pre-#917 the
    /// `frames_since_creation` counter advanced at dispatch-record time,
    /// meaning a record-time-failure or submit-time-failure path would
    /// leave the counter advanced without any corresponding GPU-side
    /// history write — the next frame would then think a valid history
    /// existed and skip the force-reset gate. The two-step recording-
    /// then-completion handshake gates the counter advance on submit
    /// success. See #917 / REN-D10-NEW-03.
    dispatched_this_frame: bool,
}

impl SvgfPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        normal_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(raw_indirect_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(motion_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(normal_views.len(), MAX_FRAMES_IN_FLIGHT);

        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            raw_indirect_views,
            motion_views,
            mesh_id_views,
            normal_views,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("SVGF pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        normal_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            shader_module: vk::ShaderModule::null(),
            indirect_history: Vec::new(),
            moments_history: Vec::new(),
            point_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
            frames_since_creation: 0,
            dispatched_this_frame: false,
        };

        macro_rules! try_or_cleanup {
            // SAFETY (inside macro): `partial` is local to this fn and
            // not yet referenced by any command buffer / descriptor set;
            // cleanup-on-error closes the partial state before returning.
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

        // ── 1. Allocate history images (per frame-in-flight) ──────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let ind = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                INDIRECT_HIST_FORMAT,
                &format!("svgf_indirect_{i}"),
            ));
            partial.indirect_history.push(ind);
            let mom = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                MOMENTS_HIST_FORMAT,
                &format!("svgf_moments_{i}"),
            ));
            partial.moments_history.push(mom);
        }

        // ── 2. Sampler ────────────────────────────────────────────────
        // SAFETY: SamplerCreateInfo fully populated above; handle owned
        // by `partial.point_sampler`, freed by destroy().
        partial.point_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("SVGF point sampler")
        });

        // ── 3. Parameter UBOs ─────────────────────────────────────────
        let param_size = std::mem::size_of::<SvgfTemporalParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        // ── 4. Descriptor set layout ──────────────────────────────────
        let bindings = [
            // 0: curr indirect (sampler2D)
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1: motion
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: curr mesh_id (usampler2D)
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3: prev mesh_id
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 4: prev indirect history
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 5: prev moments history
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6: out indirect
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7: out moments
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 8: params UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 9: curr normal (sampler2D, octahedral RG16_SNORM) — #650
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 10: prev normal (other frame-in-flight slot of the same
            // G-buffer normal attachment) — #650
            vk::DescriptorSetLayoutBinding::default()
                .binding(10)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "svgf_temporal.comp",
                spirv: SVGF_TEMPORAL_COMP_SPV,
            }],
            "svgf",
            &[],
        )
        .expect("svgf descriptor layout drifted against svgf_temporal.comp (see #427)");
        // SAFETY: `bindings` validated against svgf_temporal.comp above.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("SVGF descriptor set layout")
        });

        // SAFETY: descriptor_set_layout just created above; slice-from-ref
        // borrow lives only for this call.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("SVGF pipeline layout")
        });

        // ── 5. Compute pipeline ───────────────────────────────────────
        partial.shader_module = try_or_cleanup!(super::pipeline::load_shader_module(
            device,
            SVGF_TEMPORAL_COMP_SPV
        ));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(partial.shader_module)
            .name(c"main");
        // SAFETY: `stage` references `partial.shader_module` (loaded above)
        // and `partial.pipeline_layout` (just created above). pipeline_cache
        // is caller-provided (may be null per spec).
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
                .context("SVGF temporal compute pipeline")
        } {
            Ok(p) => p[0],
            Err(e) => {
                // SAFETY: cleanup-on-error. shader_module survives on
                // `partial.shader_module` and is freed by destroy.
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // ── 6. Descriptor pool + sets ─────────────────────────────────
        // 8 sampler bindings per set after #650: curr indirect, motion,
        // curr/prev mesh_id, prev indirect/moments history, curr/prev
        // normal.
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::new()
            .pool(
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                (MAX_FRAMES_IN_FLIGHT * 8) as u32,
            )
            .pool(
                vk::DescriptorType::STORAGE_IMAGE,
                (MAX_FRAMES_IN_FLIGHT * 2) as u32,
            )
            .pool(
                vk::DescriptorType::UNIFORM_BUFFER,
                MAX_FRAMES_IN_FLIGHT as u32,
            )
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
            .build(device, "SVGF descriptor pool"));

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: pool just sized for MAX_FRAMES_IN_FLIGHT sets with the
        // same descriptor_set_layout.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("SVGF descriptor sets")
        });

        // ── 7. Write descriptor sets ──────────────────────────────────
        partial.write_descriptor_sets(
            device,
            raw_indirect_views,
            motion_views,
            mesh_id_views,
            normal_views,
        );

        log::info!("SVGF pipeline created: {}x{}", width, height);
        Ok(partial)
    }

    fn create_history_image(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        format: vk::Format,
        name: &str,
    ) -> Result<HistorySlot> {
        let img_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: `img_info` fully populated above (TYPE_2D, history
        // format, STORAGE | SAMPLED usage). Bubbling `?` on Err means
        // no further allocation runs.
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
                // SAFETY: `image` just created above.
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: cleanup-on-error — `image` was created but
                // never bound; no other reference exists.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        // SAFETY: `image` matches the memory requirements that produced
        // `alloc`; bound once per image.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .with_context(|| format!("bind {name}"))
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: bind failed; free the alloc first, then destroy
            // the unbound image.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        // SAFETY: `image` is bound (line above); view owned by the
        // returned HistorySlot on Ok.
        let view = match unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
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
                // SAFETY: view creation failed; free alloc first then
                // destroy the bound image.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(HistorySlot {
            image,
            view,
            allocation: Some(alloc),
        })
    }

    fn write_descriptor_sets(
        &self,
        device: &ash::Device,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        normal_views: &[vk::ImageView],
    ) {
        let param_size = std::mem::size_of::<SvgfTemporalParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let prev = (f + 1) % MAX_FRAMES_IN_FLIGHT;

            let curr_indirect = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(raw_indirect_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let motion = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(motion_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let curr_mid = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let prev_mid = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[prev])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            // #650 / SH-5 — curr / prev normal taps for the bilinear
            // consistency loop. Same ping-pong source as mesh_id.
            let curr_norm = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(normal_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let prev_norm = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(normal_views[prev])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let prev_ind = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(self.indirect_history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let prev_mom = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(self.moments_history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_ind = [vk::DescriptorImageInfo::default()
                .image_view(self.indirect_history[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_mom = [vk::DescriptorImageInfo::default()
                .image_view(self.moments_history[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];

            let set = self.descriptor_sets[f];
            let writes = [
                write_combined_image_sampler(set, 0, &curr_indirect),
                write_combined_image_sampler(set, 1, &motion),
                write_combined_image_sampler(set, 2, &curr_mid),
                write_combined_image_sampler(set, 3, &prev_mid),
                write_combined_image_sampler(set, 4, &prev_ind),
                write_combined_image_sampler(set, 5, &prev_mom),
                write_storage_image(set, 6, &out_ind),
                write_storage_image(set, 7, &out_mom),
                write_uniform_buffer(set, 8, &params),
                write_combined_image_sampler(set, 9, &curr_norm),
                write_combined_image_sampler(set, 10, &prev_norm),
            ];
            // SAFETY: descriptor sets owned by `self`; writes reference
            // image views and param buffer owned by `self`, and the
            // caller-borrowed G-buffer views (live for this call's duration).
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// View for the accumulated indirect light (this frame's output), used
    /// by the composite pass to sample the denoised result.
    pub fn indirect_view(&self, frame: usize) -> vk::ImageView {
        self.indirect_history[frame].view
    }

    /// One-time layout transition UNDEFINED → GENERAL for every history
    /// image, so the first dispatch and first descriptor sampling see a
    /// valid layout. Call once after `new()`.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT * 2);
            for slot in self
                .indirect_history
                .iter()
                .chain(self.moments_history.iter())
            {
                barriers.push(image_barrier_undef_to_general(slot.image));
            }
            // SAFETY: caller of `initialize_layouts` (unsafe fn)
            // guarantees device/queue/pool validity; `cmd` is the
            // recording buffer from `with_one_time_commands`. Each
            // barrier targets a history image we own.
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

    /// Upload the per-frame temporal params UBO (host write only).
    ///
    /// Must be called BEFORE the pre-render-pass bulk HOST→{VS|FS|COMPUTE}
    /// barrier in `draw_frame`; that barrier covers the host-write →
    /// uniform-read execution dependency for this UBO so [`Self::dispatch`]
    /// no longer needs to emit its own. Mirrors the composite-UBO fold
    /// landed in #909 / REN-D1-NEW-03. See #961 / REN-D10-NEW-04.
    pub unsafe fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        alpha_color: f32,
        alpha_moments: f32,
    ) -> Result<()> {
        // #648 / RP-2 — force a full history reset for the first
        // `MAX_FRAMES_IN_FLIGHT` frames after creation or
        // `recreate_on_resize`. Pre-#648 the audit was concerned the
        // freshly-allocated G-buffer attachments would feed garbage
        // memory into SVGF's prev-frame mesh-id / motion taps; the
        // history-reset gate (params.z >= 0.5 in the shader) is what
        // protects against that. See `should_force_history_reset`'s
        // doc for the cross-link.
        let first_frame = if should_force_history_reset(self.frames_since_creation) {
            1.0
        } else {
            0.0
        };
        let params = SvgfTemporalParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            // Temporal blend α — caller-controlled. Schied 2017 §4
            // recommends 0.2 as the steady-state floor, with per-pixel
            // age-modulated recovery already in-shader. Bumping the
            // host-side α (e.g. 0.5) for a few frames after a
            // discontinuity (cell load, weather flip, fast camera
            // turn) gives a coarse-grained recovery the per-pixel
            // weights complement. See #674 / DEN-4.
            params: [alpha_color, alpha_moments, first_frame, 0.0],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))
    }

    /// Dispatch the temporal accumulation compute shader.
    ///
    /// Must be called AFTER the main render pass ends (raw_indirect, motion,
    /// mesh_id are in SHADER_READ_ONLY_OPTIMAL via render pass final_layout)
    /// and BEFORE the composite pass (which samples `indirect_view(frame)`).
    /// [`Self::upload_params`] must have been called this frame BEFORE the
    /// pre-render-pass bulk HOST→{VS|FS|COMPUTE} barrier in `draw_frame`;
    /// that barrier covers the UBO host-write → uniform-read execution
    /// dependency so this method no longer emits its own (#961 /
    /// REN-D10-NEW-04, mirror of #909 / REN-D1-NEW-03).
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) -> Result<()> {
        // Barrier: the previous use of this frame's OUT slots (writes in
        // the previous use of this frame-in-flight index, at least two
        // frames ago) finished long before — the both-slots
        // `wait_for_fences` at `draw.rs:170-181` (#282) guarantees the
        // prior-frame COMPUTE write AND the prior-frame composite
        // FRAGMENT read of `indirect_view(frame)` have retired. We
        // still emit an execution dependency on SHADER_WRITE so
        // descriptor sampling of the same slot in the previous frame is
        // ordered correctly under any future relaxation of the fence
        // wait.
        //
        // #962 / REN-D10-NEW-05 — audit flagged the `FRAGMENT_SHADER`
        // src bit as over-specified: composite's FRAGMENT consumer of
        // this slot is already serialised by the both-slots fence wait
        // above, so the bit is redundant under today's pin. The
        // narrowing is deferred to a RenderDoc-validated session per
        // the speculative-Vulkan-fix policy — cosmetic over-sync is
        // strictly safer than a missed access-mask dep that only some
        // IHV drivers flag. Sibling barriers in `taa.rs:789`,
        // `caustic.rs:816`, `volumetrics.rs:846` follow the same
        // defensive pattern and would warrant their own re-audits if
        // this site is ever narrowed.
        let out_ind_img = self.indirect_history[frame].image;
        let out_mom_img = self.moments_history[frame].image;
        let img_barrier = |img: vk::Image| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(img)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let img_barriers = [img_barrier(out_ind_img), img_barrier(out_mom_img)];
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &img_barriers,
        );

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );

        let gx = (self.width + 7) / 8;
        let gy = (self.height + 7) / 8;
        device.cmd_dispatch(cmd, gx, gy, 1);

        // Barrier: compute write → composite's fragment shader sampling.
        // Keeps layout in GENERAL — composite's descriptor sets use GENERAL.
        let out_barrier = |img: vk::Image| {
            vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(img)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let out_barriers = [out_barrier(out_ind_img), out_barrier(out_mom_img)];
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            // dst = FRAGMENT for composite's read this frame + COMPUTE for
            // next frame's SVGF dispatch reading this slot as
            // `prev_indirect_hist`. The per-frame fence implicitly serialises
            // both consumers today, but if MAX_FRAMES_IN_FLIGHT goes past 2 or
            // a future timeline-semaphore refactor relaxes the fence wait,
            // missing the COMPUTE bit would become a real hazard. #653.
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &out_barriers,
        );

        // #917 / REN-D10-NEW-03 — dispatch was successfully recorded
        // into `cmd`. `mark_frame_completed` will bump
        // `frames_since_creation` after `queue_submit` returns success;
        // a record-time error before this point leaves the flag false
        // and the counter doesn't advance.
        self.dispatched_this_frame = true;
        Ok(())
    }

    /// Mark the previous frame's dispatch as having reached
    /// queue-submit-success. Advances `frames_since_creation` iff a
    /// dispatch was actually recorded this frame (the
    /// `dispatched_this_frame` flag set by [`Self::dispatch`]). Called
    /// from `draw_frame` after `queue_submit` returns Ok; the gate
    /// guarantees no advance on a skipped / failed dispatch. See #917 /
    /// REN-D10-NEW-03.
    pub fn mark_frame_completed(&mut self) {
        if self.dispatched_this_frame {
            self.frames_since_creation = self.frames_since_creation.saturating_add(1);
            self.dispatched_this_frame = false;
        }
    }

    /// Recreate history images at a new extent after a swapchain resize.
    /// Caller must supply the G-buffer's new views (which they just
    /// recreated via `GBuffer::recreate_on_resize`).
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        raw_indirect_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        normal_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        for mut slot in self.indirect_history.drain(..) {
            // SAFETY: `recreate_on_resize` runs from the fenced
            // swapchain-resize path (`VulkanContext::recreate_swapchain`
            // waits both frames-in-flight first). History image / view
            // handles are unreferenced by any in-flight command.
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation.take() {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for mut slot in self.moments_history.drain(..) {
            // SAFETY: same fenced-resize contract as the indirect loop above.
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation.take() {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }

        self.width = width;
        self.height = height;
        self.frames_since_creation = 0; // history is meaningless after resize
                                        // Drop any pending mark — pre-resize-recorded dispatch (if any)
                                        // never sees `queue_submit` because the resize path waited on
                                        // both fences. See #917.
        self.dispatched_this_frame = false;

        let result = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.indirect_history.push(Self::create_history_image(
                    device,
                    allocator,
                    width,
                    height,
                    INDIRECT_HIST_FORMAT,
                    &format!("svgf_indirect_{i}"),
                )?);
                self.moments_history.push(Self::create_history_image(
                    device,
                    allocator,
                    width,
                    height,
                    MOMENTS_HIST_FORMAT,
                    &format!("svgf_moments_{i}"),
                )?);
            }
            Ok(())
        })();
        if let Err(ref e) = result {
            log::error!("SVGF recreate partial failure: {e} — destroying partial state");
            // SAFETY: fenced-resize path; partial state is unreferenced.
            unsafe { self.destroy(device, allocator) };
            return result;
        }

        self.write_descriptor_sets(
            device,
            raw_indirect_views,
            motion_views,
            mesh_id_views,
            normal_views,
        );
        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY (whole function): caller of `destroy` (unsafe fn)
        // guarantees no in-flight command buffer references any object
        // owned by `self`. The per-handle `if != null()` guards make this
        // safe to call on partially-initialised state from the
        // `try_or_cleanup` path. Each per-handle destroy below shares
        // this contract.
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.pipeline != vk::Pipeline::null() {
            unsafe { device.destroy_pipeline(self.pipeline, None) };
        }
        if self.shader_module != vk::ShaderModule::null() {
            unsafe { device.destroy_shader_module(self.shader_module, None) };
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
        if self.point_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.point_sampler, None) };
        }
        for mut slot in self.indirect_history.drain(..) {
            // SAFETY: `recreate_on_resize` runs from the fenced
            // swapchain-resize path (`VulkanContext::recreate_swapchain`
            // waits both frames-in-flight first). History image / view
            // handles are unreferenced by any in-flight command.
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation.take() {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for mut slot in self.moments_history.drain(..) {
            // SAFETY: same fenced-resize contract as the indirect loop above.
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation.take() {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #674 — at the steady state (no recent discontinuity), the
    /// temporal-α floor is 0.2 for both color and moments. Counter
    /// stays at 0; subsequent calls keep the floor.
    #[test]
    fn steady_state_alpha_is_schied_floor() {
        let (a_color, a_moments, next) = next_svgf_temporal_alpha(0);
        assert!((a_color - SVGF_ALPHA_STEADY_STATE).abs() < 1e-6);
        assert!((a_moments - SVGF_ALPHA_STEADY_STATE).abs() < 1e-6);
        assert_eq!(next, 0);
    }

    /// In the recovery window, both α values bump to 0.5 and the
    /// counter decrements by 1 per frame so the elevated weighting
    /// expires naturally.
    #[test]
    fn recovery_window_uses_elevated_alpha_and_decrements() {
        let (a_color, a_moments, next) = next_svgf_temporal_alpha(5);
        assert!((a_color - SVGF_ALPHA_RECOVERY).abs() < 1e-6);
        assert!((a_moments - SVGF_ALPHA_RECOVERY).abs() < 1e-6);
        assert_eq!(next, 4);
    }

    /// Recovery → steady-state transition: when the counter reaches
    /// 1, the current frame still uses the elevated α, but the
    /// next-frame counter is 0 so subsequent frames return to the
    /// floor. Guards against an off-by-one that would keep the
    /// elevated weighting one frame too long (or one frame too short).
    #[test]
    fn last_recovery_frame_uses_elevated_alpha_then_reverts() {
        let (a_color, _, next) = next_svgf_temporal_alpha(1);
        assert!((a_color - SVGF_ALPHA_RECOVERY).abs() < 1e-6);
        assert_eq!(next, 0);

        let (a_color2, _, next2) = next_svgf_temporal_alpha(next);
        assert!((a_color2 - SVGF_ALPHA_STEADY_STATE).abs() < 1e-6);
        assert_eq!(next2, 0, "counter must NOT underflow past 0");
    }

    /// Regression for #648 / RP-2: after `recreate_on_resize` zeroes
    /// `frames_since_creation`, the next `MAX_FRAMES_IN_FLIGHT`
    /// dispatches must force a full history reset so the freshly-
    /// allocated G-buffer attachments don't feed garbage taps into
    /// the temporal blend.
    #[test]
    fn first_frames_after_resize_force_history_reset() {
        // Sentinel boundary cases at the documented threshold.
        for frames in 0..MAX_FRAMES_IN_FLIGHT as u32 {
            assert!(
                should_force_history_reset(frames),
                "frames_since_creation = {frames} must reset history"
            );
        }
    }

    /// Sibling pin: once `MAX_FRAMES_IN_FLIGHT` dispatches have run,
    /// the history is populated and subsequent frames must NOT force
    /// a reset (otherwise SVGF would lose all temporal accumulation
    /// and oscillate between freshly-noisy and reset every frame).
    #[test]
    fn history_reset_disables_after_threshold() {
        for frames in [
            MAX_FRAMES_IN_FLIGHT as u32,
            MAX_FRAMES_IN_FLIGHT as u32 + 1,
            10_000,
            u32::MAX,
        ] {
            assert!(
                !should_force_history_reset(frames),
                "frames_since_creation = {frames} (≥ {}) must use history",
                MAX_FRAMES_IN_FLIGHT,
            );
        }
    }

    /// Regression for #801 / STRM-N1: a cell-streaming event bumps
    /// recovery to N frames; the next N dispatches must run with the
    /// elevated α and then revert exactly on frame N+1. Walks the
    /// state machine from N=8 (the cell-streaming default) down to 0
    /// to pin the sequence end-to-end — guards against an off-by-one
    /// that would keep ghosting one extra frame or drop the elevated
    /// weighting one frame early.
    #[test]
    fn streaming_recovery_window_runs_full_n_frames_then_reverts() {
        const N: u32 = 8;
        let mut counter = N;
        for frame in 0..N {
            let (a_color, a_moments, next) = next_svgf_temporal_alpha(counter);
            assert!(
                (a_color - SVGF_ALPHA_RECOVERY).abs() < 1e-6,
                "frame {frame} of {N}-frame recovery: expected α={SVGF_ALPHA_RECOVERY}, got {a_color}",
            );
            assert!((a_moments - SVGF_ALPHA_RECOVERY).abs() < 1e-6);
            assert_eq!(
                next,
                N - 1 - frame,
                "counter must decrement by 1 per dispatch"
            );
            counter = next;
        }
        // Frame N+1 — recovery exhausted, back to steady-state.
        let (a_color, _, next) = next_svgf_temporal_alpha(counter);
        assert!((a_color - SVGF_ALPHA_STEADY_STATE).abs() < 1e-6);
        assert_eq!(next, 0);
    }
}
