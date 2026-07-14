//! Vulkan instance creation with validation layers.

use anyhow::{Context, Result};
use ash::vk;
use raw_window_handle::RawDisplayHandle;
use std::ffi::{CStr, CString};

/// Required validation layers.
const VALIDATION_LAYERS: &[&CStr] = &[c"VK_LAYER_KHRONOS_validation"];

/// Whether to enable the validation layer + debug messenger. Always on in
/// debug builds; opt-in in **release** via `BYRO_VALIDATION=<v>` so
/// streaming / device-loss hazards can be caught at release speed (a debug
/// build is often too slow to stream into the dense cells that fault).
/// Messages route to the Rust `log` through the debug messenger
/// (`debug::create_debug_messenger`), so no `VK_INSTANCE_LAYERS` /
/// `VK_LAYER_ENABLES` env juggling is needed — and the chosen features are
/// logged at startup so it's unambiguous whether validation is live.
pub fn validation_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("BYRO_VALIDATION").is_some()
}

/// `BYRO_VALIDATION=gpuav` (or any value containing "gpu") additionally
/// turns on GPU-Assisted Validation, which catches GPU-side faults that
/// synchronization validation cannot — shader out-of-bounds buffer access
/// and invalid device addresses (e.g. an RT ray query against a freed
/// BLAS/TLAS). Heavier; opt-in on top of the default sync-validation.
fn gpu_assisted_requested() -> bool {
    std::env::var("BYRO_VALIDATION")
        .map(|v| v.to_ascii_lowercase().contains("gpu"))
        .unwrap_or(false)
}

/// Creates a Vulkan instance with the appropriate extensions for the given
/// display handle, and validation layers in debug builds.
pub fn create_instance(
    entry: &ash::Entry,
    display_handle: RawDisplayHandle,
) -> Result<ash::Instance> {
    let app_name = CString::new("ByroRedux")?;
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

    let validation = validation_enabled();

    // Debug utils extension for validation layer callbacks.
    if validation {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
    }

    let mut layer_names_raw: Vec<*const i8> = Vec::new();
    if validation {
        check_validation_layer_support(entry)?;
        layer_names_raw = VALIDATION_LAYERS.iter().map(|l| l.as_ptr()).collect();
    }

    // Enable Synchronization Validation (always, when validation is on) and
    // GPU-Assisted Validation (opt-in via `BYRO_VALIDATION=gpuav`) through the
    // instance pNext chain — app-side, so the user doesn't need
    // `VK_LAYER_ENABLES` and the choice is explicit + logged. `enabled` must
    // outlive `create_instance` (it does — same scope).
    let mut enabled_features = vec![vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
    if gpu_assisted_requested() {
        enabled_features.push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED);
        enabled_features.push(vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT);
    }
    let mut validation_features =
        vk::ValidationFeaturesEXT::default().enabled_validation_features(&enabled_features);

    let mut create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layer_names_raw);
    if validation {
        create_info = create_info.push_next(&mut validation_features);
    }

    let instance = unsafe {
        // SAFETY: `entry` is the live Vulkan entry point; `create_info` and
        // everything it borrows — the `extensions` and `layer_names_raw`
        // name-pointer slices and the pushed `validation_features` — are
        // stack-local and outlive this call.
        entry
            .create_instance(&create_info, None)
            .context("Failed to create Vulkan instance")?
    };

    if validation {
        log::warn!(
            "Vulkan VALIDATION ENABLED — sync-validation{} (BYRO_VALIDATION / debug build). \
             Hazards route to this log as `vulkan::debug` errors.",
            if gpu_assisted_requested() {
                " + GPU-Assisted (shader OOB / bad device address)"
            } else {
                " (set BYRO_VALIDATION=gpuav to add GPU-Assisted)"
            },
        );
    }
    log::info!("Vulkan instance created (API 1.3)");
    Ok(instance)
}

fn check_validation_layer_support(entry: &ash::Entry) -> Result<()> {
    let available = unsafe {
        // SAFETY: `entry` is the live Vulkan entry point; enumeration writes
        // only into the returned Vec of layer properties.
        entry
            .enumerate_instance_layer_properties()
            .context("Failed to enumerate instance layer properties")?
    };

    for required in VALIDATION_LAYERS {
        let found = available.iter().any(|layer| {
            // SAFETY: VkLayerProperties::layerName is a null-terminated [c_char; 256]
            // array per the Vulkan spec. The pointer is valid for the lifetime of
            // `layer` (borrowed from the `available` Vec for this iteration).
            let name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
            name == *required
        });
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
