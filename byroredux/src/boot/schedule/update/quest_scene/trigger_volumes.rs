//! Trigger-volume detection and scene-trigger actor approach.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Trigger-volume detection and scene-trigger actor approach.
pub(super) fn register_trigger_volumes_systems(scheduler: &mut Scheduler) {
    fn trigger_detection_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::trigger_detection_system(world)
    }
    // M47.2 — trigger detection runs BEFORE quest_advance so an
    // OnTriggerEnterEvent emitted this frame is consumed the same
    // frame (before end-of-frame cleanup drains it).
    scheduler.add_exclusive(Stage::Update, trigger_detection_dispatch);
    // Offscreen scene actors can reach cataloged trigger REFRs whose cells
    // are not resident. Emit their logical OnTriggerEnter handoff here so
    // quest_advance consumes it in the same frame.
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::make_scene_trigger_actor_approach_system(),
    );
}
