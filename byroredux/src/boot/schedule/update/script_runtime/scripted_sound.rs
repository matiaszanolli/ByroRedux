//! Scripted sound.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scripted sound.
pub(super) fn register_scripted_sound_systems(scheduler: &mut Scheduler) {
    // M47.2 — `Sound.Play` requests the scripts above recorded.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_sound::scripted_sound_system);
}
