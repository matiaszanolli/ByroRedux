//! Recurring-update ticks.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Recurring-update ticks.
pub(super) fn register_recurring_update_systems(scheduler: &mut Scheduler) {
    // #5842 — the demo dispatchers below (dlc2_ttr4a, mg07) and
    // `recurring_update_tick_system` run every frame with nothing to do
    // in production: nothing spawns their components (they are test
    // surface / living evidence per scripting.md's R5 section), and no
    // recognizer emits `RecurringUpdate`. They stay scheduler-wired so
    // the demos' fixture tests drive the real schedule shape.
    //
    // `recurring_update_tick_system` ticks `RecurringUpdate` and emits
    // `OnUpdateEvent`. It sits between the demo's OnInit (which
    // subscribes via `RegisterForUpdate`) and its OnUpdate consumer so
    // a fired event is handled the same frame, before cleanup drains it.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::recurring_update_tick_system,
    );
}
