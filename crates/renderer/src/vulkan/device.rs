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
#[derive(Debug, Clone, Copy)]
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
    /// `maxPerStageDescriptorUpdateAfterBindSampledImages` from
    /// `VkPhysicalDeviceDescriptorIndexingProperties` (Vulkan 1.2 core),
    /// already clamped to a sane ceiling. The `TextureRegistry` sizes its
    /// bindless array with this so we don't silently truncate on cells with
    /// many unique textures. Fixes the REN-MEM-C3 residency cliff (#425).
    pub max_bindless_sampled_images: u32,
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
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
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
            let props = unsafe { instance.get_physical_device_properties(device) };
            // SAFETY: device_name is a fixed-size [c_char; 256] array null-terminated by the
            // Vulkan driver. The pointer is valid for the lifetime of `props` (stack-local).
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
            log::info!(
                "Selected GPU: {:?} (ray query: {})",
                name,
                caps.ray_query_supported
            );
            return Ok((device, indices, caps));
        }
    }

    anyhow::bail!("No suitable GPU found (need graphics + present queues and swapchain support)")
}

fn is_device_suitable(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> Result<Option<(QueueFamilyIndices, DeviceCapabilities)>> {
    let available_extensions = unsafe {
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

    // Query features + limits for optional features we care about.
    // `samplerAnisotropy` is the only one right now (issue #136).
    let features = unsafe { instance.get_physical_device_features(device) };
    let properties = unsafe { instance.get_physical_device_properties(device) };
    let sampler_anisotropy_supported = features.sampler_anisotropy == vk::TRUE;
    // Cap at 16× — the point of diminishing returns on every modern
    // GPU, and the common "16x AF" preset players expect.
    let max_sampler_anisotropy = if sampler_anisotropy_supported {
        properties.limits.max_sampler_anisotropy.min(16.0)
    } else {
        0.0
    };
    let multi_draw_indirect_supported = features.multi_draw_indirect == vk::TRUE;

    // Query descriptor indexing properties for the UPDATE_AFTER_BIND bindless
    // array ceiling. Vulkan 1.2 core exposes this via the pNext chain on
    // `vkGetPhysicalDeviceProperties2`. Falls back to the plain
    // `maxPerStageDescriptorSampledImages` limit on older drivers — still
    // vastly larger than the hardcoded 1024 we were using before (#425).
    let mut indexing_props = vk::PhysicalDeviceDescriptorIndexingProperties::default();
    let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut indexing_props);
    unsafe {
        instance.get_physical_device_properties2(device, &mut props2);
    }
    // 65535 is the hard ceiling imposed by `R16_UINT` mesh_id encoding in the
    // G-buffer (#275/#318) — no point sizing the bindless array larger than
    // the maximum instance count can reference.
    const BINDLESS_CEILING: u32 = 65535;
    let reported_limit = if indexing_props.max_per_stage_descriptor_update_after_bind_sampled_images
        > 0
    {
        indexing_props.max_per_stage_descriptor_update_after_bind_sampled_images
    } else {
        properties.limits.max_per_stage_descriptor_sampled_images
    };
    let max_bindless_sampled_images = reported_limit.min(BINDLESS_CEILING);

    // Find queue families.
    let queue_families = unsafe { instance.get_physical_device_queue_family_properties(device) };

    let mut graphics = None;
    let mut present = None;

    for (i, family) in queue_families.iter().enumerate() {
        let i = i as u32;

        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(i);
        }

        let present_support = unsafe {
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
                max_bindless_sampled_images,
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
        .independent_blend(true)
        // #309 — `vkCmdDrawIndexedIndirect` with drawCount > 1
        // collapses the per-batch `cmd_draw_indexed` loop into one API
        // call per pipeline group. Universally supported on desktop
        // GPUs since Vulkan 1.0 and the fallback path (the
        // pre-#309 per-batch loop) kicks in if the device doesn't
        // expose it.
        .multi_draw_indirect(caps.multi_draw_indirect_supported);

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
        // Host-side `vkResetQueryPool` is required by the BLAS
        // compaction path in `acceleration.rs` — reset happens on the
        // CPU before the command buffer records `vkCmdWriteAccelerationStructuresPropertiesKHR`.
        // Only the RT pipeline uses this; gating on `ray_query_supported`
        // keeps the non-RT fallback from requesting an unused feature.
        .host_query_reset(caps.ray_query_supported);

    let mut accel_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
        .acceleration_structure(caps.ray_query_supported);

    let mut ray_query_features =
        vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(caps.ray_query_supported);

    // Always push Vulkan 1.2 features (descriptor indexing is needed for
    // bindless textures even without RT). RT features are only pushed when available.
    let create_info = if caps.ray_query_supported {
        vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_features(&device_features)
            .enabled_extension_names(&extensions)
            .push_next(&mut vulkan12_features)
            .push_next(&mut accel_features)
            .push_next(&mut ray_query_features)
    } else {
        vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_features(&device_features)
            .enabled_extension_names(&extensions)
            .push_next(&mut vulkan12_features)
    };

    let device = unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .context("Failed to create logical device")?
    };

    let graphics_queue = unsafe { device.get_device_queue(indices.graphics, 0) };
    let present_queue = unsafe { device.get_device_queue(indices.present, 0) };

    log::info!(
        "Logical device created (graphics queue family: {}, present: {})",
        indices.graphics,
        indices.present
    );

    Ok((device, graphics_queue, present_queue))
}
