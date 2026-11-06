//! Scripted motion-type changes and Havok impulses.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scripted motion-type changes and Havok impulses.
pub(super) fn register_scripted_physics_systems(scheduler: &mut Scheduler) {
    // Apply Papyrus SetMotionType requests after both immediate and resumed
    // fragment effects, before the Physics stage consumes body state.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_motion_type_system);
    // ApplyHavokImpulse requests, after the motion-type change that made the body dynamic.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_havok_impulse_system);
}
