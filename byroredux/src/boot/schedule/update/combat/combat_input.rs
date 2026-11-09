//! Combat input: the player's attack and hit-contract entry.
//!
//! Split out of `combat.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, Transform};
use crate::interaction::ActionState;

/// Combat input: the player's attack and hit-contract entry.
pub(super) fn register_combat_input_systems(scheduler: &mut Scheduler) {
    // Combat follows the same producer-before-consumer event contract as
    // activation: physical Attack emits HitEvent, then health/death resolves
    // before script consumers and Late-stage transient cleanup.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_input_system,
        Access::new()
            // PERK-SUB-002 — the outgoing-damage composition: its table, the owner's perks, the
            // `Add Range` draw, and the scratch weapon subject it lends a base form to.
            .reads_resource::<byroredux_scripting::perk_entry_points::PerkEntryTable>()
            .reads::<byroredux_core::character::Perks>()
            .writes_resource::<byroredux_scripting::story_manager::StoryManagerRng>()
            .reads_resource::<byroredux_scripting::perk_entry_points::WeaponSubject>()
            // PERK-HOOK-008 — an applied FO4 entry with a bound fragment reports its run (`PendingPerkEntries`),
            // looking the owner's reference id up first.
            .reads_resource::<byroredux_scripting::PerkFragments>()
            .writes_resource::<byroredux_scripting::PendingPerkEntries>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes::<byroredux_scripting::SceneAliasCandidate>()
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
}
