//! SwfPlayer — wraps Ruffle's Player for offscreen SWF rendering.
//!
//! Follows the same pattern as Ruffle's own exporter crate: create a wgpu
//! Descriptors bundle, build a TextureTarget for offscreen rendering, then
//! use `capture_frame()` to read back RGBA pixels.

use anyhow::{anyhow, Result};
use std::any::Any;
use std::sync::{Arc, Mutex, OnceLock};

use ruffle_core::external::Value as ExternalValue;
use ruffle_core::limits::ExecutionLimit;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};
use ruffle_render_wgpu::backend::{
    create_wgpu_instance, request_adapter_and_device, WgpuRenderBackend,
};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::target::TextureTarget;

use crate::avm2_host::DESTROY_CALLBACK;
use crate::navigator::{ScaleformNavigator, ScaleformNavigatorRuntime};
use crate::prepare::prepare_movie;
use crate::{
    ScaleformHostBridge, ScaleformHostObjectState, ScaleformProfile, ScaleformResourceLoad,
    ScaleformResourceProvider, ScaleformValue, UiInputEvent,
};

const MAX_ARCHIVE_PRELOAD_PASSES: usize = 64;

/// Consecutive frames an unsettled archive preload may suppress the movie's
/// own `tick` before the player advances it anyway (#2720).
///
/// The pre-fix code returned from `tick` on every unsettled frame, forever,
/// with no log line, no recorded error and no state change — a preload that
/// never settles was indistinguishable from a hang, and the identical
/// condition is a hard `Err` at construction. One second of grace at 60 Hz is
/// far more than the handful of frames a real cross-archive import needs, and
/// after it a degraded menu (some imported symbols missing) beats a frozen
/// one: the stall is logged once and surfaced through
/// [`SwfPlayer::preload_stalled`].
const MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES: u32 = 60;

/// Whether an unsettled preload has held the movie back long enough that
/// `tick` should advance it anyway (#2720).
///
/// Split out (pure) so the default suite can pin the property that actually
/// matters — that the wait *terminates* — without having to synthesize a
/// preload that never settles.
fn preload_stall_grace_expired(consecutive_stall_frames: u32) -> bool {
    consecutive_stall_frames >= MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES
}

/// Distinct archive-fetch failures a player retains (#2720).
///
/// Failures are deduplicated, so this only binds a movie that keeps asking
/// for *different* missing files every frame. It is a leak stop, not a
/// capacity estimate — a real menu records zero or one.
const MAX_RECORDED_RESOURCE_ERRORS: usize = 64;

/// Distinct archive paths a player retains fetch records for (#2967).
///
/// Deduplicated by `archive_path` with a hit counter (see
/// [`SwfPlayer::record_resource_loads`]), so this only binds a menu that
/// fetches endlessly many *different* paths — a menu polling the same
/// resource every frame, the realistic shape `fetch` sees, costs one entry
/// no matter how many times it repeats.
const MAX_RECORDED_RESOURCE_LOADS: usize = 64;

/// Process-wide Ruffle GPU bundle, created on first menu load (#2733).
///
/// Pre-fix every [`SwfPlayer`] built its own wgpu instance, adapter, device
/// and queue, synchronously via `block_on` on the winit main-loop thread.
/// That meant a second live Vulkan device per menu — and because
/// `UiManager::install_player` only assigns `self.player` once the new
/// player is fully built, a menu swap transiently held *two* Ruffle devices
/// alongside the engine's, with a visible hitch on every menu load.
///
/// Sharing is the model Ruffle itself uses: `WgpuRenderBackend::new` takes
/// an `Arc<Descriptors>` precisely so several players can render against one
/// device, each with its own `TextureTarget`. Nothing here is per-player.
///
/// **What this trades.** The device now outlives any single player and
/// lives for the process, instead of being released with each one. That
/// is the point — a menu can reopen at any time, and paying the device-
/// creation hitch once beats paying it per open — but it does mean one idle
/// logical device is retained after the last menu's player is dropped.
///
/// A failed creation is deliberately **not** cached, so a transient failure
/// doesn't permanently disable the UI. If two threads race the very first
/// call, the loser drops its device and adopts the winner's; menus are built
/// on the main thread, so this is a correctness guard rather than a live
/// path.
fn shared_descriptors() -> Result<Arc<Descriptors>> {
    static SHARED: OnceLock<Arc<Descriptors>> = OnceLock::new();

    get_or_try_init(&SHARED, || {
        let instance =
            create_wgpu_instance(wgpu::Backends::VULKAN, wgpu::BackendOptions::default());
        let (adapter, device, queue) = futures::executor::block_on(request_adapter_and_device(
            wgpu::Backends::VULKAN,
            &instance,
            None,
            wgpu::PowerPreference::HighPerformance,
        ))
        .map_err(|e| anyhow!("Failed to create wgpu device: {e}"))?;
        log::info!("Scaleform UI: created the shared Ruffle wgpu device (once per process, #2733)");
        Ok(Arc::new(Descriptors::new(instance, adapter, device, queue)))
    })
}

/// The caching rule [`shared_descriptors`] implements, extracted so it can be
/// exercised without a GPU (#2733).
///
/// First success populates the slot and every later call returns that same
/// `Arc`; a failure leaves the slot empty so the next call retries. Keeping
/// this generic is what lets the default test suite pin the contract on a
/// machine with no Vulkan device, where `shared_descriptors` itself cannot
/// run at all.
fn get_or_try_init<T>(
    slot: &OnceLock<Arc<T>>,
    make: impl FnOnce() -> Result<Arc<T>>,
) -> Result<Arc<T>> {
    if let Some(existing) = slot.get() {
        return Ok(Arc::clone(existing));
    }
    let created = make()?;
    Ok(Arc::clone(slot.get_or_init(|| created)))
}

/// Wraps a Ruffle Flash player instance with offscreen wgpu rendering.
///
/// Renders SWF content to an RGBA pixel buffer each frame through a wgpu
/// device that is separate from the engine's `VulkanContext` but **shared by
/// every player in the process** — see [`shared_descriptors`] (#2733).
pub struct SwfPlayer {
    player: Arc<Mutex<Player>>,
    width: u32,
    height: u32,
    pixel_buffer: Vec<u8>,
    dirty: bool,
    /// Whether [`Self::render`] has handed its buffer to the caller at least
    /// once, so the content comparison in #2719 can't suppress the very first
    /// upload (see there).
    uploaded_once: bool,
    host_bridge: ScaleformHostBridge,
    host_object_state: ScaleformHostObjectState,
    navigator_runtime: Option<ScaleformNavigatorRuntime>,
    /// Distinct archive-fetch failures seen so far, oldest first (#2720).
    /// Recorded and reported, never latched — see [`Self::tick`].
    resource_errors: Vec<String>,
    /// One-shot latch for the [`MAX_RECORDED_RESOURCE_ERRORS`] cap warning.
    resource_errors_capped: bool,
    /// Distinct archive resources fetched so far, deduplicated by
    /// `archive_path` with a hit counter (#2967). Recorded and reported,
    /// never cleared except by menu replacement — see [`Self::tick`].
    resource_loads: Vec<ScaleformResourceLoad>,
    /// One-shot latch for the [`MAX_RECORDED_RESOURCE_LOADS`] cap warning.
    resource_loads_capped: bool,
    /// Consecutive frames the archive preload has failed to settle (#2720).
    preload_stall_frames: u32,
    /// Whether the stall grace period has been exhausted and the movie is
    /// being advanced without a settled preload (#2720).
    preload_stalled: bool,
}

impl SwfPlayer {
    /// Create a new SwfPlayer from raw SWF bytes.
    ///
    /// Sets up a headless wgpu device and configures Ruffle for
    /// offscreen rendering at the given dimensions.
    pub fn new(swf_data: &[u8], width: u32, height: u32) -> Result<Self> {
        // #2968 — one decode drives detection, injection and Ruffle's parse.
        let prepared = prepare_movie(swf_data, None, None)
            .map_err(|error| anyhow!("Failed to prepare Scaleform movie: {error}"))?;
        let movie = SwfMovie::from_data(&prepared.data, "file:///menu.swf".to_string(), None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        Self::from_movie(
            movie,
            width,
            height,
            prepared.profile,
            prepared.host_object_state,
            None,
        )
    }

    /// Create a player with an explicit Bethesda Scaleform profile.
    pub fn new_with_profile(
        swf_data: &[u8],
        width: u32,
        height: u32,
        profile: ScaleformProfile,
    ) -> Result<Self> {
        // #2968 — the profile mismatch is still raised before any injection
        // work, now off the same decode that answers it.
        let prepared =
            prepare_movie(swf_data, Some(profile), None).map_err(|error| anyhow!("{error}"))?;
        let movie = SwfMovie::from_data(&prepared.data, "file:///menu.swf".to_string(), None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        Self::from_movie(
            movie,
            width,
            height,
            profile,
            prepared.host_object_state,
            None,
        )
    }

    /// Load a menu and all of its relative imports through one archive source.
    pub fn from_resource_provider(
        provider: Arc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        width: u32,
        height: u32,
        profile: ScaleformProfile,
    ) -> Result<Self> {
        let swf_data = provider
            .load(movie_path)
            .map_err(|error| anyhow!("Failed to load Scaleform movie {movie_path:?}: {error}"))?
            .ok_or_else(|| anyhow!("Scaleform movie not found in archive: {movie_path:?}"))?;
        // #2968 — the archive route used to decompress this movie four times
        // and tag-walk it twice before frame 1 (detect, inject, ImportAssets
        // scan, then Ruffle's own parse). One prepare drives the first three.
        let movie_url = crate::navigator::archive_movie_url(movie_path)
            .map_err(|error| anyhow!("Failed to configure Scaleform archive loading: {error}"))?;
        let prepared = prepare_movie(&swf_data, Some(profile), Some(&movie_url))
            .map_err(|error| anyhow!("{error}"))?;
        let (navigator, runtime, movie_url) =
            ScaleformNavigatorRuntime::create(movie_url, prepared.import_asset_paths, provider)
                .map_err(|error| {
                    anyhow!("Failed to configure Scaleform archive loading: {error}")
                })?;
        let movie = SwfMovie::from_data(&prepared.data, movie_url, None)
            .map_err(|e| anyhow!("Failed to parse SWF: {e}"))?;
        let mut player = Self::from_movie(
            movie,
            width,
            height,
            profile,
            prepared.host_object_state,
            Some((navigator, runtime)),
        )?;
        // #2720 — a dependency that fails to fetch is recorded, not fatal: the
        // root movie loaded, and a menu missing an imported font is worth more
        // than no menu.
        //
        // The two failure modes are the same event in Ruffle, which is why
        // this is one check rather than two. `MovieClip::preload` returns
        // `false` forever while `awaiting_import` is set, and
        // `LoadManager::load_asset_movie` only calls `finish_importing()` on
        // its success path — so a failed `ImportAssets` fetch *is* a preload
        // that never settles. Refusing to load in that case would just move
        // the pre-fix freeze from tick time to load time.
        //
        // A stall we cannot explain is still a hard error: nothing was
        // reported, so there is no diagnosis to degrade gracefully under.
        if !player.drive_archive_preload() {
            if player.resource_errors.is_empty() {
                return Err(anyhow!(
                    "Scaleform archive preload did not settle after \
                     {MAX_ARCHIVE_PRELOAD_PASSES} passes"
                ));
            }
            log::warn!(
                "Scaleform archive preload for {movie_path:?} did not settle after \
                 {MAX_ARCHIVE_PRELOAD_PASSES} passes; loading the menu anyway with \
                 {} failed dependency fetch(es) (#2720)",
                player.resource_errors.len(),
            );
            player.preload_stalled = true;
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

        // Headless wgpu device, created once per process (#2733) rather
        // than once per menu.
        let descriptors = shared_descriptors()?;

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
            uploaded_once: false,
            host_bridge,
            host_object_state,
            navigator_runtime,
            resource_errors: Vec::new(),
            resource_errors_capped: false,
            resource_loads: Vec::new(),
            resource_loads_capped: false,
            preload_stall_frames: 0,
            preload_stalled: false,
        })
    }

    /// Advance the player by `dt` seconds. Ruffle handles frame accumulation
    /// internally — just call tick() each frame with the real delta time.
    ///
    /// #2720 — neither an archive failure nor an unsettled preload latches the
    /// movie off any more. A failed *dependency* fetch is recorded and playback
    /// continues (see [`Self::resource_errors`]); only a failure of the root
    /// movie, which happens at construction, is fatal. An unsettled preload
    /// still suppresses the tick, but for a bounded number of frames and with a
    /// log line and an observable state, instead of silently forever.
    pub fn tick(&mut self, dt: f64) {
        if self.navigator_runtime.is_some() && !self.drive_archive_preload() {
            self.preload_stall_frames = self.preload_stall_frames.saturating_add(1);
            if !self.preload_stalled {
                if self.preload_stall_frames == 1 {
                    log::warn!(
                        "Scaleform archive preload did not settle after \
                         {MAX_ARCHIVE_PRELOAD_PASSES} passes; retrying next frame"
                    );
                }
                if !preload_stall_grace_expired(self.preload_stall_frames) {
                    return;
                }
                self.preload_stalled = true;
                self.record_resource_errors(vec![format!(
                    "Scaleform archive preload stalled for \
                     {MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES} consecutive frames; advancing the \
                     movie without a settled preload"
                )]);
            }
            // Fall through: a menu missing some imported symbols is worth more
            // than a menu frozen on its last uploaded frame.
        } else {
            self.preload_stall_frames = 0;
            self.preload_stalled = false;
        }
        let needs_render = {
            let mut player = self.player.lock().unwrap();
            player.tick(FloatDuration::from_secs(dt));
            player.needs_render()
        };
        if let Some(runtime) = &mut self.navigator_runtime {
            runtime.run_until_stalled();
            let errors = runtime.take_errors();
            let loads = runtime.take_loads();
            self.record_resource_errors(errors);
            self.record_resource_loads(loads);
        }
        // #2719 — only mark dirty when Ruffle says the stage actually changed.
        // An unconditional `self.dirty = true` here made `render`'s early exit
        // dead code, so a *static* menu re-rendered, re-read back and
        // re-uploaded a full-viewport RGBA image every frame — at 1920×1080
        // that is an 8.3 MB readback plus a fresh `VkImage` and a blocking
        // one-time submit, ahead of `draw_frame`, for a picture that did not
        // move. Ruffle raises this flag whenever a frame ran or the mouse
        // state changed, and clears it in `Player::render`.
        self.dirty |= needs_render;
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

    /// Tell Ruffle whether the native pointer is currently inside the movie.
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool) {
        self.player.lock().unwrap().set_mouse_in_stage(is_in_stage);
        self.dirty = true;
    }

    /// Render the current frame to the internal pixel buffer.
    ///
    /// Returns the RGBA pixel data only when it differs from what the caller
    /// was last handed, and `None` otherwise — a `None` means "keep showing
    /// the texture you already have", not "nothing was drawn".
    ///
    /// #2719 — the caller's response to `Some` is a full-viewport
    /// `update_rgba`, which builds a **new** `VkImage` and blocks on a
    /// one-time submit's fence ahead of `draw_frame`. Ruffle re-rendering is
    /// not the same thing as the picture changing (a timeline frame can
    /// advance with nothing visibly moving), so the returned-pixels decision
    /// is made on content, not on the render having happened.
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
        //
        // #2722 (SAFEUI-05) — each of these three failure paths must leave
        // `dirty` set and return `None` instead of falling through to
        // publish `pixel_buffer` as if capture had succeeded. All three are
        // unreachable today (the renderer is always this concrete backend
        // type, `TextureTarget::new` always populates its buffer, and
        // `pixel_buffer`/the target share one fixed `(width, height)`), but
        // the pre-fix fallthrough silently reported failure as success —
        // and cleared `dirty`, so a genuine future failure would never
        // retry — which is the wrong failure mode if any of those
        // invariants ever changes.
        let changed;
        {
            let mut player = self.player.lock().unwrap();
            let renderer = player.renderer_mut();
            let Some(wgpu_backend) =
                <dyn Any>::downcast_mut::<WgpuRenderBackend<TextureTarget>>(renderer)
            else {
                log::warn!("Ruffle renderer is not the expected WgpuRenderBackend<TextureTarget>");
                return None;
            };
            let Some(image) = wgpu_backend.capture_frame() else {
                log::warn!("Ruffle capture_frame() returned no image");
                return None;
            };
            let rgba = image.into_raw();
            if rgba.len() != self.pixel_buffer.len() {
                log::warn!(
                    "Ruffle frame size mismatch: got {} bytes, expected {}",
                    rgba.len(),
                    self.pixel_buffer.len()
                );
                return None;
            }
            changed = rgba != self.pixel_buffer;
            if changed {
                self.pixel_buffer.copy_from_slice(&rgba);
            }
        }

        self.dirty = false;
        // The very first render must always be handed over: the caller has no
        // texture yet, and an all-zero movie would otherwise compare equal to
        // the freshly zeroed buffer and never upload.
        if !changed && self.uploaded_once {
            return None;
        }
        self.uploaded_once = true;
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

    /// Successful relative resources loaded through the archive navigator,
    /// deduplicated by `archive_path` with a hit counter and bounded by
    /// [`MAX_RECORDED_RESOURCE_LOADS`] (#2967).
    ///
    /// This used to clone the navigator's raw, unbounded, un-deduped fetch
    /// log on every call. `tick`/`drive_archive_preload` now drain that log
    /// each pass into this player-side accumulator, so reading it is a plain
    /// borrow — cheap regardless of how long the menu has been open — and a
    /// menu that refetches the same path every frame (the expected shape)
    /// costs one entry with a growing `hit_count`, not one entry per fetch.
    pub fn resource_loads(&self) -> &[ScaleformResourceLoad] {
        &self.resource_loads
    }

    /// First archive loading failure encountered after construction.
    pub fn resource_error(&self) -> Option<&str> {
        self.resource_errors.first().map(String::as_str)
    }

    /// Every distinct archive loading failure seen so far, oldest first.
    ///
    /// #2720 — these are diagnostics, not a kill switch: the movie keeps
    /// playing after each one. A non-empty list on a menu that looks wrong is
    /// the first thing to read.
    pub fn resource_errors(&self) -> &[String] {
        &self.resource_errors
    }

    /// Whether the archive preload exhausted its stall grace period and the
    /// movie is being advanced without having settled (#2720).
    pub fn preload_stalled(&self) -> bool {
        self.preload_stalled
    }

    /// Record fetch failures, deduplicated and bounded.
    ///
    /// Deduplication is what keeps a per-frame retry of the same missing file
    /// from both flooding the log and growing the list without limit; the hard
    /// cap covers the pathological case of endlessly *distinct* failures.
    fn record_resource_errors(&mut self, errors: Vec<String>) {
        for error in errors {
            if self.resource_errors.contains(&error) {
                continue;
            }
            if self.resource_errors.len() >= MAX_RECORDED_RESOURCE_ERRORS {
                if !self.resource_errors_capped {
                    self.resource_errors_capped = true;
                    log::error!(
                        "Scaleform resource failures hit the {MAX_RECORDED_RESOURCE_ERRORS}-entry \
                         cap; further distinct failures are neither recorded nor logged"
                    );
                }
                continue;
            }
            log::error!("{error}");
            self.resource_errors.push(error);
        }
    }

    /// Record resource loads, deduplicated by `archive_path` with a hit
    /// counter, and bounded (#2967).
    ///
    /// Mirrors [`Self::record_resource_errors`]'s dedup-then-cap shape for
    /// the sibling channel that was missing it: a menu that keeps re-fetching
    /// the same path (`fetch`'s expected shape — a timer, a per-frame poll)
    /// bumps that one entry's `hit_count` instead of growing the list, and
    /// the hard cap covers the pathological case of endlessly *distinct*
    /// archive paths.
    fn record_resource_loads(&mut self, loads: Vec<ScaleformResourceLoad>) {
        for load in loads {
            if let Some(existing) = self
                .resource_loads
                .iter_mut()
                .find(|existing| existing.archive_path == load.archive_path)
            {
                existing.hit_count += 1;
                continue;
            }
            if self.resource_loads.len() >= MAX_RECORDED_RESOURCE_LOADS {
                if !self.resource_loads_capped {
                    self.resource_loads_capped = true;
                    log::error!(
                        "Scaleform resource loads hit the {MAX_RECORDED_RESOURCE_LOADS}-entry \
                         cap; further distinct archive paths are neither recorded nor \
                         diagnosable"
                    );
                }
                continue;
            }
            self.resource_loads.push(load);
        }
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

    /// Pump Ruffle's preload and the navigator's local executor until the
    /// preload settles, returning whether it did.
    ///
    /// #2720 — a dependency fetch that fails is drained into
    /// [`Self::resource_errors`] and the pump continues. It used to abort the
    /// whole preload with an `Err`, which the caller turned into the permanent
    /// latch; but `ScaleformNavigator::fail` fires for the entirely routine
    /// "this file is not in the configured archive" case, and the navigator
    /// holds exactly one archive.
    fn drive_archive_preload(&mut self) -> bool {
        for _ in 0..MAX_ARCHIVE_PRELOAD_PASSES {
            let finished = {
                let mut execution_limit = ExecutionLimit::none();
                self.player.lock().unwrap().preload(&mut execution_limit)
            };
            let (errors, loads) = self
                .navigator_runtime
                .as_mut()
                .map(|runtime| {
                    runtime.run_until_stalled();
                    (runtime.take_errors(), runtime.take_loads())
                })
                .expect("archive preload requires a navigator runtime");
            self.record_resource_errors(errors);
            self.record_resource_loads(loads);
            if finished {
                return true;
            }
        }
        false
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

#[cfg(test)]
mod preload_stall_tests {
    use super::{preload_stall_grace_expired, MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES};

    /// #2720 — the pre-fix `tick` mapped an unsettled preload to a bare
    /// `return` with no log, no recorded error and no state change, re-checked
    /// the same condition next frame, and so suppressed the movie *forever* if
    /// the preload never settled: a hang and a stall were indistinguishable.
    /// The property that fixes it is simply that the wait ends — pin the
    /// boundary in both directions so a future edit can't quietly restore an
    /// unbounded one (e.g. by making the comparison strict on a counter that
    /// saturates).
    #[test]
    fn the_preload_stall_wait_terminates() {
        assert!(!preload_stall_grace_expired(0));
        assert!(!preload_stall_grace_expired(
            MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES - 1
        ));
        assert!(preload_stall_grace_expired(
            MAX_CONSECUTIVE_PRELOAD_STALL_FRAMES
        ));
        // `preload_stall_frames` saturates rather than wrapping, so the
        // saturated value must still count as expired.
        assert!(preload_stall_grace_expired(u32::MAX));
    }
}

#[cfg(test)]
mod shared_descriptor_tests {
    use super::get_or_try_init;
    use anyhow::anyhow;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};

    /// #2733 — the whole point of the singleton: the expensive constructor
    /// runs once, and every later caller gets that same instance rather than
    /// a second live device.
    #[test]
    fn a_successful_init_runs_once_and_is_shared_by_later_callers() {
        static SLOT: OnceLock<Arc<u32>> = OnceLock::new();
        let calls = AtomicUsize::new(0);
        let mut make = || {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(Arc::new(7u32))
        };

        let first = get_or_try_init(&SLOT, &mut make).expect("first init succeeds");
        let second = get_or_try_init(&SLOT, &mut make).expect("second call is cached");
        let third = get_or_try_init(&SLOT, &mut make).expect("and stays cached");

        assert_eq!(calls.load(Ordering::Relaxed), 1, "device built once");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(Arc::ptr_eq(&first, &third));
        assert_eq!(*first, 7);
    }

    /// A failure must not poison the slot. Caching the error would turn one
    /// bad adapter request into a permanently UI-less process, which is worse
    /// than the per-menu device this replaced.
    #[test]
    fn a_failed_init_is_not_cached_and_the_next_call_retries() {
        static SLOT: OnceLock<Arc<u32>> = OnceLock::new();
        let attempts = AtomicUsize::new(0);
        let mut make = || match attempts.fetch_add(1, Ordering::Relaxed) {
            0 => Err(anyhow!("no adapter this time")),
            _ => Ok(Arc::new(42u32)),
        };

        assert!(
            get_or_try_init(&SLOT, &mut make).is_err(),
            "first attempt fails"
        );
        assert!(SLOT.get().is_none(), "a failure must leave the slot empty");

        let recovered = get_or_try_init(&SLOT, &mut make).expect("retry succeeds");
        assert_eq!(*recovered, 42);
        assert_eq!(attempts.load(Ordering::Relaxed), 2, "the retry really ran");
    }
}

#[cfg(test)]
mod resource_loads_tests {
    use super::{ScaleformResourceLoad, SwfPlayer, MAX_RECORDED_RESOURCE_LOADS};

    /// A minimal, valid, single-frame AVM1 SWF — enough for `SwfPlayer::new`
    /// (which doesn't need a navigator/archive at all) so these tests can
    /// exercise `record_resource_loads` directly without one.
    fn minimal_swf() -> Vec<u8> {
        let mut header = swf::Header::default_with_swf_version(6);
        header.num_frames = 1;
        let mut bytes = Vec::new();
        swf::write_swf(&header, &[swf::Tag::ShowFrame], &mut bytes)
            .expect("writing a fixed, minimal in-memory SWF cannot fail");
        bytes
    }

    fn load(archive_path: &str) -> ScaleformResourceLoad {
        ScaleformResourceLoad {
            request_url: archive_path.to_string(),
            archive_path: archive_path.to_string(),
            byte_len: 0,
            import_preload_rewritten: false,
            hit_count: 1,
        }
    }

    /// #2967 — `fetch`'s realistic repeat shape (a menu polling the same
    /// resource on a timer) must cost one entry with a growing counter, not
    /// one entry per fetch. Three raw drained events for the same
    /// `archive_path` — exactly what three separate `tick()` passes would
    /// hand `record_resource_loads` — must merge into one.
    #[test]
    fn repeated_fetches_of_the_same_path_bump_a_hit_counter_not_the_list() {
        let mut player = SwfPlayer::new(&minimal_swf(), 4, 4).unwrap();

        player.record_resource_loads(vec![load("interface\\fonts_en.swf")]);
        player.record_resource_loads(vec![load("interface\\fonts_en.swf")]);
        player.record_resource_loads(vec![load("interface\\fonts_en.swf")]);

        assert_eq!(player.resource_loads().len(), 1);
        assert_eq!(player.resource_loads()[0].hit_count, 3);
    }

    /// The pathological case #2967 filed for: a movie fetching endlessly many
    /// *distinct* paths (`ExternalInterface.call`'s cousin for this channel)
    /// must hold at the cap rather than grow the list forever.
    #[test]
    fn distinct_paths_stop_growing_at_the_cap() {
        let mut player = SwfPlayer::new(&minimal_swf(), 4, 4).unwrap();

        let overflow = 10usize;
        let loads = (0..MAX_RECORDED_RESOURCE_LOADS + overflow)
            .map(|i| load(&format!("interface\\asset{i}.swf")))
            .collect();
        player.record_resource_loads(loads);

        assert_eq!(player.resource_loads().len(), MAX_RECORDED_RESOURCE_LOADS);
    }
}

#[cfg(test)]
mod render_failure_tests {
    use super::SwfPlayer;

    /// A minimal, valid, single-frame AVM1 SWF — mirrors
    /// `resource_loads_tests::minimal_swf`.
    fn minimal_swf() -> Vec<u8> {
        let mut header = swf::Header::default_with_swf_version(6);
        header.num_frames = 1;
        let mut bytes = Vec::new();
        swf::write_swf(&header, &[swf::Tag::ShowFrame], &mut bytes)
            .expect("writing a fixed, minimal in-memory SWF cannot fail");
        bytes
    }

    /// #2722 (SAFEUI-05) — a capture failure (forced here via the size-
    /// mismatch branch: `pixel_buffer` no longer matches the real captured
    /// frame's length) must leave `dirty` set and return `None`, not
    /// publish a stale/zeroed frame and clear `dirty` as if capture had
    /// succeeded — which pre-fix meant a genuine future failure would
    /// never retry.
    #[test]
    fn render_leaves_dirty_set_and_returns_none_on_a_size_mismatch() {
        let mut player = SwfPlayer::new(&minimal_swf(), 4, 4).unwrap();
        assert!(player.dirty, "a freshly constructed player starts dirty");
        // `capture_frame()` will still produce a real 4x4 RGBA buffer from
        // the actual render; corrupt `pixel_buffer`'s length so the two no
        // longer match, forcing the size-mismatch branch.
        player.pixel_buffer.push(0);

        let frame = player.render();

        assert!(
            frame.is_none(),
            "a size-mismatch capture must not be published as a real frame"
        );
        assert!(
            player.dirty,
            "a failed capture must leave dirty set so the next tick retries (#2722)"
        );
    }
}
