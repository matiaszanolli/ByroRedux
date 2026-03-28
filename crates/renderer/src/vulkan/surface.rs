//! Vulkan surface creation from raw window handles.

use anyhow::{Context, Result};
use ash::vk;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

pub fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    display_handle: RawDisplayHandle,
    window_handle: RawWindowHandle,
) -> Result<vk::SurfaceKHR> {
    let surface = unsafe {
        ash_window::create_surface(entry, instance, display_handle, window_handle, None)
            .context("Failed to create Vulkan surface")?
    };
    log::info!("Vulkan surface created");
    Ok(surface)
}
