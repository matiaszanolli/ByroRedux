//! Papyrus-demo OnUpdate and the MG07 door.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Papyrus-demo OnUpdate and the MG07 door.
pub(super) fn register_papyrus_demo_on_update_systems(scheduler: &mut Scheduler) {
    fn dlc2_ttr4a_on_update_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_update_system(world)
    }
    fn mg07_on_load_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_load_system(world)
    }
    fn mg07_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_activate_system(world)
    }
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_update_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_load_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::mg07_door::mg07_tick_system,
    );
}
