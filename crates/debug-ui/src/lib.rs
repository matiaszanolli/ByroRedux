//! Embedded egui debug-UI overlay.
//!
//! Phase 4a lands the minimum viable integration: an egui context
//! that runs on every frame, an `egui-ash-renderer`-backed Vulkan
//! pipeline that draws over the composite output, an F-key toggle,
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
    SettingChange, SettingEntry, SettingValue, SettingsError, SettingsRegistry,
};
use egui_winit::winit;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub use panels::{PanelOutputs, PanelSnapshot, PanelTab, QueuedLoad};

/// Stable registry key for the overlay's own scale control. Other engine
/// modules can register settings beside it without depending on this crate.
pub const OVERLAY_SCALE_SETTING_ID: &str = "interface.overlay_scale";

/// Register settings owned by the overlay itself. The binary calls this while
/// assembling the universal [`SettingsRegistry`]; renderer, audio, input, and
/// gameplay modules can add their own entries through the same API over time.
pub fn register_builtin_settings(registry: &mut SettingsRegistry) -> Result<(), SettingsError> {
    registry.register(SettingEntry::slider(
        OVERLAY_SCALE_SETTING_ID,
        "Interface",
        "Overlay scale",
        "Scale the complete on-screen console and settings interface.",
        1.0,
        0.75,
        2.0,
        0.05,
        "×",
    ))
}

/// Persistent egui state shared between the App's event loop and
/// the renderer's draw pass.
///
/// `visible == false` is the steady state on engine boot. Toggled
/// by F3 (or any other key the App wires). When `visible` is
/// false, [`Self::run`] short-circuits — no UI work happens, no
/// texture uploads queued, no GPU vertex/index data produced.
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

    /// Run one egui frame against a pre-built [`PanelSnapshot`].
    /// Returns the operator's actions in [`PanelOutputs`] — the
    /// binary applies those to the World after this method returns
    /// (queueing loads, dispatching console expressions, etc.). The
    /// closure-as-arg form of the Phase-4a placeholder is gone
    /// because the panels need the snapshot + outputs by value, not
    /// the binary's `&self.world` (which would conflict with the
    /// `&mut self.debug_ui` borrow).
    ///
    /// Returns an empty `PanelOutputs` when the overlay is hidden.
    pub fn run(&mut self, window: &Window, snapshot: &PanelSnapshot) -> PanelOutputs {
        if !self.visible {
            return PanelOutputs::default();
        }
        let raw_input = self.egui_winit.take_egui_input(window);
        // begin_pass / end_pass split so the panel draw can capture
        // `&mut self.panels` without the `Context::run`'s FnMut
        // sugar fighting the borrow.
        self.egui_ctx.begin_pass(raw_input);
        let mut outputs = PanelOutputs::default();
        panels::draw(&self.egui_ctx, snapshot, &mut self.panels, &mut outputs);
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

// Re-export the public-facing pieces of the upstream crates so the
// binary doesn't have to add a direct dep on each one.
pub use egui;
pub use egui_winit;
