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
//! Dispatched from `record_svgf_pass`, inside the SVGF GPU-timer bracket.

use super::allocator::SharedAllocator;
use super::descriptors::{
    image_barrier_general_write_to_read, image_barrier_undef_to_general, write_storage_buffer,
    write_storage_image, DescriptorPoolBuilder,
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
    frames: Vec<FrameImages>,
    width: u32,
    height: u32,
}

impl ShadowFilterPipeline {
    pub fn new(
        ctx: GpuUploadCtx,
        pipeline_cache: vk::PipelineCache,
        reservoirs: &ReservoirBuffers,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let device = ctx.device;
        let mut partial = Self {
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            frames: Vec::new(),
            width,
            height,
        };
        if let Err(e) = partial.new_inner(ctx, pipeline_cache, reservoirs) {
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

        self.create_images(ctx)?;
        self.write_descriptor_sets(device, reservoirs);
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

    fn write_descriptor_sets(&self, device: &ash::Device, reservoirs: &ReservoirBuffers) {
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
            let writes = [
                write_storage_buffer(set, 0, &reservoir_info),
                write_storage_buffer(set, 1, &unshadowed_info),
                write_storage_image(set, 2, &a),
                write_storage_image(set, 3, &b),
                write_storage_image(set, 4, &d),
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
        super::texture::with_one_time_commands(ctx.device, ctx.queue, ctx.command_pool, |cmd| {
            let barriers: Vec<_> = self
                .frames
                .iter()
                .flat_map(|f| [f.ratio_a.image, f.ratio_b.image, f.delta.image])
                .map(image_barrier_undef_to_general)
                .collect();
            // SAFETY: `cmd` is the recording one-time buffer; every image is
            // owned here and unused by any submission yet.
            unsafe {
                ctx.device.cmd_pipeline_barrier(
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
                    std::slice::from_raw_parts(
                        (&push as *const Push).cast::<u8>(),
                        std::mem::size_of::<Push>(),
                    ),
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
    /// (also recreated) reservoir buffers. Called after
    /// `ReservoirBuffers::recreate_on_resize` from the fenced resize path.
    ///
    /// Submits nothing: the caller follows with `initialize_layouts`
    /// so it can tell a failure that left GPU work pending from one that did
    /// not (#6063). An `Err` here is safe to `destroy`.
    pub fn recreate_on_resize(
        &mut self,
        ctx: GpuUploadCtx,
        reservoirs: &ReservoirBuffers,
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
        self.write_descriptor_sets(ctx.device, reservoirs);
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
        }
    }
}

#[cfg(test)]
mod tests {
    /// #6063 — a failed layout submit may still be executing against the
    /// pass's images, so both the construction and the resize arm must branch
    /// on the in-flight class instead of destroying unconditionally.
    #[test]
    fn a_failed_layout_submit_leaks_instead_of_destroying() {
        let own = crate::source_scan::production_text_or_whole(include_str!("shadow_filter.rs"));
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
