//! Idle clip playback, resume and selection.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Idle clip playback, resume and selection.
pub(super) fn register_idles_systems(scheduler: &mut Scheduler) {
    // Translate the resulting Skyrim PlayIdle FormID requests into decoded
    // HKX AnimationPlayers. The parallel animation batch observes a request
    // no later than the following frame, preserving deterministic restarts.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_clip_playback_system);
    // A one-shot `PlayIdle` that has run its length gives the actor its earlier animation back.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_resume_system);
    // `PickIdle`: the actor starts its generic idle again.
    scheduler.add_exclusive(Stage::Update, crate::systems::pick_idle_system);
}
