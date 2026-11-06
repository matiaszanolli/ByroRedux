//! Papyrus-demo DLC2 TTR4a OnInit.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Papyrus-demo DLC2 TTR4a OnInit.
pub(super) fn register_papyrus_demo_on_init_systems(scheduler: &mut Scheduler) {
    fn dlc2_ttr4a_on_init_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_init_system(world)
    }
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_init_dispatch);
}
