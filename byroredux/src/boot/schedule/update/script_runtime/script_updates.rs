//! Script update registrations and distance events.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Script update registrations and distance events.
pub(super) fn register_script_updates_systems(scheduler: &mut Scheduler) {
    // M47.2 — Papyrus update registrations count the host's game clock.
    fn script_update_dispatch(world: &World, dt: f32) {
        let game_hours = world
            .try_resource::<crate::components::GameTimeRes>()
            .map(|time| time.absolute_hours());
        // M47.3 — the `GameHour` global is the game clock (CS *GetCurrentTime*).
        if let Some((day, hour)) = world.try_resource::<crate::components::GameTimeRes>().map(|time| (time.day, time.hour)) {
            byroredux_scripting::globals::sync_game_clock_globals(world, day, hour);
        }
        byroredux_scripting::script_update_system(world, dt, game_hours)
    }
    // OnUpdate / OnUpdateGameTime for the scripts registered for them,
    // beside the other latent-script clock.
    scheduler.add_exclusive(Stage::Update, script_update_dispatch);
    // M47.2 — the distance events whose pair is now close (or far).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::distance_event_system);
}
