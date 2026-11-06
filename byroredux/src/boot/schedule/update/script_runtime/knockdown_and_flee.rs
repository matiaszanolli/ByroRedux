//! Knockdown, get-up and flee.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Knockdown, get-up and flee.
pub(super) fn register_knockdown_and_flee_systems(scheduler: &mut Scheduler) {
    // M47.3 — `PushActorAway` (knock the actor down, then stand it up where it came to rest), `ForceFlee`,
    // `UseWeapon`, `ResetInventory`, `SetPCSleepHours`, `PlayMusic` and `EssentialDeathReload`: the app's
    // half of the commands that raise a request.
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::push_actor_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::get_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::flee::flee_system);
}
