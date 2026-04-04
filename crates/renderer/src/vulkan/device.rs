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

    let device_features = vk::PhysicalDeviceFeatures::default();

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

    // Feature chain for RT (pNext).
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::default()
        .buffer_device_address(caps.ray_query_supported);

    let mut accel_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
        .acceleration_structure(caps.ray_query_supported);

    let mut ray_query_features =
        vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(caps.ray_query_supported);

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
