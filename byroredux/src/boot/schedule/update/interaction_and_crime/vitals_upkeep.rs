//! Vitals upkeep: restoration, bleedout and encumbrance.
//!
//! Split out of `interaction_and_crime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Vitals upkeep: restoration, bleedout and encumbrance.
pub(super) fn register_vitals_upkeep_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::restoration::restoration_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::TimedRestorations>(),
    );
    // M47.2 — essential actors get back up when their bleedout runs out.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        byroredux_scripting::bleedout_system,
        Access::new()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // M47.2 — the player's carried weight against Carry Weight; the
    // movement effect reads it next frame.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::encumbrance::encumbrance_system,
        Access::new()
            .reads_resource::<crate::encumbrance::EncumbranceRules>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes_resource::<crate::encumbrance::PlayerEncumbrance>(),
    );
}
