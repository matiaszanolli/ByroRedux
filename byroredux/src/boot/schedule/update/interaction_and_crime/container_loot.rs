//! Container and corpse loot.
//!
//! Split out of `interaction_and_crime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Container and corpse loot.
pub(super) fn register_container_loot_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::inventory::container_loot_system,
        Access::new()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::ecs::components::EquipmentSlots>()
            .writes::<byroredux_core::ecs::components::EquippedWeapon>()
            .writes::<byroredux_scripting::EquipmentEventBatch>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads::<byroredux_core::ecs::components::Locked>()
            // #4574 — pickup_loot stamps PickedUp on the target and its
            // subtree meshes; theft_owner reads ownership.
            .writes::<crate::inventory::PickedUp>()
            .reads::<byroredux_core::ecs::components::Owned>()
            // #5866 — the theft rule reads a script's ownership change first.
            .reads_resource::<byroredux_scripting::OwnerOverrides>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            // #4706 — the placed stack size pickup_loot grants.
            .reads::<crate::inventory::PlacedItemCount>()
            .writes::<byroredux_core::ecs::components::Inventory>()
            // #4996 — pickup_loot's helpers. `mesh_entities_under` walks the
            // subtree; `mark_picked_up` resolves the ref's FormId and records
            // it; `remove_collision_bodies` (#4818) matches the ref's Havok
            // bodies by source form and removes them from Rapier. Exclusive,
            // so no race today — but the row must be complete for the
            // analyzer if this ever joins a parallel batch.
            .reads::<byroredux_core::ecs::Children>()
            .reads::<byroredux_core::ecs::MeshHandle>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>()
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads::<byroredux_physics::RapierHandles>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            // `crime::report_theft` — the bounty, its witnesses and the sight test between them.
            .reads_resource::<byroredux_core::character::CrimeModel>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .reads_resource::<byroredux_scripting::OwnerKinds>()
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
            .reads::<crate::systems::faction_hostility::CombatDisposition>()
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads_resource::<crate::systems::faction_hostility::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            // #6546 — `AIPL` (loot.rs): `raise_story_event`'s marker, or its pending queue.
            .writes::<byroredux_scripting::StoryEvent>()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>(),
    );
}
