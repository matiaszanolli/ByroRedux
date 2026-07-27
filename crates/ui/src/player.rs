//! SwfPlayer — wraps Ruffle's Player for offscreen SWF rendering.
//!
//! Follows the same pattern as Ruffle's own exporter crate: create a wgpu
//! Descriptors bundle, build a TextureTarget for offscreen rendering, then
//! use `capture_frame()` to read back RGBA pixels.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use ruffle_core::external::Value as ExternalValue;
use ruffle_core::limits::ExecutionLimit;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};
use ruffle_render_wgpu::backend::{
    create_wgpu_instance, request_adapter_and_device, WgpuRenderBackend,
};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;

use crate::avm2_host::{inject_host_object_adapter, DESTROY_CALLBACK};
use crate::navigator::{ScaleformNavigator, ScaleformNavigatorRuntime};
use crate::{
    ScaleformHostBridge, ScaleformHostObjectState, ScaleformProfile, ScaleformResourceLoad,
    ScaleformResourceProvider, ScaleformValue, UiInputEvent,
};

const MAX_ARCHIVE_PRELOAD_PASSES: usize = 64;

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
    host_bridge: ScaleformHostBridge,
    host_object_state: ScaleformHostObjectState,
    navigator_runtime: Option<ScaleformNavigatorRuntime>,
    resource_error: Option<String>,
}

impl SwfPlayer {
    /// Create a new SwfPlayer from raw SWF bytes.
    ///
    /// Sets up a headless wgpu device and configures Ruffle for
    /// offscreen rendering at the given dimensions.
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> Result<Self> {
        let profile = ScaleformProfile::detect(swf_data)?;
        let catalog = crate::ScaleformHostCatalog::for_profile(profile);
        let (swf_data, host_object_state) = inject_host_object_adapter(swf_data, catalog)
            .map_err(|error| anyhow!("Failed to prepare Scaleform host object: {error}"))?;
        let movie = SwfMovie::from_data(&swf_data, "file:///menu.swf".to_string(), None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        Self::from_movie(movie, width, height, profile, host_object_state, None)
    }

    /// Create a player with an explicit Bethesda Scaleform profile.
    pub fn new_with_profile(
        swf_data: &[u8],
        width: u32,
        height: u32,
        profile: ScaleformProfile,
    ) -> Result<Self> {
        let detected = ScaleformProfile::detect(swf_data)?;
        if detected != profile {
            return Err(anyhow!(
                "Scaleform profile mismatch: requested {profile:?}, movie requires {detected:?}"
            ));
        }
        let catalog = crate::ScaleformHostCatalog::for_profile(profile);
        let (swf_data, host_object_state) = inject_host_object_adapter(swf_data, catalog)
            .map_err(|error| anyhow!("Failed to prepare Scaleform host object: {error}"))?;
        let movie = SwfMovie::from_data(&swf_data, "file:///menu.swf".to_string(), None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        Self::from_movie(movie, width, height, profile, host_object_state, None)
    }

    /// Load a menu and all of its relative imports through one archive source.
    pub fn from_resource_provider(
        provider: Rc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        width: u32,
        height: u32,
        profile: ScaleformProfile,
    ) -> Result<Self> {
        let swf_data = provider
            .load(movie_path)
            .map_err(|error| anyhow!("Failed to load Scaleform movie {movie_path:?}: {error}"))?
            .ok_or_else(|| anyhow!("Scaleform movie not found in archive: {movie_path:?}"))?;
        let detected = ScaleformProfile::detect(&swf_data)?;
        if detected != profile {
            return Err(anyhow!(
                "Scaleform profile mismatch: requested {profile:?}, movie requires {detected:?}"
            ));
        }
        let catalog = crate::ScaleformHostCatalog::for_profile(profile);
        let (swf_data, host_object_state) = inject_host_object_adapter(&swf_data, catalog)
            .map_err(|error| anyhow!("Failed to prepare Scaleform host object: {error}"))?;
        let (navigator, runtime, movie_url) = ScaleformNavigatorRuntime::create(
            movie_path, &swf_data, provider,
        )
        .map_err(|error| anyhow!("Failed to configure Scaleform archive loading: {error}"))?;
        let movie = SwfMovie::from_data(&swf_data, movie_url, None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        let mut player = Self::from_movie(
            movie,
            width,
            height,
            profile,
            host_object_state,
            Some((navigator, runtime)),
        )?;
        if !player.drive_archive_preload()? {
            return Err(anyhow!(
                "Scaleform archive preload did not settle after {MAX_ARCHIVE_PRELOAD_PASSES} passes"
            ));
        }
        Ok(player)
    }

    fn from_movie(
        movie: SwfMovie,
        width: u32,
        height: u32,
        profile: ScaleformProfile,
        host_object_state: ScaleformHostObjectState,
        navigator: Option<(ScaleformNavigator, ScaleformNavigatorRuntime)>,
    ) -> Result<Self> {
        let host_bridge = ScaleformHostBridge::new(profile);

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
        let mut player_builder = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_video(ruffle_video_software::backend::SoftwareVideoBackend::new())
            .with_external_interface(host_bridge.provider())
            .with_movie(movie)
            .with_load_behavior(LoadBehavior::Blocking)
            .with_viewport_dimensions(width, height, 1.0);
        let navigator_runtime = if let Some((navigator, runtime)) = navigator {
            player_builder = player_builder.with_navigator(navigator);
            Some(runtime)
        } else {
            None
        };
        let player = player_builder.build();

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
            host_bridge,
            host_object_state,
            navigator_runtime,
            resource_error: None,
        })
    }

    /// Advance the player by `dt` seconds. Ruffle handles frame accumulation
    /// internally — just call tick() each frame with the real delta time.
    pub fn tick(&mut self, dt: f64) {
        if self.resource_error.is_some() {
            return;
        }
        if self.navigator_runtime.is_some() {
            match self.drive_archive_preload() {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => {
                    let error = error.to_string();
                    log::error!("{error}");
                    self.resource_error = Some(error);
                    return;
                }
            }
        }
        {
            let mut player = self.player.lock().unwrap();
            player.tick(FloatDuration::from_secs(dt));
        }
        if let Some(runtime) = &mut self.navigator_runtime {
            runtime.run_until_stalled();
            if let Some(error) = runtime.first_error() {
                log::error!("{error}");
                self.resource_error = Some(error);
            }
        }
        self.dirty = true;
    }

    /// Forward a platform-neutral input event to Ruffle.
    ///
    /// The return value is Ruffle's per-event handled result. UiManager uses
    /// focus ownership, rather than this value, to decide modal capture.
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool {
        let handled = self.player.lock().unwrap().handle_event(event.into());
        self.dirty = true;
        handled
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

    pub fn profile(&self) -> ScaleformProfile {
        self.host_bridge.profile()
    }

    pub fn host_bridge(&self) -> ScaleformHostBridge {
        self.host_bridge.clone()
    }

    pub fn host_object_state(&self) -> ScaleformHostObjectState {
        self.host_object_state
    }

    /// Current root timeline frame, if playback has started.
    pub fn current_frame(&self) -> Option<u16> {
        self.player.lock().unwrap().current_frame()
    }

    /// Successful relative resources loaded through the archive navigator.
    pub fn resource_loads(&self) -> Vec<ScaleformResourceLoad> {
        self.navigator_runtime
            .as_ref()
            .map(ScaleformNavigatorRuntime::loads)
            .unwrap_or_default()
    }

    /// First archive loading failure encountered after construction.
    pub fn resource_error(&self) -> Option<&str> {
        self.resource_error.as_deref()
    }

    /// Invoke a callback registered through `ExternalInterface.addCallback`.
    ///
    /// `None` distinguishes an unknown callback from a registered callback
    /// whose valid return value is `ScaleformValue::Null`.
    pub fn invoke_callback(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ScaleformValue>,
    ) -> Option<ScaleformValue> {
        if !self.host_bridge.has_callback(name) {
            return None;
        }

        let arguments = arguments.into_iter().map(ExternalValue::from);
        let result = self
            .player
            .lock()
            .unwrap()
            .call_internal_interface(name, arguments);
        self.dirty = true;
        Some(ScaleformValue::from(&result))
    }

    fn drive_archive_preload(&mut self) -> Result<bool> {
        for _ in 0..MAX_ARCHIVE_PRELOAD_PASSES {
            let finished = {
                let mut execution_limit = ExecutionLimit::none();
                self.player.lock().unwrap().preload(&mut execution_limit)
            };
            let runtime = self
                .navigator_runtime
                .as_mut()
                .expect("archive preload requires a navigator runtime");
            runtime.run_until_stalled();
            if let Some(error) = runtime.first_error() {
                return Err(anyhow!("Scaleform archive preload failed: {error}"));
            }
            if finished {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Drop for SwfPlayer {
    fn drop(&mut self) {
        if self.host_bridge.has_callback(DESTROY_CALLBACK) {
            if let Ok(mut player) = self.player.lock() {
                let _ = player.call_internal_interface(DESTROY_CALLBACK, []);
            }
        }
    }
}
