//! Guards rushing a witnessed crime.
//!
//! Split out of `combat.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Guards rushing a witnessed crime.
pub(super) fn register_guard_rush_systems(scheduler: &mut Scheduler) {
    // Guards who see a wanted player come for them: stamps the force-greet directive, so it runs
    // before the force-greet system below.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::crime::make_guard_rush_system(),
        Access::new()
            .reads_resource::<byroredux_core::character::CrimeModel>()
            .reads_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .reads_resource::<byroredux_scripting::guards::GuardClasses>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::npc_dialogue::DialogueSurfaceState>()
            .reads_resource::<crate::systems::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<crate::systems::CombatDisposition>()
            .reads::<byroredux_core::character::Background>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_scripting::AiCombatState>()
            .writes::<crate::systems::forcegreet::ForceGreetDirective>()
    );
}
