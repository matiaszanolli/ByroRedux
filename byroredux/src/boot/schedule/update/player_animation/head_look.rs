//! Head look.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Head look.
pub(super) fn register_head_look_systems(scheduler: &mut Scheduler) {
    // M47.3 — ObScript `Look`: turn the head bones of actors told to head-track, on top of the pose the
    // animation batch just wrote and before propagation reads it.
    scheduler.add_exclusive(Stage::Update, crate::systems::head_look::head_look_system);
}
