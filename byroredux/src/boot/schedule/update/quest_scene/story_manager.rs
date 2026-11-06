//! Story Manager dispatch: location changes, random conversations and the event walk.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Story Manager dispatch: location changes, random conversations and the event walk.
pub(super) fn register_story_manager_systems(scheduler: &mut Scheduler) {
    // #5366 — Story Manager producers + dispatch. The wrapper shapes
    // exist so the registrations below read as a pair: the CLOC
    // producer raises the marker, the dispatcher drains and walks the
    // tree in the same frame.
    fn story_change_location_dispatch(world: &World, _dt: f32) {
        crate::systems::story_events::story_change_location_system(world)
    }
    fn story_manager_dispatch(world: &World, _dt: f32) {
        // #5366 Phase 3 — sync the Story Manager's clock from the
        // canonical game time (advanced by weather_system in Early)
        // before dispatch reads it for RNAM reset windows.
        if let Some(time) = world.try_resource::<crate::components::GameTimeRes>() {
            if let Some(mut clock) =
                world.try_resource_mut::<byroredux_scripting::StoryClock>()
            {
                clock.hours = time.day as f64 * 24.0 + f64::from(time.hour);
            }
        }
        byroredux_scripting::story_manager_dispatch_system(world)
    }
    // #5366 — Story Manager: raise CLOC on session-location change, then
    // dispatch every raised story event. Runs right after quest_startup
    // and before the alias refresh so an SM-started quest is
    // indistinguishable from a Start Game Enabled one to everything
    // downstream (aliases, scenes, fragments) — and the KILL marker the
    // combat damage system raises is drained here too, one frame later
    // at the latest.
    scheduler.add_exclusive(Stage::Update, story_change_location_dispatch);
    register_random_conversations(scheduler);
    scheduler.add_exclusive(Stage::Update, story_manager_dispatch);
}

/// M43 — the `ADIA` producer (NPC-to-NPC random conversations). Registered ahead of the Story Manager
/// dispatch so the event is walked the same frame; `BYRO_NO_RANDOM_CONVERSATIONS=1` leaves it out,
/// like the locomotion kill switch.
fn register_random_conversations(scheduler: &mut Scheduler) {
    // `REMP`: a dropped item's event waits for its reference to spawn, so it is polled each frame.
    scheduler.add_exclusive(Stage::Update, crate::inventory::dropped_item_story_event_system);
    // `DEAD`: an actor that sees a corpse discovers it (idle when the tree has no `DEAD` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::dead_body::dead_body_discovery_system);
    // `IRON`: the player begins aiming a gun (idle when the tree has no `IRON` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::iron_sights::iron_sights_system);
    // `TMEE`: placed proximity mines trip, burn their fuse and blast (idle when the load order has no mines).
    scheduler.add_exclusive(Stage::Update, crate::systems::mines::mine_system);
    // `AOBJ`: an actor near an object that shares an attraction keyword reacts (idle without an `AOBJ` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::attraction::attraction_system);
    if std::env::var_os("BYRO_NO_RANDOM_CONVERSATIONS").is_none() {
        scheduler.add_exclusive(Stage::Update, crate::systems::random_conversation::random_conversation_system);
        // The hold system walks / releases the pairs it takes over; `BYRO_NO_AI_LOCOMOTION=1` leaves the
        // walk out (the event is then raised from where the initiator stands).
        let walk = std::env::var_os("BYRO_NO_AI_LOCOMOTION").is_none();
        scheduler.add_exclusive(Stage::Update, crate::systems::random_conversation::make_conversation_hold_system(walk));
    }
}
