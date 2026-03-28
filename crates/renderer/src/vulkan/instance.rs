//! Vulkan instance creation with validation layers.

use anyhow::{Context, Result};
use ash::vk;
use raw_window_handle::RawDisplayHandle;
use std::ffi::{CStr, CString};

/// Required validation layers (debug builds only).
const VALIDATION_LAYERS: &[&CStr] = &[
    // SAFETY: null-terminated literal
    unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") },
];

/// Creates a Vulkan instance with the appropriate extensions for the given
/// display handle, and validation layers in debug builds.
pub fn create_instance(
    entry: &ash::Entry,
    display_handle: RawDisplayHandle,
) -> Result<ash::Instance> {
    let app_name = CString::new("Gamebyro Redux")?;
    let engine_name = CString::new("Gamebyro")?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_3);

    // Surface extensions required by the platform's display handle.
    let mut extensions = ash_window::enumerate_required_extensions(display_handle)
        .context("Failed to enumerate required surface extensions")?
        .to_vec();

    // Debug utils extension for validation layer callbacks.
    if cfg!(debug_assertions) {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
    }

    let mut layer_names_raw: Vec<*const i8> = Vec::new();
    if cfg!(debug_assertions) {
        check_validation_layer_support(entry)?;
        layer_names_raw = VALIDATION_LAYERS.iter().map(|l| l.as_ptr()).collect();
    }

    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layer_names_raw);

    let instance = unsafe {
        entry
            .create_instance(&create_info, None)
            .context("Failed to create Vulkan instance")?
    };

    log::info!("Vulkan instance created (API 1.3)");
    Ok(instance)
}

fn check_validation_layer_support(entry: &ash::Entry) -> Result<()> {
    let available = unsafe {
        entry
            .enumerate_instance_layer_properties()
            .context("Failed to enumerate instance layer properties")?
    };

    for required in VALIDATION_LAYERS {
        let found = available
            .iter()
            .any(|layer| unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) } == *required);
        if !found {
            anyhow::bail!(
                "Required validation layer {:?} not available. \
                 Install the Vulkan SDK / vulkan-validationlayers package.",
                required
            );
        }
    }
    Ok(())
}
