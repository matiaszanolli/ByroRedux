//! ObScript destruction and revival.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// ObScript destruction and revival.
pub(super) fn register_destruction_and_revival_systems(scheduler: &mut Scheduler) {
    // M47.3 — `OnDestructionStageChange` for the objects a `DamageObject` moved to another stage.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_destruction_system);
    // M47.3 — `Resurrect`: bring the actors back that scripts named this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::revive::revive_system);
}
