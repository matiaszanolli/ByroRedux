//! Archetype recognizers — one module per recognizable behavior shape.
//!
//! Each module exposes a `pub fn recognize(&RecognizeCtx) -> Option<Recognized>`
//! that is added to [`super::RECOGNIZERS`] in priority order. Per-script
//! recognizers (a single named script's signature) precede generic ones
//! (a behavior family).
//! - Phase 3 — `rumble` (promotes `defaultRumbleOnActivate`). ✓
//! - Phase 4 — `quest_stage_gate` (generic DA10-family → `QuestAdvanceOnActivate`). ✓

pub mod quest_stage_gate;
pub mod rumble;
