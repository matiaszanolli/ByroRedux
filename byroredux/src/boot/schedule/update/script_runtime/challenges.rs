//! Challenge tracking.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Challenge tracking.
pub(super) fn register_challenges_systems(scheduler: &mut Scheduler) {
    // M47.3 — the New Vegas challenges whose stat has risen far enough complete (reward script and all).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::challenges::challenge_system);
}
