//! SKYAL — sky cubemap bake.
//!
//! Evaluates the shared analytic sky (`shaders/include/sky.glsl`, the same
//! code `composite.frag` paints the background with) once per cube texel,
//! so the rest of the frame can sample the sky as a plain `samplerCube`.
//!
//! See `docs/engine/skyal.md` for why this is a bake rather than a call:
//! cost, roughness prefiltering, the fact that a volumetric cloud march is
//! only affordable baked — and the hard constraint that `triangle.frag`
//! binds the bindless arrays at set 0 while `composite.frag` uses set 1,
//! so `include/sky.glsl` cannot simply be included into the ray-tracing
//! path.
//!
//! The image is `CUBE_COMPATIBLE` and carries **two** views over the same
//! six layers: the `TYPE_2D_ARRAY` view `GpuImage` builds, which the bake
//! writes through (a storage image cannot be a cube view), and a `CUBE`
//! view this module creates for consumers to sample.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::cloud_noise::CloudNoiseViews;
use super::descriptors::{
    write_combined_image_sampler, write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::image::{GpuImage, GpuImageDesc};
use super::reflect::{validate_set_layout, ReflectedShader};
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
use anyhow::{Context, Result};
use ash::vk;

mod filter;
mod irradiance;
pub use irradiance::SH_BYTES as SKY_IRRADIANCE_BYTES;

const SKY_CUBE_COMP_SPV: &[u8] = include_bytes!("../../shaders/sky_cube.comp.spv");

/// Edge length of one cube face, in texels.
///
/// 128 is chosen against what the cubemap is *for*. It feeds reflections,
/// ray-traced miss shading and (later) an irradiance projection — all
/// low-frequency consumers. The background keeps evaluating the sky
/// analytically at full resolution precisely because a face this size
/// cannot hold a sharp sun disc, so nothing here needs to.
///
/// Cost scales as the square: 6 x 128^2 = 98'304 invocations.
pub const SKY_CUBE_FACE_SIZE: u32 = 128;
pub const SKY_CUBE_MIP_LEVELS: u32 = SKY_CUBE_FACE_SIZE.ilog2() + 1;

/// `R16G16B16A16_SFLOAT`. The sky is HDR — the sun disc reaches
/// `sun_color * sun_intensity * sun_glare`, well past 1.0 — so an 8-bit
/// format would clip exactly the values reflections most need. Alpha is
/// unused; the narrower `B10G11R11` the bloom chain uses was rejected
/// because its 5-bit exponent handles the sun's dynamic range less well
/// and the whole resource is under 1 MB per frame either way.
const SKY_CUBE_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// Number of cube faces. Named rather than spelled `6` at the four sites
/// that need it (image layers, view layers, dispatch depth, face switch).
const CUBE_FACES: u32 = 6;

/// Host mirror of `include/sky.glsl`'s `SkyDome`.
///
/// Field-for-field, same order, same names. `sky_dome.rs` pins the GLSL
/// struct's field list against every *shader-side* builder; this is the
/// host-side builder for the bake's own UBO, so it is pinned the same way
/// by `sky_cube_params_mirrors_the_sky_dome_struct`.
///
/// A `vec4` per field with no scalars means std140 and `#[repr(C)]` agree
/// trivially: every member is 16-byte aligned and 16 bytes wide, so there
/// is no padding to get wrong.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkyCubeParams {
    pub sky_zenith: [f32; 4],
    pub sky_horizon: [f32; 4],
    pub sky_lower: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_color: [f32; 4],
    pub cloud_params: [f32; 4],
    pub cloud_params_1: [f32; 4],
    pub cloud_params_2: [f32; 4],
    pub cloud_params_3: [f32; 4],
    pub cloud_tint_0: [f32; 4],
    pub cloud_tint_1: [f32; 4],
    pub cloud_tint_2: [f32; 4],
    pub cloud_tint_3: [f32; 4],
    pub weather_params: [f32; 4],
    pub weather_wind: [f32; 4],
    pub weather_lightning: [f32; 4],
    pub weather_sky: [f32; 4],
    pub weather_aurora: [f32; 4],
    pub depth_params: [f32; 4],
    pub sun_illuminance: [f32; 4],
}

// SAFETY: every field is `[f32; 4]` — homogeneous scalar arrays tile the
// struct's declared size with no implicit padding (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for SkyCubeParams {}

impl SkyCubeParams {
    /// Copy the sky fields out of the composite pass's own parameters.
    ///
    /// Deriving the bake's inputs from `CompositeParams` rather than
    /// rebuilding them from `SkyParams` is the point: the background and
    /// the bake then cannot disagree about what sky they are drawing, by
    /// construction rather than by two builders being kept in step. This
    /// is the host-side twin of `composite.frag`'s `build_sky_dome()`.
    pub fn from_composite(p: &crate::vulkan::composite::CompositeParams) -> Self {
        Self {
            sky_zenith: p.sky_zenith,
            sky_horizon: p.sky_horizon,
            sky_lower: p.sky_lower,
            sun_dir: p.sun_dir,
            sun_color: p.sun_color,
            cloud_params: p.cloud_params,
            cloud_params_1: p.cloud_params_1,
            cloud_params_2: p.cloud_params_2,
            cloud_params_3: p.cloud_params_3,
            cloud_tint_0: p.cloud_tint_0,
            cloud_tint_1: p.cloud_tint_1,
            cloud_tint_2: p.cloud_tint_2,
            cloud_tint_3: p.cloud_tint_3,
            weather_params: p.weather_params,
            weather_wind: p.weather_wind,
            weather_lightning: p.weather_lightning,
            weather_sky: p.weather_sky,
            weather_aurora: p.weather_aurora,
            depth_params: p.depth_params,
            sun_illuminance: p.sun_illuminance,
        }
    }
}

/// VRAM the sky cubemap holds, per frame in flight, in bytes.
///
/// Sum of all mip texels × six faces × eight bytes. Exposed so the memory budget can
/// account for it the way `SSAO_BYTES_PER_PIXEL` does — this one is not
/// render-extent-scaled, so it is a flat number rather than per-pixel.
pub const fn sky_cube_bytes_per_frame() -> u64 {
    let mut size = SKY_CUBE_FACE_SIZE as u64;
    let mut texels = 0;
    while size > 0 {
        texels += size * size;
        size /= 2;
    }
    CUBE_FACES as u64 * texels * 8
}

pub struct SkyCubePipeline {
    filter: Option<filter::SkyFilter>,
    irradiance: Option<irradiance::SkyIrradiance>,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame parameter UBOs.
    param_buffers: Vec<GpuBuffer>,
    /// Per-frame cube images, each with its `TYPE_2D_ARRAY` storage view.
    ///
    /// Per-frame for the #267 reason SSAO's AO target is: frame N's bake
    /// must not race frame N-1's shader reads of the same image.
    cubes: Vec<GpuImage>,
    /// `CUBE` views over `cubes[i]`, for consumers to sample. A second
    /// view over the same image rather than a second image.
    cube_views: Vec<vk::ImageView>,
    /// Sampler for the cube views. `CLAMP_TO_EDGE` on all three axes:
    /// cube sampling is direction-based, but the addressing mode still
    /// governs the edge taps a `LINEAR` filter makes across a face
    /// boundary, and `REPEAT` there wraps to the opposite edge of the same
    /// face — a visible seam.
    pub sampler: vk::Sampler,
}

impl SkyCubePipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        bindless_layout: vk::DescriptorSetLayout,
        noise: CloudNoiseViews,
        max_frames: usize,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            bindless_layout,
            noise,
            max_frames,
        );
        if let Err(ref e) = result {
            log::debug!("sky cubemap pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        bindless_layout: vk::DescriptorSetLayout,
        noise: CloudNoiseViews,
        max_frames: usize,
    ) -> Result<Self> {
        // Partially-valid Self so `destroy()` is the single cleanup path;
        // `vkDestroy*` on a null handle is a spec-guaranteed no-op.
        let mut partial = Self {
            filter: None,
            irradiance: None,
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            param_buffers: Vec::new(),
            cubes: Vec::new(),
            cube_views: Vec::new(),
            sampler: vk::Sampler::null(),
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `partial` holds only handles created by this
                        // device earlier in this initializer; none has been bound
                        // to an in-flight command buffer, so tearing them down in
                        // reverse creation order is sound.
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        for _ in 0..max_frames {
            let cube = try_or_cleanup!(GpuImage::create(
                device,
                allocator,
                &GpuImageDesc {
                    mip_levels: SKY_CUBE_MIP_LEVELS,
                    ..GpuImageDesc::color_cube(
                        "sky cubemap",
                        SKY_CUBE_FACE_SIZE,
                        SKY_CUBE_FORMAT,
                        vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                    )
                },
            ));
            // The sampled `CUBE` view over the same six layers. Created
            // after the image is pushed so the cleanup path owns the image
            // even if this call fails.
            partial.cubes.push(cube);
            let image = partial.cubes[partial.cubes.len() - 1].image;
            // SAFETY: `image` is the live cube-compatible image this device
            // just created, with `CUBE_FACES` array layers and the full mip chain —
            // exactly the range the create info names; the view is owned by
            // `partial` from the push below onward.
            let view = try_or_cleanup!(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(image)
                            .view_type(vk::ImageViewType::CUBE)
                            .format(SKY_CUBE_FORMAT)
                            .subresource_range(
                                vk::ImageSubresourceRange::default()
                                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                                    .base_mip_level(0)
                                    .level_count(SKY_CUBE_MIP_LEVELS)
                                    .base_array_layer(0)
                                    .layer_count(CUBE_FACES),
                            ),
                        None,
                    )
                    .context("sky cubemap CUBE view")
            });
            partial.cube_views.push(view);
        }

        // SAFETY: fully-populated create info; the handle is owned by
        // `partial` from the assignment onward.
        partial.sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .max_lod((SKY_CUBE_MIP_LEVELS - 1) as f32)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("sky cubemap sampler")
        });

        let param_size = std::mem::size_of::<SkyCubeParams>() as vk::DeviceSize;
        for _ in 0..max_frames {
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
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
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
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "sky_cube.comp",
                spirv: SKY_CUBE_COMP_SPV,
            }],
            "sky cubemap",
            &[],
        )
        .expect("sky cubemap descriptor layout drifted against sky_cube.comp (see #427)");

        // SAFETY: `bindings` is validated against the shader above.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("sky cubemap descriptor set layout")
        });

        // TWO sets, not one. `include/sky.glsl` declares the bindless
        // texture array at set 1 because the sky samples the WTHR cloud
        // layers and the CLMT sun sprite by index, so this pipeline
        // statically uses set 1 even though none of its *own* bindings live
        // there. Declaring only set 0 is accepted by the shader compiler and
        // rejected at pipeline creation
        // (VUID-VkComputePipelineCreateInfo-layout-07988) — caught by the
        // validation layers, invisible to `cargo test`.
        let set_layouts = [partial.descriptor_set_layout, bindless_layout];
        // SAFETY: set 0's layout was just created above; `bindless_layout`
        // is caller-owned and outlives this pipeline. The borrow lives only
        // for this call.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts),
                    None,
                )
                .context("sky cubemap pipeline layout")
        });

        partial.pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            SKY_CUBE_COMP_SPV,
            partial.pipeline_layout,
            "sky cubemap",
        ));

        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            max_frames as u32,
        )
        .max_sets(max_frames as u32)
        .build(device, "sky cubemap descriptor pool"));

        let layouts = vec![partial.descriptor_set_layout; max_frames];
        // SAFETY: the pool was just created with capacity for `max_frames`
        // sets of exactly this layout.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("sky cubemap descriptor sets")
        });

        for i in 0..max_frames {
            // The bake writes through the `TYPE_2D_ARRAY` view, in GENERAL.
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.cubes[i].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let param_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[i].buffer,
                offset: 0,
                range: param_size,
            }];
            // The shared cloud density volumes (`CloudNoiseVolumes`). Not
            // owned here: composite binds the same views, and the volumes
            // must outlive both pipelines.
            let base_info = [vk::DescriptorImageInfo::default()
                .sampler(noise.sampler)
                .image_view(noise.base)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let detail_info = [vk::DescriptorImageInfo::default()
                .sampler(noise.sampler)
                .image_view(noise.detail)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let set = partial.descriptor_sets[i];
            let writes = [
                write_storage_image(set, 0, &image_info),
                write_uniform_buffer(set, 1, &param_info),
                write_combined_image_sampler(set, 2, &base_info),
                write_combined_image_sampler(set, 3, &detail_info),
            ];
            // SAFETY: this runs during construction, before any command
            // buffer can reference these sets.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        partial.filter = Some(try_or_cleanup!(filter::SkyFilter::new(
            device,
            pipeline_cache,
            &partial.cubes,
            partial.sampler,
        )));
        partial.irradiance = Some(try_or_cleanup!(irradiance::SkyIrradiance::new(
            device,
            allocator,
            pipeline_cache,
            &partial.cubes,
            partial.sampler,
        )));
        Ok(partial)
    }

    /// The `CUBE` view consumers sample for frame slot `frame`.
    pub fn cube_view(&self, frame: usize) -> vk::ImageView {
        self.cube_views[frame]
    }

    /// Per-frame E/PI SH projection, produced together with the ready cube.
    pub fn irradiance_buffer(&self, frame: usize) -> vk::Buffer {
        self.irradiance
            .as_ref()
            .expect("fully constructed sky")
            .buffers[frame]
            .buffer
    }

    /// Upload this frame's sky parameters.
    pub fn upload_params(
        &mut self,
        device: &ash::Device,
        frame: usize,
        params: &SkyCubeParams,
    ) -> Result<()> {
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(params))
    }

    /// Record the bake with its own layout transitions.
    ///
    /// The cube is written as a storage image and read as a sampled cube,
    /// so it needs a transition on both sides. Keeping them here rather
    /// than at the call site is deliberate: the `GENERAL` ->
    /// `SHADER_READ_ONLY_OPTIMAL` half is the one a caller would forget,
    /// and the symptom — sampling an image in the wrong layout — is
    /// undefined behaviour that a validation-layer-free release build will
    /// happily render something plausible for.
    ///
    /// The pre-barrier uses `UNDEFINED` as the old layout, which discards
    /// the previous contents. That is correct and intentional: every texel
    /// is overwritten by the dispatch, and it means the first frame needs
    /// no separate initialisation path the way SSAO's AO image does.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `cmd` are valid and live, `cmd` is
    /// recording, the device is not lost, and this frame slot's cube is not
    /// concurrently accessed by another in-flight command buffer.
    pub unsafe fn record_bake(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        bindless_set: vk::DescriptorSet,
    ) {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(CUBE_FACES);

        let to_general = vk::ImageMemoryBarrier::default()
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.cubes[frame].image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_general],
        );

        self.dispatch(device, cmd, frame, bindless_set);

        let to_read = vk::ImageMemoryBarrier::default()
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(self.cubes[frame].image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_read],
        );
        if let Some(filter) = &self.filter {
            filter.record(device, cmd, self.cubes[frame].image, frame);
        }
        if let Some(irradiance) = &self.irradiance {
            irradiance.record(device, cmd, frame);
        }
    }

    /// Record the bake.
    ///
    /// One invocation per texel, `CUBE_FACES` deep. The caller owns the
    /// layout transitions around this: the image must be in `GENERAL`
    /// before the dispatch and transitioned to `SHADER_READ_ONLY_OPTIMAL`
    /// after it, before any sampled read.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `cmd` are valid and live, `cmd` is
    /// recording, the device is not lost, and this frame slot's cube image
    /// is not concurrently accessed by another in-flight command buffer.
    unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        bindless_set: vk::DescriptorSet,
    ) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame], bindless_set],
            &[],
        );
        device.cmd_dispatch(
            cmd,
            SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_X),
            SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_Y),
            CUBE_FACES,
        );
    }

    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that none of these resources are still in
    /// use by an in-flight command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if let Some(mut irradiance) = self.irradiance.take() {
            irradiance.destroy(device, allocator);
        }
        if let Some(mut filter) = self.filter.take() {
            filter.destroy(device);
        }
        // The extra CUBE views are ours, not `GpuImage`'s — destroy them
        // before the images they view.
        for view in self.cube_views.drain(..) {
            device.destroy_image_view(view, None);
        }
        for mut cube in self.cubes.drain(..) {
            cube.destroy(device, allocator);
        }
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        // #732 LIFE-N1 — drop the `GpuBuffer` structs after their GPU
        // allocations are freed so each one's allocator Arc clone releases
        // now rather than at the tail of `VulkanContext::Drop`.
        self.param_buffers.clear();
        device.destroy_sampler(self.sampler, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKY_GLSL: &str = include_str!("../../shaders/include/sky.glsl");
    const SKY_CUBE_COMP: &str = include_str!("../../shaders/sky_cube.comp");

    /// Host mirror of `sky_cube.comp`'s `sky_cube_face_direction`.
    ///
    /// Kept in lockstep by `face_direction_mirrors_the_shader_table`; the
    /// round-trip below is what actually validates it.
    fn face_direction(face: u32, u: f32, v: f32) -> [f32; 3] {
        match face {
            0 => [1.0, -v, -u],
            1 => [-1.0, -v, u],
            2 => [u, 1.0, v],
            3 => [u, -1.0, -v],
            4 => [u, -v, 1.0],
            _ => [-u, -v, -1.0],
        }
    }

    /// The Vulkan spec's cube-map face selection (§16.5.4), transcribed
    /// **forwards**: direction -> (face, s, t).
    ///
    /// This is an independent derivation, not a rearrangement of
    /// `face_direction`. Round-tripping one against the other is what makes
    /// the test meaningful — a sign flip in either shows up as a mismatch
    /// rather than cancelling out.
    fn select_face(d: [f32; 3]) -> (u32, f32, f32) {
        let [rx, ry, rz] = d;
        let (ax, ay, az) = (rx.abs(), ry.abs(), rz.abs());
        let (face, sc, tc, ma) = if ax >= ay && ax >= az {
            if rx > 0.0 {
                (0, -rz, -ry, ax)
            } else {
                (1, rz, -ry, ax)
            }
        } else if ay >= az {
            if ry > 0.0 {
                (2, rx, rz, ay)
            } else {
                (3, rx, -rz, ay)
            }
        } else if rz > 0.0 {
            (4, rx, -ry, az)
        } else {
            (5, -rx, -ry, az)
        };
        (face, (sc / ma + 1.0) / 2.0, (tc / ma + 1.0) / 2.0)
    }

    /// Every face, every sampled `(u, v)`, must come back as the same face
    /// at the same `(s, t)` under the spec's own selection rule.
    ///
    /// A mirrored or rotated face is the classic cubemap bug and it does
    /// not announce itself: the sky still looks like a sky, just wrong
    /// where it meets its neighbours. This is the check that stops it
    /// being "fixed" by flipping signs until the seam moves.
    #[test]
    fn face_directions_round_trip_through_the_vulkan_face_selection_rule() {
        for face in 0..CUBE_FACES {
            for iu in 0..7 {
                for iv in 0..7 {
                    let s = (iu as f32 + 0.5) / 7.0;
                    let t = (iv as f32 + 0.5) / 7.0;
                    let (u, v) = (2.0 * s - 1.0, 2.0 * t - 1.0);
                    let dir = face_direction(face, u, v);
                    let (got_face, got_s, got_t) = select_face(dir);
                    assert_eq!(
                        got_face, face,
                        "face {face} at (u={u}, v={v}) selects back as face {got_face}",
                    );
                    assert!(
                        (got_s - s).abs() < 1.0e-5 && (got_t - t).abs() < 1.0e-5,
                        "face {face}: (s={s}, t={t}) round-tripped to ({got_s}, {got_t}) — \
                         the face is mirrored or rotated",
                    );
                }
            }
        }
    }

    /// The mirror above only means something if it is the table the GPU
    /// runs.
    #[test]
    fn face_direction_mirrors_the_shader_table() {
        for row in [
            "case 0u: return vec3( 1.0,   -v,   -u); // +X",
            "case 1u: return vec3(-1.0,   -v,    u); // -X",
            "case 2u: return vec3(   u,  1.0,    v); // +Y",
            "case 3u: return vec3(   u, -1.0,   -v); // -Y",
            "case 4u: return vec3(   u,   -v,  1.0); // +Z",
            "default: return vec3(  -u,   -v, -1.0); // -Z",
        ] {
            assert!(
                include_str!("../../shaders/include/sky_cube_direction.glsl").contains(row),
                "sky_cube.comp's face table no longer contains `{row}` — the host \
                 mirror in this module is now validating a table the GPU does not run",
            );
        }
    }

    /// The bake's UBO is a host-filled `SkyDome`. Same completeness hazard
    /// as the shader-side builders `sky_dome.rs` guards: a field present in
    /// the GLSL struct but missing here shifts every subsequent field by 16
    /// bytes, and the sky silently reads the wrong parameters.
    #[test]
    fn sky_cube_params_mirrors_the_sky_dome_struct() {
        let fields = super::super::sky_dome::sky_dome_fields(SKY_GLSL);
        let src = include_str!("sky_cube.rs");
        let decl = src
            .split_once("pub struct SkyCubeParams {")
            .expect("SkyCubeParams is still declared")
            .1
            .split_once('}')
            .expect("its declaration is still terminated")
            .0;
        let mirrored: Vec<&str> = decl
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub "))
            .filter_map(|l| l.split(':').next())
            .collect();
        assert_eq!(
            mirrored, fields,
            "SkyCubeParams must mirror SkyDome field-for-field IN ORDER — a missing \
             or reordered field shifts every later one by 16 bytes and the bake reads \
             the wrong sky",
        );
    }

    /// The dispatch must cover every texel of every face. A `div_ceil` that
    /// became a plain divide would silently leave the last partial
    /// workgroup's texels unwritten.
    #[test]
    fn the_dispatch_covers_every_texel_of_every_face() {
        let groups_x = SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_X);
        let groups_y = SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_Y);
        assert!(groups_x * WORKGROUP_X >= SKY_CUBE_FACE_SIZE);
        assert!(groups_y * WORKGROUP_Y >= SKY_CUBE_FACE_SIZE);

        let src = include_str!("sky_cube.rs");
        let body = src
            .split_once("unsafe fn dispatch(")
            .expect("dispatch still exists")
            .1;
        assert!(
            body.contains("SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_X)")
                && body.contains("SKY_CUBE_FACE_SIZE.div_ceil(WORKGROUP_Y)")
                && body.contains("CUBE_FACES,"),
            "the dispatch must round the face up to whole workgroups and be \
             CUBE_FACES deep",
        );
        assert!(
            SKY_CUBE_COMP.contains("coord.z >= size.z"),
            "the shader must bound-check the layer too, not just x/y",
        );
    }

    /// Texel centres, not corners. Sampling the corner biases every face by
    /// half a texel and leaves a visible seam under linear filtering — the
    /// kind of thing that is invisible until someone looks at a reflection
    /// of the horizon.
    #[test]
    fn the_bake_samples_texel_centres() {
        assert!(
            SKY_CUBE_COMP.contains("(vec2(coord.xy) + 0.5) / vec2(size.xy) * 2.0 - 1.0"),
            "sky_cube.comp must map the texel CENTRE into [-1, 1]",
        );
    }

    /// Every sky-cube consumer must gate on the ready flag.
    ///
    /// The bake is optional and set 1 / binding 20 is PARTIALLY_BOUND, so
    /// an ungated read samples a descriptor that was never written when the
    /// pipeline failed to initialise. Nothing else catches this: it needs a
    /// VRAM-pressure failure at init to reach, and the result is undefined
    /// data rather than a crash.
    ///
    /// #4292 — the gate lives once, in `exteriorSkyRadianceOr`
    /// (`include/bindings.glsl`). This used to hand-list two consumer files,
    /// which is how three exterior sky-escape sites kept returning the flat
    /// pre-SKYAL blend unnoticed. Now it walks the whole shader tree: no
    /// source but the helper may sample `skyCube`, and every known escape
    /// site must call the helper.
    #[test]
    fn every_sky_cube_consumer_gates_on_the_ready_flag() {
        let bindings = include_str!("../../shaders/include/bindings.glsl");
        let helper = bindings
            .split_once(
                "vec3 exteriorSkyRadianceOr(vec3 direction, vec3 fallback, float roughness) {",
            )
            .expect("bindings.glsl must define the one gated sky-cube sampler")
            .1
            .split_once("\n}")
            .expect("unterminated exteriorSkyRadianceOr")
            .0;
        assert!(
            helper.contains("exteriorSkyTint.w > 0.5") && helper.contains("(skyCube,"),
            "exteriorSkyRadianceOr must sample skyCube only behind the ready flag — when \
             the bake fails to initialise, binding 20 is never written",
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
        for dir in [root.clone(), root.join("include")] {
            for entry in std::fs::read_dir(&dir).expect("shader directory must exist") {
                let path = entry.expect("readable dir entry").path();
                let is_glsl = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext, "vert" | "frag" | "comp" | "glsl"));
                if !is_glsl {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("readable shader source");
                let samples = text.matches("(skyCube").count();
                let allowed = 2 * usize::from(path.ends_with("include/bindings.glsl"));
                assert_eq!(
                    samples,
                    allowed,
                    "{} passes skyCube to a sampling call directly; route it through \
                     exteriorSkyRadianceOr so the ready-flag gate stays in one place",
                    path.display(),
                );
            }
        }

        for (name, src, sites) in [
            (
                "raytrace.glsl",
                include_str!("../../shaders/include/raytrace.glsl"),
                1,
            ),
            (
                "lighting.glsl",
                include_str!("../../shaders/include/lighting.glsl"),
                1,
            ),
            (
                "triangle.frag",
                include_str!("../../shaders/triangle.frag"),
                2,
            ),
            ("water.frag", include_str!("../../shaders/water.frag"), 1),
        ] {
            assert!(
                src.matches("exteriorSkyRadianceOr(").count() >= sites,
                "{name} must resolve each exterior sky escape through \
                 exteriorSkyRadianceOr (#4292)",
            );
        }
    }

    /// The bake statically uses set 1 (the bindless array, via
    /// `include/sky.glsl`) even though none of its own bindings live there.
    ///
    /// Declaring only set 0 compiles fine and is rejected at pipeline
    /// creation with VUID-VkComputePipelineCreateInfo-layout-07988 — a
    /// validation-layer-only failure. Both halves were hit for real while
    /// wiring this: the missing set, and then the bindless layout's stage
    /// flags not including COMPUTE.
    #[test]
    fn the_bake_declares_both_the_sets_it_uses() {
        let src = include_str!("sky_cube.rs");
        assert!(
            src.contains("let set_layouts = [partial.descriptor_set_layout, bindless_layout];"),
            "the bake's pipeline layout must declare set 0 AND the bindless set 1",
        );
        let sky = include_str!("../../shaders/include/sky.glsl");
        assert!(
            sky.contains("layout(set = 1, binding = 0) uniform sampler2D textures[];"),
            "sky.glsl is what puts the bindless array in set 1 — if it moved, the \
             pipeline layout above is now wrong",
        );

        let registry = include_str!("../texture_registry/mod.rs");
        assert!(
            registry.contains("ShaderStageFlags::FRAGMENT | vk::ShaderStageFlags::COMPUTE"),
            "the bindless descriptor layout must expose the texture array to COMPUTE — \
             the bake samples WTHR cloud layers through it, and a FRAGMENT-only \
             stageFlags fails pipeline creation",
        );
    }

    /// The cloud march must take its coverage from the canonical lane EXAL
    /// already derives per weather, not from an invented WTHR mapping.
    ///
    /// `SkyDome::weather_aurora.z` is the procedural-cloud coverage the
    /// authored cloud-plane path already uses. Deriving a second, different
    /// coverage from the WTHR classification flags would be exactly the
    /// guess `docs/engine/skyal.md` flags as unresolved — it has to be
    /// measured against real data, the way the WATR DATA offsets were.
    #[test]
    fn the_cloud_march_uses_the_canonical_coverage_lane() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        assert!(
            clouds.contains("float coverage = clamp(dome.weather_aurora.z, 0.0, 1.0);"),
            "the cloud march must read the same coverage lane the authored cloud-plane \
             path does — a second mapping here would be an invented WTHR derivation",
        );
        // The 2D body that used to read this lane is gone, so the march is the
        // only cloud coverage reader. A second derivation appearing in
        // sky.glsl would be two cloud bodies disagreeing about the weather.
        let sky = include_str!("../../shaders/include/sky.glsl");
        assert!(
            !sky.contains("weather_aurora.z"),
            "sky.glsl reads cloud coverage itself again — only include/clouds.glsl may",
        );
    }

    #[test]
    fn cloud_morphology_keeps_weather_drivers() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        for term in [
            "float precipitation = max(dome.weather_params.x, dome.weather_params.y);",
            "max(precipitation, dome.weather_params.z)",
            "vec3 morphology = cloud_morphology(dome);",
        ] {
            assert!(
                clouds.contains(term),
                "weather-driven cloud morphology lost `{term}`"
            );
        }
    }

    /// The terms the march is built from — shape from Schneider & Vos 2015,
    /// lighting from Hillaire 2016. Each one is load-bearing and degrades
    /// silently rather than failing if dropped: without the remap, coverage
    /// thins clouds everywhere instead of eroding them; without the octaves
    /// and their calibration, clouds render darker than the sky behind them
    /// (measured); without the height gradient, the shell cuts clouds flat.
    ///
    /// Schneider's powder term is deliberately absent: its required
    /// view-dependent gradient is unspecified in the source (slide 66), and
    /// Hillaire's energy-conserving model the rest of the lighting follows
    /// does not use it.
    #[test]
    fn the_cloud_march_keeps_its_reference_terms() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        assert!(
            !clouds.contains("float powder"),
            "the powder term must not return until its view-dependent gradient is \
             sourced — see this test's doc",
        );
        for (term, why) in [
            ("cloud_henyey_greenstein", "forward-scattering phase"),
            (
                "cloud_remap(base, 1.0 - coverage, 1.0, 0.0, 1.0)",
                "coverage SUBTRACTED, not multiplied",
            ),
            ("cloud_height_gradient", "vertical density profile"),
            (
                "exp(-light_optical_depth * extinction_scale)",
                "Beer-Lambert self-shadowing per octave",
            ),
            (
                "cloud_phase(cos_angle, eccentricity_scale)",
                "per-octave phase eccentricity (Wrenninge c^n)",
            ),
            (
                "scattering_scale *= CLOUD_MS_SCATTERING_FALLOFF",
                "per-octave scattering falloff (Hillaire Eq. 20, a^n)",
            ),
            (
                "12.566370614359172 / octave_scattering_sum",
                "diffuse-surface calibration K = 4pi / sum(a^n)",
            ),
            (
                "medium_slab(sigma_t, step_size)",
                "energy-conserving slab integration",
            ),
        ] {
            assert!(
                clouds.contains(term),
                "the cloud march has lost its {why} (`{term}`)",
            );
        }
    }

    /// The self-shadow march is spaced geometrically from one mean free path
    /// to the shell top. Evenly spaced 583 m light steps aliased the same way
    /// the fixed view step did, leaving grain on cloud edges and undersides.
    #[test]
    fn the_light_march_is_geometric_from_one_mean_free_path() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        for (term, why) in [
            (
                "float light_first = mean_free_path;",
                "first light sample one mean free path out",
            ),
            (
                "cloud_shell_distance(sun_dir, max(position.y, 0.0), CLOUD_LAYER_TOP)",
                "last light sample at the shell top along the sun direction",
            ),
            (
                "light_distance *= light_ratio;",
                "geometric spacing (Hillaire 2016 §5.5.2)",
            ),
            (
                "light_first * pow(light_ratio, jitter - 0.5)",
                "light samples jittered within their cells (zero offset in the bake)",
            ),
            (
                "* (light_distance - light_previous);",
                "each sample weighted by its own segment",
            ),
        ] {
            assert!(
                clouds.contains(term),
                "the light march has lost its {why} (`{term}`)"
            );
        }
        assert!(
            !clouds.contains("float light_step ="),
            "the evenly spaced light march must not return — it aliased",
        );
    }

    /// The adaptive march's load-bearing pieces. A fixed step aliased at the
    /// sourced extinction (optical depth ~9 per step, visible as terraces
    /// or, jittered, as grain); each of these is what prevents that, and
    /// each fails silently if dropped.
    #[test]
    fn the_cloud_march_steps_adaptively() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        for (term, why) in [
            (
                "cloud_base_shape(position, height_fraction, coverage, wind, morphology, base_noise)",
                "cheap base-shape-only samples until the iso-surface (Schneider slides 74-77)",
            ),
            (
                "t = max(t - cheap_step, marched_until);",
                "step back on entry, never behind already-integrated distance",
            ),
            (
                "min(mean_free_path / density, cheap_step)",
                "full step = one mean free path at the local density (optical depth <= 1)",
            ),
            (
                "float(CLOUD_CHEAP_SAMPLES_HORIZON),",
                "cheap budget interpolated toward 128 at the horizon (slide 80)",
            ),
            (
                "i < int(CLOUD_MAX_MARCH_ITERATIONS)",
                "the derived iteration bound",
            ),
            (
                "marched_until = t;",
                "progress tracking that prevents re-entry loops",
            ),
        ] {
            assert!(
                clouds.contains(term),
                "the cloud march has lost its {why} (`{term}`)"
            );
        }
        assert!(
            !clouds.contains("CLOUD_VIEW_STEPS"),
            "the fixed-step march must not return — it aliased at the sourced extinction",
        );
    }

    /// The bake binds the shared cloud volumes; it must not own a copy.
    /// Composite marches the same field for the visible sky, so a private
    /// set here would let reflections and the sky seen directly disagree —
    /// and would be a third upload of texels two resources already hold.
    #[test]
    fn the_bake_binds_the_shared_cloud_volumes_rather_than_owning_them() {
        let src = include_str!("sky_cube.rs");
        let production = src.split("#[cfg(test)]").next().expect("production half");
        assert!(
            !production.contains("cached_base_density_noise")
                && !production.contains("initialize_noise"),
            "SkyCubePipeline must not create or upload its own cloud noise — bind \
             CloudNoiseVolumes' views instead",
        );
        for binding in [
            "write_combined_image_sampler(set, 2, &base_info)",
            "write_combined_image_sampler(set, 3, &detail_info)",
        ] {
            assert!(
                production.contains(binding),
                "the bake must still write `{binding}`"
            );
        }
        assert!(
            production.contains(".image_view(noise.base)")
                && production.contains(".image_view(noise.detail)"),
            "bindings 2/3 must come from the shared CloudNoiseViews",
        );
    }

    /// VRAM accounting stays derived from the face size, not restated.
    #[test]
    fn the_vram_figure_follows_the_face_size() {
        // Complete power-of-two pyramid: (4 * base texels - 1) / 3.
        assert_eq!(sky_cube_bytes_per_frame(), 6 * (4 * 128 * 128 - 1) / 3 * 8);
        assert_eq!(SKY_CUBE_FACE_SIZE, 128, "update the figure above with it");
    }
}
