//! NPC combat AI, with its declared access.
//!
//! Split out of `combat.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage, Transform};

/// NPC combat AI, with its declared access.
pub(super) fn register_npc_combat_ai_systems(scheduler: &mut Scheduler) {
    // MQ101's dragon-attack/keep-escape combat gate (stages 270+, ROADMAP.md
    // "MQ101 end-to-end playability"): `Effect::StartCombat` arms
    // `AiCombatState`; this system chases and strikes exactly like the
    // player's own `combat_input_system` above, so it shares the same
    // producer-before-consumer HitEvent contract — scheduled after the
    // player's own attack input and before `combat_damage_system` resolves
    // the frame's HitEvents.
    register_npc_combat_ai(scheduler);
}

/// The NPC combat AI row and its declared access. A helper of its own so `register_combat` stays
/// within its size pin; it is called at the exact point in the registration order the system needs.
fn register_npc_combat_ai(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::make_npc_combat_ai_system(),
        Access::new()
            // PERK-SUB-002 — the outgoing-damage composition: its table, the owner's perks, the
            // `Add Range` draw, and the scratch weapon subject it lends a base form to.
            .reads_resource::<byroredux_scripting::perk_entry_points::PerkEntryTable>()
            .reads::<byroredux_core::character::Perks>()
            .writes_resource::<byroredux_scripting::story_manager::StoryManagerRng>()
            .reads_resource::<byroredux_scripting::perk_entry_points::WeaponSubject>()
            .writes::<byroredux_scripting::SceneAliasCandidate>()
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
