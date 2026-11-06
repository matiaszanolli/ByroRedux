//! Faction hostility: who is hostile to whom this frame.
//!
//! Split out of `combat.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Faction hostility: who is hostile to whom this frame.
pub(super) fn register_faction_hostility_systems(scheduler: &mut Scheduler) {
    // #4414 — ambient faction hostility: an NPC that sees an actor its
    // faction relations and aggression say it attacks arms `AiCombatState`,
    // the same state `Effect::StartCombat` arms. Scheduled just before the
    // combat AI so a combat started this frame is fought this frame.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::make_faction_hostility_system(),
        Access::new()
            .reads_resource::<crate::systems::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<byroredux_scripting::FactionRelations>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<crate::systems::CombatDisposition>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_scripting::AiCombatState>()
            .writes::<byroredux_scripting::AiCombatState>()
            // #5017 — unconscious perceivers start no combat.
            .reads::<byroredux_scripting::ActorControlState>()
            // #4816 — the ambient-combat marker it stamps, ages and drops.
            .reads::<crate::systems::AmbientEngagement>()
            .writes::<crate::systems::AmbientEngagement>(),
    );
}
