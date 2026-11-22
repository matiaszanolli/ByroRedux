//! The snapshot the operator overlay renders and the actions it hands back (#6578). The
//! player-facing state is `byroredux_native_ui`'s (GUI-PAGE-006), nested as `game`.

use byroredux_native_ui::{PageOutputs, PageSnapshot};
use byroredux_sdk::studio::{StudioCommand, StudioSnapshot};

/// Read-only snapshot of the engine-side state the overlay host renders: the player's HUD and
/// pages (`game`) and the operator panels' own data. Built each frame by the binary right before
/// `DebugUiState::run`. Cloning is cheap — `MetricsSnapshot` is small, the entity list is
/// name-only.
#[derive(Default, Clone)]
pub struct PanelSnapshot {
    /// What the HUD and the native pages draw.
    pub game: PageSnapshot,
    /// #5822 — core's snapshot directly; debug-ui depends on core, so
    /// no hand-synced view copy is needed.
    pub metrics: Option<byroredux_core::ecs::MetricsSnapshot>,
    /// `(entity_id, name)` pairs. `None` until the operator opens
    /// the Entities panel — populating this on every frame would
    /// be unnecessary work for an overlay that's hidden most of
    /// the time.
    pub entities: Option<Vec<(u32, String)>>,
    /// Present when the executable is hosting an SDK Studio document.
    pub studio: Option<StudioSnapshot>,
}

/// Actions the overlay asked the App to perform: the player's (`game`) and the operator
/// panels'. Drained by the binary after [`crate::DebugUiState::run`] returns.
#[derive(Default, Clone)]
pub struct PanelOutputs {
    /// What the player asked on the HUD and the native pages (and the settings changes both
    /// settings surfaces make).
    pub game: PageOutputs,
    /// NIF / cell loads to queue against `PendingDebugLoadSlot`.
    pub queued_loads: Vec<QueuedLoad>,
    /// Console expressions to evaluate via the existing
    /// `CommandRegistry`. The binary translates each into the same
    /// path the debug-server's `Eval` request takes.
    pub console_evals: Vec<String>,
    /// Renderer-independent Studio mutations for the host to apply.
    pub studio_commands: Vec<StudioCommand>,
    /// True when the operator asked to refresh the entity list.
    /// The binary rebuilds the snapshot's `entities` next frame.
    pub refresh_entities: bool,
}

/// One queued load request. The binary maps this 1:1 onto a
/// `PendingDebugLoad` enum variant — kept as a separate type here
/// so the debug-ui crate doesn't need to depend on core's
/// `PendingDebugLoad` directly.
#[derive(Debug, Clone)]
pub enum QueuedLoad {
    Nif { path: String, label: Option<String> },
}
