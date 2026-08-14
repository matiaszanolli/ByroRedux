//! Caustic scatter pass (#321, Option A).
//!
//! One compute dispatch per frame that splats refracted-light contributions
//! from every caustic-source pixel into a screen-space accumulator. The
//! accumulator is a three-layer R32_UINT storage image so the shader can use
//! `imageAtomicAdd` independently for RGB; composite samples it as a
//! `usampler2DArray`, divides out the fixed-point scale, and adds the colored
//! result to direct lighting.
//!
//! ## Resource layout
//!
//! - **caustic_accum[frame]** (R32_UINT, STORAGE | SAMPLED | TRANSFER_DST,
//!   per frame-in-flight): accumulator written by this module, read by the
//!   composite pass as a three-layer unsigned texture (R, G, B).
//!
//! Layout lives in `GENERAL` throughout the frame (required for storage
//! image access) — same policy as SVGF's history images. Composite samples
//! it through a `usampler2DArray` view, which is legal in `GENERAL`.
//!
//! ## Per-frame flow
//!
//! 1. `vkCmdClearColorImage` resets the accumulator to zero.
//! 2. Params UBO uploaded (screen size, IOR, strength, fixed-point scale).
//! 3. HOST→COMPUTE barrier for the UBO + CLEAR→COMPUTE barrier for the image.
//! 4. `vkCmdDispatch` — one invocation per screen pixel; only caustic-source
//!    pixels do work.
//! 5. COMPUTE→FRAGMENT barrier so the composite pass can sample the result.
//!
//! ## Descriptor set (binding layout)
//!
//! | Binding | Resource           | Type                                |
//! |---------|--------------------|-------------------------------------|
//! | 0       | depth              | sampler2D (gbuffer, shared)         |
//! | 1       | normal             | sampler2D (gbuffer, per-frame)      |
//! | 2       | mesh_id            | usampler2D (gbuffer, per-frame)     |
//! | 3       | LightBuffer        | SSBO (scene_buffers, per-frame)     |
//! | 4       | CameraUBO          | UBO (scene_buffers, per-frame)      |
//! | 5       | InstanceBuffer     | SSBO (scene_buffers, per-frame)     |
//! | 6       | TLAS               | acceleration_structure (per-frame)  |
//! | 7       | caustic accum      | uimage2DArray r32ui (RGB layers)    |
//! | 8       | CausticParams      | UBO (this module, per-frame)        |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    memory_barrier, write_acceleration_structure, write_combined_image_sampler,
    write_storage_buffer, write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::CAUSTIC_FIXED_SCALE;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

const CAUSTIC_SPLAT_COMP_SPV: &[u8] = include_bytes!("../../shaders/caustic_splat.comp.spv");

/// Per-channel caustic accumulator — RGB radiance packed as fixed-point in
/// three R32_UINT array layers. Composite divides by `CAUSTIC_FIXED_SCALE`
/// on read to recover the accumulated color. Separate layers preserve glass
/// hue while retaining portable scalar image atomics.
///
/// `R32_UINT` is used precisely because shader image atomics require it: the
/// Vulkan "Required Format Support" table makes
/// `VK_FORMAT_FEATURE_STORAGE_IMAGE_ATOMIC_BIT` (alongside `STORAGE_IMAGE`)
/// **mandatory** for `R32_UINT` and `R32_SINT` — the only two formats
/// guaranteed for image atomics on every conformant implementation. So no
/// `vkGetPhysicalDeviceFormatProperties` gate is needed here (it could never
/// fail); the choice of format IS the capability guarantee. See #1404.
pub const CAUSTIC_FORMAT: vk::Format = vk::Format::R32_UINT;
const CAUSTIC_COLOR_LAYERS: u32 = 3;

/// Screen-sized bytes per pixel this pipeline allocates across every
/// frame-in-flight — the figure `docs/engine/memory-budget.md` publishes in
/// its "Glass + Water Caustics" table.
///
/// #2679 / PERF-D3-03 — the RGB conversion (`610cb170`) tripled the glass
/// accumulator's layer count and the ledger did not follow, understating it
/// by 16 B/px. Deriving the number from the live constants and pinning it in
/// a test makes the next layer-count change fail here instead of drifting
/// silently.
pub(crate) const CAUSTIC_BYTES_PER_PIXEL: u32 =
    4 * CAUSTIC_COLOR_LAYERS * MAX_FRAMES_IN_FLIGHT as u32;

/// Decimal MB (matching `memory-budget.md`) this pipeline's accumulators
/// occupy at `width × height`.
fn caustic_megabytes(width: u32, height: u32) -> f64 {
    (width as f64) * (height as f64) * (CAUSTIC_BYTES_PER_PIXEL as f64) / 1_000_000.0
}

/// Ceiling on the parked-camera EMA decay factor. `N/(N+1)` would reach
/// 1.0 only in the limit, but the cap keeps it strictly below so the
/// accumulator never stops admitting new energy (a true 1.0 freezes the
/// pool forever).
const CAUSTIC_DECAY_MAX: f32 = 0.995;

/// Advance this FIF slot's parked-visit counter and derive the decay
/// factor for its accumulator (#2401 / CHAIN2-D2-02).
///
/// Extracted as a pure function so the per-slot accounting is testable
/// without a device — the bug it fixes is arithmetic, not Vulkan.
///
/// The counter is **per slot** because the accumulator is per slot:
/// `slots[frame].image` is never cross-seeded, so it only gains a sample
/// when this frame index comes round again. A single global counter (the
/// pre-fix shape) made a slot's k-th visit decay with `n = 2k-1` at
/// `MAX_FRAMES_IN_FLIGHT == 2`, admitting new energy at weight `1/(2k)`
/// after only `k` real samples — the running average converged at ~`1/√k`
/// instead of `1/k`.
///
/// Camera motion zeroes **every** slot, not just this one: the decay of
/// 0.0 returned here wipes the current slot immediately, and the other
/// slots' images are equally stale from the old viewpoint, so their
/// counters must not survive to over-weight the next parked run.
fn advance_parked_visits(
    parked: &mut [u32; MAX_FRAMES_IN_FLIGHT],
    frame: usize,
    camera_static: bool,
) -> f32 {
    if !camera_static {
        *parked = [0; MAX_FRAMES_IN_FLIGHT];
        return 0.0;
    }
    parked[frame] = parked[frame].saturating_add(1);
    let n = parked[frame] as f32;
    (n / (n + 1.0)).min(CAUSTIC_DECAY_MAX)
}

#[inline]
fn caustic_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: CAUSTIC_COLOR_LAYERS,
    }
}

/// UBO uploaded once per frame. Matches `CausticParams` in
/// `shaders/caustic_splat.comp` exactly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CausticParams {
    /// xy = pixel size, zw = 1 / pixel size.
    pub screen: [f32; 4],
    /// x = fixed-point scale, y = IOR (glass = 1.5), z = max lights to
    /// iterate, w = caustic strength multiplier (all ≥ 0).
    pub tune: [f32; 4],
}

struct CausticSlot {
    image: vk::Image,
    /// `r32ui` storage view for atomic writes from the compute shader.
    storage_view: vk::ImageView,
    /// Separate view used by composite to sample as a `usampler2DArray`.
    sampled_view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct CausticPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,

    /// Per-FIF accumulator images.
    slots: Vec<CausticSlot>,
    /// Point sampler for gbuffer reads (depth, normal, mesh_id).
    point_sampler: vk::Sampler,

    param_buffers: Vec<GpuBuffer>,

    pub width: u32,
    pub height: u32,

    /// Tuning knobs, mirrored to the params UBO each dispatch.
    pub ior: f32,
    pub strength: f32,
    pub max_lights: u32,

    /// Consecutive parked (camera-static) **visits to this FIF slot**, for
    /// progressive 1/N EMA convergence. Reset to 0 on camera motion. Capped
    /// so the decay factor `N/(N+1)` approaches but never reaches 1.0 (a
    /// true 1.0 would freeze the pool and never admit new energy).
    ///
    /// Per-slot, not global (#2401 / CHAIN2-D2-02). The accumulator image
    /// is per-FIF (`slots[frame].image`) and never cross-seeded, so a slot
    /// is only visited every `MAX_FRAMES_IN_FLIGHT` frames. A single
    /// global counter made a slot's k-th visit decay with `n = 2k-1` and
    /// admit new energy at weight `1/(2k)` after only `k` real samples —
    /// the running average converged at ~`1/√k` instead of `1/k`, leaving
    /// residual half-rate shimmer for the first ~2 s of a parked camera
    /// (the exact artifact the EMA exists to remove).
    parked_frames: [u32; MAX_FRAMES_IN_FLIGHT],
}

impl CausticPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let result = Self::new_inner(
            device,
            allocator,
            pipeline_cache,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
            width,
            height,
        );
        if let Err(ref e) = result {
            log::debug!("Caustic pipeline creation failed at: {e}");
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        debug_assert_eq!(normal_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(mesh_id_views.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(light_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(camera_buffers.len(), MAX_FRAMES_IN_FLIGHT);
        debug_assert_eq!(instance_buffers.len(), MAX_FRAMES_IN_FLIGHT);

        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            slots: Vec::new(),
            point_sampler: vk::Sampler::null(),
            param_buffers: Vec::new(),
            width,
            height,
            ior: 1.5,
            strength: 1.0,
            max_lights: 8,
            parked_frames: [0; MAX_FRAMES_IN_FLIGHT],
        };

        // SAFETY (inside macro): `partial` is local to this fn and not
        // yet referenced by any command buffer / descriptor set;
        // cleanup-on-error closes the partial state before returning.
        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `partial` holds only handles created by this
                        // device earlier in `new`; on this init error path none
                        // has been bound to an in-flight command buffer yet, so
                        // destroying them in reverse creation order is sound.
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // ── 1. Accumulator images ─────────────────────────────────────
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_slot(
                device,
                allocator,
                width,
                height,
                &format!("caustic_accum_{i}")
            ));
            partial.slots.push(slot);
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
                .context("caustic point sampler")
        });

        // ── 3. Parameter UBOs ─────────────────────────────────────────
        let param_size = std::mem::size_of::<CausticParams>() as vk::DeviceSize;
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
            // 0 depth
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1 normal
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2 mesh_id
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3 lights
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 4 camera UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 5 instances
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6 TLAS
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7 output
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 8 params
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "caustic_splat.comp",
                spirv: CAUSTIC_SPLAT_COMP_SPV,
            }],
            "caustic",
            &[],
        )
        .expect("caustic descriptor layout drifted against caustic_splat.comp (see #427)");
        // SAFETY: `bindings` validated against caustic_splat.comp above.
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("caustic descriptor set layout")
        });

        // Push constant: { u32 decay_only, f32 decay_factor } — 8 bytes,
        // drives the temporal-EMA decay pass (see caustic_splat.comp).
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(8);
        // SAFETY: descriptor_set_layout just created above.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout))
                        .push_constant_ranges(std::slice::from_ref(&push_range)),
                    None,
                )
                .context("caustic pipeline layout")
        });

        // ── 5. Compute pipeline ───────────────────────────────────────
        // Shared builder (#1751); frees the shader module immediately —
        // its SPIR-V is already baked into `partial.pipeline` by the time
        // this returns, so there's no live consumer to keep it around for.
        partial.pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            CAUSTIC_SPLAT_COMP_SPV,
            partial.pipeline_layout,
            "caustic",
        ));

        // ── 6. Descriptor pool + sets ─────────────────────────────────
        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "caustic descriptor pool"));

        let set_layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: pool just sized for MAX_FRAMES_IN_FLIGHT sets with the
        // same descriptor_set_layout handle.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("caustic descriptor sets")
        });

        // ── 7. Write non-TLAS descriptors (TLAS is written per-frame) ─
        partial.write_descriptor_sets(
            device,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
        );

        // #2679 / PERF-D3-03 — attributing telemetry, same mechanism #1814
        // established for the ReSTIR reservoirs: the accumulator is
        // screen-sized AND layer-count-sized, so the two ways it can grow
        // (resolution, RGB layers) both report themselves here rather than
        // being rediscovered by the next memory audit.
        log::info!(
            "Caustic pipeline created: {}x{}, {} layers × {} FIF = {} B/px, {:.1} MB",
            width,
            height,
            CAUSTIC_COLOR_LAYERS,
            MAX_FRAMES_IN_FLIGHT,
            CAUSTIC_BYTES_PER_PIXEL,
            caustic_megabytes(width, height),
        );
        Ok(partial)
    }

    fn create_slot(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
        name: &str,
    ) -> Result<CausticSlot> {
        // Single-mip image. The downstream `base_mip_level: 0` /
        // `level_count: 1` literals (view subresource, clear range,
        // pre/post barriers — all paired with this image) are pinned
        // to that 1 here. Going wider (e.g. mipmapped for blur or
        // half-res accumulation) requires updating every subresource
        // range alongside the `mip_levels` bump. See REN-D13-NEW-06
        // (audit 2026-05-09).
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(CAUSTIC_FORMAT)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(CAUSTIC_COLOR_LAYERS)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        // SAFETY: `info` fully populated above (TYPE_2D, CAUSTIC_FORMAT,
        // STORAGE | SAMPLED | TRANSFER_DST usage). On Err the `?` bubbles
        // up before any subsequent allocation.
        let image = unsafe { device.create_image(&info, None).context("caustic image")? };
        // The MutexGuard from `.lock()` lives until the end of the `let`
        // statement; the Err arm only destroys `image` — no allocator
        // re-lock — so no deadlock. Cf. ssao.rs for the #1163 separate-let
        // pattern required when an Err arm calls partial.destroy() which
        // re-locks the allocator.
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
            .context("caustic image allocate")
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: alloc failed; image was created but never bound.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };
        // SAFETY: `image` matches the memory requirements that produced
        // `alloc`; bound once per image.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .context("caustic bind image memory")
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: bind failed; free alloc first, then destroy unbound image.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        let make_view = |img: vk::Image| -> Result<vk::ImageView> {
            // SAFETY: callers below pass `image` (bound above) twice —
            // once for storage view, once for sampled view. Both views
            // are owned by the returned CausticSlot.
            Ok(unsafe {
                device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(img)
                            .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
                            .format(CAUSTIC_FORMAT)
                            .subresource_range(caustic_subresource_range()),
                        None,
                    )
                    .context("caustic image view")?
            })
        };
        let storage_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: storage view creation failed; free alloc first,
                // destroy bound image.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };
        let sampled_view = match make_view(image) {
            Ok(v) => v,
            Err(e) => {
                // SAFETY: sampled view creation failed; tear down
                // already-created storage view, free alloc, destroy image.
                unsafe { device.destroy_image_view(storage_view, None) };
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: `image` was created by this device just above and has
                // not been bound to any in-flight command buffer on this error
                // path, so destroying it here is sound.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(CausticSlot {
            image,
            storage_view,
            sampled_view,
            allocation: Some(alloc),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn write_descriptor_sets(
        &self,
        device: &ash::Device,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
    ) {
        let param_size = std::mem::size_of::<CausticParams>() as vk::DeviceSize;
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let depth_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(depth_view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let normal_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(normal_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let mesh_id_info = [vk::DescriptorImageInfo::default()
                .sampler(self.point_sampler)
                .image_view(mesh_id_views[f])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let light_info = [vk::DescriptorBufferInfo {
                buffer: light_buffers[f].buffer,
                offset: 0,
                range: light_buffer_size,
            }];
            let camera_info = [vk::DescriptorBufferInfo {
                buffer: camera_buffers[f].buffer,
                offset: 0,
                range: camera_buffer_size,
            }];
            let instance_info = [vk::DescriptorBufferInfo {
                buffer: instance_buffers[f].buffer,
                offset: 0,
                range: instance_buffer_size,
            }];
            let caustic_info = [vk::DescriptorImageInfo::default()
                .image_view(self.slots[f].storage_view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: self.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];

            let set = self.descriptor_sets[f];
            let writes = [
                write_combined_image_sampler(set, 0, &depth_info),
                write_combined_image_sampler(set, 1, &normal_info),
                write_combined_image_sampler(set, 2, &mesh_id_info),
                write_storage_buffer(set, 3, &light_info),
                write_uniform_buffer(set, 4, &camera_info),
                write_storage_buffer(set, 5, &instance_info),
                write_storage_image(set, 7, &caustic_info),
                write_uniform_buffer(set, 8, &params_info),
            ];
            // SAFETY: descriptor sets owned by `self`; writes reference
            // buffers / image views owned by `self` and caller-borrowed
            // G-buffer / scene resources (live for this call's duration).
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
    }

    /// Caustic accumulator view used by the composite pass as `usampler2DArray`.
    pub fn sampled_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].sampled_view
    }

    /// Update the TLAS binding for a given frame (binding 6). Mirrors the
    /// scene descriptor set's `write_tlas` flow — TLAS is rebuilt per frame
    /// so this must be called every frame before `dispatch`.
    pub fn write_tlas(
        &self,
        device: &ash::Device,
        frame: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = write_acceleration_structure(self.descriptor_sets[frame], 6, &mut accel_write);
        // SAFETY: `write` references `accel_write` (which carries the
        // caller-provided `tlas` handle, live for the call duration) and
        // `self.descriptor_sets[frame]` (live for `self`'s lifetime).
        unsafe { device.update_descriptor_sets(&[write], &[]) };
    }

    /// One-time transition UNDEFINED → GENERAL on every slot so the first
    /// dispatch + composite sample see a valid layout. Call once after
    /// `new()`.
    ///
    /// # Safety
    /// Device, queue and command pool must be valid; queue must support
    /// graphics/transfer for pipeline barriers.
    pub unsafe fn initialize_layouts(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let mut barriers = Vec::with_capacity(self.slots.len());
            for slot in &self.slots {
                barriers.push(
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(
                            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
                        )
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::GENERAL)
                        .image(slot.image)
                        .subresource_range(caustic_subresource_range()),
                );
            }
            // SAFETY: caller of `initialize_layouts` (unsafe fn) guarantees
            // device/queue/pool validity; `cmd` is the recording buffer
            // from `with_one_time_commands`. Each barrier targets a slot
            // image we own.
            // NONE as srcStageMask on UNDEFINED → GENERAL transitions: there
            // are no previous writes to make visible (the prior contents are
            // discarded), so TOP_OF_PIPE and NONE are semantically equivalent.
            // NONE is the Vulkan 1.3 replacement for the deprecated use of
            // TOP_OF_PIPE as a source stage in memory barriers (#949 / #1100).
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

    /// Clear + dispatch. Call after the main render pass ends (gbuffer is
    /// in SHADER_READ_ONLY_OPTIMAL) and the TLAS has been rebuilt+bound,
    /// but before the composite pass samples the result.
    ///
    /// # Safety
    /// `cmd` must be a valid recording command buffer. `frame` must be
    /// < MAX_FRAMES_IN_FLIGHT.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        camera_static: bool,
    ) -> Result<()> {
        // ── Upload params ─────────────────────────────────────────────
        let params = CausticParams {
            screen: [
                self.width as f32,
                self.height as f32,
                1.0 / self.width as f32,
                1.0 / self.height as f32,
            ],
            tune: [
                CAUSTIC_FIXED_SCALE,
                self.ior,
                self.max_lights as f32,
                self.strength,
            ],
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&params))?;

        // HOST → COMPUTE_SHADER (UBO flush before dispatch).
        memory_barrier(
            device,
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::UNIFORM_READ,
        );

        // ── Decay (parked) or clear (moving), then splat ──────────────
        // The caustic is composited AFTER TAA, so its own per-frame
        // TAA-jitter flicker is never resolved by the engine's temporal
        // passes. With a parked camera we therefore replace the per-frame
        // clear with an exponential moving average: run the splat pipeline
        // in *decay* mode first (accum *= DECAY), then splat only
        // (1 - DECAY) of this frame's energy on top. The focused caustic
        // spot — whose landing point jitters frame to frame — converges to
        // a stable pool instead of stippling. On camera motion we clear
        // and deposit full energy, so a stale, mis-registered pool from the
        // old viewpoint can never smear across the screen.
        //
        // Cross-frame safety: each FIF slot is its own image and the
        // per-frame fence guarantees the slot is idle before this command
        // buffer runs, so the decay→splat read-modify-write chain only
        // needs the intra-frame barriers below — exactly as the old
        // clear→splat path did. (Steady state: previous use of this slot
        // was compute-write in the prior splat → fragment-read in
        // composite; frame 0 the slot is GENERAL from initialize_layouts,
        // so the wait stages are merely over-specified, not wrong — see
        // REN-D13-NEW-03, audit 2026-05-09.)
        // Parked-camera EMA. Progressive 1/N convergence (matches the SVGF
        // GI path) instead of a fixed-window decay: a constant decay (e.g.
        // 0.96) plateaus at a fixed noise floor (~1/√25) and never resolves
        // the per-deposit white-noise jitter; `decay = N/(N+1)` makes the
        // accumulator a true running average that converges to ground truth
        // the longer the camera stays parked. Capped at 0.995 so it never
        // freezes (a true 1.0 would stop admitting new energy). Resets on
        // motion (decay 0 → single-frame clear, no smear).
        let decay_factor = advance_parked_visits(&mut self.parked_frames, frame, camera_static);
        let slot_img = self.slots[frame].image;
        let clear_range = caustic_subresource_range();

        // Bind pipeline + descriptor once; the decay and splat passes share
        // them and differ only by push constant.
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );
        let gx = self.width.div_ceil(8);
        let gy = self.height.div_ceil(8);
        let push_bytes = |decay_only: u32, factor: f32| -> [u8; 8] {
            let mut b = [0u8; 8];
            b[0..4].copy_from_slice(&decay_only.to_ne_bytes());
            b[4..8].copy_from_slice(&factor.to_ne_bytes());
            b
        };

        if camera_static {
            // Wait for the slot's previous use (prior splat compute-write +
            // composite fragment-read) before the decay pass scales it.
            let pre_decay = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[pre_decay],
            );
            // Decay pass: every pixel multiplies its accumulator by DECAY.
            device.cmd_push_constants(
                cmd,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &push_bytes(1, decay_factor),
            );
            device.cmd_dispatch(cmd, gx, gy, 1);
            // Decay-write → splat read-modify-write (atomicAdd).
            let mid = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[mid],
            );
        } else {
            // Moving camera: clear the slot so an old-viewpoint pool can't
            // smear, then deposit full energy (decay_factor == 0).
            let pre_clear_barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[pre_clear_barrier],
            );
            let clear_value = vk::ClearColorValue {
                uint32: [0, 0, 0, 0],
            };
            device.cmd_clear_color_image(
                cmd,
                slot_img,
                vk::ImageLayout::GENERAL,
                &clear_value,
                &[clear_range],
            );
            // TRANSFER → COMPUTE so the splat's atomic adds see zeros.
            let post_clear_barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[post_clear_barrier],
            );
        }

        // ── Splat dispatch ────────────────────────────────────────────
        // decay_factor drives the EMA new-sample weight (1 - decay_factor)
        // in the shader: 0.15 of this frame while parked, full energy while
        // moving (decay_factor == 0).
        device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &push_bytes(0, decay_factor),
        );
        device.cmd_dispatch(cmd, gx, gy, 1);

        // ── COMPUTE → FRAGMENT barrier for composite sample ───────────
        let out_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(caustic_subresource_range());
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[out_barrier],
        );

        Ok(())
    }

    /// One-shot clear of `slots[frame]` to zero for a frame where
    /// [`Self::dispatch`] is skipped entirely (TLAS absent, or the pass has
    /// permanently failed) — #2507. Composite unconditionally samples
    /// `causticTex` with no validity gate, so without this the
    /// accumulator's last-written contents (a screen-space pattern that
    /// does NOT track camera motion, since it's built from a fixed
    /// viewpoint) would be re-composited every subsequent frame instead of
    /// degrading to "no caustics" (black). Callers should latch this to
    /// once per skip streak per frame-slot — clearing an already-zero slot
    /// every frame is correct but wasteful.
    ///
    /// Ends with the same GENERAL/`SHADER_READ` state [`Self::dispatch`]'s
    /// final barrier leaves the slot in, so composite's subsequent sample
    /// sees a consistent state either way.
    ///
    /// # Safety
    /// `cmd` must be in the recording state (outside a render pass) at the
    /// same command-buffer position `dispatch` would have run from — i.e.
    /// after the main render pass ends and before composite reads the
    /// accumulator.
    pub unsafe fn clear_for_skip(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        let slot_img = self.slots[frame].image;
        let clear_range = caustic_subresource_range();
        // Same over-specified wait stages `dispatch`'s moving-camera clear
        // uses: the slot's prior use was either this pipeline's own
        // compute-write + composite's fragment-read (steady state), or
        // GENERAL-but-untouched from `initialize_layouts` (frame 0 / just
        // after resize) — safe either way.
        let pre_clear_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(clear_range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[pre_clear_barrier],
        );
        let clear_value = vk::ClearColorValue {
            uint32: [0, 0, 0, 0],
        };
        device.cmd_clear_color_image(
            cmd,
            slot_img,
            vk::ImageLayout::GENERAL,
            &clear_value,
            &[clear_range],
        );
        // TRANSFER → FRAGMENT directly (no compute dispatch follows this
        // clear, unlike `dispatch`'s TRANSFER → COMPUTE mid-barrier).
        let post_clear_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(clear_range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post_clear_barrier],
        );
    }

    /// Recreate accumulator images and rewrite descriptor sets on resize.
    ///
    /// Self-contained per #1031 / REN-D10-NEW-11: fresh slot images
    /// are created at `initial_layout: UNDEFINED` and walked to
    /// GENERAL via [`Self::initialize_layouts`] internally, so
    /// post-resize first dispatches see a valid storage layout.
    #[allow(clippy::too_many_arguments)]
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        depth_view: vk::ImageView,
        normal_views: &[vk::ImageView],
        mesh_id_views: &[vk::ImageView],
        light_buffers: &[GpuBuffer],
        light_buffer_size: vk::DeviceSize,
        camera_buffers: &[GpuBuffer],
        camera_buffer_size: vk::DeviceSize,
        instance_buffers: &[GpuBuffer],
        instance_buffer_size: vk::DeviceSize,
        width: u32,
        height: u32,
    ) -> Result<()> {
        for slot in self.slots.drain(..) {
            // SAFETY: `recreate_on_resize` runs from the fenced
            // swapchain-resize path (`VulkanContext::recreate_swapchain`
            // waits both frames-in-flight first). Slot view / image
            // handles are unreferenced by any in-flight command.
            unsafe {
                device.destroy_image_view(slot.storage_view, None);
                device.destroy_image_view(slot.sampled_view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        self.width = width;
        self.height = height;

        let res = (|| -> Result<()> {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.slots.push(Self::create_slot(
                    device,
                    allocator,
                    width,
                    height,
                    &format!("caustic_accum_{i}"),
                )?);
            }
            Ok(())
        })();
        if let Err(ref e) = res {
            log::error!("Caustic recreate partial failure: {e}");
            // SAFETY: fenced-resize path; partial state is unreferenced
            // by any in-flight command.
            unsafe { self.destroy(device, allocator) };
            return res;
        }

        self.write_descriptor_sets(
            device,
            depth_view,
            normal_views,
            mesh_id_views,
            light_buffers,
            light_buffer_size,
            camera_buffers,
            camera_buffer_size,
            instance_buffers,
            instance_buffer_size,
        );

        // #1031 — walk fresh slot images from UNDEFINED to GENERAL.
        // SAFETY: fenced-resize contract — no concurrent reader on
        // these images. Warn-log on failure matches the caller's
        // pre-#1031 behaviour.
        if let Err(e) = unsafe { self.initialize_layouts(device, queue, command_pool) } {
            log::warn!("Caustic layout re-init after resize failed: {e}");
        }
        // #2679 / PERF-D3-03 — a resize is the other point where this
        // footprint changes; see the sibling log in `new_inner`.
        log::info!(
            "Caustic pipeline recreated: {}x{}, {} B/px, {:.1} MB",
            width,
            height,
            CAUSTIC_BYTES_PER_PIXEL,
            caustic_megabytes(width, height),
        );
        Ok(())
    }

    /// # Safety
    /// Must be called before the device + allocator are dropped.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // SAFETY (whole function): caller of `destroy` (unsafe fn)
        // guarantees no in-flight command buffer references any object
        // owned by `self`. Per-handle `if != null()` guards make this
        // safe to call on partially-initialised state from
        // `try_or_cleanup`.
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        self.param_buffers.clear();
        if self.pipeline != vk::Pipeline::null() {
            // SAFETY: `self.pipeline` was built by this `device` in `initialize_layouts`, is non-null (guarded above), and per the whole-function contract is unreferenced by any in-flight command buffer.
            unsafe { device.destroy_pipeline(self.pipeline, None) };
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            // SAFETY: `self.pipeline_layout` was created by this `device`, is non-null (guarded above), and its dependent pipeline is destroyed first (above); no in-flight command references it.
            unsafe { device.destroy_pipeline_layout(self.pipeline_layout, None) };
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            // SAFETY: `self.descriptor_pool` was created by this `device`, is non-null (guarded above); destroying it frees all sets allocated from it, none of which are in flight per the whole-function contract.
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            // SAFETY: `self.descriptor_set_layout` was created by this `device`, is non-null (guarded above), and its dependent pool/layout users are destroyed first (above).
            unsafe { device.destroy_descriptor_set_layout(self.descriptor_set_layout, None) };
            self.descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
        if self.point_sampler != vk::Sampler::null() {
            // SAFETY: `self.point_sampler` was created by this `device`, is non-null (guarded above), and per the whole-function contract is unreferenced by any in-flight command buffer.
            unsafe { device.destroy_sampler(self.point_sampler, None) };
            self.point_sampler = vk::Sampler::null();
        }
        for slot in self.slots.drain(..) {
            // SAFETY: caller's unsafe-fn contract — no in-flight cmd
            // buffer references slot resources.
            unsafe {
                device.destroy_image_view(slot.storage_view, None);
                device.destroy_image_view(slot.sampled_view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

// CAUSTIC_FIXED_SCALE drift test moved to shader_constants::tests after #1038
// folded the constant into the build.rs codegen path. Canonical check:
//   shader_constants::tests::generated_header_contains_all_defines
//   shader_constants::tests::affected_shaders_include_constants_header

#[cfg(test)]
mod tests {
    use super::{
        caustic_megabytes, caustic_subresource_range, CAUSTIC_BYTES_PER_PIXEL, CAUSTIC_COLOR_LAYERS,
    };

    /// #2679 / PERF-D3-03 — pins the "Glass + Water Caustics" table in
    /// `docs/engine/memory-budget.md` against the live layer count. The doc
    /// still said 16 B/px combined after the RGB conversion tripled the glass
    /// side to 24 B/px (water contributes the remaining 8: one R32_UINT layer
    /// × 2 FIF, `water_caustic.rs`'s `.array_layers(1)`). If the layers or
    /// frames-in-flight change, this fails as the nudge to update the doc —
    /// same guard #1814 put on the ReSTIR reservoirs.
    #[test]
    fn caustic_bytes_per_pixel_matches_documented_memory_budget() {
        const WATER_BYTES_PER_PIXEL: u32 = 4 * super::MAX_FRAMES_IN_FLIGHT as u32;
        assert_eq!(CAUSTIC_BYTES_PER_PIXEL, 24);
        assert_eq!(CAUSTIC_BYTES_PER_PIXEL + WATER_BYTES_PER_PIXEL, 32);

        // (width, height, glass-side MB, glass+water MB as documented)
        for (w, h, glass_mb, combined_mb) in [
            (1920u32, 1080u32, 49.8, 66.4),
            (2560, 1440, 88.5, 118.0),
            (3840, 2160, 199.1, 265.4),
        ] {
            let glass = caustic_megabytes(w, h);
            let combined = (w as f64)
                * (h as f64)
                * ((CAUSTIC_BYTES_PER_PIXEL + WATER_BYTES_PER_PIXEL) as f64)
                / 1_000_000.0;
            assert!(
                (glass - glass_mb).abs() < 0.1,
                "{w}x{h}: glass {glass:.1} MB != documented {glass_mb} MB"
            );
            assert!(
                (combined - combined_mb).abs() < 0.1,
                "{w}x{h}: combined {combined:.1} MB != documented {combined_mb} MB"
            );
        }
    }

    #[test]
    fn caustic_accumulator_spans_rgb_array_layers() {
        assert_eq!(CAUSTIC_COLOR_LAYERS, 3);
        assert_eq!(caustic_subresource_range().layer_count, 3);
        let shader = include_str!("../../shaders/caustic_splat.comp");
        assert!(shader.contains("uniform uimage2DArray causticAccum;"));
        assert!(shader.contains("imageAtomicAdd(causticAccum, ivec3(q, channel), fv)"));
    }

    #[test]
    fn caustic_source_light_is_visibility_tested_before_refraction() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        for contract in [
            "bool needsVisibility = visibilityMaskNeedsTrace(L.params.z);",
            "uint sourceMask = visibilityOpaqueMask(L.params.z);",
            "visibilityMaskUsesGlass(L.params.z)",
            "G + ns * 0.1",
            "-LtoG",
            "sourceVisibilityDist",
        ] {
            assert!(
                shader.contains(contract),
                "caustic source path lost structural visibility contract: {contract}"
            );
        }
    }

    /// #2239 — under the parked-camera EMA (`pc.decayFactor > 0`), a dim
    /// caustic's per-tap deposit can round below one fixed-point ULP EVERY
    /// frame; the plain `uint()` floor then deposits exactly 0 forever while
    /// the decay pass keeps shrinking the pool, so the running average dies
    /// to zero instead of converging to its true steady state. Pin that the
    /// fixed-point deposit is stochastically rounded (not unconditionally
    /// floored) so a regression silently re-drops sub-ULP contributions.
    #[test]
    fn parked_camera_caustic_deposit_is_stochastically_rounded() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        for contract in [
            "contrib[channel] * w * scale, 0.0, clamp_max",
            "uint fv = uint(depositF);",
            "if (pc.decayFactor > 0.0) {",
            "float fracPart = depositF - float(fv);",
            "if (fracPart > ditherThreshold) {",
        ] {
            assert!(
                shader.contains(contract),
                "caustic deposit lost the sub-ULP stochastic-rounding contract: {contract}"
            );
        }
        // The old unconditional-floor deposit must not reappear.
        assert!(
            !shader.contains("uint fv = uint(clamp(contrib * w * scale, 0.0, clamp_max));"),
            "caustic deposit regressed to an unconditional floor — dim parked-camera \
             caustics will decay to zero (#2239)"
        );
    }
}

#[cfg(test)]
mod parked_visit_tests {
    use super::{advance_parked_visits, CAUSTIC_DECAY_MAX, MAX_FRAMES_IN_FLIGHT};

    /// #2401 / CHAIN2-D2-02 — after k parked *global* frames, the slot
    /// visited on the k-th of them must see `n` equal to its own visit
    /// count, not the global frame count. Pre-fix a slot's 5th visit
    /// (global frame 10) decayed with n = 9 and admitted new energy at
    /// 1/10 after only 5 samples.
    #[test]
    fn each_slot_counts_only_its_own_visits() {
        let mut parked = [0u32; MAX_FRAMES_IN_FLIGHT];
        let mut last_per_slot = [0.0f32; MAX_FRAMES_IN_FLIGHT];
        // 10 consecutive parked global frames, round-robin over the slots.
        for global in 0..10 {
            let frame = global % MAX_FRAMES_IN_FLIGHT;
            last_per_slot[frame] = advance_parked_visits(&mut parked, frame, true);
        }
        let visits_per_slot = (10 / MAX_FRAMES_IN_FLIGHT) as u32;
        for (slot, n) in parked.iter().enumerate() {
            assert_eq!(
                *n, visits_per_slot,
                "slot {slot} counted {n} visits over 10 global frames; each \
                 slot is visited exactly 10/{MAX_FRAMES_IN_FLIGHT} times and \
                 its accumulator only gains a sample on those visits (#2401)",
            );
        }
        // …and the decay factor follows the slot's own n, not the global
        // frame count: n/(n+1), not (2n-1)/(2n).
        let expected = visits_per_slot as f32 / (visits_per_slot as f32 + 1.0);
        for (slot, got) in last_per_slot.iter().enumerate() {
            assert!(
                (got - expected).abs() < 1e-6,
                "slot {slot} decayed with {got}, expected {expected} — a \
                 global counter would have produced a larger factor and \
                 under-weighted this frame's energy (#2401)",
            );
        }
    }

    /// Camera motion resets every slot, not just the one being dispatched:
    /// the other slots' images are equally stale from the old viewpoint.
    #[test]
    fn motion_resets_every_slot() {
        let mut parked = [0u32; MAX_FRAMES_IN_FLIGHT];
        for global in 0..8 {
            advance_parked_visits(&mut parked, global % MAX_FRAMES_IN_FLIGHT, true);
        }
        assert!(parked.iter().all(|n| *n > 0));

        let decay = advance_parked_visits(&mut parked, 0, false);
        assert_eq!(decay, 0.0, "motion must fully clear, not decay");
        assert!(
            parked.iter().all(|n| *n == 0),
            "every slot's counter must reset on motion, or the next parked \
             run over-weights a slot holding pre-motion energy (#2401)",
        );
    }

    /// The cap keeps the accumulator admitting new energy no matter how
    /// long the camera stays parked.
    #[test]
    fn decay_is_capped_below_one() {
        let mut parked = [0u32; MAX_FRAMES_IN_FLIGHT];
        let mut decay = 0.0;
        for _ in 0..10_000 {
            decay = advance_parked_visits(&mut parked, 0, true);
        }
        assert_eq!(decay, CAUSTIC_DECAY_MAX);
        assert!(decay < 1.0);
    }

    /// #2741 — `destroy()` must be safe to call twice: a failed
    /// `recreate_on_resize` calls it once and propagates the error without
    /// clearing the field, so `Drop`/`destroy_allocator_owned_resources`
    /// calls it again. That's only sound if every scalar handle is reset to
    /// its null sentinel immediately after being destroyed, so the second
    /// pass's `!= null()` guard is never armed. Source-scan pin, no device
    /// needed — mirrors
    /// `resize.rs::old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`.
    #[test]
    fn destroy_nulls_every_scalar_handle_after_destroying_it() {
        let src = include_str!("caustic.rs");
        for (destroy_call, null_reset) in [
            (
                "device.destroy_pipeline(self.pipeline, None)",
                "self.pipeline = vk::Pipeline::null();",
            ),
            (
                "device.destroy_pipeline_layout(self.pipeline_layout, None)",
                "self.pipeline_layout = vk::PipelineLayout::null();",
            ),
            (
                "device.destroy_descriptor_pool(self.descriptor_pool, None)",
                "self.descriptor_pool = vk::DescriptorPool::null();",
            ),
            (
                "device.destroy_descriptor_set_layout(self.descriptor_set_layout, None)",
                "self.descriptor_set_layout = vk::DescriptorSetLayout::null();",
            ),
            (
                "device.destroy_sampler(self.point_sampler, None)",
                "self.point_sampler = vk::Sampler::null();",
            ),
        ] {
            let destroy_pos = src
                .find(destroy_call)
                .unwrap_or_else(|| panic!("destroy call not found: {destroy_call}"));
            let null_pos = src
                .find(null_reset)
                .unwrap_or_else(|| panic!("null reset not found: {null_reset}"));
            assert!(
                null_pos > destroy_pos && null_pos - destroy_pos < 200,
                "{null_reset} must immediately follow {destroy_call} so a \
                 second destroy() call (double-free on failed resize, #2741) \
                 finds the guard already disarmed"
            );
        }
    }
}
