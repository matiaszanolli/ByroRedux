//! ObScript cell resets and object scripts.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// ObScript cell resets and object scripts.
pub(super) fn register_obscript_objects_systems(scheduler: &mut Scheduler) {
    // M47.3 — legacy ObScript object scripts (the `SCRI` scripts on placed
    // references): `GameMode` every frame, `OnLoad` and `OnDeath` on their
    // markers. Before `reference_script_event_system` below, which drains
    // `OnDeathEvent` at its head; after the package, activation and dialogue
    // systems whose state the commands read.
    // M47.3 — a cell that reset as the player entered it owes its scripted references an `OnReset`.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_state::mark_cell_reset);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_objects::obscript_object_system,
    );
    // GUI-EVT-006 / #7293 — the object scripts' `Begin MenuMode [code]` blocks, on every frame a
    // menu-mode menu is open; `obscript_object_system` above runs no `GameMode` then. A menu that
    // pauses nothing (dialogue) still advances the schedule, so it runs here as its quest sibling
    // does; the paused frames a native page holds run it by hand (`run_menu_mode_frame`).
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_objects::obscript_object_menu_mode_system,
    );
}
