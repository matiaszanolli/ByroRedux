//! Combat input, faction hostility, guard rush and NPC combat AI, in the order the hit contract needs them.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use super::*;

/// Combat input, faction hostility, guard rush and NPC combat AI, in the order the hit contract needs them.
pub(super) fn register_combat(scheduler: &mut Scheduler) {
    // Combat follows the same producer-before-consumer event contract as
    // activation: physical Attack emits HitEvent, then health/death resolves
    // before script consumers and Late-stage transient cleanup.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_input_system,
        Access::new()
            .reads_resource::<ActionState>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<byroredux_core::character::MeleeDamageConfig>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes_resource::<crate::combat::CombatState>()
            .reads::<Transform>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_physics::ActorColliderOwner>()
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_core::ecs::components::Dead>()
            // #4574 — the cooldown arm's own query_mut/query on MeleeState
            // (#3709) and attack_damage's CreatureAttack read.
            .reads::<crate::combat::MeleeState>()
            .writes::<crate::combat::MeleeState>()
            .reads::<byroredux_core::ecs::components::CreatureAttack>()
            .writes::<byroredux_scripting::HitEvent>(),
    );
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
    // MQ101's dragon-attack/keep-escape combat gate (stages 270+, ROADMAP.md
    // "MQ101 end-to-end playability"): `Effect::StartCombat` arms
    // `AiCombatState`; this system chases and strikes exactly like the
    // player's own `combat_input_system` above, so it shares the same
    // producer-before-consumer HitEvent contract — scheduled after the
    // player's own attack input and before `combat_damage_system` resolves
    // the frame's HitEvents.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::make_npc_combat_ai_system(),
        Access::new()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<byroredux_core::character::MeleeDamageConfig>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .reads::<Transform>()
            .writes::<Transform>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads::<byroredux_core::ecs::components::CreatureAttack>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_scripting::AiCombatState>()
            .writes::<byroredux_scripting::AiCombatState>()
            // #5017 — an unconscious attacker's combat ends.
            .reads::<byroredux_scripting::ActorControlState>()
            // #4574 — the chase arm reads the authored stride (M42.11).
            .reads::<crate::components::WalkSpeed>()
            // #4703 — each live attacker's ambient package is suspended
            // through `clear_ambient_behavior`: the runtime winner reset,
            // the seat released and its animation park undone, plus every
            // ambient Behavior/State pair that function removes.
            .reads::<crate::components::AmbientPackageRuntime>()
            .writes::<crate::components::AmbientPackageRuntime>()
            .reads::<byroredux_core::ecs::components::Seated>()
            .writes::<byroredux_core::ecs::components::Seated>()
            .writes_resource::<crate::components::SeatReservations>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            // #4821 — the pairs themselves, removed through the generic
            // `remove_component::<T>`, were claimed above but never declared.
            .writes::<byroredux_core::ecs::components::SandboxBehavior>()
            .writes::<byroredux_core::ecs::components::WanderBehavior>()
            .writes::<byroredux_core::ecs::components::WanderState>()
            .writes::<byroredux_core::ecs::components::TravelBehavior>()
            .writes::<byroredux_core::ecs::components::TravelState>()
            .writes::<byroredux_core::ecs::components::Traveled>()
            .writes::<byroredux_core::ecs::components::FollowBehavior>()
            .writes::<byroredux_core::ecs::components::FollowState>()
            .writes::<byroredux_core::ecs::components::EscortBehavior>()
            .writes::<byroredux_core::ecs::components::EscortState>()
            .writes::<byroredux_core::ecs::components::Escorted>()
            .writes::<byroredux_core::ecs::components::GuardBehavior>()
            .writes::<byroredux_core::ecs::components::GuardState>()
            .writes::<byroredux_core::ecs::components::FleePackageBehavior>()
            .writes::<byroredux_core::ecs::components::AmbushBehavior>()
            .writes::<byroredux_core::ecs::components::FindPackageBehavior>()
            .writes::<byroredux_core::ecs::components::FindProgress>()
            .writes::<byroredux_core::ecs::components::UseWeaponPackageBehavior>()
            .writes::<byroredux_core::ecs::components::UseWeaponPackageStarted>()
            .writes::<crate::systems::script_requests::UseWeaponRun>()
            .writes::<byroredux_core::ecs::components::PatrolBehavior>()
            .writes::<byroredux_core::ecs::components::PatrolState>()
            // M42 Eat/Sleep + the Dialogue force-greet bridge — same
            // `clear_ambient_behavior` teardown family as the pairs
            // above.
            .writes::<byroredux_core::ecs::components::EatBehavior>()
            .writes::<byroredux_core::ecs::components::SleepBehavior>()
            .writes::<byroredux_core::ecs::components::EatSleepState>()
            .writes::<crate::systems::forcegreet::ForceGreetDirective>()
            .writes::<crate::components::WalkStuckTimer>()
            .writes::<crate::components::NavPath>()
            .writes::<byroredux_scripting::HitEvent>(),
    );
}
