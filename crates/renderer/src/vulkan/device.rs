//! Physical device selection and logical device creation.

use anyhow::{Context, Result};
use ash::vk;
use std::ffi::CStr;

/// Holds the selected queue family indices.
#[derive(Debug, Clone, Copy)]
pub struct QueueFamilyIndices {
    pub graphics: u32,
    pub present: u32,
}

/// Runtime GPU capabilities detected during device creation.
///
/// `Default` (all-false / zero) is the "no capabilities" baseline — used by
/// gating-logic unit tests; the real device-creation path fully populates
/// every field.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceCapabilities {
    /// True if VK_KHR_acceleration_structure + VK_KHR_ray_query are available.
    pub ray_query_supported: bool,
    /// True if the physical device exposes `samplerAnisotropy` in
    /// `VkPhysicalDeviceFeatures`. Required to enable anisotropic
    /// filtering on the shared texture sampler. Universally available
    /// on desktop GPUs; we still guard it to be safe on SoCs.
    pub sampler_anisotropy_supported: bool,
    /// `maxSamplerAnisotropy` from `VkPhysicalDeviceLimits`, already
    /// clamped to our configured target (16×). Zero when
    /// `sampler_anisotropy_supported` is false.
    pub max_sampler_anisotropy: f32,
    /// True if the physical device exposes `multiDrawIndirect` in
    /// `VkPhysicalDeviceFeatures`. Enables `vkCmdDrawIndexedIndirect`
    /// with `drawCount > 1` — one API call dispatches an arbitrary
    /// number of draws reading their parameters from an
    /// `INDIRECT_BUFFER`. Universally supported on desktop GPUs since
    /// Vulkan 1.0. The draw path uses it to collapse consecutive
    /// batches sharing `(pipeline_key, is_decal)` into a single
    /// command-buffer entry. See #309.
    pub multi_draw_indirect_supported: bool,
    /// True if the physical device exposes `fillModeNonSolid` in
    /// `VkPhysicalDeviceFeatures`. Required to bind a pipeline whose
    /// `polygon_mode` is `vk::PolygonMode::LINE` (wireframe). Universally
    /// available on desktop GPUs but missing on some mobile drivers.
    /// When false, the wireframe pipeline variants are not created
    /// and `NiWireframeProperty` meshes silently fall back to FILL —
    /// the audit at #869 noted Oblivion vanilla ships zero wireframe
    /// meshes, so the fallback is invisible to gameplay content.
    pub fill_mode_non_solid_supported: bool,
    /// `maxPerStageDescriptorUpdateAfterBindSampledImages` from
    /// `VkPhysicalDeviceDescriptorIndexingProperties` (Vulkan 1.2 core),
    /// already clamped to a sane ceiling. The `TextureRegistry` sizes its
    /// bindless array with this so we don't silently truncate on cells with
    /// many unique textures. Fixes the REN-MEM-C3 residency cliff (#425).
    pub max_bindless_sampled_images: u32,
    /// `minAccelerationStructureScratchOffsetAlignment` from
    /// `VkPhysicalDeviceAccelerationStructurePropertiesKHR`. Every BLAS /
    /// TLAS build's `scratch_data.device_address` must be a multiple of
    /// this value (typically 128 or 256 on desktop drivers, but the spec
    /// permits any power of two). `gpu-allocator` returns GpuOnly
    /// allocations at >= 256 B alignment on every desktop driver we've
    /// shipped against, so the constraint is currently met for free, but
    /// nothing in the allocator API guarantees it. The
    /// `debug_assert!(scratch_address % align == 0)` at every
    /// `cmd_build_acceleration_structures` call site catches a future
    /// driver that returns a smaller alignment than the AS spec needs.
    /// Zero when `ray_query_supported` is false. See #659 / #260 R-05.
    pub min_accel_struct_scratch_offset_alignment: u32,
    /// `VkPhysicalDeviceLimits::timestampPeriod` — nanoseconds per
    /// `vkCmdWriteTimestamp` tick on this device. Multiply
    /// `(end_tick - start_tick) * timestamp_period_ns` to get the
    /// elapsed time in ns for the bracketed work. Typically `1.0` on
    /// NVIDIA + AMD desktop drivers, `52.083...` on Intel Arc, but
    /// the spec only guarantees > 0. Used by the per-pass GPU timer
    /// (#1194 / PERF-DIM7-INSTR) — zero if `timestamp_supported`
    /// is false (host driver lacks query support entirely).
    pub timestamp_period_ns: f32,
    /// `timestampComputeAndGraphics` from `VkPhysicalDeviceLimits`:
    /// when true, both compute and graphics queues support
    /// `vkCmdWriteTimestamp`. We only use the graphics queue today
    /// so the weaker `timestamp_valid_bits[graphics_queue_family] > 0`
    /// would suffice, but `timestampComputeAndGraphics` is the
    /// universal-true guarantee and matches our usage. Zero on every
    /// shipped desktop GPU is unheard of; the gate exists so the
    /// query pool creation skips cleanly on a hypothetical driver
    /// that lacks it.
    pub timestamp_supported: bool,
    /// `synchronization2` from `VkPhysicalDeviceVulkan13Features`.
    /// Required by the renderer: `PipelineStageFlags::NONE` (== 0) is
    /// only a legal stage-mask value in sync1 `vkCmdPipelineBarrier`
    /// calls when this feature is enabled (VUID-vkCmdPipelineBarrier-
    /// srcStageMask-4957). `is_device_suitable` rejects any GPU that
    /// doesn't expose it, so this field is always `true` at runtime.
    /// Kept in `DeviceCapabilities` for diagnostic logging only. #1437.
    pub synchronization2_supported: bool,
    /// `hostQueryReset` from `VkPhysicalDeviceVulkan12Features`. Enables
    /// the host-side `vkResetQueryPool` (no command buffer). Two
    /// independent consumers need it: the BLAS-compaction path
    /// (`blas_static.rs`, RT-only) and the per-pass GPU timers
    /// (`gpu_timers.rs`, #1194 — RT-*independent*). It was previously
    /// gated on `ray_query_supported`, which silently broke the timer
    /// path on a timestamp-capable but RT-less GPU (the feature was
    /// disabled yet `GpuPerFrameTimers::new` still called
    /// `reset_query_pool` → VUID-vkResetQueryPool-None-02665). We now
    /// probe and enable it on its own merits (#1478 / REN-D23-NEW-01).
    /// Vulkan 1.2 core; universally available on every RT-capable
    /// desktop GPU, so the BLAS-compaction path is unaffected.
    pub host_query_reset_supported: bool,
    /// `VK_EXT_memory_budget` exposes a live per-heap usage / budget
    /// pair via `vkGetPhysicalDeviceMemoryProperties2` chained with
    /// `VkPhysicalDeviceMemoryBudgetPropertiesEXT`. Without it, the
    /// debug-UI's "VRAM usage" line has to fall back to
    /// `gpu_allocator::generate_report` (which reports our own
    /// allocations, not the driver's view including swapchain images
    /// plus descriptor pools plus every other Vulkan-internal residency
    /// cost). Universally supported on shipped desktop drivers since
    /// 2018; the gate exists so device creation doesn't fail when a
    /// SoC / software rasteriser doesn't advertise it.
    pub memory_budget_supported: bool,
    /// `textureCompressionBC` from `VkPhysicalDeviceFeatures`. Required
    /// to create images with BC1/BC2/BC3/BC4/BC5/BC6H/BC7 compressed
    /// formats. Without this feature enabled the driver rejects every
    /// BC-compressed `vkCreateImage` call — all DDS textures (which
    /// Bethesda BSA archives store exclusively as BC-family formats)
    /// fall back to the checker placeholder. Universally available on
    /// desktop GPUs (x86/x64 hardware since DX10-era); the gate exists
    /// for completeness and catches hypothetical SoC / software
    /// rasteriser configurations.
    pub texture_compression_bc: bool,
}

impl DeviceCapabilities {
    /// Whether the per-pass GPU timers (`gpu_timers.rs`) can run on this
    /// device. BOTH gates are required and independent of ray-query support:
    /// `timestamp_supported` for `vkCmdWriteTimestamp`, and
    /// `host_query_reset_supported` because the timer's `new()` /
    /// `read_and_reset()` use the host-side `vkResetQueryPool` (no command
    /// buffer). `host_query_reset` was historically gated on RT support, so a
    /// timestamp-capable but RT-less GPU reached `reset_query_pool` with the
    /// feature disabled (VUID-vkResetQueryPool-None-02665). This predicate is
    /// the single source of truth for that gate. See #1478 / #1636.
    pub fn gpu_timers_supported(&self) -> bool {
        self.timestamp_supported && self.host_query_reset_supported
    }
}

/// Sum of `VkMemoryHeap.size` across every `DEVICE_LOCAL` heap exposed
/// by the physical device — approximates total VRAM. Used as the
/// denominator in budget computations (BLAS residency cap, memory-
/// usage warn threshold). See #505.
///
/// Note: `heap.size` is the heap's *capacity*, not the driver's
/// *free* figure. `VK_EXT_memory_budget` gives a more accurate live
/// budget but is still an extension (not core); heap size is a
/// stable upper bound and works uniformly across all Vulkan 1.0+
/// drivers.
pub fn total_device_local_bytes(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::DeviceSize {
    // SAFETY: `get_physical_device_memory_properties` has no preconditions
    // beyond a valid physical device handle, which the caller holds through
    // the `VulkanContext` construction chain.
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    mem_props.memory_heaps[..mem_props.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .sum()
}

/// Size of the *smallest* `DEVICE_LOCAL` heap. On systems with a
/// discrete + integrated GPU exposed through the same Vulkan device
/// (rare but possible in hybrid laptops), this is the tighter of the
/// two — running an allocator to that heap's limit fails first.
/// Returns 0 when no `DEVICE_LOCAL` heap exists.
pub fn smallest_device_local_heap_bytes(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::DeviceSize {
    let mem_props = unsafe {
        // SAFETY: `instance` is the live Vulkan instance and `physical_device`
        // was enumerated from it; the query has no preconditions beyond a valid
        // handle and writes only into the returned properties struct.
        instance.get_physical_device_memory_properties(physical_device)
    };
    mem_props.memory_heaps[..mem_props.memory_heap_count as usize]
        .iter()
        .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|heap| heap.size)
        .min()
        .unwrap_or(0)
}

/// Required device extensions (always needed).
const REQUIRED_EXTENSIONS: &[&CStr] = &[ash::khr::swapchain::NAME];

/// Optional RT extensions (enabled when available).
const RT_EXTENSIONS: &[&CStr] = &[
    ash::khr::acceleration_structure::NAME,
    ash::khr::ray_query::NAME,
    ash::khr::deferred_host_operations::NAME,
];

/// Picks a suitable physical device that supports our required extensions
/// and has graphics + present queue families for the given surface.
pub fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, QueueFamilyIndices, DeviceCapabilities)> {
    let devices = unsafe {
        // SAFETY: `instance` is the live Vulkan instance; enumeration writes
        // only into the returned Vec of device handles.
        instance
            .enumerate_physical_devices()
            .context("Failed to enumerate physical devices")?
    };

    if devices.is_empty() {
        anyhow::bail!("No Vulkan-capable GPU found");
    }

    for &device in &devices {
        if let Some((indices, caps)) =
            is_device_suitable(instance, surface_loader, surface, device)?
        {
            let props = unsafe {
                // SAFETY: `instance` is live and `device` was enumerated from it
                // above; the query writes only into the returned properties struct.
                instance.get_physical_device_properties(device)
            };
            // SAFETY: device_name is a fixed-size [c_char; 256] array null-terminated by the
            // Vulkan driver. The pointer is valid for the lifetime of `props` (stack-local).
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
            log::info!(
                "Selected GPU: {:?} (ray query: {}, sync2: {})",
                name,
                caps.ray_query_supported,
                caps.synchronization2_supported,
            );
            return Ok((device, indices, caps));
        }
    }

    anyhow::bail!(
        "No suitable GPU found (need graphics + present queues, swapchain support, \
         and Vulkan 1.3 synchronization2 — RTX 20-series / RDNA1 / Arc or newer required)"
    )
}

fn is_device_suitable(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> Result<Option<(QueueFamilyIndices, DeviceCapabilities)>> {
    let available_extensions = unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it; the
        // query writes only into the returned Vec of extension properties.
        instance
            .enumerate_device_extension_properties(device)
            .context("Failed to enumerate device extensions")?
    };

    let has_extension = |name: &CStr| -> bool {
        available_extensions.iter().any(|ext| {
            // SAFETY: extension_name is a fixed-size null-terminated [c_char; 256]
            // from the Vulkan driver. Pointer valid for lifetime of `ext` (in Vec).
            let ext_name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            ext_name == name
        })
    };

    // Check required extensions.
    for required in REQUIRED_EXTENSIONS {
        if !has_extension(required) {
            return Ok(None);
        }
    }

    // Check optional RT extensions.
    let ray_query_supported = RT_EXTENSIONS.iter().all(|ext| has_extension(ext));

    // Optional `VK_EXT_memory_budget`. Live per-heap usage / budget
    // for the debug-UI VRAM panel — without it we fall back to
    // `gpu_allocator::generate_report` which only sees our own
    // allocations, not the driver's full residency view.
    let memory_budget_supported = has_extension(ash::ext::memory_budget::NAME);

    // Query features + limits for optional features we care about.
    // `samplerAnisotropy` is the only one right now (issue #136).
    let features = unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it; the
        // query writes only into the returned features struct.
        instance.get_physical_device_features(device)
    };
    let properties = unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it; the
        // query writes only into the returned properties struct.
        instance.get_physical_device_properties(device)
    };
    let sampler_anisotropy_supported = features.sampler_anisotropy == vk::TRUE;
    // Cap at 16× — the point of diminishing returns on every modern
    // GPU, and the common "16x AF" preset players expect.
    let max_sampler_anisotropy = if sampler_anisotropy_supported {
        properties.limits.max_sampler_anisotropy.min(16.0)
    } else {
        0.0
    };
    let multi_draw_indirect_supported = features.multi_draw_indirect == vk::TRUE;
    let fill_mode_non_solid_supported = features.fill_mode_non_solid == vk::TRUE;
    let texture_compression_bc = features.texture_compression_bc == vk::TRUE;

    // Probe Vulkan 1.3 core feature `synchronization2`. Required: the
    // renderer uses `PipelineStageFlags::NONE` in sync1 barriers
    // across bloom, SSAO, caustic, texture upload, and volumetrics.
    // Without this feature those barriers violate VUID-vkCmdPipeline
    // Barrier-srcStageMask-4957. Available on all RTX-class GPUs
    // (Vulkan 1.3 core); devices that return FALSE are rejected. #1437.
    //
    // Also probe `VkPhysicalDeviceVulkan12Features.hostQueryReset` in the
    // same round-trip — needed (decoupled from RT) by the GPU timer path
    // (#1478 / REN-D23-NEW-01).
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13_features = vk::PhysicalDeviceVulkan13Features::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut vulkan12_features)
        .push_next(&mut vulkan13_features);
    unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it;
        // `features2` and the `vulkan12_features` / `vulkan13_features` structs
        // it chains via pNext are stack-local and outlive this call, which
        // writes only into them.
        instance.get_physical_device_features2(device, &mut features2);
    }
    let synchronization2_supported = vulkan13_features.synchronization2 == vk::TRUE;
    if !synchronization2_supported {
        return Ok(None);
    }
    let host_query_reset_supported = vulkan12_features.host_query_reset == vk::TRUE;

    // Query descriptor indexing properties for the UPDATE_AFTER_BIND bindless
    // array ceiling. Vulkan 1.2 core exposes this via the pNext chain on
    // `vkGetPhysicalDeviceProperties2`. Falls back to the plain
    // `maxPerStageDescriptorSampledImages` limit on older drivers — still
    // vastly larger than the hardcoded 1024 we were using before (#425).
    //
    // Same `vkGetPhysicalDeviceProperties2` call also pulls
    // `VkPhysicalDeviceAccelerationStructurePropertiesKHR` so the BLAS /
    // TLAS scratch alignment requirement is available without a second
    // round-trip. The AS struct is only meaningful when the RT extensions
    // are present; we still chain it unconditionally because Vulkan
    // tolerates pNext entries the driver doesn't recognise (returns
    // zero-initialised), and reading zero out lets us default to a
    // trivial alignment of 1 when RT is unsupported (#659).
    let mut indexing_props = vk::PhysicalDeviceDescriptorIndexingProperties::default();
    let mut accel_props = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
    let mut props2 = vk::PhysicalDeviceProperties2::default()
        .push_next(&mut indexing_props)
        .push_next(&mut accel_props);
    unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it;
        // `props2` and the `indexing_props` / `accel_props` structs it chains
        // via pNext are stack-local and outlive this call, which writes only
        // into them.
        instance.get_physical_device_properties2(device, &mut props2);
    }
    // 65535 is a u16 ceiling — it dates back to the pre-#992 `R16_UINT`
    // mesh_id encoding where the per-pixel instance index couldn't
    // address more than this. Post-#992 the mesh_id is `R32_UINT` and
    // `MAX_INSTANCES = 0x40000`, but the bindless texture array sized
    // at 65535 still has plenty of headroom for the worst observed
    // cell's unique-texture count (~10K). Bumping this further pays
    // descriptor-set rebuild cost without any observable win, so it
    // stays at the historical value pending a future pass that
    // actually needs more bindless textures.
    const BINDLESS_CEILING: u32 = 65535;
    let reported_limit =
        if indexing_props.max_per_stage_descriptor_update_after_bind_sampled_images > 0 {
            indexing_props.max_per_stage_descriptor_update_after_bind_sampled_images
        } else {
            properties.limits.max_per_stage_descriptor_sampled_images
        };
    let max_bindless_sampled_images = reported_limit.min(BINDLESS_CEILING);

    // AS scratch alignment. Default to 1 (trivial — every address is a
    // multiple of 1) when ray_query is unsupported so the
    // `debug_assert!` at each `scratch_data` site is a no-op on
    // RT-disabled GPUs. When RT IS supported but the driver still
    // reports zero (spec violation, but cheap to handle), we also
    // fall back to 1 — the assert can't catch what the driver lied
    // about, and crashing on init is worse than letting the build
    // run.
    let min_accel_struct_scratch_offset_alignment = if ray_query_supported
        && accel_props.min_acceleration_structure_scratch_offset_alignment > 0
    {
        accel_props.min_acceleration_structure_scratch_offset_alignment
    } else {
        1
    };

    // Find queue families.
    let queue_families = unsafe {
        // SAFETY: `instance` is live and `device` was enumerated from it; the
        // query writes only into the returned Vec of queue-family properties.
        instance.get_physical_device_queue_family_properties(device)
    };

    let mut graphics = None;
    let mut present = None;

    for (i, family) in queue_families.iter().enumerate() {
        let i = i as u32;

        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(i);
        }

        let present_support = unsafe {
            // SAFETY: `surface_loader` wraps the live instance; `device` was
            // enumerated from it, `surface` was created against it, and `i` is a
            // valid queue-family index bounded by `queue_families.len()`. The
            // query writes only into the returned support flag.
            surface_loader
                .get_physical_device_surface_support(device, i, surface)
                .unwrap_or(false)
        };

        if present_support {
            present = Some(i);
        }

        if graphics.is_some() && present.is_some() {
            break;
        }
    }

    match (graphics, present) {
        (Some(g), Some(p)) => Ok(Some((
            QueueFamilyIndices {
                graphics: g,
                present: p,
            },
            DeviceCapabilities {
                ray_query_supported,
                sampler_anisotropy_supported,
                max_sampler_anisotropy,
                multi_draw_indirect_supported,
                fill_mode_non_solid_supported,
                max_bindless_sampled_images,
                min_accel_struct_scratch_offset_alignment,
                // #1194 — TIMESTAMP query support. `timestamp_period`
                // is nanoseconds-per-tick (e.g. 1.0 on NVIDIA);
                // `timestampComputeAndGraphics == true` means the
                // graphics queue's timestamp_valid_bits is non-zero.
                timestamp_period_ns: properties.limits.timestamp_period,
                timestamp_supported: properties.limits.timestamp_compute_and_graphics == vk::TRUE,
                synchronization2_supported,
                host_query_reset_supported,
                memory_budget_supported,
                texture_compression_bc,
            },
        ))),
        _ => Ok(None),
    }
}

/// Creates a logical device with graphics and present queues.
/// Enables RT extensions and features when `caps.ray_query_supported` is true.
pub fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    indices: QueueFamilyIndices,
    caps: &DeviceCapabilities,
) -> Result<(ash::Device, vk::Queue, vk::Queue)> {
    let unique_families: Vec<u32> = if indices.graphics == indices.present {
        vec![indices.graphics]
    } else {
        vec![indices.graphics, indices.present]
    };

    let queue_priorities = [1.0_f32];
    let queue_create_infos: Vec<vk::DeviceQueueCreateInfo> = unique_families
        .iter()
        .map(|&family| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&queue_priorities)
        })
        .collect();

    // Enable only the features we actually need. Anisotropic filtering
    // is gated behind a device-features toggle (see issue #136).
    // Independent blend: required because the alpha-blend and UI pipelines
    // use different blend states per color attachment (HDR blends, G-buffer
    // attachments overwrite). Without this feature the validation layer
    // rejects any pipeline where pAttachments[i] != pAttachments[0].
    let device_features = vk::PhysicalDeviceFeatures::default()
        .sampler_anisotropy(caps.sampler_anisotropy_supported)
        .texture_compression_bc(caps.texture_compression_bc)
        .independent_blend(true)
        // #309 — `vkCmdDrawIndexedIndirect` with drawCount > 1
        // collapses the per-batch `cmd_draw_indexed` loop into one API
        // call per pipeline group. Universally supported on desktop
        // GPUs since Vulkan 1.0 and the fallback path (the
        // pre-#309 per-batch loop) kicks in if the device doesn't
        // expose it.
        .multi_draw_indirect(caps.multi_draw_indirect_supported)
        // #869 — enables `vk::PolygonMode::LINE` so wireframe pipeline
        // variants can be created. Silently downgrades to FILL when
        // the device doesn't expose it (mobile / some compute-only
        // GPUs); Oblivion vanilla ships zero wireframe meshes so the
        // fallback is invisible to gameplay content.
        .fill_mode_non_solid(caps.fill_mode_non_solid_supported)
        // Required for atomicAdd on the ray budget SSBO (binding 11) in
        // the fragment shader. Universally available on desktop Vulkan 1.0
        // GPUs; the RT mipmap system won't compile pipelines without it.
        // VUID-RuntimeSpirv-NonWritable-06340.
        .fragment_stores_and_atomics(true);

    // Build extension list: required + optional RT.
    let mut extensions: Vec<*const i8> = REQUIRED_EXTENSIONS.iter().map(|e| e.as_ptr()).collect();
    if caps.ray_query_supported {
        for ext in RT_EXTENSIONS {
            extensions.push(ext.as_ptr());
        }
        log::info!(
            "Enabling RT extensions: acceleration_structure, ray_query, deferred_host_operations"
        );
    }
    if caps.memory_budget_supported {
        extensions.push(ash::ext::memory_budget::NAME.as_ptr());
        log::info!("Enabling VK_EXT_memory_budget for live VRAM usage queries");
    }

    // Feature chain (pNext): Vulkan 1.2 features.
    // - buffer_device_address: required for RT acceleration structures.
    // - descriptor_indexing features: required for bindless texture arrays.
    //   These are Vulkan 1.2 core (no extension needed), universally available
    //   on desktop GPUs that support Vulkan 1.2+.
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::default()
        .buffer_device_address(caps.ray_query_supported)
        .shader_sampled_image_array_non_uniform_indexing(true)
        .runtime_descriptor_array(true)
        .descriptor_binding_partially_bound(true)
        .descriptor_binding_sampled_image_update_after_bind(true)
        // Host-side `vkResetQueryPool`. Two consumers: the BLAS-compaction
        // path (`blas_static.rs`, RT-only) AND the per-pass GPU timers
        // (`gpu_timers.rs`, #1194 — RT-independent). It was previously
        // gated on `ray_query_supported`, which left the feature disabled
        // on a timestamp-capable but RT-less GPU while the timer path still
        // issued `reset_query_pool` (VUID-vkResetQueryPool-None-02665,
        // #1478 / REN-D23-NEW-01). Enable it on its own probed support so
        // both consumers are correct; `GpuPerFrameTimers::new` gates on the
        // same flag so it self-disables on a device that lacks it.
        .host_query_reset(caps.host_query_reset_supported);

    let mut accel_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
        .acceleration_structure(caps.ray_query_supported);

    let mut ray_query_features =
        vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(caps.ray_query_supported);

    // Vulkan 1.3 core feature chain. `synchronization2` is required
    // (#1437) — `is_device_suitable` already rejected any GPU without
    // it, so this is always `true`. The feature makes
    // `PipelineStageFlags::NONE` a legal stage-mask in sync1
    // `vkCmdPipelineBarrier` calls (bloom, SSAO, caustic, texture,
    // volumetrics all use NONE as the "no prior writes" form since
    // #1160 / #949 / #1100 / #1121 / #1122).
    let mut vulkan13_features =
        vk::PhysicalDeviceVulkan13Features::default().synchronization2(true);

    // Always push Vulkan 1.2 + 1.3 features. RT features are only pushed when available.
    let mut create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&device_features)
        .enabled_extension_names(&extensions)
        .push_next(&mut vulkan12_features)
        .push_next(&mut vulkan13_features);
    if caps.ray_query_supported {
        create_info = create_info
            .push_next(&mut accel_features)
            .push_next(&mut ray_query_features);
    }

    let device = unsafe {
        // SAFETY: `instance` is live and `physical_device` was enumerated from
        // it; `create_info` and everything it borrows — the `queue_create_infos`
        // slice, the `extensions` name-pointer slice, and the pNext feature
        // structs — are stack-local and outlive this call.
        instance
            .create_device(physical_device, &create_info, None)
            .context("Failed to create logical device")?
    };

    // #1759 / TD7-002 — `buffer::aligned_flush_range` rounds non-coherent
    // host-visible flush ranges up to a hardcoded `NON_COHERENT_ATOM_SIZE`
    // (256) instead of the device-reported `nonCoherentAtomSize`. 256 is a
    // conservative upper bound — every known GPU reports <= 256 (typically
    // 64) — so over-aligning is safe. But a device reporting a LARGER atom
    // size would make every flush under-align and silently corrupt the
    // mapped range (VUID-VkMappedMemoryRange-size-01390). Pin the
    // assumption at device-create time so such an exotic device trips here
    // in debug builds rather than producing invisible GPU corruption. The
    // query is `cfg(debug_assertions)`-gated so it costs nothing — and
    // leaves no unused binding — in release. Promote to real
    // `PhysicalDeviceLimits` plumbing if this ever fires.
    #[cfg(debug_assertions)]
    {
        let atom = unsafe {
            // SAFETY: `instance` is live and `physical_device` was enumerated
            // from it; the query writes only into the returned properties struct.
            instance.get_physical_device_properties(physical_device)
        }
        .limits
        .non_coherent_atom_size;
        debug_assert!(
            atom <= super::buffer::NON_COHERENT_ATOM_SIZE,
            "device reports nonCoherentAtomSize={atom} > the {} the flush \
             path aligns to; flushes would under-align. Plumb \
             PhysicalDeviceLimits into buffer::aligned_flush_range. \
             See buffer.rs NON_COHERENT_ATOM_SIZE / #1759.",
            super::buffer::NON_COHERENT_ATOM_SIZE,
        );
    }

    let graphics_queue = unsafe {
        // SAFETY: `device` was just created above with a queue-create-info for
        // `indices.graphics`, so queue index 0 of that family is guaranteed to
        // exist.
        device.get_device_queue(indices.graphics, 0)
    };
    let present_queue = unsafe {
        // SAFETY: `device` was just created above with a queue-create-info for
        // `indices.present`, so queue index 0 of that family is guaranteed to
        // exist.
        device.get_device_queue(indices.present, 0)
    };

    log::info!(
        "Logical device created (graphics queue family: {}, present: {}, sync2: {})",
        indices.graphics,
        indices.present,
        caps.synchronization2_supported,
    );

    Ok((device, graphics_queue, present_queue))
}

#[cfg(test)]
mod caps_tests {
    use super::DeviceCapabilities;

    /// #1636 / #1478 — the GPU-timer gate must require BOTH `timestamp` and
    /// `host_query_reset`, and must NOT depend on ray-query. A regression to
    /// the old RT-coupled gate would re-arm a host `vkResetQueryPool` with the
    /// feature disabled on a timestamp-capable, RT-less GPU
    /// (VUID-vkResetQueryPool-None-02665).
    #[test]
    fn gpu_timers_gate_requires_both_flags_independent_of_rt() {
        let caps = |timestamp: bool, host_reset: bool, rt: bool| DeviceCapabilities {
            timestamp_supported: timestamp,
            host_query_reset_supported: host_reset,
            ray_query_supported: rt,
            ..Default::default()
        };
        // Both present → enabled, regardless of ray-query.
        assert!(caps(true, true, false).gpu_timers_supported());
        assert!(caps(true, true, true).gpu_timers_supported());
        // Either missing → disabled.
        assert!(!caps(true, false, true).gpu_timers_supported());
        assert!(!caps(false, true, true).gpu_timers_supported());
        assert!(!caps(false, false, false).gpu_timers_supported());
    }
}
