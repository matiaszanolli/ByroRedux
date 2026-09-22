//! Exposure metering pass (Stage 1, `RENDERING-PLAN.md`).
//!
//! Wraps `exposure_meter.comp`: single-workgroup reduction of the post-bloom
//! composite scene to a geometric-mean luminance, then the Frostbite EV100 →
//! linear-exposure chain (see `exposure.rs`'s module docs for the photometry)
//! written into this frame's `ExposureResource` slot. Fixed mode skips
//! metering and writes the configured constant, which keeps the slots live
//! even when auto exposure is off — `presentation.frag` and the FSR dispatch
//! sample the texel unconditionally.
//!
//! Follows the bloom pipeline's shape: descriptor set per frame in flight,
//! externally-owned bindings (scene view, exposure slot view) rewritten per
//! dispatch, internally-owned ones (params UBO) written at construction and
//! re-written per frame by the host (`upload_params`, called from the
//! per-frame UBO site in `build_and_upload_instances.rs`).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    write_combined_image_sampler, write_storage_image, write_uniform_buffer,
    DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

const EXPOSURE_METER_COMP_SPV: &[u8] = include_bytes!("../../shaders/exposure_meter.comp.spv");

// One 64-thread workgroup performs the whole reduction — no cross-workgroup
// sync exists or is needed; the dispatch is always (1, 1, 1).

/// Host mirror of `exposure_meter.comp`'s `Params` UBO.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MeterParams {
    /// x = mode (0 fixed, 1 auto), y = fixed exposure, z = compensation in
    /// stops (positive = darker), w = adaptation alpha for this frame.
    pub mode: [f32; 4],
    /// x = min exposure clamp, y = max exposure clamp, zw unused.
    pub limits: [f32; 4],
}

// SAFETY: two `[f32; 4]` fields — no implicit padding possible (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for MeterParams {}

pub struct ExposureMeterPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    dsl: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    sampler: vk::Sampler,
    /// One per frame in flight: binding 0/1 are rewritten per dispatch
    /// (externally-owned views), binding 2's UBO is written per frame by
    /// [`Self::upload_params`].
    descriptor_sets: Vec<vk::DescriptorSet>,
    param_buffers: Vec<GpuBuffer>,
}

impl ExposureMeterPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<Self> {
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            dsl: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            sampler: vk::Sampler::null(),
            descriptor_sets: Vec::new(),
            param_buffers: Vec::new(),
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `destroy` is unsafe; on this error path the
                        // device is live and no meter command buffers are in
                        // flight (mid-construction).
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // Any valid sampler — the shader reads via texelFetch, which ignores
        // sampler state; the binding type just requires one.
        // SAFETY: trivial ash create; `device` live, create-info outlives call.
        partial.sampler = try_or_cleanup!(unsafe {
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
                .context("exposure meter sampler")
        });

        let bindings = [
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
            &bindings,
            &[ReflectedShader {
                name: "exposure_meter.comp",
                spirv: EXPOSURE_METER_COMP_SPV,
            }],
            "exposure_meter",
            &[],
        )
        .expect("exposure meter layout drifted against exposure_meter.comp");
        // SAFETY: `bindings` outlives the call; the returned layout is owned.
        partial.dsl = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("exposure meter DSL")
        });

        // SAFETY: `partial.dsl` just created and live.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.dsl)),
                    None,
                )
                .context("exposure meter pipeline layout")
        });

        partial.pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            EXPOSURE_METER_COMP_SPV,
            partial.pipeline_layout,
            "exposure meter",
        ));

        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "exposure meter descriptor pool"));

        let layouts = vec![partial.dsl; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: pool + layout are live and owned here.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("exposure meter descriptor sets")
        });

        let param_size = std::mem::size_of::<MeterParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buffer = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buffer);
        }
        // Write the internally-owned UBO binding now; 0/1 are per-dispatch.
        for frame in 0..MAX_FRAMES_IN_FLIGHT {
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[frame].buffer,
                offset: 0,
                range: param_size,
            }];
            let write =
                write_uniform_buffer(partial.descriptor_sets[frame], 2, &ubo_info);
            // SAFETY: freshly allocated set + buffer, not yet bound.
            unsafe { device.update_descriptor_sets(&[write], &[]) };
        }

        Ok(partial)
    }

    /// Per-frame parameter upload — the pass's only fallible step (mapped
    /// host write). Called from the per-frame UBO site before recording; a
    /// failure latches the pass off for the session (frozen exposure), the
    /// same discipline as `latch_taa_failure`.
    pub fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        params: MeterParams,
    ) -> Result<()> {
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))
    }

    /// Record the metering dispatch for `frame`. Pure command recording —
    /// infallible by construction (#3981 discipline).
    ///
    /// Layout contract: `scene_view` must be `SHADER_READ_ONLY_OPTIMAL`
    /// (composite's post-bloom steady state; `apply_to_scene`'s outgoing
    /// barrier already covers this dispatch's compute read). The exposure
    /// slot arrives `SHADER_READ_ONLY_OPTIMAL` and leaves in the same layout,
    /// ready for the FSR wrapper and `presentation.frag`'s sampler.
    ///
    /// # Safety
    /// `cmd` is recording outside a render pass; all handles live for this
    /// frame; no other in-flight command buffer accesses the exposure slot.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        scene_view: vk::ImageView,
        exposure_image: vk::Image,
        exposure_view: vk::ImageView,
    ) {
        let set = self.descriptor_sets[frame];
        let scene_info = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(scene_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let dst_info = [vk::DescriptorImageInfo::default()
            .image_view(exposure_view)
            .image_layout(vk::ImageLayout::GENERAL)];
        let writes = [
            write_combined_image_sampler(set, 0, &scene_info),
            write_storage_image(set, 1, &dst_info),
        ];
        // SAFETY: caller contract — the set is not in use by another
        // in-flight command buffer (per-frame slot ownership).
        unsafe { device.update_descriptor_sets(&writes, &[]) };

        let subresource = super::descriptors::color_subresource_single_mip();
        // SHADER_READ → GENERAL for the storage write. The src scope names
        // the slot's previous readers within frame-graph terms; cross-frame
        // reuse is already ordered by the per-slot fence.
        let to_general = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(exposure_image)
            .subresource_range(subresource);
        // SAFETY: `cmd` is recording (fn contract); the exposure image is
        // this frame's slot, in SHADER_READ_ONLY_OPTIMAL per the layout
        // contract, and is not concurrently accessed by another in-flight
        // command buffer.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_general],
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[set],
                &[],
            );
            device.cmd_dispatch(cmd, 1, 1, 1);

            // GENERAL → SHADER_READ: the written texel must be visible to
            // this frame's FSR dispatch (compute) and presentation pass
            // (fragment), both later in this same command buffer.
            let to_shader_read = super::descriptors::image_barrier_general_to_shader_read(
                exposure_image,
            )
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_shader_read],
            );
        }
    }

    /// # Safety
    /// No in-flight command buffer may reference this pipeline.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buffer in &mut self.param_buffers {
            buffer.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.pipeline, None);
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.dsl != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.dsl, None);
            self.dsl = vk::DescriptorSetLayout::null();
        }
        if self.sampler != vk::Sampler::null() {
            device.destroy_sampler(self.sampler, None);
            self.sampler = vk::Sampler::null();
        }
        self.descriptor_sets.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reduction fits one workgroup by construction — a larger thread
    /// count would need a second reduction stage that does not exist. The
    /// shader pins the same number in its `layout(local_size_x = 64)`.
    #[test]
    fn meter_fits_one_workgroup() {
        let shader = include_str!("../../shaders/exposure_meter.comp");
        assert!(
            shader.contains("layout(local_size_x = 64)"),
            "exposure_meter.comp changed its workgroup size — the Rust side \
             dispatches (1,1,1) and has no cross-workgroup reduction stage"
        );
    }

    /// The params UBO is two vec4s (32 B) on both sides of the wire; std140
    /// places no padding surprises at that shape, but pin it so a field added
    /// later must consciously re-check the GLSL block.
    /// #4597 — the shader must divide the tree-reduced log sum by the
    /// tree-reduced TOTAL sample count, not invocation 0's own count
    /// (~64 of ~4096): the old divisor produced exp2(64 × the geometric
    /// mean) — bang-bang metering pinned at the clamp limits. The shared
    /// `shared_count` reduction is the fix; this pin fails if the shader's
    /// divisor reverts to a per-invocation count or the count reduction
    /// disappears.
    #[test]
    fn the_exposure_average_divides_by_the_total_reduced_count() {
        let src = include_str!("../../shaders/exposure_meter.comp");
        // The count reduction rides the same tree loop as the log sum.
        assert!(
            src.contains("shared_count[gl_LocalInvocationID.x] += shared_count[gl_LocalInvocationID.x + s];"),
            "shared_count must reduce in the same tree loop as shared_log_sum (#4597)"
        );
        // The divisor is the reduced total, not the per-invocation count.
        assert!(
            src.contains("float samples = max(float(shared_count[0]), 1.0);"),
            "the average must divide by the reduced total count (#4597)"
        );
        assert!(
            !src.contains("float samples = max(float(count), 1.0);"),
            "the per-invocation `count` must not be the divisor (#4597)"
        );
    }

    #[test]
    fn meter_params_are_two_vec4s() {
        assert_eq!(std::mem::size_of::<MeterParams>(), 32);
    }
}
