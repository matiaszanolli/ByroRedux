//! Script-requested actions: weapon use, inventory reset, sleep, music, reload, the chargen catch-up, UI requests and actor alpha.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Script-requested actions: weapon use, inventory reset, sleep, music, reload, the chargen catch-up, UI requests and actor alpha.
pub(super) fn register_script_requests_systems(scheduler: &mut Scheduler) {
    // M42 — the UseWeapon PACK procedure raises its request first, so the volley starts this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::use_weapon_package_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::use_weapon_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reset_inventory_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::sleep_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::music_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reload_system);
    scheduler.add_exclusive(Stage::Update, crate::combat::chargen_catch_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::script_ui_request_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::actor_alpha_system);
}
