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
            .with(byroredux_scripting::perk_entry_points::perk_entry_composition_access)
            .with(byroredux_scripting::perk_entry_points::perk_fragment_report_access)
            .reads_resource::<ActionState>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<byroredux_core::character::MeleeDamageConfig>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            // BOW-DRW-001 — `bow_in_hand`: a wielded bow is not a melee swing.
            .reads_resource::<byroredux_core::character::BowModel>()
            .reads_resource::<crate::components::LoadedCellIndex>()
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
            // #6942 — an attack draws the weapon and the Ready Weapon key draws or puts it away.
            .reads::<byroredux_scripting::obscript_state::WeaponDrawn>()
            .writes::<byroredux_scripting::obscript_state::WeaponDrawn>()
            .writes::<byroredux_scripting::HitEvent>(),
    );
    // Player-spellcasting (PSC-CST-009): the one system that owns the spend. It ticks the casting interval, keeps the
    // readied spell on the player's list, and on a `CastSpell` edge gates, applies the Magicka and Fatigue damage and
    // starts the effects through the shared `cast_spell`; `NextSpell` / `PreviousSpell` cycle the readied spell.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::player_cast::player_cast_system,
        Access::new()
            .reads_resource::<ActionState>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::components::VatsState>()
            .reads_resource::<byroredux_core::character::SpellCastModel>()
            .reads_resource::<byroredux_scripting::magic::SpellCatalog>()
            .reads_resource::<byroredux_scripting::ActorValueNames>()
            .writes_resource::<byroredux_scripting::player_cast::CastState>()
            .reads::<byroredux_scripting::magic::SpellList>()
            .writes::<byroredux_scripting::player_cast::ReadiedSpell>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            // `cast_spell` starts the effects and raises the Story Manager `CAST` event.
            .writes::<byroredux_scripting::magic_effects::ActiveMagicEffects>()
            .writes::<byroredux_scripting::story_manager::StoryEvent>(),
    );
    // Player-archery (BOW-DRW-005): the one system that owns the draw, the release and the spend. It reads the held
    // attack action, ticks the interval, and on the release edge gates, spends one arrow of the equipped type and
    // launches it on the shared projectile flight.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::bow::player_bow_system,
        Access::new()
            .reads_resource::<ActionState>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::components::VatsState>()
            .reads_resource::<byroredux_core::character::BowModel>()
            .reads_resource::<crate::components::LoadedCellIndex>()
            .reads_resource::<byroredux_core::settings::SettingsRegistry>()
            .reads_resource::<ActiveCamera>()
            .writes_resource::<crate::bow::BowState>()
            .writes_resource::<crate::systems::projectiles::ActiveProjectiles>()
            .reads::<Transform>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads::<byroredux_core::ecs::components::EquippedAmmo>()
            .writes::<byroredux_core::ecs::components::EquippedAmmo>()
            // `remove_items` spends the arrow.
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::Inventory>(),
    );
}
