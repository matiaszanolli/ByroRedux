//! M65 — per-detector awareness of the player (`docs/engine/sneak-detection.md`
//! §1, §4). Written by the binary's detection tick, the only producer of levels
//! (SNK-DET-001); read by every consumer that asks about the player. Not saved:
//! the first tick after a load re-derives it (SNK-SAVE-002, Q4).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::stealth::{AlertState, DetectionLevel, DetectionTerms};

/// One detector's awareness of the player, as of its last evaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Awareness {
    /// The level toward the player.
    pub level: DetectionLevel,
    /// The detector's own alert state toward the player.
    pub alert: AlertState,
    /// The last evaluation's terms and value (`stealth.status`, SNK-DX-001).
    /// Zero on a sight-only row.
    pub terms: DetectionTerms,
    /// Distance to the player at the last evaluation.
    pub distance: f32,
    /// An unobstructed ray to the player at the last evaluation.
    pub line_of_sight: bool,
    /// The player was inside the view cone at the last evaluation.
    pub in_cone: bool,
    /// Seconds until the next evaluation (its own phase, SNK-DET-006).
    pub timer: f32,
    /// Seconds since the detector last noticed anything (the alert timer).
    pub since_noticed: f32,
    /// The detector would attack the player.
    pub hostile: bool,
}

impl Component for Awareness {
    type Storage = SparseSetStorage<Self>;
}
