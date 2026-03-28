//! Platform window abstraction built on winit.

use anyhow::Result;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// Configuration for creating a platform window.
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Gamebyro Redux".into(),
            width: 1280,
            height: 720,
        }
    }
}

/// Creates a winit window from the given event loop and config.
pub fn create_window(event_loop: &ActiveEventLoop, config: &WindowConfig) -> Result<Window> {
    let attrs = WindowAttributes::default()
        .with_title(&config.title)
        .with_inner_size(LogicalSize::new(config.width, config.height));

    let window = event_loop.create_window(attrs)?;
    log::info!(
        "Window created: {}x{} \"{}\"",
        config.width,
        config.height,
        config.title
    );
    Ok(window)
}

/// Returns the raw display and window handles needed by Vulkan surface creation.
pub fn raw_handles(
    window: &Window,
) -> Result<(
    raw_window_handle::RawDisplayHandle,
    raw_window_handle::RawWindowHandle,
)> {
    let display = window.display_handle()?.as_raw();
    let window_handle = window.window_handle()?.as_raw();
    Ok((display, window_handle))
}
