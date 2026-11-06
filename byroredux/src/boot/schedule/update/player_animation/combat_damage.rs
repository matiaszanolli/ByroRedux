//! Combat damage, with its declared access.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Combat damage, with its declared access.
pub(super) fn register_combat_damage_systems(scheduler: &mut Scheduler) {
    register_combat_damage_system(scheduler);
}

/// The damage pass and its declared access. A helper of its own (#6546) so `register_player_and_animation`
/// stays within its size pin; it is called at the exact point in the registration order the system always held.
fn register_combat_damage_system(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_damage_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::EquipmentSlots>()
            .writes::<byroredux_core::ecs::components::EquippedWeapon>()
            .writes_resource::<crate::combat::CombatState>()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<crate::components::AnimationTarget>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::Dead>()
            // M47.2 — spare_from_death: essential / protected actors bleed out.
            .reads::<byroredux_scripting::ActorProtection>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            // PERK-SUB-002 — the Damage Threshold composition: its table, the owners' perks, the
            // `Add Range` draw, and the scratch weapon subject it lends a base form to.
            .reads_resource::<byroredux_scripting::perk_entry_points::PerkEntryTable>()
            .reads::<byroredux_core::character::Perks>()
            .writes_resource::<byroredux_scripting::story_manager::StoryManagerRng>()
            .reads_resource::<byroredux_scripting::perk_entry_points::WeaponSubject>()
            .writes::<byroredux_scripting::SceneAliasCandidate>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            // M47.2 — reconcile_dead_actor's note_actor_death (cleared locations).
            .reads_resource::<byroredux_scripting::LocationTable>()
            .writes_resource::<byroredux_scripting::LocationClearState>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            .writes::<byroredux_core::animation::AnimationStack>()
            // #5861 / #5870 — reconcile_dead_actor drops the corpse's behavior
            // graph runtime and reports its package ending.
            .writes::<byroredux_scripting::ObjectAnimationGraph>()
            .writes::<crate::systems::GraphLocomotion>()
            .writes::<crate::systems::ConsoleWalk>()
            // #6288 / #6289 — death ends the actor's magic effects and its forced head-look.
            .writes::<byroredux_scripting::magic_effects::ActiveMagicEffects>()
            .writes::<byroredux_scripting::obscript_anim::HeadLook>()
            .writes_resource::<byroredux_scripting::ActivePackages>()
            .writes_resource::<byroredux_scripting::PendingPackageTransitions>()
            // `crime::report_strike` — the assault / murder bounty, its witnesses, and the
            // hostility judgement of the victim.
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_core::character::CrimeModel>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .writes::<crate::crime::AssaultedByPlayer>()
            // `crime::raise_alarm` / `record_crime` — who saw a crime, and the guards a witness calls.
            .writes_resource::<byroredux_scripting::crime_knowledge::CrimeKnowledge>()
            .reads_resource::<byroredux_scripting::guards::GuardClasses>()
            .reads::<byroredux_core::character::Background>()
            .writes::<crate::systems::forcegreet::ForceGreetDirective>()
            // `crime::charge_crime_factions` — Skyrim's per-hold bounty: the witnesses' crime factions.
            .reads_resource::<byroredux_scripting::crime_faction::CrimeFactionData>()
            .reads_resource::<byroredux_scripting::CrimeFactions>()
            .reads_resource::<byroredux_scripting::CrimeFactionOverrides>()
            .writes_resource::<byroredux_scripting::FactionCrimeGold>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<crate::systems::faction_hostility::CombatDisposition>()
            .reads_resource::<crate::systems::faction_hostility::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<byroredux_scripting::FactionRelations>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<byroredux_physics::RapierHandles>()
            // #6546 — what M43 and the XP / threshold work made the damage pass acquire.
            // `armor::mitigate`: the armor model's tables and each worn piece's smithing quality.
            .reads_resource::<crate::armor::ArmorBook>()
            .reads_resource::<byroredux_core::ecs::resources::ItemInstancePool>()
            .reads_resource::<byroredux_scripting::ActorValueNames>()
            .reads_resource::<crate::combat::DamageThresholdRules>()
            .reads::<byroredux_scripting::HitShots>()
            .reads::<crate::ragdoll::RagdollActive>()
            // Kill XP: the rules, the damage the player dealt, and the level / points they raise.
            .reads_resource::<crate::combat::ExperienceRules>()
            .writes_resource::<crate::combat::PlayerDamageDealt>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes::<byroredux_core::character::CharacterLevel>()
            .writes::<byroredux_core::character::LevelUpPoints>()
            // A struck actor's combat state and the ambient package it drops (`clear_ambient_behavior`).
            .writes::<byroredux_scripting::AiCombatState>()
            .writes::<crate::components::AmbientPackageRuntime>()
            .writes::<byroredux_scripting::package::EvaluatePackageRequest>()
            // `ASSU` / `KILL` (`raise_story_event`: a marker, or the pending queue on a collision).
            .writes::<byroredux_scripting::StoryEvent>()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>(),
    );
}
