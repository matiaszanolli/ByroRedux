//! M65 (SNK-DET-001) — the detection tick, the first `Stage::Update` segment.
//!
//! Every consumer of a level toward the player (witnesses in interaction and
//! crime, the strike's `sneak_attack` in combat input, hostility and guard rush,
//! dead-body discovery, flee) runs later in Update and reads this frame's
//! levels (`docs/engine/sneak-detection.md` §4.1).

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// The detection tick.
pub(super) fn register_detection(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::detection::detection_tick_system,
        Access::new()
            .reads_resource::<byroredux_core::stealth::StealthRules>()
            // M65 Phase 4 — the indicator table the fold reads (SNK-HUD-002).
            .reads_resource::<byroredux_core::stealth::StealthIndicator>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::interaction::ActionState>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<crate::detection::DetectionConfig>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<byroredux_scripting::FactionRelations>()
            .writes_resource::<crate::detection::PlayerActionNoise>()
            .writes_resource::<crate::detection::PlayerDetection>()
            .writes_resource::<byroredux_scripting::detection_events::PendingDetectionEvents>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Sneaking>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::LightSource>()
            .reads::<byroredux_core::ecs::components::SleepBehavior>()
            .reads::<byroredux_core::ecs::components::EquipmentSlots>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_scripting::AiCombatState>()
            // SNK-DET-014 — `SetNotShowOnStealthMeter` keeps a detector out of the fold.
            .reads::<byroredux_scripting::ActorControlState>()
            .reads::<crate::systems::CombatDisposition>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_core::ecs::components::Awareness>()
            .writes::<byroredux_core::ecs::components::Awareness>(),
    );
}
