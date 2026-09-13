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
use super::descriptors::{
    write_combined_image_sampler, write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::image::{GpuImage, GpuImageDesc};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::volumetrics::noise::{
    cached_base_density_noise, cached_detail_density_noise, BASE_NOISE_SIZE, DETAIL_NOISE_SIZE,
};
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
use anyhow::{Context, Result};
use ash::vk;

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
        }
    }
}

/// VRAM the sky cubemap holds, per frame in flight, in bytes.
///
/// `6 * size^2 * 8` (four half-floats). Exposed so the memory budget can
/// account for it the way `SSAO_BYTES_PER_PIXEL` does — this one is not
/// render-extent-scaled, so it is a flat number rather than per-pixel.
pub const fn sky_cube_bytes_per_frame() -> u64 {
    CUBE_FACES as u64 * (SKY_CUBE_FACE_SIZE as u64) * (SKY_CUBE_FACE_SIZE as u64) * 8
}

pub struct SkyCubePipeline {
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
    /// Cloud density volumes for the raymarch — the SAME two
    /// `volumetrics/noise.rs` generates (Perlin-Worley base, Worley
    /// detail), which is the pair Schneider & Vos specify. Owned here
    /// rather than borrowed from `VolumetricsPipeline` because that
    /// pipeline is optional AND rebuilt on every resize, so its views
    /// would dangle in this descriptor set.
    cloud_base_noise: Option<GpuImage>,
    cloud_detail_noise: Option<GpuImage>,
    /// Sampler for the noise volumes. `REPEAT`, because both volumes are
    /// generated tileable and the march relies on that to advect a
    /// world-space field through them without a seam.
    noise_sampler: vk::Sampler,
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
        max_frames: usize,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            bindless_layout,
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
        max_frames: usize,
    ) -> Result<Self> {
        // Partially-valid Self so `destroy()` is the single cleanup path;
        // `vkDestroy*` on a null handle is a spec-guaranteed no-op.
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            param_buffers: Vec::new(),
            cubes: Vec::new(),
            cube_views: Vec::new(),
            cloud_base_noise: None,
            cloud_detail_noise: None,
            noise_sampler: vk::Sampler::null(),
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
                &GpuImageDesc::color_cube(
                    "sky cubemap",
                    SKY_CUBE_FACE_SIZE,
                    SKY_CUBE_FORMAT,
                    vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                ),
            ));
            // The sampled `CUBE` view over the same six layers. Created
            // after the image is pushed so the cleanup path owns the image
            // even if this call fails.
            partial.cubes.push(cube);
            let image = partial.cubes[partial.cubes.len() - 1].image;
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
                                    .level_count(1)
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
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("sky cubemap sampler")
        });

        for (slot, size) in [(0usize, BASE_NOISE_SIZE), (1usize, DETAIL_NOISE_SIZE)] {
            let image = try_or_cleanup!(GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_3d(
                    "sky cloud noise",
                    vk::Extent3D {
                        width: size,
                        height: size,
                        depth: size,
                    },
                    vk::Format::R8_UNORM,
                    vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                ),
            ));
            if slot == 0 {
                partial.cloud_base_noise = Some(image);
            } else {
                partial.cloud_detail_noise = Some(image);
            }
        }

        // SAFETY: fully-populated create info; the handle is owned by
        // `partial` from the assignment onward.
        partial.noise_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT),
                    None,
                )
                .context("sky cloud noise sampler")
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
            let base_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.noise_sampler)
                .image_view(partial.cloud_base_noise.as_ref().expect("base noise").view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let detail_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.noise_sampler)
                .image_view(
                    partial
                        .cloud_detail_noise
                        .as_ref()
                        .expect("detail noise")
                        .view,
                )
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

        Ok(partial)
    }

    /// Upload the two cloud density volumes and move them into their
    /// sampled layout.
    ///
    /// Separate from `new` for the reason `SsaoPipeline::initialize_ao_images`
    /// is: it needs a queue and a command pool to run a one-time submit,
    /// which construction does not otherwise require. Must be called before
    /// the first `record_bake`, or the march samples images still in
    /// `UNDEFINED`.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device`, `queue` and `pool` are valid and live,
    /// the device is not lost, and no command buffer is in flight against
    /// these freshly-created images (they cannot be — nothing has published
    /// this pipeline yet).
    pub unsafe fn initialize_noise(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        // Memoized in `volumetrics::noise` — regenerating ~10^7 hashes here
        // would duplicate work the froxel pipeline has already paid for.
        let payloads = [
            (cached_base_density_noise(), BASE_NOISE_SIZE, true),
            (cached_detail_density_noise(), DETAIL_NOISE_SIZE, false),
        ];
        let mut staging: Vec<GpuBuffer> = Vec::with_capacity(2);
        for (bytes, _, _) in payloads {
            let mut buffer = match GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
            ) {
                Ok(buffer) => buffer,
                Err(error) => {
                    for mut done in staging {
                        done.destroy(device, allocator);
                    }
                    return Err(error);
                }
            };
            if let Err(error) = buffer.write_mapped(device, bytes) {
                buffer.destroy(device, allocator);
                for mut done in staging {
                    done.destroy(device, allocator);
                }
                return Err(error);
            }
            staging.push(buffer);
        }

        let range = super::descriptors::color_subresource_single_mip();
        let result = super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let images = [
                self.cloud_base_noise.as_ref().expect("base noise").image,
                self.cloud_detail_noise
                    .as_ref()
                    .expect("detail noise")
                    .image,
            ];
            let to_dst = images
                .map(|image| super::descriptors::image_barrier_undef_to_transfer_dst(image, 1));
            // SAFETY: `cmd` is recording; both images are freshly allocated
            // and exclusively owned by this pipeline.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_dst,
                );
            }

            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            for (index, (_, size, _)) in payloads.iter().enumerate() {
                let copy = vk::BufferImageCopy::default()
                    .image_subresource(subresource)
                    .image_extent(vk::Extent3D {
                        width: *size,
                        height: *size,
                        depth: *size,
                    });
                // SAFETY: the staging buffer is live and fully populated;
                // the destination is in TRANSFER_DST_OPTIMAL with a matching
                // R8 extent and TRANSFER_DST usage.
                unsafe {
                    device.cmd_copy_buffer_to_image(
                        cmd,
                        staging[index].buffer,
                        images[index],
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[copy],
                    );
                }
            }

            let ready = images.map(|image| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(image)
                    .subresource_range(range)
            });
            // SAFETY: `cmd` is recording and both images were written by the
            // copies above; this publishes those writes and moves each image
            // into the layout its descriptor declares.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &ready,
                );
            }
            Ok(())
        });

        // The one-time submit waits on the queue before returning, so no
        // staging buffer can still be referenced here.
        for mut buffer in staging {
            buffer.destroy(device, allocator);
        }
        result
    }

    /// The `CUBE` view consumers sample for frame slot `frame`.
    pub fn cube_view(&self, frame: usize) -> vk::ImageView {
        self.cube_views[frame]
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
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image(self.cubes[frame].image)
            .subresource_range(range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_read],
        );
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
        // The extra CUBE views are ours, not `GpuImage`'s — destroy them
        // before the images they view.
        for view in self.cube_views.drain(..) {
            device.destroy_image_view(view, None);
        }
        for mut cube in self.cubes.drain(..) {
            cube.destroy(device, allocator);
        }
        for mut noise in [self.cloud_base_noise.take(), self.cloud_detail_noise.take()]
            .into_iter()
            .flatten()
        {
            noise.destroy(device, allocator);
        }
        device.destroy_sampler(self.noise_sampler, None);
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
                SKY_CUBE_COMP.contains(row),
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
            .split_once("pub unsafe fn dispatch(")
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

    /// Both sky-cube consumers must gate on the ready flag.
    ///
    /// The bake is optional and set 1 / binding 20 is PARTIALLY_BOUND, so
    /// an ungated read samples a descriptor that was never written when the
    /// pipeline failed to initialise. Nothing else catches this: it needs a
    /// VRAM-pressure failure at init to reach, and the result is undefined
    /// data rather than a crash.
    #[test]
    fn every_sky_cube_consumer_gates_on_the_ready_flag() {
        for (name, src) in [
            (
                "raytrace.glsl",
                include_str!("../../shaders/include/raytrace.glsl"),
            ),
            (
                "lighting.glsl",
                include_str!("../../shaders/include/lighting.glsl"),
            ),
        ] {
            assert!(
                src.contains("texture(skyCube,"),
                "{name} is expected to consume the baked sky cubemap",
            );
            assert!(
                src.contains("exteriorSkyTint.w > 0.5"),
                "{name} samples skyCube without gating on the ready flag — when the \
                 bake fails to initialise, binding 20 is never written and this reads \
                 an unwritten descriptor",
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
        let sky = include_str!("../../shaders/include/sky.glsl");
        assert!(
            sky.contains("clamp(dome.weather_aurora.z, 0.0, 1.0)"),
            "that lane is supposed to be the one `weather_procedural_cloud` already \
             uses; if it moved, the two cloud representations have diverged",
        );
    }

    /// The pieces of Schneider & Vos the march is built from. Each one is
    /// load-bearing and silently degrades rather than failing if dropped:
    /// without the remap, coverage thins clouds everywhere instead of
    /// eroding them; without powder, lit edges are as dark as cores;
    /// without the height gradient, the shell cuts clouds off flat.
    #[test]
    fn the_cloud_march_keeps_its_reference_terms() {
        let clouds = include_str!("../../shaders/include/clouds.glsl");
        for (term, why) in [
            ("cloud_henyey_greenstein", "forward-scattering phase"),
            (
                "cloud_remap(base, 1.0 - coverage, 1.0, 0.0, 1.0)",
                "coverage SUBTRACTED, not multiplied",
            ),
            ("cloud_height_gradient", "vertical density profile"),
            (
                "1.0 - exp(-density * step_size",
                "powder / inverse-Beer term",
            ),
            ("exp(-light_optical_depth", "Beer-Lambert self-shadowing"),
            (
                "1.0 - sample_transmittance",
                "energy-conserving slab integration",
            ),
        ] {
            assert!(
                clouds.contains(term),
                "the cloud march has lost its {why} (`{term}`)",
            );
        }
    }

    /// The march samples the volumes `volumetrics/noise.rs` generates, and
    /// they must be uploaded before the first bake — otherwise it reads
    /// images still in `UNDEFINED`.
    #[test]
    fn the_cloud_noise_is_shared_and_uploaded_before_the_first_bake() {
        let src = include_str!("sky_cube.rs");
        assert!(
            src.contains("cached_base_density_noise()")
                && src.contains("cached_detail_density_noise()"),
            "the cloud volumes must be the ones volumetrics/noise.rs already generates, \
             not a second near-identical set",
        );
        let init = include_str!("context/init.rs");
        let ctor = init
            .split_once("SkyCubePipeline::new(")
            .expect("the sky cube is still constructed at init")
            .1;
        let ctor = &ctor[..ctor.len().min(2000)];
        assert!(
            ctor.contains("initialize_noise("),
            "init must upload the cloud noise before publishing the pipeline — the \
             march otherwise samples images still in UNDEFINED layout",
        );
    }

    /// VRAM accounting stays derived from the face size, not restated.
    #[test]
    fn the_vram_figure_follows_the_face_size() {
        assert_eq!(sky_cube_bytes_per_frame(), 6 * 128 * 128 * 8);
        assert_eq!(SKY_CUBE_FACE_SIZE, 128, "update the figure above with it");
    }
}
