//! UI overlay and HUD state carried by [`crate::App`] (#6695).
//!
//! The Scaleform/SWF manager and its input translation, the two HUD drivers
//! (MenuXml and Scaleform), the host-call diagnostics latches, and the
//! objective-list cache lent to the always-on HUD. One `ui` field on `App`
//! replaces ten loose ones. `Default` is the construction contract: no
//! manager, no HUD, empty latches.

use byroredux_ui::UiManager;

/// UI overlay and HUD state. See the module docs.
#[derive(Default)]
pub(crate) struct UiState {
    pub(crate) manager: Option<UiManager>,
    /// MenuXml HUD (M48.4 Oblivion / M48.5 FO3-FNV legacy-UI track).
    /// Mutually exclusive with the Scaleform overlay in practice:
    /// `--hud` is the pre-Skyrim route, `--menu` the Skyrim+/FO4 one,
    /// and the frame tick prefers whichever is live.
    pub(crate) hud: Option<crate::hud::MenuXmlHud>,
    /// Scaleform HUD driver (M48.6 Skyrim `--hud` route). Owns the
    /// response handlers + push cadence for a `hudmenu.swf` launched
    /// into `manager`; `None` unless `--hud` matched a Skyrim
    /// install.
    pub(crate) scaleform_hud: Option<crate::scaleform_hud::ScaleformHudDriver>,
    /// Window-system state used to translate events into Scaleform space.
    pub(crate) input_state: crate::ui_input::UiInputState,
    /// Texture handle for the UI overlay (registered in the texture registry).
    pub(crate) texture_handle: Option<u32>,
    /// `(menu, host method)` pairs already reported by the per-frame Scaleform
    /// drain, so repeated calls report once without suppressing the same
    /// missing handler when a second menu needs it (#2714 / #3155). Bounded by
    /// `byroredux_ui::MAX_DISTINCT_HOST_METHOD_NAMES` (#2964) — this mirrors
    /// the bridge's own movie-keyed diagnostic sets, so it needs the same
    /// cap for the same reason: an unbounded-by-construction set keyed by a
    /// string untrusted ActionScript content chooses.
    pub(crate) reported_host_methods: std::collections::HashSet<(String, String)>,
    /// One-shot latch for the `reported_host_methods` cap warning.
    pub(crate) reported_host_methods_capped: bool,
    /// Last `UiManager::dropped_host_calls()` reading the per-frame Scaleform
    /// drain observed (#2969). `drain_calls`' contract is that a full batch
    /// must be read together with the drop counter — the batch may not be
    /// contiguous — and nothing outside `crates/ui` read it. Latched rather
    /// than compared against zero so the warning fires on each *increase*,
    /// not every frame after the first eviction; a decrease means a new menu
    /// brought a fresh bridge, not that calls were un-dropped.
    ///
    /// #3772 — this latches a bare number, not the *identity* of the bridge
    /// that produced it, so a swap landing on a frame where the new bridge
    /// has already evicted N < the old latch reads as "un-dropped" (a
    /// decrease) instead of "different menu" — the new menu's first N drops
    /// are silently absorbed. [`Self::dropped_host_calls_menu`] pairs the
    /// latch with the menu name that produced it so a swap is an *explicit*
    /// reset (`app_frame.rs`), not an inferred one.
    pub(crate) dropped_host_calls: u64,
    /// The menu name [`Self::dropped_host_calls`] was last latched
    /// against. `None` before the first Scaleform drain. See
    /// `dropped_host_calls`'s doc (#3772).
    pub(crate) dropped_host_calls_menu: Option<String>,
    /// #4612 — the always-on HUD's objective list, rebuilt only when quest
    /// state changes; lent to each frame's `PanelSnapshot`.
    pub(crate) objective_hud_cache: crate::objectives::ObjectiveHudCache,
}
