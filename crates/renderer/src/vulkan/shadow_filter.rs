//! Direct-shadow spatial filter: an edge-aware 5x5 B3 a-trous chain over the
//! per-pixel ReSTIR shadow ratio (`shaders/shadow_filter.comp`).
//!
//! The ratio EMA in `triangle.frag` filters shadows temporally only; with the
//! camera moving it runs at the 16-frame / 0.1-floor tail and nothing filters
//! it spatially, so penumbrae boiled in motion. This pass reads the claim
//! winner's reservoir (`accum` = the shadow ratio, plus its normal and
//! view-ray distance as edge guides) and its unshadowed direct radiance U
//! from the current frame's reservoir buffer, filters the ratio, and writes
//! the correction `U * (filtered - raw)` that composite adds to direct light.
//! Only the ratio is filtered, never U, so texture and highlight detail stay
//! put; with the pass absent, composite reads no correction at all.
//!
//! #6249 — a guide (and the centre pixel) is valid only when the slot's
//! owner is the visible surface: the pass samples the G-buffer mesh-ID
//! attachment (binding 5) and compares it against the reservoir's packed
//! surface tag, so corrections never land on unlit screens, blades, glass,
//! decals or torn slots that never claimed the pixel they shade.
//!
//! Dispatched from `record_svgf_pass`, inside the SVGF GPU-timer bracket.

use super::allocator::SharedAllocator;
use super::descriptors::{
    clamp_sampler, image_barrier_general_write_to_read, image_barrier_undef_to_general,
    sampled_image_info, write_combined_image_sampler, write_storage_buffer, write_storage_image,
    DescriptorPoolBuilder,
};
use super::image::{GpuImage, GpuImageDesc};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::restir::ReservoirBuffers;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use super::GpuUploadCtx;
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
use crate::vulkan::descriptors::compute_binding;
use anyhow::{Context, Result};
use ash::vk;

const SHADOW_FILTER_COMP_SPV: &[u8] = include_bytes!("../../shaders/shadow_filter.comp.spv");

/// A-trous iterations: strides 1, 2 (a 6-pixel reach with the 5x5 kernel).
const ITERATIONS: u32 = 2;

/// Ratio ping-pong and correction images: RGBA16F (alpha unused).
const FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Screen-sized bytes per pixel this pass allocates per frame-in-flight:
/// two RGBA16F ratio images and one RGBA16F correction image.
pub const SHADOW_FILTER_BYTES_PER_PIXEL: u64 = 3 * 8;

const FLAG_READ_RAW: u32 = 1;
const FLAG_SOURCE_B: u32 = 2;
const FLAG_LAST: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy)]
struct Push {
    screen: [u32; 2],
    step: u32,
    flags: u32,
}

// SAFETY: `#[repr(C)]`, all-`u32` fields tiling the 16-byte block with no
// padding. Mirrors the GLSL `Push` block in `shadow_filter.comp`, which
// `name_diverging_glsl_rust_mirrors_stay_in_lockstep` compares field for field.
unsafe impl super::buffer::NoUninit for Push {}

struct FrameImages {
    ratio_a: GpuImage,
    ratio_b: GpuImage,
    delta: GpuImage,
}

pub struct ShadowFilterPipeline {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Point sampler for the mesh-ID attachment (#6249): exact texel
    /// identity, never filtered.
    point_sampler: vk::Sampler,
    frames: Vec<FrameImages>,
    width: u32,
    height: u32,
}

impl ShadowFilterPipeline {
    pub fn new(
        ctx: GpuUploadCtx,
        pipeline_cache: vk::PipelineCache,
        reservoirs: &ReservoirBuffers,
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(mesh_id_views.len(), super::sync::MAX_FRAMES_IN_FLIGHT);
        let device = ctx.device;
        let mut partial = Self {
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            point_sampler: vk::Sampler::null(),
            frames: Vec::new(),
            width,
            height,
        };
        if let Err(e) = partial.new_inner(ctx, pipeline_cache, reservoirs, mesh_id_views) {
            // SAFETY: nothing created so far has been submitted.
            unsafe { partial.destroy(device, ctx.allocator) };
            return Err(e);
        }
        if let Err(e) = partial.initialize_layouts(ctx) {
            if super::texture::OneTimeCommandError::may_be_in_flight(&e) {
                // #6063 — the layout submit may still be executing against
                // the pass's images, so a host-side destroy could race it.
                // Leak them, as the #4891 upload paths and #5947 do.
                std::mem::forget(partial);
            } else {
                // SAFETY: the layout submit never reached the queue and no
                // frame has used the pass, so nothing references its handles.
                unsafe { partial.destroy(device, ctx.allocator) };
            }
            return Err(e);
        }
        Ok(partial)
    }

    fn new_inner(
        &mut self,
        ctx: GpuUploadCtx,
        pipeline_cache: vk::PipelineCache,
        reservoirs: &ReservoirBuffers,
        mesh_id_views: &[vk::ImageView],
    ) -> Result<()> {
        let device = ctx.device;
        let bindings = [
            // 0: this frame's reservoirs (ratio + guides)
            compute_binding(0, vk::DescriptorType::STORAGE_BUFFER),
            // 1: this frame's unshadowed direct radiance U
            compute_binding(1, vk::DescriptorType::STORAGE_BUFFER),
            // 2, 3: ratio ping-pong
            compute_binding(2, vk::DescriptorType::STORAGE_IMAGE),
            compute_binding(3, vk::DescriptorType::STORAGE_IMAGE),
            // 4: the correction composite adds
            compute_binding(4, vk::DescriptorType::STORAGE_IMAGE),
            // 5: the G-buffer mesh-ID attachment — slot-ownership gate
            // (#6249), SHADER_READ_ONLY_OPTIMAL like SVGF's taps of it.
            compute_binding(5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "shadow_filter.comp",
                spirv: SHADOW_FILTER_COMP_SPV,
            }],
            "shadow_filter",
            &[],
        )
        .expect("shadow_filter descriptor layout drifted against shadow_filter.comp");

        // SAFETY: bindings validated against the shader above.
        self.descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("shadow filter descriptor set layout")?
        };
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<Push>() as u32);
        // SAFETY: the set layout was just created; borrows live for the call.
        self.pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&self.descriptor_set_layout))
                        .push_constant_ranges(std::slice::from_ref(&push_range)),
                    None,
                )
                .context("shadow filter pipeline layout")?
        };
        self.pipeline = super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            SHADOW_FILTER_COMP_SPV,
            self.pipeline_layout,
            "shadow filter",
        )?;

        let set_count = MAX_FRAMES_IN_FLIGHT as u32;
        self.descriptor_pool = DescriptorPoolBuilder::from_layout_bindings(&bindings, set_count)
            .max_sets(set_count)
            .build(device, "shadow filter descriptor pool")?;
        let set_layouts = vec![self.descriptor_set_layout; set_count as usize];
        // SAFETY: the pool is sized for `set_count` sets of this layout.
        self.descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("shadow filter descriptor sets")?
        };

        // SAFETY: info fully populated; the handle is owned here and freed
        // in `destroy` (also on the partial-failure path above via `new`).
        self.point_sampler = unsafe {
            clamp_sampler(device, vk::Filter::NEAREST, vk::SamplerMipmapMode::NEAREST)
                .context("shadow filter point sampler")?
        };

        self.create_images(ctx)?;
        self.write_descriptor_sets(device, reservoirs, mesh_id_views);
        Ok(())
    }

    fn create_images(&mut self, ctx: GpuUploadCtx) -> Result<()> {
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let make = |name: &str| {
                GpuImage::create(
                    ctx.device,
                    ctx.allocator,
                    &GpuImageDesc::color_2d(
                        name,
                        self.width,
                        self.height,
                        FORMAT,
                        vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                    ),
                )
            };
            let ratio_a = make(&format!("shadow filter ratio A [{f}]"))?;
            let ratio_b = match make(&format!("shadow filter ratio B [{f}]")) {
                Ok(img) => img,
                Err(e) => {
                    let mut ratio_a = ratio_a;
                    ratio_a.destroy(ctx.device, ctx.allocator);
                    return Err(e);
                }
            };
            let delta = match make(&format!("shadow filter correction [{f}]")) {
                Ok(img) => img,
                Err(e) => {
                    let (mut ratio_a, mut ratio_b) = (ratio_a, ratio_b);
                    ratio_a.destroy(ctx.device, ctx.allocator);
                    ratio_b.destroy(ctx.device, ctx.allocator);
                    return Err(e);
                }
            };
            self.frames.push(FrameImages {
                ratio_a,
                ratio_b,
                delta,
            });
        }
        Ok(())
    }

    fn write_descriptor_sets(
        &self,
        device: &ash::Device,
        reservoirs: &ReservoirBuffers,
        mesh_id_views: &[vk::ImageView],
    ) {
        for (f, frame) in self.frames.iter().enumerate() {
            let set = self.descriptor_sets[f];
            let reservoir_info = [vk::DescriptorBufferInfo {
                buffer: reservoirs.curr_buffer(f),
                offset: 0,
                range: reservoirs.buffer_size(),
            }];
            let unshadowed_info = [vk::DescriptorBufferInfo {
                buffer: reservoirs.curr_buffer(f),
                offset: reservoirs.unshadowed_offset(),
                range: reservoirs.unshadowed_size(),
            }];
            let image_info = |view| {
                [vk::DescriptorImageInfo {
                    sampler: vk::Sampler::null(),
                    image_view: view,
                    image_layout: vk::ImageLayout::GENERAL,
                }]
            };
            let a = image_info(frame.ratio_a.view);
            let b = image_info(frame.ratio_b.view);
            let d = image_info(frame.delta.view);
            // #6249 — the mesh-ID attachment in its post-render-pass
            // SHADER_READ_ONLY_OPTIMAL layout, point-sampled.
            let mesh_id = [sampled_image_info(self.point_sampler, mesh_id_views[f])];
            let writes = [
                write_storage_buffer(set, 0, &reservoir_info),
                write_storage_buffer(set, 1, &unshadowed_info),
                write_storage_image(set, 2, &a),
                write_storage_image(set, 3, &b),
                write_storage_image(set, 4, &d),
                write_combined_image_sampler(set, 5, &mesh_id),
            ];
            // SAFETY: `set` is live; every write borrows locals that outlive
            // the call; the buffers and views are owned and live.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Walk every image UNDEFINED → GENERAL in one submit.
    ///
    /// A failure here is the only one that can leave GPU work pending against
    /// the pass's images: callers must consult
    /// `OneTimeCommandError::may_be_in_flight` (`texture.rs`)
    /// before freeing them (#6063).
    pub(crate) fn initialize_layouts(&self, ctx: GpuUploadCtx) -> Result<()> {
        let barriers: Vec<_> = self
            .frames
            .iter()
            .flat_map(|f| [f.ratio_a.image, f.ratio_b.image, f.delta.image])
            .map(image_barrier_undef_to_general)
            .collect();
        // SAFETY: every image is owned here and unused by any submission yet; `ctx` carries
        // the live device, queue and pool.
        unsafe {
            super::texture::submit_initial_layouts(
                ctx.device,
                ctx.queue,
                ctx.command_pool,
                &barriers,
                vk::PipelineStageFlags::COMPUTE_SHADER,
            )
        }
    }

    /// The correction image composite adds to direct light this frame.
    pub fn correction_view(&self, frame: usize) -> vk::ImageView {
        self.frames[frame].delta.view
    }

    /// Record the filter chain. Infallible, like `SvgfPipeline::dispatch`.
    ///
    /// The main pass writes `reservoirsCurr` (and U beside it) from the
    /// fragment stage; no render-pass dependency covers a storage buffer, so
    /// the first barrier here makes those writes visible to this compute
    /// read. The last iteration's correction write is made visible to
    /// composite's fragment read.
    pub fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        reservoirs: &ReservoirBuffers,
    ) {
        let images = &self.frames[frame];
        let reservoir_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(reservoirs.curr_buffer(frame))
            .offset(0)
            .size(vk::WHOLE_SIZE);
        let groups = [
            self.width.div_ceil(WORKGROUP_X),
            self.height.div_ceil(WORKGROUP_Y),
        ];
        // SAFETY: `cmd` is recording outside a render pass; every handle is
        // owned by this pass or by the live reservoir buffers.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&reservoir_barrier),
                &[],
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
            for iter in 0..ITERATIONS {
                let last = iter + 1 == ITERATIONS;
                // Iteration 0 reads the raw ratio and writes B; afterwards
                // the source alternates (B -> A -> B ...).
                let source_b = iter % 2 == 1;
                let mut flags = 0;
                if iter == 0 {
                    flags |= FLAG_READ_RAW;
                }
                if source_b {
                    flags |= FLAG_SOURCE_B;
                }
                if last {
                    flags |= FLAG_LAST;
                }
                let push = Push {
                    screen: [self.width, self.height],
                    step: 1 << iter,
                    flags,
                };
                device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    super::buffer::byte_view(std::slice::from_ref(&push)),
                );
                device.cmd_dispatch(cmd, groups[0], groups[1], 1);
                let written = if last {
                    images.delta.image
                } else if source_b {
                    images.ratio_a.image
                } else {
                    images.ratio_b.image
                };
                let barrier = image_barrier_general_write_to_read(written);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    if last {
                        vk::PipelineStageFlags::FRAGMENT_SHADER
                    } else {
                        vk::PipelineStageFlags::COMPUTE_SHADER
                    },
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&barrier),
                );
            }
        }
    }

    /// Recreate the screen-sized images at a new extent and rebind the
    /// (also recreated) reservoir buffers and G-buffer mesh-ID views.
    /// Called after `ReservoirBuffers::recreate_on_resize` and the G-buffer
    /// recreate from the fenced resize path.
    ///
    /// Submits nothing: the caller follows with `initialize_layouts`
    /// so it can tell a failure that left GPU work pending from one that did
    /// not (#6063). An `Err` here is safe to `destroy`.
    pub fn recreate_on_resize(
        &mut self,
        ctx: GpuUploadCtx,
        reservoirs: &ReservoirBuffers,
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        for mut frame in self.frames.drain(..) {
            frame.ratio_a.destroy(ctx.device, ctx.allocator);
            frame.ratio_b.destroy(ctx.device, ctx.allocator);
            frame.delta.destroy(ctx.device, ctx.allocator);
        }
        self.width = width;
        self.height = height;
        self.create_images(ctx)?;
        self.write_descriptor_sets(ctx.device, reservoirs, mesh_id_views);
        Ok(())
    }

    /// Destroy every owned handle; safe to call twice.
    ///
    /// # Safety
    /// No in-flight command buffer may reference this pass.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut frame in self.frames.drain(..) {
            frame.ratio_a.destroy(device, allocator);
            frame.ratio_b.destroy(device, allocator);
            frame.delta.destroy(device, allocator);
        }
        // SAFETY: caller guarantees no in-flight use; each handle is nulled
        // after destruction so a second call is a no-op.
        unsafe {
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
                self.descriptor_sets.clear();
            }
            if self.pipeline != vk::Pipeline::null() {
                device.destroy_pipeline(self.pipeline, None);
                self.pipeline = vk::Pipeline::null();
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }
            if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                self.descriptor_set_layout = vk::DescriptorSetLayout::null();
            }
            if self.point_sampler != vk::Sampler::null() {
                device.destroy_sampler(self.point_sampler, None);
                self.point_sampler = vk::Sampler::null();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// #6249 — the filter must gate every guide (and the centre pixel) on
    /// the slot's owner being the visible surface: the mesh-ID attachment
    /// sampled at binding 5, checked for stable history and compared against
    /// the reservoir's packed surface tag. Pins the GLSL gate and the
    /// descriptor write that feeds it; the GPU behaviour needs RenderDoc.
    #[test]
    fn guides_require_the_slot_owner_to_be_the_visible_surface() {
        let comp = include_str!("../../shaders/shadow_filter.comp");
        assert!(
            comp.contains("layout(set = 0, binding = 5) uniform highp usampler2D meshIdTex;"),
            "the mesh-ID attachment must be bound at set 0 binding 5"
        );
        assert!(
            comp.contains("bool slotOwnsVisiblePixel(ivec2 q, uint lightAndSurface) {")
                && comp.contains("meshIdHasStableHistory(visible)")
                && comp.contains(
                    "(visible & RESERVOIR_SURFACE_MASK) == (lightAndSurface >> RESERVOIR_LIGHT_BITS)"
                ),
            "the ownership test must reject blended mesh IDs and foreign surface tags"
        );
        // The gate must be part of guide validity — the centre pixel and
        // every neighbour tap go through guideAt.
        assert!(
            comp.contains("g.valid = histDepth.x != 0.0 && !isnan(histDepth.x)")
                && comp.contains("&& slotOwnsVisiblePixel(q, r.lightAndSurface);"),
            "guideAt must fold the ownership test into validity"
        );

        let own = crate::source_scan::production_text(include_str!("shadow_filter.rs"));
        assert!(
            own.contains("compute_binding(5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER)")
                && own.contains("write_combined_image_sampler(set, 5, &mesh_id)")
                && own.contains("sampled_image_info(self.point_sampler, mesh_id_views[f])"),
            "the Rust side must declare, write, and point-sample binding 5 per frame"
        );
    }

    /// #6035 — the published figure the BLAS reservation bills must be the
    /// three `FrameImages` at `FORMAT`'s 8 B/px.
    #[test]
    fn bytes_per_pixel_matches_frame_images() {
        assert_eq!(super::FORMAT, ash::vk::Format::R16G16B16A16_SFLOAT);
        let own = crate::source_scan::production_text(include_str!("shadow_filter.rs"));
        let start = own.find("struct FrameImages {").expect("FrameImages");
        let body = &own[start..start + own[start..].find('}').expect("FrameImages end")];
        assert_eq!(body.matches(": GpuImage,").count(), 3);
        assert_eq!(super::SHADOW_FILTER_BYTES_PER_PIXEL, 3 * 8);
    }

    /// #6063 — a failed layout submit may still be executing against the
    /// pass's images, so both the construction and the resize arm must branch
    /// on the in-flight class instead of destroying unconditionally.
    #[test]
    fn a_failed_layout_submit_leaks_instead_of_destroying() {
        let own = crate::source_scan::production_text(include_str!("shadow_filter.rs"));
        let new_start = own.find("pub fn new(").expect("ShadowFilterPipeline::new");
        let new_end = new_start + own[new_start..].find("fn new_inner").expect("new_inner follows new");
        let new_body = &own[new_start..new_end];
        assert!(
            new_body.contains("OneTimeCommandError::may_be_in_flight(&e)")
                && new_body.contains("std::mem::forget(partial)"),
            "ShadowFilterPipeline::new must leak the pass when the layout submit may be in flight"
        );
        let after_inner = &own[new_end..];
        let inner_end = after_inner.find("fn create_images").expect("create_images follows new_inner");
        assert!(
            !after_inner[..inner_end].contains("initialize_layouts"),
            "new_inner must not submit: its failure arm destroys unconditionally"
        );
        let resize = include_str!("context/resize.rs");
        let arm = resize
            .find("shadow filter resize failed")
            .expect("the shadow filter resize arm");
        let window = &resize[arm.saturating_sub(1500)..arm + 1500];
        assert!(
            window.contains("may_be_in_flight") && window.contains("std::mem::forget(filter)"),
            "the shadow filter resize arm must leak the pass when the layout submit may be in flight (#6063)"
        );
    }
}
