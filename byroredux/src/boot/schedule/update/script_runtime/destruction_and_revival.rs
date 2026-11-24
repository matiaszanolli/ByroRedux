//! Destruction events and ObScript revival.

use byroredux_core::ecs::{Scheduler, Stage};

/// Advance destruction before dispatching the ObScript stage-change blocks.
pub(super) fn register_destruction_and_revival_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::destruction::self_damage_system);
    // M47.3 — `OnDestructionStageChange` for the objects a `DamageObject` moved to another stage.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_destruction_system);
    // M47.3 — `Resurrect`: bring the actors back that scripts named this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::revive::revive_system);
}
