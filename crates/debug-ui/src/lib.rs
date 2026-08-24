//! Embedded egui debug-UI overlay.
//!
//! Phase 4a lands the minimum viable integration: an egui context
//! that runs on every frame, an `egui-ash-renderer`-backed Vulkan
//! pipeline that draws over the composite output, an F3 toggle,
//! and a stub panel that proves the round trip. Phase 4b fills in
//! the actual Metrics / Loader / Entities / Console / Settings panels.
//!
//! The overlay is driven through three touch points on the binary's
//! main loop:
//!
//! 1. **Event** — the App forwards every `winit::WindowEvent` to
//!    [`DebugUiState::on_window_event`] **before** the existing
//!    camera input layer. When the event response carries
//!    `consumed = true` the App should skip writing it into its
//!    own `InputState` so the fly camera doesn't fight egui.
//!
//! 2. **Frame** — once per frame, before `VulkanContext::draw_frame`,
//!    the App calls [`DebugUiState::run`] with the window handle.
//!    That builds + finalises the egui frame and stashes
//!    `FullOutput` for the renderer to consume.
//!
//! 3. **Render** — the renderer reads the stashed output, uploads
//!    any new textures, tessellates the shape list, and draws into
//!    the swapchain image inside the new `EguiPass`. Sequenced
//!    inside `draw_frame` right after the composite pass.
//!
//! `DebugUiState` is stored as an ECS resource so any system can
//! flip `visible` or read the last-known panel state. The renderer
//! reads the egui pixels-per-point + viewport ID directly from the
//! resource so the App doesn't have to thread a separate context.

pub mod panels;

use byroredux_core::ecs::Resource;
use byroredux_core::settings::{
    SettingChange, SettingChoice, SettingEntry, SettingValue, SettingsError, SettingsRegistry,
};
use egui_winit::winit;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub use panels::{
    GameMenuPage, GameMenuState, InteractionPrompt, InventoryAction, InventoryItemView,
    InventorySnapshot, PanelOutputs, PanelSnapshot, PanelTab, QueuedLoad,
};

/// Stable registry key for the overlay's own scale control. Other engine
/// modules can register settings beside it without depending on this crate.
pub const OVERLAY_SCALE_SETTING_ID: &str = "interface.overlay_scale";

/// Stable keys for the player-facing HUD and camera settings.
pub const SHOW_CROSSHAIR_SETTING_ID: &str = "interface.show_crosshair";
pub const SHOW_PROMPTS_SETTING_ID: &str = "interface.show_prompts";
pub const FOV_SETTING_ID: &str = "gameplay.field_of_view";

/// Stable registry key for the temporal reconstruction path. The value is the
/// same spec string the `r.upscaler` console command and the `--upscaler` /
/// `--fsr-quality` CLI pair accept, so all three routes share one grammar and
/// the binary parses it in exactly one place.
pub const UPSCALER_SETTING_ID: &str = "render.upscaler";

/// Register settings owned by the overlay itself. The binary calls this while
/// assembling the universal [`SettingsRegistry`]; renderer, audio, input, and
/// gameplay modules can add their own entries through the same API over time.
pub fn register_builtin_settings(registry: &mut SettingsRegistry) -> Result<(), SettingsError> {
    registry.register(SettingEntry::slider(
        FOV_SETTING_ID,
        "Gameplay",
        "Field of view",
        "Vertical camera field of view. Applies immediately without rebuilding the renderer.",
        // Match `Camera::default()` so merely loading the settings registry
        // does not change the established view or benchmark framing.
        45.0,
        45.0,
        110.0,
        1.0,
        "°",
    ))?;
    registry.register(SettingEntry::slider(
        OVERLAY_SCALE_SETTING_ID,
        "Interface",
        "UI scale",
        "Scale the HUD, pause menu, settings, and developer overlay.",
        1.0,
        0.75,
        2.0,
        0.05,
        "×",
    ))?;
    registry.register(SettingEntry::toggle(
        SHOW_CROSSHAIR_SETTING_ID,
        "Interface",
        "Show crosshair",
        "Keep a small reticle at screen center while controlling the world.",
        true,
    ))?;
    registry.register(SettingEntry::toggle(
        SHOW_PROMPTS_SETTING_ID,
        "Interface",
        "Show interaction prompts",
        "Show the active key and action when an object can be used.",
        true,
    ))?;
    registry.register(SettingEntry::choice(
        UPSCALER_SETTING_ID,
        "Rendering",
        "Upscaler",
        "Temporal reconstruction path. FSR renders the scene below output \
         resolution and reconstructs it; TAA renders at native resolution. \
         Switching rebuilds every render-resolution target and resets \
         temporal history.",
        // Must match `UpscalerMode::default()` and the CLI's no-flag default;
        // `App::new` overwrites this from the parsed config at startup anyway,
        // but a divergent literal here would show the wrong entry for the one
        // frame before that happens.
        "fsr3/quality",
        vec![
            SettingChoice::new("taa", "TAA (native resolution)"),
            SettingChoice::new("fsr3/native-aa", "FSR 3.1 — Native AA"),
            SettingChoice::new("fsr3/quality", "FSR 3.1 — Quality"),
            SettingChoice::new("fsr3/balanced", "FSR 3.1 — Balanced"),
            SettingChoice::new("fsr3/performance", "FSR 3.1 — Performance"),
        ],
    ))
}

/// Persistent egui state shared between the App's event loop and
/// the renderer's draw pass.
///
/// `visible == false` is the debug-panel steady state on engine boot.
/// Toggled by F3 (or any other key the App wires). A gameplay HUD prompt
/// can still produce a lightweight egui frame while the debug panels are
/// hidden; with neither surface active, [`Self::run`] short-circuits.
pub struct DebugUiState {
    pub visible: bool,
    /// egui's central context — holds layout state, persisted
    /// widget memory, the texture cache.
    pub egui_ctx: egui::Context,
    /// egui-winit's input translator. Owns the OS-clipboard
    /// interface plus the per-window viewport state.
    egui_winit: egui_winit::State,
    /// The most recent `FullOutput` produced by [`Self::run`]. The
    /// renderer consumes this in `draw_frame` and clears it back
    /// to `None` so a hypothetical missed render doesn't replay
    /// stale shapes. `None` when the overlay is hidden or before
    /// the first frame.
    last_output: Option<egui::FullOutput>,
    /// Per-panel input + history state (loader form fields,
    /// console buffer + log, active tab). Persisted across frames.
    pub panels: PanelState,
    /// Player-facing pause/settings navigation, independent from the F3
    /// developer overlay.
    game_menu: GameMenuState,
    /// Short player-facing save/load status toast. The same text is retained
    /// in console history for later diagnostics.
    player_message: Option<(String, std::time::Instant)>,
}

/// Per-panel input + history state. Lives on [`DebugUiState`] so it
/// persists across frames the way egui's internal widget memory does.
#[derive(Default, Clone)]
pub struct PanelState {
    pub active_tab: PanelTab,
    pub loader_path: String,
    pub loader_label: String,
    pub console_input: String,
    /// Case-insensitive filter for the universal Settings tab.
    pub settings_filter: String,
    /// Bounded scrollback for the Console tab.
    pub console_history: Vec<String>,
}

/// Cap on the Console tab's scrollback so a long debugging session
/// doesn't grow unbounded.
pub const CONSOLE_HISTORY_CAP: usize = 200;

impl Resource for DebugUiState {}

impl DebugUiState {
    /// Construct the overlay state. Call once at engine boot after
    /// the window has been created. The `event_loop` is needed
    /// because `egui_winit::State::new` queries the loop for its
    /// initial display handle.
    pub fn new(event_loop: &ActiveEventLoop, window: &Window) -> Self {
        let egui_ctx = egui::Context::default();
        let viewport_id = egui_ctx.viewport_id();
        // `max_texture_side` is queried so egui's font atlas + image
        // widgets don't try to allocate a texture larger than the
        // Vulkan device exposes. The default cap (`None`) is fine on
        // desktop GPUs; we leave it unset so egui uses its own
        // sensible default (8192 today).
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            viewport_id,
            event_loop,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            visible: false,
            egui_ctx,
            egui_winit,
            last_output: None,
            panels: PanelState::default(),
            game_menu: GameMenuState::default(),
            player_message: None,
        }
    }

    /// Append a line to the console scrollback, trimming the oldest
    /// entries past [`CONSOLE_HISTORY_CAP`].
    pub fn push_console_line(&mut self, line: String) {
        self.panels.console_history.push(line);
        if self.panels.console_history.len() > CONSOLE_HISTORY_CAP {
            let overflow = self.panels.console_history.len() - CONSOLE_HISTORY_CAP;
            self.panels.console_history.drain(..overflow);
        }
    }

    pub fn push_player_message(&mut self, line: String) {
        self.push_console_line(line.clone());
        self.player_message = Some((
            line,
            std::time::Instant::now() + std::time::Duration::from_secs(4),
        ));
    }

    /// Apply settings that are owned by the overlay presentation layer.
    /// Unknown IDs deliberately no-op so every universal setting change can
    /// flow through this hook without coupling the UI to other subsystems.
    pub fn apply_setting_change(&self, change: &SettingChange) {
        if change.id == OVERLAY_SCALE_SETTING_ID {
            if let SettingValue::Number(scale) = &change.value {
                self.egui_ctx.set_zoom_factor(*scale);
            }
        }
    }

    /// Reapply overlay-owned values after egui is recreated, for example when
    /// the platform resumes and creates a new window/context.
    pub fn sync_registered_settings(&self, registry: &SettingsRegistry) {
        if let Some(entry) = registry.get(OVERLAY_SCALE_SETTING_ID) {
            self.apply_setting_change(&SettingChange::new(&entry.id, entry.value.clone()));
        }
    }

    /// Forward a `WindowEvent` to egui. Returns the response so the
    /// App can short-circuit camera input when egui consumed the
    /// event.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
    ) -> egui_winit::EventResponse {
        self.egui_winit.on_window_event(window, event)
    }

    /// Toggle the overlay. Idempotent.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            // Drop any stashed FullOutput so the renderer sees the
            // overlay as cleanly hidden — otherwise a one-frame
            // ghost panel could linger.
            self.last_output = None;
        }
    }

    pub fn game_menu_visible(&self) -> bool {
        self.game_menu.visible
    }

    /// Toggle the player-facing pause menu. Opening always returns to the
    /// pause landing page so Escape is a predictable escape hatch from any
    /// settings category.
    pub fn toggle_game_menu(&mut self) -> bool {
        self.game_menu.visible = !self.game_menu.visible;
        if self.game_menu.visible {
            self.game_menu.page = GameMenuPage::Pause;
        } else {
            self.last_output = None;
        }
        self.game_menu.visible
    }

    /// Open the native menu directly on the player's inventory page.
    pub fn open_inventory_menu(&mut self) {
        self.game_menu.visible = true;
        self.game_menu.page = GameMenuPage::Inventory;
    }

    pub fn inventory_menu_visible(&self) -> bool {
        self.game_menu.visible && self.game_menu.page == GameMenuPage::Inventory
    }

    pub fn close_game_menu(&mut self) {
        self.game_menu.visible = false;
        self.game_menu.page = GameMenuPage::Pause;
        self.last_output = None;
    }

    /// Native pause/settings owns every gameplay input event while open,
    /// regardless of whether an individual egui widget consumed it.
    pub fn captures_gameplay_input(&self) -> bool {
        self.game_menu.visible || self.visible
    }

    /// Run one egui frame against a pre-built [`PanelSnapshot`].
    /// Returns the operator's actions in [`PanelOutputs`] — the
    /// binary applies those to the World after this method returns
    /// (queueing loads, dispatching console expressions, etc.). The
    /// closure-as-arg form of the Phase-4a placeholder is gone
    /// because the panels need the snapshot + outputs by value, not
    /// the binary's `&self.world` (which would conflict with the
    /// `&mut self.debug_ui` borrow).
    ///
    /// Returns an empty `PanelOutputs` when the debug overlay is hidden;
    /// a supplied gameplay prompt can still produce render output.
    pub fn run(&mut self, window: &Window, snapshot: &PanelSnapshot) -> PanelOutputs {
        if self
            .player_message
            .as_ref()
            .is_some_and(|(_, expires)| std::time::Instant::now() >= *expires)
        {
            self.player_message = None;
        }
        if !self.visible
            && !self.game_menu.visible
            && snapshot.interaction_prompt.is_none()
            && !snapshot.show_crosshair
            && self.player_message.is_none()
        {
            // #2831 — still drain. `on_window_event` is forwarded for EVERY
            // `WindowEvent`, unconditionally, and appends onto egui-winit's
            // private `egui_input.events`; `take_egui_input` is its only
            // drain and this is its only caller. Returning early therefore
            // retained one `egui::Event` per forwarded mouse-move / key /
            // wheel / touch for the lifetime of the process — and a hidden
            // overlay is the steady state, since it is opt-in behind F3, so
            // a fly-camera session grows the queue monotonically.
            //
            // Draining rather than gating the forwarding: egui-winit also
            // tracks viewport, modifier and focus state in the same call, and
            // skipping `on_window_event` would lose that across the toggle so
            // the first visible frame came up with stale modifiers. This way
            // the bookkeeping stays current and only the event backlog — which
            // no one will ever consume — is discarded. Without it the first F3
            // press replays the entire accumulated backlog in one `RawInput`.
            let _ = self.egui_winit.take_egui_input(window);
            return PanelOutputs::default();
        }
        let raw_input = self.egui_winit.take_egui_input(window);
        // begin_pass / end_pass split so the panel draw can capture
        // `&mut self.panels` without the `Context::run`'s FnMut
        // sugar fighting the borrow.
        self.egui_ctx.begin_pass(raw_input);
        let mut outputs = PanelOutputs::default();
        if !self.game_menu.visible {
            panels::draw_hud(&self.egui_ctx, snapshot);
        }
        if let Some((message, _)) = &self.player_message {
            panels::draw_player_message(&self.egui_ctx, message);
        }
        if self.visible {
            panels::draw(&self.egui_ctx, snapshot, &mut self.panels, &mut outputs);
        }
        if self.game_menu.visible {
            panels::draw_game_menu(&self.egui_ctx, snapshot, &mut self.game_menu, &mut outputs);
        }
        let output = self.egui_ctx.end_pass();
        // Hand the platform output back to egui-winit so OS-level
        // cursor / clipboard changes get applied. Done here (not
        // in the renderer) so the renderer stays a pure-GPU layer.
        self.egui_winit
            .handle_platform_output(window, output.platform_output.clone());
        self.last_output = Some(output);
        outputs
    }

    /// Drain the stashed `FullOutput`. The renderer calls this in
    /// `draw_frame`; returns `None` when the overlay is hidden or
    /// the App didn't call [`Self::run`] this frame.
    pub fn take_output(&mut self) -> Option<egui::FullOutput> {
        self.last_output.take()
    }

    /// Pixels-per-point (DPI scale) the renderer should use when
    /// tessellating shapes. Reads from the egui context so the
    /// renderer doesn't need a separate window handle.
    pub fn pixels_per_point(&self) -> f32 {
        self.egui_ctx.pixels_per_point()
    }
}

#[cfg(test)]
mod builtin_settings_tests {
    use super::*;

    #[test]
    fn default_fov_preserves_the_core_camera_framing() {
        let mut registry = SettingsRegistry::default();
        register_builtin_settings(&mut registry).unwrap();
        let SettingValue::Number(degrees) = registry.get(FOV_SETTING_ID).unwrap().value else {
            panic!("FOV must remain a numeric setting");
        };

        assert!(
            (degrees.to_radians() - byroredux_core::ecs::Camera::default().fov_y).abs()
                < f32::EPSILON
        );
    }
}

#[cfg(test)]
mod hidden_overlay_drain_tests {
    /// #2831 — `DebugUiState::run`'s hidden-overlay early return must still
    /// drain egui-winit's raw-input queue.
    ///
    /// A runtime assertion is not reachable here: exercising the real queue
    /// needs a live `winit::Window` and an `egui_winit::State`, neither of
    /// which can be constructed in a headless test, and egui-winit keeps
    /// `egui_input.events` private so its length is not observable even with
    /// one. The defect is structural anyway — an early return placed *before*
    /// the only `take_egui_input` call — so the source is the right thing to
    /// pin.
    #[test]
    fn hidden_run_drains_before_returning() {
        let src = include_str!("lib.rs");
        let run = src
            .split_once("pub fn run(&mut self, window: &Window, snapshot: &PanelSnapshot)")
            .expect("DebugUiState::run must exist")
            .1;
        let early_return = run
            .find("return PanelOutputs::default();")
            .expect("the hidden-overlay early return must exist");
        let drain = run
            .find("take_egui_input(window)")
            .expect("run must drain egui-winit's raw input somewhere");
        assert!(
            drain < early_return,
            "the hidden-overlay branch returns before draining take_egui_input — \
             every forwarded pointer/key event then accumulates for the lifetime \
             of the process, and the first F3 replays the whole backlog (#2831)"
        );
    }

    /// The fix must stay a *drain*, not a gate on the forwarding side:
    /// skipping `on_window_event` would lose egui's modifier/focus/viewport
    /// bookkeeping across the visibility toggle, so the first visible frame
    /// would come up with stale state.
    #[test]
    fn window_events_are_still_forwarded_unconditionally() {
        let src = include_str!("lib.rs");
        let forward = src
            .split_once("pub fn on_window_event(")
            .expect("on_window_event must exist")
            .1;
        // Body only — stop at the function's closing brace, or the neighbouring
        // `toggle()` (which legitimately reads `self.visible`) lands in scope.
        let body = forward
            .split_once("\n    }")
            .expect("on_window_event must have a body")
            .0;
        assert!(
            !body.contains("self.visible"),
            "on_window_event must not gate on visibility — drain in `run` instead (#2831)"
        );
    }
}
