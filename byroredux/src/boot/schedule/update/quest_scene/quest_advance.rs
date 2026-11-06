//! Quest-stage advance and quest startup.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Quest-stage advance and quest startup.
pub(super) fn register_quest_advance_systems(scheduler: &mut Scheduler) {
    fn quest_advance_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::quest_advance::quest_advance_system(world)
    }
    scheduler.add_exclusive(Stage::Update, quest_advance_dispatch);
    // ESM data is installed before the player/event sink exists. Bootstrap
    // Start Game Enabled quests here so Begin On Quest Start scenes observe
    // the same-frame transition.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::quest_startup_system);
}
