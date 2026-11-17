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
    /// M48 GUI-OPS-002 — per open movie (by `menu_name`), its host calls by behaviour class since
    /// it opened, keyed to the layer (`menu#id`) so a replaced movie starts a fresh report.
    pub(crate) host_reports: std::collections::HashMap<String, (String, byroredux_ui::HostCallReport)>,
    /// M48 GUI-PAGE-004 — the conversation on the game's own dialogue movie (S1), when open.
    pub(crate) dialogue_movie: Option<crate::scaleform_dialogue::DialogueMovie>,
    /// The dialogue movie's host calls the frame's drain took, for the driver's next step.
    pub(crate) dialogue_calls: Vec<byroredux_ui::ScaleformHostCall>,
    /// M48 GUI-PAGE-004 — the conversation on the game's own dialogue document (S2), when open.
    pub(crate) dialogue_document: Option<crate::menuxml_dialogue::DialogueDocument>,
    /// What the dialogue document's input asked, for the driver's next step.
    pub(crate) dialogue_document_actions: Vec<crate::menuxml_dialogue::DocumentAction>,
    /// Per open movie (by `menu_name`), the last `dropped_host_calls()` reading the per-frame
    /// Scaleform drain observed (#2969), paired with the layer it came from (`menu#id`, #3772):
    /// `drain_calls`' contract is that a full batch must be read together with the drop counter
    /// — the batch may not be contiguous. Latched rather than compared against zero so the warning
    /// fires on each *increase*; a menu replaced under the same name is a new layer id, so its
    /// counter resets explicitly instead of reading as "un-dropped".
    pub(crate) dropped_host_calls: std::collections::HashMap<String, (u64, String)>,
    /// #4612 — the always-on HUD's objective list, rebuilt only when quest
    /// state changes; lent to each frame's `PanelSnapshot`.
    pub(crate) objective_hud_cache: crate::objectives::ObjectiveHudCache,
}
