//! Temporal Antialiasing (TAA) — reproject-and-resolve compute pass.
//!
//! Runs between SVGF and composite:
//!
//!   main render pass → raw HDR + motion + mesh_id  (per frame-in-flight)
//!   SVGF temporal    → denoised indirect           (per frame-in-flight)
//!   TAA dispatch     → anti-aliased HDR            (THIS module)
//!   composite        → tone-mapped swapchain
//!
//! Owns a per-frame-in-flight RGBA16F history image. Each frame reads
//! the OTHER slot (the previous frame's history) via motion-vector
//! reprojection, neighborhood-clamps it against the 3×3 current-pixel
//! stats in YCoCg, and writes the resolved color to its own slot. The
//! composite pass then samples the same slot as this frame's HDR input.
//!
//! Projection-matrix jitter (Halton 2,3) is applied CPU-side in the
//! camera UBO construction; this shader just consumes the result.
//!
//! ## Descriptor set layout
//!
//! | Binding | Resource        | Type                                |
//! |---------|-----------------|-------------------------------------|
//! | 0       | curr HDR        | combined image sampler              |
//! | 1       | motion vector   | combined image sampler              |
//! | 2       | curr mesh_id    | combined image sampler (usampler2D) |
//! | 3       | prev mesh_id    | combined image sampler (usampler2D) |
//! | 4       | prev history    | combined image sampler              |
//! | 5       | out TAA         | storage image (rgba16f)             |
//! | 6       | params UBO      | uniform buffer                      |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_undef_to_general, write_combined_image_sampler, write_storage_image,
    write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::svgf::should_force_history_reset;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

// #918 / REN-D10-NEW-04 — TAA's read-previous / write-current
// ping-pong (`prev = (f + 1) % MAX_FRAMES_IN_FLIGHT` at line ~441,
// history-slot indexing throughout) requires at least 2 slots.
// Compile-time gate so a future sync-tier change that touches the
// constant fails the build here rather than producing a degenerate
// history-recovery boundary at runtime.
const _: () = assert!(
    MAX_FRAMES_IN_FLIGHT >= 2,
    "TAA ping-pong arithmetic requires MAX_FRAMES_IN_FLIGHT >= 2 — \
     lowering it aliases the read-previous and write-current slots to the same index"
);

const TAA_COMP_SPV: &[u8] = include_bytes!("../../shaders/taa.comp.spv");

/// History format. RGBA16F matches the HDR render target so no precision
/// is lost on reprojection. Alpha is always 1.0 and is never read.
const HISTORY_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TaaParams {
    /// xy = screen dimensions (pixels), zw = 1/screen.
    pub screen: [f32; 4],
    /// x = current-frame weight α in [0, 1] (0.1 is canonical, meaning 10%
    /// current + 90% history per frame → ~10-frame rolling average).
    /// y = first_frame flag (1.0 while history is not yet valid).
    /// zw = reserved.
    pub params: [f32; 4],
}

struct HistorySlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct TaaPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    shader_module: vk::ShaderModule,

    /// Per-frame-in-flight RGBA16F images. Used simultaneously as
    /// "this frame's output" (storage write) and "next frame's history"
    /// (sampled read).
    history: Vec<HistorySlot>,

    /// Sampler used for history (linear for Catmull-Rom) and for the
    /// integer mesh_id attachments (filtering ignored for usampler).
    linear_sampler: vk::Sampler,
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    frames_since_creation: u32,
    /// Set in [`Self::dispatch`] once the compute dispatch has been
    /// recorded; consumed + cleared by [`Self::mark_frame_completed`]
    /// after `queue_submit` returns success. Gates the
    /// `frames_since_creation` advance on submit success so a record-
    /// or submit-failure path doesn't mark un-dispatched frames as
    /// completed. See #917 / REN-D10-NEW-03 (mirror of the SVGF fix).
    dispatched_this_frame: bool,
}

impl TaaPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(hdr_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(motion_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);

        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            hdr_views,
            motion_views,
            mesh_id_views,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("TAA pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
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
            history: Vec::new(),
            linear_sampler: vk::Sampler::null(),
            point_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
            frames_since_creation: 0,
            dispatched_this_frame: false,
        };

        // SAFETY (inside macro below): `partial` is local to this fn and
        // not yet referenced by any command buffer / descriptor set —
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

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_history_image(
                device,
                allocator,
                width,
                height,
                &format!("taa_history_{i}"),
            ));
            partial.history.push(slot);
        }

        // SAFETY: SamplerCreateInfo populated above; handle owned by
        // `partial.linear_sampler`, freed by destroy().
        partial.linear_sampler = try_or_cleanup!(unsafe {
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
                .context("TAA linear sampler")
        });
        // SAFETY: same contract as `linear_sampler` above.
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
                .context("TAA point sampler")
        });

        let param_size = std::mem::size_of::<TaaParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
        }

        let bindings = [
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
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "taa.comp",
                spirv: TAA_COMP_SPV,
            }],
            "taa",
            &[],
        )
        .expect("taa descriptor layout drifted against taa.comp (see #427)");
        // SAFETY: `bindings` was validated above by `validate_set_layout`
        // against taa.comp; layout handle owned by `partial`.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("TAA descriptor set layout")
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
                .context("TAA pipeline layout")
        });

        partial.shader_module =
            try_or_cleanup!(super::pipeline::load_shader_module(device, TAA_COMP_SPV));
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(partial.shader_module)
            .name(c"main");
        // SAFETY: stage references `partial.shader_module` (loaded above)
        // and `partial.pipeline_layout` (just created above); pipeline
        // cache is the caller-provided handle (may be null — valid per spec).
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
                .context("TAA compute pipeline")
        } {
            Ok(p) => p[0],
            Err(e) => {
                // SAFETY: cleanup-on-error. Note: `shader_module` survives
                // on `partial.shader_module` and is freed by `destroy`.
                unsafe { partial.destroy(device, allocator) };
                return Err(e);
            }
        };

        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "TAA descriptor pool"));

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: pool was just sized for MAX_FRAMES_IN_FLIGHT sets;
        // `set_layouts` is a length-MAX_FRAMES_IN_FLIGHT vec of the same
        // descriptor_set_layout handle.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("TAA descriptor sets")
        });

        partial.write_descriptor_sets(device, hdr_views, motion_views, mesh_id_views);

        log::info!("TAA pipeline created: {}x{}", width, height);
        Ok(partial)
    }

    fn create_history_image(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        name: &str,
    ) -> Result<HistorySlot> {
        let img_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(HISTORY_FORMAT)
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
        // SAFETY: `img_info` fully populated above (TYPE_2D, HISTORY_FORMAT,
        // STORAGE | SAMPLED usage). On Ok, ownership transfers to the
        // caller's HistorySlot; on Err the `?` bubbles up before any
        // bind/view runs.
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
                // SAFETY: `image` just created above; handle is live.
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: cleanup-on-error — `image` was created above
                // but never bound; no other reference exists.
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
            // SAFETY: same as the destroy in the alloc-error arm above —
            // image is never bound to a live allocation after the free.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        // SAFETY: `image` is bound (line above). View ownership transfers
        // to caller's HistorySlot on Ok.
        let view = match unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(HISTORY_FORMAT)
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
                // SAFETY: image was bound (above); free the alloc first,
                // then destroy the image. No view was created on this arm.
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
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
    ) {
        let param_size = std::mem::size_of::<TaaParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            // `prev` selects the OTHER FIF slot — the descriptor for
            // frame slot `f` reads its previous-frame inputs
            // (`prev_mid`, `prev_history`) from slot `(f + 1) %
            // MAX_FRAMES_IN_FLIGHT`. Steady state this is the slot
            // that just submitted last frame and is now resident in
            // `SHADER_READ_ONLY_OPTIMAL` / `GENERAL`.
            //
            // First-frame note (REN-D11-NEW-04, audit 2026-05-09):
            // on session frame 0, the OTHER slot's images are in
            // `UNDEFINED` layout (initialized but never written).
            // The shader's `params.params.y > 0.5` first-frame guard
            // (`taa.comp:93`) skips the `prev_mid` / `prev_history`
            // texelFetch entirely on that frame, so the UNDEFINED
            // contents never reach a sample site. If the first-frame
            // guard is ever dropped or moved, this descriptor write
            // needs to pre-clear the OTHER slot's images to a defined
            // colour first, or skip the dispatch for frame 0.
            let prev = (f + 1) % MAX_FRAMES_IN_FLIGHT;

            let curr_hdr = [vk::DescriptorImageInfo::default()
                .sampler(self.linear_sampler)
                .image_view(hdr_views[f])
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
            let prev_hist = [vk::DescriptorImageInfo::default()
                .sampler(self.linear_sampler)
                .image_view(self.history[prev].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let out_taa = [vk::DescriptorImageInfo::default()
                .image_view(self.history[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];

            let set = self.descriptor_sets[f];
            let writes = [
                write_combined_image_sampler(set, 0, &curr_hdr),
                write_combined_image_sampler(set, 1, &motion),
                write_combined_image_sampler(set, 2, &curr_mid),
                write_combined_image_sampler(set, 3, &prev_mid),
                write_combined_image_sampler(set, 4, &prev_hist),
                write_storage_image(set, 5, &out_taa),
                write_uniform_buffer(set, 6, &params),
            ];
            // SAFETY: descriptor sets owned by `self`; `writes` references
            // image views / buffer owned by `self` and `hdr_views` /
            // `motion_views` / `mesh_id_views` slices (caller-borrowed for
            // the call duration).
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// View for this frame's resolved TAA output (= composite's input).
    pub fn output_view(&self, frame: usize) -> vk::ImageView {
        self.history[frame].view
    }

    /// Force the next [`MAX_FRAMES_IN_FLIGHT`] frames to skip the
    /// temporal tap, dropping ghost-resolved history when scene
    /// content changes underneath the camera (cell load / unload,
    /// weather flip, fast camera turn).
    ///
    /// Same effect as `recreate_on_resize` for `frames_since_creation`
    /// — `should_force_history_reset(self.frames_since_creation)` flips
    /// to `true` for two dispatches and the param.y reset flag forces
    /// the shader to use only the current frame. No GPU resources are
    /// touched (history images stay allocated).
    ///
    /// Pairs with [`SvgfPipeline`]'s recovery-α window so SVGF and TAA
    /// recover together — without this, TAA would keep trailing
    /// ghosting on freshly-streamed geometry for ~30 frames at 60 FPS
    /// while SVGF's elevated-α window already faded. See #801.
    pub fn signal_history_reset(&mut self) {
        self.frames_since_creation = 0;
        // Drop any pending mark — a future-frame queue_submit success
        // shouldn't undo this reset by advancing the counter past 0
        // before the next dispatch records.
        self.dispatched_this_frame = false;
    }

    /// UNDEFINED → GENERAL for every history slot. Call once after `new()`.
    /// # Safety
    /// device / queue / pool must be valid; queue must support graphics.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(self.history.len());
            for slot in &self.history {
                barriers.push(image_barrier_undef_to_general(slot.image));
            }
            // SAFETY: caller of `initialize_layouts` (unsafe fn) guarantees
            // device/queue/pool validity; `cmd` is the recording buffer
            // from `with_one_time_commands`. Each barrier targets a history
            // image we own.
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

    /// Dispatch TAA. Must run after the main render pass (so HDR / motion /
    /// mesh_id are in SHADER_READ_ONLY_OPTIMAL) and before composite
    /// (which samples `output_view(frame)` in GENERAL).
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer. `frame < MAX_FRAMES_IN_FLIGHT`.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) -> Result<()> {
        // #648 / RP-2 SIBLING — same first-`MAX_FRAMES_IN_FLIGHT`
        // history-reset window as SVGF. The TAA path's
        // `recreate_on_resize` zeroes `frames_since_creation` (line
        // 702), and the param.y reset flag forces the shader to
        // skip the temporal tap on the freshly-allocated history.
        let first_frame = if should_force_history_reset(self.frames_since_creation) {
            1.0
        } else {
            0.0
        };
        let params = TaaParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            // 0.1 = 10% current, 90% history — canonical TAA blend.
            params: [0.1, first_frame, 0.0, 0.0],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

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

        // Order this frame's write after any lingering sample of the same
        // slot (layout stays GENERAL). The in-flight fence already
        // guarantees the previous GPU work on this slot has completed.
        let out_img = self.history[frame].image;
        let pre = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(out_img)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        // Pre-barrier: protects this slot's TAA output `out_img`
        // against write-after-read on the previous reader.
        //
        // src_stage_mask = COMPUTE_SHADER only — pre-fix this was
        // `COMPUTE_SHADER | FRAGMENT_SHADER` to cover both the
        // previous TAA dispatch (compute reader of `prev_history`)
        // and the previous composite (fragment reader of this slot
        // as input texture). The fragment reader is from the
        // PREVIOUS frame's composite; the per-FIF fence wait at
        // `draw_frame` entry serialises that fragment work against
        // anything this frame submits, so `FRAGMENT_SHADER` is
        // covered structurally and naming it here is over-spec.
        // See REN-D11-NEW-05 (audit 2026-05-09).
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[pre],
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

        // Expose the result to composite's fragment shader read.
        let post = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(out_img)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            // dst = FRAGMENT for composite's read this frame + COMPUTE for
            // next frame's TAA dispatch reading this slot as
            // `prev_history`. Per-frame fence currently serialises both
            // consumers, but the dst-stage mask still has to cover both
            // for correctness once the fence wait is relaxed. #653.
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post],
        );

        // #917 / REN-D10-NEW-03 — dispatch was successfully recorded.
        // `mark_frame_completed` advances `frames_since_creation` after
        // `queue_submit` returns success; record-time failures before
        // this point leave the flag false.
        self.dispatched_this_frame = true;
        Ok(())
    }

    /// Mark the previous frame's dispatch as having reached
    /// queue-submit-success. Advances `frames_since_creation` iff a
    /// dispatch was recorded this frame. Called from `draw_frame` after
    /// `queue_submit` returns Ok. Mirror of `SvgfPipeline::
    /// mark_frame_completed`. See #917 / REN-D10-NEW-03.
    pub fn mark_frame_completed(&mut self) {
        if self.dispatched_this_frame {
            self.frames_since_creation = self.frames_since_creation.saturating_add(1);
            self.dispatched_this_frame = false;
        }
    }

    /// Recreate history images at a new extent after swapchain resize.
    /// Caller must pass the freshly-recreated G-buffer and composite HDR views.
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        hdr_views: &[vk::ImageView],
        motion_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        width: u32,
        height: u32,
    ) -> Result<()> {
        for slot in self.history.drain(..) {
            // SAFETY: `recreate_on_resize` is called from the swapchain-
            // resize path which fences both frames-in-flight first
            // (see `VulkanContext::recreate_swapchain`). View / image
            // handles are not referenced by any in-flight command.
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }

        self.width = width;
        self.height = height;
        self.frames_since_creation = 0;
        // Drop any pending mark — pre-resize-recorded dispatch (if any)
        // never sees `queue_submit` because the resize path waited on
        // both fences. See #917 (mirror of SVGF resize path).
        self.dispatched_this_frame = false;

        let result = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.history.push(Self::create_history_image(
                    device,
                    allocator,
                    width,
                    height,
                    &format!("taa_history_{i}"),
                )?);
            }
            Ok(())
        })();
        if let Err(ref e) = result {
            log::error!("TAA recreate partial failure: {e} — destroying partial state");
            // SAFETY: same fenced-resize contract as above — partial
            // state is not referenced by any in-flight command.
            unsafe { self.destroy(device, allocator) };
            return result;
        }

        self.write_descriptor_sets(device, hdr_views, motion_views, mesh_id_views);
        Ok(())
    }

    /// Destroy all Vulkan objects. Safe to call on partially-initialized state.
    /// # Safety
    /// device and allocator must be valid; no GPU work may reference these images.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY (whole function): caller of `destroy` (unsafe fn)
        // guarantees no in-flight command buffer references any object
        // owned by `self`. The per-handle `if != null()` guards make this
        // safe to call on partially-initialised state from a `try_or_cleanup`
        // path. Each per-handle destroy below relies on this contract.
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
        if self.linear_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.linear_sampler, None) };
        }
        if self.point_sampler != vk::Sampler::null() {
            unsafe { device.destroy_sampler(self.point_sampler, None) };
        }
        for slot in self.history.drain(..) {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}
