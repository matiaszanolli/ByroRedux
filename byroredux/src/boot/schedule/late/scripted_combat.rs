//! Script-driven experience, skill and kill requests, and the shared death reconciler.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Script-driven experience, skill and kill requests, and the shared death reconciler.
pub(super) fn register_scripted_combat_systems(scheduler: &mut Scheduler) {
    // M47.2 — script RewardPlayerXP grants, through the level roll.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::combat::scripted_experience_system,
        Access::new()
            .writes_resource::<byroredux_scripting::story_manager::PendingPlayerXp>()
            .reads_resource::<crate::combat::XpModifierRules>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes::<byroredux_core::character::CharacterLevel>()
            .writes::<byroredux_core::character::LevelUpPoints>()
            .reads::<byroredux_core::ecs::components::ActorValues>(),
    );
    // M47.2 — script Game.AdvanceSkill: skill usage XP, skill level-ups, and the
    // character XP a rank-up feeds through the same level roll.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::combat::scripted_skill_system,
        Access::new()
            .writes_resource::<byroredux_scripting::PendingSkillAdvances>()
            .reads_resource::<byroredux_scripting::ActorValueNames>()
            .reads_resource::<byroredux_scripting::SkillScaling>()
            .writes_resource::<byroredux_scripting::SkillProgress>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes::<byroredux_core::character::CharacterLevel>()
            .writes::<byroredux_core::character::LevelUpPoints>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // M47.2 — script Kill requests, ahead of the shared death reconciler.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::combat::scripted_kill_system,
        Access::new()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptKills>()
            .writes_resource::<crate::combat::PendingDeathReconciliations>()
            .writes::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_scripting::story_manager::StoryEvent>()
            .writes::<byroredux_scripting::OnDeathEvent>()
            .reads::<byroredux_scripting::ActorProtection>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>(),
    );
    // Water hazards and drowning can mark actors dead from different stages.
    // Reconcile their structural consequences only after both producers have
    // run, in this exclusive lane where AI removal and ragdoll activation are
    // legal. This is the same reconciler combat and save-load use (#3119).
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::combat::reconcile_pending_dead_actors_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::EquipmentSlots>()
            .writes::<byroredux_core::ecs::components::EquippedWeapon>()
            .writes_resource::<crate::combat::PendingDeathReconciliations>()
            .reads::<byroredux_core::ecs::components::Dead>()
            // #4574 — reconcile_dead_actor reads the target before the
            // ragdoll activation clears it.
            .reads::<crate::components::AnimationTarget>()
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
            .writes_resource::<byroredux_scripting::PendingPackageTransitions>(),
    );
    // The reconciler runs only after both death producers (#3119).
    scheduler.run_after(
        byroredux_core::ecs::system_name(&crate::combat::reconcile_pending_dead_actors_system),
        byroredux_core::ecs::system_name(&crate::systems::water_damage_system),
    );
    scheduler.run_after(
        byroredux_core::ecs::system_name(&crate::combat::reconcile_pending_dead_actors_system),
        byroredux_core::ecs::system_name(&crate::combat::scripted_kill_system),
    );
}
