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
//! | 9       | GlobalVertices     | SSBO (mesh registry, current)       |
//! | 10      | GlobalIndices      | SSBO (mesh registry, current)       |

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    memory_barrier, write_acceleration_structure, write_combined_image_sampler,
    write_storage_buffer, write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::CAUSTIC_FIXED_SCALE;
use crate::shader_constants::{WORKGROUP_X, WORKGROUP_Y};
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
///
/// #2468 / REN-D14-2026-08-07-01 — `history_valid` is **not** just
/// "camera parked". Every landing point in the pool is a function of the
/// camera *and* of the scene: a swinging lantern, a walking NPC with a
/// torch, an occluder crossing between the light and the glass, or a
/// glass door opening all invalidate the accumulated pool while the
/// camera never moves. This path has no per-pixel motion-vector /
/// mesh-ID / normal rejection of the kind `svgf_temporal.comp` and
/// `taa.comp` use, so with a camera-only gate the cap held up to
/// `1/(1 - 0.995) = 200` frames (~3 s at 60 fps) of stale pool at up to
/// 99.5% weight. The host now folds a scene-dirty signal in — see
/// `context::draw`'s `caustic_history_valid`.
fn advance_parked_visits(
    parked: &mut [u32; MAX_FRAMES_IN_FLIGHT],
    frame: usize,
    history_valid: bool,
) -> f32 {
    if !history_valid {
        *parked = [0; MAX_FRAMES_IN_FLIGHT];
        return 0.0;
    }
    parked[frame] = parked[frame].saturating_add(1);
    let n = parked[frame] as f32;
    (n / (n + 1.0)).min(CAUSTIC_DECAY_MAX)
}

#[inline]
fn reset_parked_slot(parked: &mut [u32; MAX_FRAMES_IN_FLIGHT], frame: usize) {
    parked[frame] = 0;
}

/// FNV-1a offset basis / prime — the caustic scene key's mixer.
const CAUSTIC_KEY_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const CAUSTIC_KEY_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Fold one `f32` into a caustic scene key (#2468).
///
/// Hashes the raw bit pattern, so `-0.0` and `0.0` key differently and a
/// NaN keys stably — both err toward "scene changed", which only costs a
/// re-accumulation, never a stale pool.
#[inline]
pub(crate) fn fold_caustic_key_f32(key: u64, v: f32) -> u64 {
    (key ^ v.to_bits() as u64).wrapping_mul(CAUSTIC_KEY_PRIME)
}

/// Start a caustic scene key. See [`fold_caustic_key_f32`].
#[inline]
pub(crate) fn caustic_key_seed() -> u64 {
    CAUSTIC_KEY_BASIS
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

// SAFETY: two `[f32; 4]` fields — homogeneous scalar arrays tile the
// struct's declared size with no implicit padding (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for CausticParams {}

struct CausticSlot {
    image: vk::Image,
    /// The slot's only `VkImageView`, used for both roles: `r32ui` storage
    /// for the compute shader's atomic writes, and `usampler2DArray` for
    /// composite's sampling.
    ///
    /// These were two views until #2779. The `610cb170` RGB-array refactor
    /// left both built from the same `ImageViewCreateInfo` — same image,
    /// type, format and subresource range — so the pair was byte-identical,
    /// costing two extra `VkImageView`s per frame in flight and a second
    /// destroy on every teardown path. A view carries no usage or layout
    /// state (the descriptor type and the barrier's `image_layout` supply
    /// both), so nothing distinguished them.
    view: vk::ImageView,
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
            // 9 global vertices. Written per frame because streaming can
            // replace the mesh registry's backing buffer.
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 10 global indices. Paired with binding 9 for committed-hit
            // triangle reconstruction in caustic_splat.comp.
            vk::DescriptorSetLayoutBinding::default()
                .binding(10)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
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

        // One view serves both roles — see `CausticSlot::view` (#2779).
        // SAFETY: `image` is bound above and the view is owned by the
        // returned CausticSlot, which destroys it on teardown.
        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
                    .format(CAUSTIC_FORMAT)
                    .subresource_range(caustic_subresource_range()),
                None,
            )
        };
        let view = match view.context("caustic image view") {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: view creation failed; free alloc first, then
                // destroy the bound image. It was created by this device
                // just above and has not been bound to any in-flight
                // command buffer on this error path.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(CausticSlot {
            image,
            view,
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
                .image_view(self.slots[f].view)
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

    /// Point committed-hit reconstruction at the current global geometry.
    ///
    /// Mesh streaming can replace both buffers, so `draw_frame` refreshes
    /// these bindings for the idle frame-in-flight slot before recording the
    /// caustic dispatch. This mirrors SceneBuffers bindings 8/9 rather than
    /// retaining a stale buffer handle across a geometry-pool rebuild.
    pub fn write_geometry_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        vertex_buffer: vk::Buffer,
        vertex_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let vertex_info = [vk::DescriptorBufferInfo {
            buffer: vertex_buffer,
            offset: 0,
            range: vertex_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let set = self.descriptor_sets[frame_index];
        let writes = [
            write_storage_buffer(set, 9, &vertex_info),
            write_storage_buffer(set, 10, &index_info),
        ];
        unsafe {
            // SAFETY: this FIF descriptor set is idle when called from
            // draw_frame; the mesh-registry buffers and both descriptor-info
            // arrays remain live for the duration of update_descriptor_sets.
            device.update_descriptor_sets(&writes, &[])
        }
    }

    /// Caustic accumulator view used by the composite pass as
    /// `usampler2DArray` — the same view the compute pass binds as storage,
    /// see [`CausticSlot::view`] (#2779).
    pub fn sampled_view(&self, frame: usize) -> vk::ImageView {
        self.slots[frame].view
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
        history_valid: bool,
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
        // motion (decay 0 → single-frame clear, no smear) — camera motion
        // *or* scene motion (#2468); see `advance_parked_visits`.
        let decay_factor = advance_parked_visits(&mut self.parked_frames, frame, history_valid);
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
        // #2768 — from the same constants `caustic_splat.comp`'s
        // `local_size` is generated from; see `taa.rs`'s dispatch.
        let gx = self.width.div_ceil(WORKGROUP_X);
        let gy = self.height.div_ceil(WORKGROUP_Y);
        let push_bytes = |decay_only: u32, factor: f32| -> [u8; 8] {
            let mut b = [0u8; 8];
            b[0..4].copy_from_slice(&decay_only.to_ne_bytes());
            b[4..8].copy_from_slice(&factor.to_ne_bytes());
            b
        };

        if history_valid {
            // Wait for the slot's previous use before the decay pass scales
            // it. That prior use is one of three, not two: the steady-state
            // splat compute-write + composite fragment-read, OR — #3646 —
            // `clear_for_skip`'s `vkCmdClearColorImage` from an earlier
            // visit to this slot, which is a TRANSFER_WRITE. The decay pass
            // `imageLoad`s the accumulator, so leaving TRANSFER out of the
            // source scope left the skip-clear's write with no dependency
            // chain into this read.
            let pre_decay = vk::ImageMemoryBarrier::default()
                .src_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_WRITE,
                )
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
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
            // #3646 — TRANSFER_WRITE in the source scope for the same
            // reason as `pre_decay` above: the slot's prior use may have
            // been `clear_for_skip`'s clear rather than a compute/fragment
            // access.
            let pre_clear_barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_WRITE,
                )
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(slot_img)
                .subresource_range(clear_range);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
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
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // The image is emptied below, so its temporal age must restart too.
        // Otherwise a long skip streak resumes with a near-0.995 history
        // weight against black and takes ~200 slot visits to fade back in.
        reset_parked_slot(&mut self.parked_frames, frame);
        let slot_img = self.slots[frame].image;
        let clear_range = caustic_subresource_range();
        // Same over-specified wait stages `dispatch`'s moving-camera clear
        // uses: the slot's prior use was either this pipeline's own
        // compute-write + composite's fragment-read (steady state), or
        // GENERAL-but-untouched from `initialize_layouts` (frame 0 / just
        // after resize) — safe either way.
        let pre_clear_barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::SHADER_WRITE
                    | vk::AccessFlags::TRANSFER_WRITE,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(slot_img)
            .subresource_range(clear_range);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER
                | vk::PipelineStageFlags::FRAGMENT_SHADER
                | vk::PipelineStageFlags::TRANSFER,
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
        // TRANSFER → FRAGMENT for composite's sample this frame, and
        // TRANSFER → COMPUTE for the slot's *next* visit (#3646). No
        // compute dispatch follows this clear within the frame, but the
        // next visit to this slot is `dispatch`, whose decay pass
        // `imageLoad`s what was cleared here. Naming only FRAGMENT left
        // that cross-frame edge uncovered from this side; `dispatch`'s
        // own barriers now name TRANSFER in their source scope too, so
        // the chain is closed from both ends.
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
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
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
                device.destroy_image_view(slot.view, None);
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
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
            }
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
    }
}

// CAUSTIC_FIXED_SCALE reaches the two ends of the caustic round trip by two
// different channels, and each needs its own guard:
//   * compile-time `#define`, consumed by composite.frag's divide — pinned by
//     `shader_constants::tests::{generated_header_contains_all_defines,
//     affected_shaders_include_constants_header}` (the header is generated,
//     #1038) and by `composite::tests::
//     caustic_radiance_combines_glass_rgb_and_water_in_float_fixed_point`
//     (the divide is actually written).
//   * runtime UBO lane `tune.x`, consumed by caustic_splat.comp's deposit —
//     pinned by `upload_scale_matches_the_constant_composite_divides_by`
//     below (#2776). Neither shader_constants test covers this end: they
//     check that the `#define` is generated and included, not what the host
//     uploads.

#[cfg(test)]
mod tests {
    use super::{
        caustic_megabytes, caustic_subresource_range, CAUSTIC_BYTES_PER_PIXEL, CAUSTIC_COLOR_LAYERS,
    };

    /// #2776 — the caustic round trip only balances if the `tune.x` lane the
    /// host uploads is the same number `composite.frag` divides by. The two
    /// ends never meet in one expression: the deposit scales by a *runtime*
    /// UBO lane (`causticTune.x`), the decode divides by a *compile-time*
    /// `#define`. Nothing checked the upload end, so making `tune.x` a live
    /// tunable would have desynced them silently — every caustic pixel wrong
    /// by the ratio, with no test, validation layer or visual tell beyond
    /// "the pool looks off".
    ///
    /// Asserted against source because `dispatch` needs a live device and a
    /// mapped UBO; the value is a compile-time symbol either way.
    #[test]
    fn upload_scale_matches_the_constant_composite_divides_by() {
        const SOURCE: &str = include_str!("caustic.rs");
        const SHADER: &str = include_str!("../../shaders/caustic_splat.comp");

        // The `tune` lane assignment inside `dispatch`'s `CausticParams`.
        let tune = SOURCE
            .split_once("        let params = CausticParams {")
            .expect("dispatch must build its CausticParams inline")
            .1;
        let lanes = tune
            .split_once("tune: [")
            .expect("CausticParams must set `tune`")
            .1
            .split_once(']')
            .expect("unterminated tune array")
            .0;
        let x = lanes
            .split(',')
            .next()
            .expect("tune array must have an x lane")
            .trim();
        assert_eq!(
            x, "CAUSTIC_FIXED_SCALE",
            "tune.x must upload the CAUSTIC_FIXED_SCALE symbol, not a literal \
             or a runtime field — composite.frag divides by the `#define` of \
             the same name, so any other value scales every caustic pixel by \
             the ratio between them"
        );

        // The consuming end: the shader must still route that lane into the
        // scale the deposit and the atomic-range clamp are built on.
        assert!(
            SHADER.contains("float scale = causticTune.x;"),
            "caustic_splat.comp must take its fixed-point scale from \
             causticTune.x — if the deposit stops reading the uploaded lane, \
             pinning the upload proves nothing"
        );
        assert!(
            SHADER.contains("float clamp_max = float(0xFFFFFFFFu) / scale;"),
            "the atomic-range clamp must stay anchored to the same scale \
             (#1099)"
        );
    }

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

    /// #3820 (REN-WD-D2-01) — sibling of the `water.frag` fix: the glass
    /// caustic splat must bound its dispatch/atomic writes against the
    /// actual `causticAccum` image, not `causticScreen.xy` (the render
    /// extent uniform). See `water.rs`'s matching test for the failure mode
    /// this generally guards against.
    #[test]
    fn caustic_splat_bounds_against_the_accumulator_image_not_screen_extent() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        assert!(
            shader.contains("ivec2 size = imageSize(causticAccum).xy;"),
            "caustic_splat.comp's dispatch/write bound must be imageSize(causticAccum).xy, \
             not derived from causticScreen.xy"
        );
        assert!(
            !shader.contains("ivec2 size = ivec2(causticScreen.xy);"),
            "the old causticScreen.xy-derived bound must not be reintroduced"
        );
    }

    #[test]
    fn glass_caustic_source_comes_from_committed_glass_hit_not_opaque_gbuffer() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        for contract in [
            "VISIBILITY_LAYER_GLASS,",
            "sourceIdx != instIdx",
            "rayQueryGetIntersectionPrimitiveIndexEXT(sourceRQ, true)",
            "getCausticHitTriNormal(sourceIdx, sourcePrim)",
            "layout(std430, set = 0, binding = 9) readonly buffer GlobalVertices",
            "layout(std430, set = 0, binding = 10) readonly buffer GlobalIndices",
        ] {
            assert!(
                shader.contains(contract),
                "caustic source reconstruction lost committed-hit contract: {contract}"
            );
        }

        let executable: String = shader
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !executable.contains("vec3 N = octDecode"),
            "glass blend preserves the opaque normal attachment; using it as the \
             caustic source normal starts transport on the receiver behind glass"
        );

        let host = include_str!("caustic.rs");
        assert!(host.contains("write_storage_buffer(set, 9, &vertex_info)"));
        assert!(host.contains("write_storage_buffer(set, 10, &index_info)"));

        // #3282 / TD1-2026-08-24-01 — the re-point moved from `draw.rs` into
        // `sync_and_acquire_frame.rs`.
        let draw = include_str!("context/sync_and_acquire_frame.rs");
        assert!(
            draw.contains("caustic.write_geometry_buffers"),
            "streaming buffer reallocations must refresh caustic geometry descriptors"
        );
    }

    #[test]
    fn caustic_source_light_is_visibility_tested_before_refraction() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        for contract in [
            "bool needsVisibility = visibilityMaskNeedsTrace(L.params.z);",
            "uint sourceMask = visibilityOpaqueMask(L.params.z);",
            "visibilityMaskUsesGlass(L.params.z)",
            "offsetRayOriginForDirection(G, ns, -LtoG)",
            "-LtoG",
            "sourceVisibilityDist",
        ] {
            assert!(
                shader.contains(contract),
                "caustic source path lost structural visibility contract: {contract}"
            );
        }
    }

    /// #2775 — an occluded light must consume the per-pixel budget before
    /// its visibility ray can `continue`; otherwise a dense list of blocked
    /// lamps can traverse all `lightCount` entries despite `max_lights`.
    #[test]
    fn visibility_traversals_are_charged_before_occlusion_can_continue() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        let charged = shader.find("budgetedLights++;").expect("budget charge");
        let visibility = shader
            .find("bool needsVisibility = visibilityMaskNeedsTrace")
            .expect("visibility block");
        let occluded_continue = shader[visibility..]
            .find("continue;")
            .map(|offset| visibility + offset)
            .expect("occluded-light continue");

        assert!(charged < visibility && charged < occluded_continue);
        assert!(
            shader.contains("budgetedLights < maxLights"),
            "the charged counter must remain the loop's max_lights bound"
        );
        assert!(
            !shader.contains("processedLights"),
            "the old accepted-light counter lets occluded visibility rays run for free"
        );
    }

    #[test]
    fn glass_caustics_use_the_shared_gaussian_footprint() {
        let shader = include_str!("../../shaders/caustic_splat.comp");
        assert!(shader.contains("#include \"include/caustic_kernel.glsl\""));
        assert!(shader.contains("causticGauss5Weight(kx, ky)"));
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
    use super::{
        advance_parked_visits, caustic_key_seed, fold_caustic_key_f32, reset_parked_slot,
        CAUSTIC_DECAY_MAX, MAX_FRAMES_IN_FLIGHT,
    };

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

    #[test]
    fn skipped_slot_restarts_temporal_convergence_after_clear() {
        let mut parked = [1_000; MAX_FRAMES_IN_FLIGHT];
        reset_parked_slot(&mut parked, 1);
        assert_eq!(parked[1], 0);
        assert_eq!(
            advance_parked_visits(&mut parked, 1, true),
            0.5,
            "a cleared skip slot must admit a fresh sample at first-visit weight"
        );
        assert_eq!(parked[0], 1_000, "only the cleared FIF slot resets");
    }

    /// #2468 / REN-D14-2026-08-07-01 — a parked camera is not enough to
    /// keep the pool: the accumulator has no per-pixel invalidation of its
    /// own, so the host's scene-dirty signal has to reach it through the
    /// same gate camera motion uses. Without this, a player standing still
    /// while an NPC walks past with a torch kept up to
    /// `1/(1 - CAUSTIC_DECAY_MAX)` frames of stale pool.
    #[test]
    fn scene_motion_invalidates_the_pool_like_camera_motion() {
        let mut parked = [0u32; MAX_FRAMES_IN_FLIGHT];
        // Park long enough that the pool is at the decay ceiling — the
        // regime where a stale splat is held at up to 99.5% weight.
        for _ in 0..1_000 {
            advance_parked_visits(&mut parked, 0, true);
        }
        assert_eq!(
            advance_parked_visits(&mut parked, 0, true),
            CAUSTIC_DECAY_MAX
        );

        // Camera still parked, but the host reports the scene moved.
        let decay = advance_parked_visits(&mut parked, 0, false);
        assert_eq!(
            decay, 0.0,
            "a scene-dirty frame must clear the pool outright, not decay it"
        );
        assert!(parked.iter().all(|n| *n == 0));
        // And the next parked frame restarts convergence from scratch
        // rather than resuming near the ceiling.
        assert_eq!(advance_parked_visits(&mut parked, 0, true), 0.5);
    }

    /// The caustic scene key must actually distinguish the states the host
    /// feeds it — a folder that collapsed inputs would report "unchanged"
    /// through a moving light rig and reintroduce the ghost.
    #[test]
    fn caustic_key_separates_moved_inputs_and_is_order_sensitive() {
        let fold = |vals: &[f32]| {
            vals.iter()
                .fold(caustic_key_seed(), |k, v| fold_caustic_key_f32(k, *v))
        };
        let base = fold(&[1.0, 2.0, 3.0]);
        assert_eq!(base, fold(&[1.0, 2.0, 3.0]), "key must be deterministic");
        assert_ne!(
            base,
            fold(&[1.0, 2.0, 3.0001]),
            "a moved light must key differently"
        );
        assert_ne!(base, fold(&[3.0, 2.0, 1.0]), "key must be order-sensitive");
        // A light entering / leaving the set changes the length prefix the
        // host folds in, so it can't alias a same-length rig.
        assert_ne!(base, fold(&[1.0, 2.0, 3.0, 0.0]));
        // Sign of zero is a real transform difference; err toward dirty.
        assert_ne!(fold(&[0.0]), fold(&[-0.0]));
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

/// #3646 / #3647 — a skip-path clear and the next visit to the same
/// frame-in-flight slot must agree about `TRANSFER`.
///
/// `clear_for_skip` writes the accumulator with `vkCmdClearColorImage`
/// (`TRANSFER_WRITE`); the slot's *next* visit is `dispatch`, whose decay
/// pass `imageLoad`s that same image. Those two are in different command
/// buffer submissions, so the only thing that can carry the write to the
/// read is the barrier masks — and the clear published to `FRAGMENT_SHADER`
/// only while `dispatch`'s barriers named `COMPUTE | FRAGMENT` in their
/// source scope, so `TRANSFER` appeared on neither side.
///
/// Sync validation does not currently flag this (the both-slots fence wait
/// at `MAX_FRAMES_IN_FLIGHT == 2` covers it — see `sync.rs`'s #870 block),
/// which is exactly why a source-shape pin is warranted: the masks must
/// stay right independently of a fence that #870 documents as fragile, and
/// the #653 precedent (`taa.rs`, `svgf.rs`) already applies that rule
/// elsewhere in this crate.
#[cfg(test)]
mod skip_clear_mask_pin_tests {
    /// Source between `needle` and the next `fn ` after it — one function
    /// body, without needing a real parser.
    fn body_after<'a>(src: &'a str, needle: &str) -> &'a str {
        let start = src
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found — the site moved or was renamed"));
        let rest = &src[start + needle.len()..];
        let end = rest.find("\n    pub unsafe fn ").unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn caustic_skip_clear_and_next_visit_agree_on_transfer() {
        const CAUSTIC_RS: &str = include_str!("caustic.rs");

        let clear = body_after(CAUSTIC_RS, "pub unsafe fn clear_for_skip");
        assert!(
            clear.contains(
                "PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER"
            ),
            "clear_for_skip's post-clear barrier must publish to COMPUTE as \
             well as FRAGMENT — the slot's next visit is `dispatch`'s decay \
             compute read, not just this frame's composite sample (#3646)",
        );

        let dispatch = body_after(CAUSTIC_RS, "pub unsafe fn dispatch");
        let transfer_srcs = dispatch
            .matches("| vk::AccessFlags::TRANSFER_WRITE,")
            .count();
        assert!(
            transfer_srcs >= 2,
            "`dispatch`'s pre-decay and pre-clear barriers must both name \
             TRANSFER_WRITE in their source scope, so a prior \
             `clear_for_skip` on this slot chains into them — found \
             {transfer_srcs} (#3646)",
        );
    }

    #[test]
    fn volumetrics_neutral_clear_and_next_visit_agree_on_transfer() {
        const VOLUMETRICS_RS: &str = include_str!("volumetrics.rs");

        assert!(
            VOLUMETRICS_RS.contains(
                ".src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_WRITE)"
            ),
            "`dispatch`'s `pre_int_write` must name TRANSFER_WRITE in its \
             source scope — the slot's prior use is `record_neutral_frame`'s \
             clear on every frame before the TLAS exists (#3647)",
        );
        let neutral = VOLUMETRICS_RS
            .split_once("pub unsafe fn record_neutral_frame")
            .expect("record_neutral_frame")
            .1;
        assert!(
            neutral.contains("| vk::AccessFlags::TRANSFER_WRITE,"),
            "`record_neutral_frame`'s own `to_clear` must name TRANSFER_WRITE \
             in its source scope — at load every frame on this slot is a \
             repeat neutral clear (#3647)",
        );
    }
}
