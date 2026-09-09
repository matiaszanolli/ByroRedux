//! Volumetrics pipeline construction — the one-time build path.
//!
//! Split out of `volumetrics.rs` under #2256, which tracked that file
//! crossing the 2000-LOC threshold behind the Session-62 fog/shadow-policy
//! feature arc. The seam is the one the issue proposed and the one
//! `context/{mod,draw,resize,…}.rs` already established for `VulkanContext`:
//! **construct here, record next door**. This module owns `new` /
//! `new_inner` / `create_volume` / `initialize_layouts`; the per-frame
//! recording path (`dispatch`, `record_neutral_frame`, the `write_*`
//! descriptor updates) stays in the parent, along with every pure helper and
//! GPU-struct definition the shader contract is pinned against.
//!
//! Nothing here changed in the move — the functions were relocated verbatim,
//! and the `impl VolumetricsPipeline` block below continues the parent's, so
//! `VolumetricsPipeline::new` and `initialize_layouts` remain inherent
//! methods and no call site outside this crate moved with them.

use super::*;
// The relocated code addressed these as `super::<mod>` while it lived one
// level up in `volumetrics.rs`. Importing them by name — rather than
// re-spelling six call sites as `super::super::<mod>` — keeps every moved
// line at or below its original length, so the move introduces no
// reformatting and stays diffable against the original.
use super::super::{descriptors, pipeline, texture};

impl VolumetricsPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        render_extent: vk::Extent2D,
        config: VolumetricsConfig,
    ) -> Result<Self> {
        let config = config.validate()?;
        let result = Self::new_inner(device, allocator, pipeline_cache, render_extent, config);
        if let Err(ref e) = result {
            log::debug!("Volumetrics pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        render_extent: vk::Extent2D,
        config: VolumetricsConfig,
    ) -> Result<Self> {
        let extent = froxel_extent(render_extent, config);
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            history_sampler: vk::Sampler::null(),
            transport_sampler: vk::Sampler::null(),
            density_noise_sampler: vk::Sampler::null(),
            base_noise_volume: None,
            detail_noise_volume: None,
            extent,
            config,
            lighting_volumes: Vec::new(),
            emission_history_volumes: Vec::new(),
            combustion_state_volumes: Vec::new(),
            combustion_dynamics_volumes: Vec::new(),
            combustion_optical_volumes: Vec::new(),
            param_buffers: Vec::new(),
            fog_volume_buffers: Vec::new(),
            fog_cluster_buffers: Vec::new(),
            fog_cluster_index_buffers: Vec::new(),
            combustion_light_moment_buffers: Vec::new(),
            combustion_light_grid_centers: [[0.0; 3]; MAX_FRAMES_IN_FLIGHT],
            combustion_light_grid_valid: [false; MAX_FRAMES_IN_FLIGHT],
            // `true`: the allocation is not zeroed, so the first drain must
            // still run its `fill(0)` to prime it for the first `atomicAdd`.
            combustion_moment_dirty: [true; MAX_FRAMES_IN_FLIGHT],
            combustion_light_candidates: Vec::new(),
            last_combustion_light_topology: None,
            fog_volume_upload: Box::new(GpuFogVolumeUpload::default()),
            fog_cluster_entries: fog_cluster_entries_with_offsets(),
            fog_cluster_indices: Box::new([0; FOG_VOLUME_INDEX_COUNT]),
            // Full extent: forces the first write to each buffer to cover the
            // whole range, since the allocation is not zero-initialised.
            fog_cluster_dirty_hi: [FOG_VOLUME_CLUSTER_COUNT; MAX_FRAMES_IN_FLIGHT],
            integration_pipeline: vk::Pipeline::null(),
            integration_pipeline_layout: vk::PipelineLayout::null(),
            integration_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            integration_descriptor_pool: vk::DescriptorPool::null(),
            integration_descriptor_sets: Vec::new(),
            integrated_volumes: Vec::new(),
            integration_param_buffers: Vec::new(),
            history_valid: false,
            dispatched_this_frame: false,
            last_simulation_time_seconds: None,
            pending_simulation_time_seconds: None,
            combustion_active_until_seconds: f32::NEG_INFINITY,
            tlas_written: [false; MAX_FRAMES_IN_FLIGHT],
            lights_written: [false; MAX_FRAMES_IN_FLIGHT],
            boundary_geometry_written: [false; MAX_FRAMES_IN_FLIGHT],
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: cleanup path on construction failure; `partial` owns only
                        // objects created so far in this fn, none submitted to the GPU, and
                        // `device`/`allocator` outlive this call.
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // ── 1. Allocate per-frame-in-flight froxel volumes ────────────
        // Six volumes per frame: lighting (injection output → integration
        // input), integrated (integration output → composite read), scalar
        // emission provenance, and three transported-combustion history fields.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_lighting_{i}"),
                extent,
                FROXEL_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.lighting_volumes.push(slot);
            let emission_history = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_emission_history_{i}"),
                extent,
                EMISSION_HISTORY_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.emission_history_volumes.push(emission_history);
            let combustion_state = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_state_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.combustion_state_volumes.push(combustion_state);
            let combustion_dynamics = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_dynamics_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial
                .combustion_dynamics_volumes
                .push(combustion_dynamics);
            let combustion_optical = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_optical_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.combustion_optical_volumes.push(combustion_optical);
            let integrated = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_integrated_{i}"),
                extent,
                FROXEL_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.integrated_volumes.push(integrated);
        }
        partial.base_noise_volume = Some(try_or_cleanup!(Self::create_volume(
            device,
            allocator,
            "volumetrics_base_noise",
            vk::Extent3D {
                width: BASE_NOISE_SIZE,
                height: BASE_NOISE_SIZE,
                depth: BASE_NOISE_SIZE,
            },
            DENSITY_NOISE_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )));
        partial.detail_noise_volume = Some(try_or_cleanup!(Self::create_volume(
            device,
            allocator,
            "volumetrics_detail_noise",
            vk::Extent3D {
                width: DETAIL_NOISE_SIZE,
                height: DETAIL_NOISE_SIZE,
                depth: DETAIL_NOISE_SIZE,
            },
            DENSITY_NOISE_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )));

        // History must never filter across XY froxel boundaries: adjacent
        // columns can be separated by opaque structure. The shader performs
        // explicit nearest-XY/linear-Z texelFetch reconstruction, and the
        // nearest sampler keeps accidental future texture() calls conservative.
        // All volumes remain in GENERAL for storage writes and history reads.
        // SAFETY: `device` is live; the create info contains no borrowed
        // extension chain, and the resulting sampler is owned by `partial`
        // until construction rollback or `destroy`.
        partial.history_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("Volumetrics history sampler")
        });

        // Transport needs sub-froxel motion: nearest sampling would keep
        // velocity pinned until it crossed a whole cell and produce blocky
        // smoke. A dedicated linear/clamp sampler is safe here because the
        // chemistry field is continuous; raw radiance history deliberately
        // keeps its separate nearest-column sampler to avoid wall bleeding.
        // SAFETY: same ownership/lifetime contract as `history_sampler`.
        partial.transport_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("Volumetrics transport sampler")
        });

        // The density generator is periodic at the voxel boundary, so
        // trilinear filtering remains continuous across every repeat seam.
        // SAFETY: `device` is live, the create info has no extension chain,
        // and the sampler handle is owned by `partial` until rollback or
        // explicit pipeline destruction.
        partial.density_noise_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT),
                    None,
                )
                .context("Volumetrics density-noise sampler")
        });

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
            partial
                .fog_volume_buffers
                .push(try_or_cleanup!(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<GpuFogVolumeUpload>() as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )));
            partial
                .fog_cluster_buffers
                .push(try_or_cleanup!(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]>()
                        as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )));
            partial.fog_cluster_index_buffers.push(try_or_cleanup!(
                GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<[u32; FOG_VOLUME_INDEX_COUNT]>() as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )
            ));
            let mut moment_buffer = try_or_cleanup!(GpuBuffer::create_host_readback(
                device,
                allocator,
                std::mem::size_of::<[GpuCombustionLightMoment; COMBUSTION_LIGHT_GRID_COUNT]>()
                    as vk::DeviceSize,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ));
            try_or_cleanup!((|| -> Result<()> {
                moment_buffer.mapped_slice_mut()?.fill(0);
                moment_buffer.flush_if_needed(device)
            })());
            partial.combustion_light_moment_buffers.push(moment_buffer);
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
            // `write_tlas` from `draw_frame` before dispatch — same flow
            // as `CausticPipeline::write_tlas`. Used by the injection
            // shader's shadow visibility ray query.
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3/4/5: Phase 2b point/spot light injection — the same
            // per-frame lights SSBO + cluster grid + light-index list
            // `ClusterCullPipeline` already builds for the fragment
            // shader (`triangle.frag`'s `lights[]` / `clusters[]` /
            // `clusterLightIndices[]`), reused here rather than
            // building a separate froxel-space light-culling
            // structure. Written per-frame via `write_lights_and_clusters`
            // (same deferred-write flow as `write_tlas`, since the
            // buffer *contents* are rebuilt every frame even though the
            // handles are frame-in-flight-stable).
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6: previous frame's raw V-buffer for temporal reprojection.
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7/8/9: analytic local fog primitives and their camera-centered
            // 16^3 world-space clustered cull list.
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // Immutable boot-generated Perlin-Worley base density and
            // higher-frequency erosion detail.
            vk::DescriptorSetLayoutBinding::default()
                .binding(10)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(11)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 12/13: current write + previous sampled scalar emission share.
            // This provenance cannot be reconstructed from the RGBA V-buffer
            // after thermal emission and stochastic lighting are combined.
            vk::DescriptorSetLayoutBinding::default()
                .binding(12)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(13)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 14/15: current write + previous sampled canonical chemistry
            // `(fuel, temperature, soot extinction, radiance calibration)`.
            vk::DescriptorSetLayoutBinding::default()
                .binding(14)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(15)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 16/17: current write + previous sampled canonical dynamics
            // `(world-space velocity xyz, specific overpressure)`.
            vk::DescriptorSetLayoutBinding::default()
                .binding(16)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(17)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 18: fixed-point transported-field radiant moments. GPU writes
            // this slot; the host drains it only after the slot fence.
            vk::DescriptorSetLayoutBinding::default()
                .binding(18)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 19/20/21: current GpuInstance table plus canonical global
            // vertex/index SSBOs. A TLAS committed hit's custom index lands
            // directly in binding 19, and its primitive index addresses the
            // matching rigid triangle in bindings 20/21.
            vk::DescriptorSetLayoutBinding::default()
                .binding(19)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(20)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(21)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 22/23: current write + previous sampled spectral scattering
            // coefficient. Keeping optics separate leaves bindings 14-17 as
            // single-purpose thermochemical and kinematic state.
            vk::DescriptorSetLayoutBinding::default()
                .binding(22)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(23)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
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
        // SAFETY: `device` is live; `bindings` outlives the call; the layout
        // is owned by `partial` and destroyed on the error path / in destroy().
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("Volumetrics descriptor set layout")
        });

        // SAFETY: `device` is live; the referenced descriptor set layout was
        // just created above and is still live; result owned by `partial`.
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
        // load-module → create → destroy-module centralized in
        // pipeline::create_compute_pipeline (#1751); try_or_cleanup! adds the
        // partial-struct rollback on error.
        partial.pipeline = try_or_cleanup!(pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            VOLUMETRICS_INJECT_COMP_SPV,
            partial.pipeline_layout,
            "Volumetrics clear",
        ));

        // ── 5. Descriptor pool + sets ─────────────────────────────────
        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "Volumetrics descriptor pool"));

        let layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: `device` is live; `partial.descriptor_pool` was just built and
        // `layouts` (clones of the live set layout) outlive the call.
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
            let previous = (f + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
            let lighting_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let history_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.history_sampler)
                .image_view(partial.lighting_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let emission_history_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.emission_history_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_emission_history_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.history_sampler)
                .image_view(partial.emission_history_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_state_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_state_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_state_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_state_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_dynamics_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_dynamics_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_dynamics_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_dynamics_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_optical_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_optical_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_optical_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_optical_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];
            let fog_volume_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_volume_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<GpuFogVolumeUpload>() as vk::DeviceSize,
            }];
            let fog_cluster_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_cluster_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]>()
                    as vk::DeviceSize,
            }];
            let fog_index_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_cluster_index_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[u32; FOG_VOLUME_INDEX_COUNT]>() as vk::DeviceSize,
            }];
            let combustion_light_moment_info = [vk::DescriptorBufferInfo {
                buffer: partial.combustion_light_moment_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[GpuCombustionLightMoment; COMBUSTION_LIGHT_GRID_COUNT]>(
                ) as vk::DeviceSize,
            }];
            let set = partial.descriptor_sets[f];
            let base_noise_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.density_noise_sampler)
                .image_view(partial.base_noise_volume.as_ref().expect("base noise").view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let detail_noise_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.density_noise_sampler)
                .image_view(
                    partial
                        .detail_noise_volume
                        .as_ref()
                        .expect("detail noise")
                        .view,
                )
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [
                write_storage_image(set, 0, &lighting_info),
                write_uniform_buffer(set, 1, &params_info),
                write_combined_image_sampler(set, 6, &history_info),
                write_storage_buffer(set, 7, &fog_volume_info),
                write_storage_buffer(set, 8, &fog_cluster_info),
                write_storage_buffer(set, 9, &fog_index_info),
                write_combined_image_sampler(set, 10, &base_noise_info),
                write_combined_image_sampler(set, 11, &detail_noise_info),
                write_storage_image(set, 12, &emission_history_write_info),
                write_combined_image_sampler(set, 13, &previous_emission_history_info),
                write_storage_image(set, 14, &combustion_state_write_info),
                write_combined_image_sampler(set, 15, &previous_combustion_state_info),
                write_storage_image(set, 16, &combustion_dynamics_write_info),
                write_combined_image_sampler(set, 17, &previous_combustion_dynamics_info),
                write_storage_buffer(set, 18, &combustion_light_moment_info),
                write_storage_image(set, 22, &combustion_optical_write_info),
                write_combined_image_sampler(set, 23, &previous_combustion_optical_info),
            ];
            // SAFETY: the written descriptor sets and the referenced froxel image
            // view + param UBO are freshly created here and not yet in use by any
            // in-flight frame.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // ── 7. Per-FIF integration parameters ─────────────────────────
        // The hybrid distribution makes slab thickness depend on Z. Keep the
        // parameters per-FIF so future weather-driven reach changes cannot
        // introduce a host-write / in-flight-read WAR hazard.
        let int_param_size = std::mem::size_of::<IntegrationParams>() as vk::DeviceSize;
        let int_params = IntegrationParams {
            grid: [
                config.grid_far_meters as f32 * WORLD_UNITS_PER_METER,
                LINEAR_DEPTH,
                LINEAR_SLICE_FRACTION,
                extent.depth as f32,
            ],
        };
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let mut buffer = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                int_param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            try_or_cleanup!(buffer
                .write_mapped(device, std::slice::from_ref(&int_params))
                .context("write integration params"));
            partial.integration_param_buffers.push(buffer);
        }

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
        // SAFETY: `device` is live; `int_bindings` outlives the call; result is
        // owned by `partial` and destroyed on error / in destroy().
        partial.integration_descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&int_bindings),
                    None,
                )
                .context("Volumetrics integration descriptor set layout")
        });

        // SAFETY: `device` is live; the integration descriptor set layout was
        // just created above and is still live; result owned by `partial`.
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
        // Shared builder (#1751); try_or_cleanup! rolls back `partial` on error.
        partial.integration_pipeline = try_or_cleanup!(pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            VOLUMETRICS_INTEGRATE_COMP_SPV,
            partial.integration_pipeline_layout,
            "Volumetrics integration",
        ));

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
        // SAFETY: `device` is live; the integration descriptor pool was just
        // built and `int_layouts` (clones of the live set layout) outlive the call.
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
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let inj_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let int_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.integrated_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: partial.integration_param_buffers[f].buffer,
                offset: 0,
                range: int_param_size,
            }];
            let set = partial.integration_descriptor_sets[f];
            let int_writes = [
                write_storage_image(set, 0, &inj_info),
                write_storage_image(set, 1, &int_info),
                write_uniform_buffer(set, 2, &ubo_info),
            ];
            // SAFETY: the written integration descriptor sets and the referenced
            // froxel image views + dt UBO are freshly created here and not yet in
            // use by any in-flight frame.
            unsafe { device.update_descriptor_sets(&int_writes, &[]) };
        }

        log::info!(
            "Volumetrics pipeline created from render {}x{}: {}x{}x{} froxels (1/{} XY), {} MiB / slot ({} volumes: 5×RGBA16F + R32F), far={} m",
            render_extent.width,
            render_extent.height,
            extent.width,
            extent.height,
            extent.depth,
            config.froxel_xy_divisor,
            (extent.width as u64
                * extent.height as u64
                * extent.depth as u64
                * FROXEL_BYTES_PER_SLOT)
                / (1024 * 1024),
            FROXEL_VOLUMES_PER_SLOT,
            config.grid_far_meters,
        );

        Ok(partial)
    }

    /// #3860 — was ~85 lines of create → allocate → bind → view with its own
    /// three-arm cleanup. The only 3D site in the renderer, which is why
    /// `GpuImageDesc` carries an `image_type`/`view_type` pair rather than
    /// assuming 2D.
    fn create_volume(
        device: &ash::Device,
        allocator: &SharedAllocator,
        name: &str,
        extent: vk::Extent3D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<FroxelSlot> {
        GpuImage::create(
            device,
            allocator,
            &GpuImageDesc::color_3d(name, extent, format, usage),
        )
    }

    /// One-time UNDEFINED → GENERAL transition for every writable froxel
    /// volume followed by a `cmd_clear_color_image`: lighting/integration use
    /// `(rgb=0 inscatter, a=1 transmittance)`, while emission provenance and
    /// transported combustion fields use zero (the shader interprets empty
    /// chemistry as ambient air). Without the integrated clear, uninitialized
    /// `vol.a ≈ 0`
    /// makes the composite formula `final = scene * vol.a + vol.rgb`
    /// collapse the scene to black on the first frame volumetrics is
    /// enabled (#1082). Call once after `new()`.
    ///
    /// This neutral value is also the entire safety net for
    /// `VOLUMETRIC_OUTPUT_CONSUMED == false`: #1926 removed
    /// `composite.frag`'s shader-side gate, so `combined * vol.a +
    /// vol.rgb` runs unconditionally regardless of the const.
    /// `post_passes.rs` skips both volumetric dispatches when the const
    /// is false, which means composite reads whatever this clear left
    /// behind — do not "optimize" it to a plain zero-fill, that would
    /// make the off-path black out the scene.
    ///
    /// # Safety
    ///
    /// Caller must ensure all passed Vulkan handles (`device`, `cmd`) are
    /// valid and live, `cmd` is in the recording state, the device is not
    /// lost, and the froxel images are not concurrently accessed by another
    /// command buffer.
    pub unsafe fn initialize_layouts(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        // #2231 / REN-D5-03 — memoized: this function reruns on every window
        // resize (the whole pipeline is rebuilt because the froxel grid
        // follows render resolution), but the noise itself is resolution-
        // independent, so regenerating it via ~10^7 hash evaluations per
        // resize was pure waste. See `noise::cached_base_density_noise`.
        let base_texels = cached_base_density_noise();
        let detail_texels = cached_detail_density_noise();
        let make_staging = |bytes: &[u8]| -> Result<GpuBuffer> {
            let mut buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?;
            if let Err(error) = buffer.write_mapped(device, bytes) {
                buffer.destroy(device, allocator);
                return Err(error);
            }
            Ok(buffer)
        };
        let mut base_staging = make_staging(base_texels)?;
        let mut detail_staging = match make_staging(detail_texels) {
            Ok(buffer) => buffer,
            Err(error) => {
                base_staging.destroy(device, allocator);
                return Err(error);
            }
        };

        let upload_result = texture::with_one_time_commands(device, queue, pool, |cmd| {
            let full_range = descriptors::color_subresource_single_mip();
            let base_noise = self.base_noise_volume.as_ref().expect("base noise");
            let detail_noise = self.detail_noise_volume.as_ref().expect("detail noise");

            // ── 1. Initialize writable froxels and upload-only noise images.
            let mut barriers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT * 6 + 2);
            for slot in self
                .lighting_volumes
                .iter()
                .chain(self.integrated_volumes.iter())
                .chain(self.emission_history_volumes.iter())
                .chain(self.combustion_state_volumes.iter())
                .chain(self.combustion_dynamics_volumes.iter())
                .chain(self.combustion_optical_volumes.iter())
            {
                barriers.push(image_barrier_undef_to_general(slot.image).dst_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_WRITE,
                ));
            }
            for noise in [base_noise, detail_noise] {
                // #2413 / TD2-116 SIBLING — same shape as the three sites
                // the issue named; `full_range` is
                // `color_subresource_single_mip()`, which is what the helper
                // builds for (mips=1, layers=1).
                barriers.push(descriptors::image_barrier_undef_to_transfer_dst(
                    noise.image,
                    1,
                ));
            }
            // SAFETY: `cmd` is recording; every image is freshly allocated
            // and exclusively owned by this pipeline. The barriers move
            // froxels/noise from UNDEFINED to the layouts used by the
            // immediately following transfer commands.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }

            // ── 2. Clear froxels to the no-fog sentinel.
            // Zero inscatter + unit transmittance is the correct "no fog"
            // sentinel: composite `final = scene * vol.a + vol.rgb`
            // becomes `scene * 1 + 0 = scene`.
            let clear_value = vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            };
            for slot in self
                .lighting_volumes
                .iter()
                .chain(self.integrated_volumes.iter())
            {
                // SAFETY: this freshly allocated froxel is in GENERAL, has
                // TRANSFER_DST usage, and cannot be referenced by an in-flight
                // frame before initialization publishes the pipeline.
                unsafe {
                    device.cmd_clear_color_image(
                        cmd,
                        slot.image,
                        vk::ImageLayout::GENERAL,
                        &clear_value,
                        &[full_range],
                    );
                }
            }
            let clear_history = vk::ClearColorValue { float32: [0.0; 4] };
            for slot in self
                .emission_history_volumes
                .iter()
                .chain(self.combustion_state_volumes.iter())
                .chain(self.combustion_dynamics_volumes.iter())
                .chain(self.combustion_optical_volumes.iter())
            {
                // SAFETY: same ownership/layout/usage contract as the RGBA
                // froxels above; R32F consumes only the first clear channel,
                // while the RGBA16F transport fields consume all four.
                unsafe {
                    device.cmd_clear_color_image(
                        cmd,
                        slot.image,
                        vk::ImageLayout::GENERAL,
                        &clear_history,
                        &[full_range],
                    );
                }
            }

            // ── 3. Copy deterministic R8 fields into their immutable images.
            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            let base_copy = vk::BufferImageCopy::default()
                .image_subresource(subresource)
                .image_extent(vk::Extent3D {
                    width: BASE_NOISE_SIZE,
                    height: BASE_NOISE_SIZE,
                    depth: BASE_NOISE_SIZE,
                });
            let detail_copy = vk::BufferImageCopy::default()
                .image_subresource(subresource)
                .image_extent(vk::Extent3D {
                    width: DETAIL_NOISE_SIZE,
                    height: DETAIL_NOISE_SIZE,
                    depth: DETAIL_NOISE_SIZE,
                });
            // SAFETY: both staging buffers are live and fully populated; both
            // destination images are in TRANSFER_DST_OPTIMAL with matching R8
            // extents and TRANSFER_DST usage.
            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    base_staging.buffer,
                    base_noise.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[base_copy],
                );
                device.cmd_copy_buffer_to_image(
                    cmd,
                    detail_staging.buffer,
                    detail_noise.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[detail_copy],
                );
            }

            // ── 4. Publish transfer writes to the compute injector.
            memory_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            let noise_ready = [base_noise, detail_noise].map(|noise| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(noise.image)
                    .subresource_range(full_range)
            });
            // SAFETY: `cmd` is recording and the noise images were written by
            // the preceding copies. This makes those writes visible and moves
            // both images into their descriptor-declared sampled layout.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &noise_ready,
                );
            }

            Ok(())
        });

        // The one-time submit waits for the queue before returning, so neither
        // staging buffer can still be referenced here.
        base_staging.destroy(device, allocator);
        detail_staging.destroy(device, allocator);
        upload_result?;
        log::info!(
            "Volumetric density noise uploaded: {}^3 base + {}^3 detail ({} KiB R8)",
            BASE_NOISE_SIZE,
            DETAIL_NOISE_SIZE,
            (base_texels.len() + detail_texels.len()) / 1024,
        );
        Ok(())
    }
}
