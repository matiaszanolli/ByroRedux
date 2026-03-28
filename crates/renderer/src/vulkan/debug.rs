//! Vulkan debug messenger setup.

use anyhow::{Context, Result};
use ash::vk;
use std::ffi::CStr;

/// Sets up the VK_EXT_debug_utils messenger that routes Vulkan validation
/// messages through the `log` crate.
pub fn create_debug_messenger(
    instance: &ash::Instance,
    entry: &ash::Entry,
) -> Result<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    let debug_utils = ash::ext::debug_utils::Instance::new(entry, instance);

    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback));

    let messenger = unsafe {
        debug_utils
            .create_debug_utils_messenger(&create_info, None)
            .context("Failed to create debug utils messenger")?
    };

    log::info!("Vulkan debug messenger installed");
    Ok((debug_utils, messenger))
}

/// Callback invoked by the validation layers.
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _msg_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if callback_data.is_null() {
        return vk::FALSE;
    }

    let msg = unsafe {
        let data = &*callback_data;
        if data.p_message.is_null() {
            "(no message)"
        } else {
            CStr::from_ptr(data.p_message).to_str().unwrap_or("(utf8 error)")
        }
    };

    match severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::error!("[Vulkan] {}", msg),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::warn!("[Vulkan] {}", msg),
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => log::info!("[Vulkan] {}", msg),
        _ => log::trace!("[Vulkan] {}", msg),
    }

    vk::FALSE
}
