//! SwfPlayer — wraps Ruffle's Player for offscreen SWF rendering.
//!
//! Follows the same pattern as Ruffle's own exporter crate: create a wgpu
//! Descriptors bundle, build a TextureTarget for offscreen rendering, then
//! use `capture_frame()` to read back RGBA pixels.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::sync::{Arc, Mutex};

use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, Player, PlayerBuilder};
use ruffle_render_wgpu::backend::{
    create_wgpu_instance, request_adapter_and_device, WgpuRenderBackend,
};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;

/// Wraps a Ruffle Flash player instance with offscreen wgpu rendering.
///
/// Creates its own wgpu device (separate from the main Vulkan renderer)
/// and renders SWF content to an RGBA pixel buffer each frame.
pub struct SwfPlayer {
    player: Arc<Mutex<Player>>,
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,
    dirty: bool,
}

impl SwfPlayer {
    /// Create a new SwfPlayer from raw SWF bytes.
    ///
    /// Sets up a headless wgpu device and configures Ruffle for
    /// offscreen rendering at the given dimensions.
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> Result<Self> {
        // Parse the SWF data.
        let movie = SwfMovie::from_data(swf_data, "file:///menu.swf".to_string(), None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;

        // Create wgpu instance and device (headless, no surface).
        let instance =
            create_wgpu_instance(wgpu::Backends::VULKAN, wgpu::BackendOptions::default());
        let (adapter, device, queue) = futures::executor::block_on(request_adapter_and_device(
            wgpu::Backends::VULKAN,
            &instance,
            None,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(|e| anyhow!("Failed to create wgpu device: {e}"))?;

        let descriptors = Arc::new(Descriptors::new(instance, adapter, device, queue));

        // Create offscreen render target.
        let target = TextureTarget::new(&descriptors.device, (width, height))
            .map_err(|e| anyhow!("Failed to create texture target: {e}"))?;

        // Create the Ruffle wgpu render backend.
        let renderer = WgpuRenderBackend::new(descriptors, target)
            .map_err(|e| anyhow!("Failed to create render backend: {e}"))?;

        // Build the Ruffle player with the parsed movie.
        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_video(ruffle_video_software::backend::SoftwareVideoBackend::new())
            .with_movie(movie)
            .with_viewport_dimensions(width, height, 1.0)
            .build();

        // Start playback.
        player.lock().unwrap().set_is_playing(true);

        let pixel_buffer = vec![0u8; (width * height * 4) as usize];

        log::info!(
            "Ruffle player created ({}x{}, wgpu/Vulkan offscreen)",
            width,
            height
        );

        Ok(Self {
            player,
            width,
            height,
            pixel_buffer,
            dirty: true,
        })
    }

    /// Advance the player by `dt` seconds. Ruffle handles frame accumulation
    /// internally — just call tick() each frame with the real delta time.
    pub fn tick(&mut self, dt: f64) {
        let mut player = self.player.lock().unwrap();
        player.tick(FloatDuration::from_secs(dt));
        self.dirty = true;
    }

    /// Render the current frame to the internal pixel buffer.
    /// Returns the RGBA pixel data if the frame is dirty, None otherwise.
    pub fn render(&mut self) -> Option<&[u8]> {
        if !self.dirty {
            return None;
        }

        // Render the frame (submits draw commands to the wgpu backend).
        {
            let mut player = self.player.lock().unwrap();
            player.render();
        }

        // Capture the rendered frame by downcasting to the concrete backend type.
        // This follows the same pattern as Ruffle's exporter crate.
        {
            let mut player = self.player.lock().unwrap();
            let renderer = player.renderer_mut();
            if let Some(wgpu_backend) =
                <dyn Any>::downcast_mut::<WgpuRenderBackend<TextureTarget>>(renderer)
            {
                if let Some(image) = wgpu_backend.capture_frame() {
                    let rgba = image.into_raw();
                    if rgba.len() == self.pixel_buffer.len() {
                        self.pixel_buffer.copy_from_slice(&rgba);
                    } else {
                        log::warn!(
                            "Ruffle frame size mismatch: got {} bytes, expected {}",
                            rgba.len(),
                            self.pixel_buffer.len()
                        );
                    }
                }
            }
        }

        self.dirty = false;
        Some(&self.pixel_buffer)
    }

    /// Get the viewport dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
