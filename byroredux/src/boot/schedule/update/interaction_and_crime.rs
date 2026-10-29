//! Vitals upkeep, canonical player interaction, activation flush, loot / jail / evidence, crime: the producer side of the activation contract.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use super::*;

/// Vitals upkeep, canonical player interaction, activation flush, loot / jail / evidence, crime: the producer side of the activation contract.
pub(super) fn register_interaction_and_crime(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::restoration::restoration_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::TimedRestorations>(),
    );
    // M47.2 — essential actors get back up when their bleedout runs out.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        byroredux_scripting::bleedout_system,
        Access::new()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // M47.2 — the player's carried weight against Carry Weight; the
    // movement effect reads it next frame.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::encumbrance::encumbrance_system,
        Access::new()
            .reads_resource::<crate::encumbrance::EncumbranceRules>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes_resource::<crate::encumbrance::PlayerEncumbrance>(),
    );
    // Canonical player interaction runs before every OnActivate consumer so
    // a fresh E-key edge is visible to scripts in the same frame.
    //
    // #3473 — the three P2 gameplay exclusives below were added after #2391
    // and inherited plain `add_exclusive`, so the newest and least-reviewed
    // subsystem reported blank `sys.accesses` rows while carrying the
    // deepest hold stack in the schedule (`combat_input_system`'s
    // `EquippedWeapon` -> CHARAL chain, since flattened in `combat.rs`).
    // Same rationale as `pool_regen_tick_system` below: the analyzer never
    // pairs exclusives, so these declarations surface no conflict row —
    // their job is to put the disputed types on the report instead of a
    // blank row, and to give `BYRO_LOCK_ORDER_CHECK`'s recorded edges a
    // declaration to be compared against if either system is ever promoted
    // to a parallel lane.
    //
    // Scope: each declaration covers the system body and the helpers it
    // calls directly (including `queue_door_transition`'s cell-index /
    // plugin-set / transition-slot triple). It deliberately stops at
    // `reconcile_dead_actor`'s ragdoll activation, whose own physics
    // surface is declared by `ragdoll_writeback_system`.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::interaction::interaction_system,
        Access::new()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .writes_resource::<byroredux_scripting::ReferenceLockState>()
            .writes::<byroredux_core::ecs::components::Locked>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_physics::ActorColliderOwner>()
            .reads::<byroredux_physics::Ragdoll>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            // #5035 — the Npc Talk arm's bulk passes: the candidate filter
            // queries `ActorValues` directly, and
            // `running_quest_bound_entities` reads the quest-alias tables
            // (`installed_alias_ids_by_quest` + the running-quest filter).
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            // #5043 / #5017 — the Talk arm skips actors in combat or
            // unconscious.
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<byroredux_scripting::ActorControlState>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .reads_resource::<ActionState>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            // #4574 — collider_belongs_to_target reads the source form
            // (reached through target_has_line_of_sight); the same
            // helper's FormIdComponent and ActorColliderOwner are declared
            // above.
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads_resource::<crate::cell_loader::LoadedPluginSet>()
            .writes_resource::<InteractionState>()
            .writes_resource::<InteractionTrace>()
            .writes_resource::<InteractionCandidateScratch>()
            .writes_resource::<crate::cell_loader::PendingCellTransitionSlot>()
            .reads::<Transform>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_core::ecs::WorldBound>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<crate::components::DoorTeleport>()
            .reads::<byroredux_core::ecs::components::Locked>()
            .reads::<byroredux_scripting::TwoStateActivator>()
            .reads::<byroredux_scripting::papyrus_demo::RumbleOnActivate>()
            .reads::<byroredux_scripting::papyrus_demo::quest_advance::QuestAdvanceOnActivate>()
            .reads::<byroredux_scripting::papyrus_demo::mg07_door::MG07LabyrinthianDoor>()
            // #4697 — pickup candidates skip taken placements; #4698 —
            // disabled placements are filtered against the enable ledger.
            .reads::<crate::inventory::PickedUp>()
            .reads_resource::<byroredux_scripting::ReferenceEnableState>()
            // #6077 — and through the enable-parent chain.
            .reads_resource::<byroredux_scripting::AuthoredDisabledReferences>()
            .writes::<byroredux_scripting::ActivateEvent>(),
    );
    // #2654 — quest fragments queue their `<Ref>.Activate()` targets rather
    // than inserting `ActivateEvent` directly, because
    // `quest_fragment_dispatch` runs *after* most of the consumers below (it
    // has to: it consumes the `QuestStageAdvanced` markers
    // `quest_advance_dispatch` emits) and `event_cleanup_system` drains the
    // marker at Stage::Late the same frame. Flush the queue here, directly
    // after the canonical player-interaction producer, so a fragment
    // activation reaches every consumer exactly once on the following frame.
    // #4712 — that includes `container_loot_system` immediately below: a
    // flush registered after it turned a scripted `ContainerRef.Activate()`
    // or loose-item `Activate()` into a silent no-op.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_activation_flush_system,
    );
    // #5868 — a scripted / relayed activation of a load door by the player
    // opens it (the E-press path queues its own transition directly).
    scheduler.add_exclusive(Stage::Update, crate::interaction::door_activation_system);
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
            .reads_resource::<crate::components::CellLightingRes>(),
    );
    // Resting in a jail cell's bed ends the sentence; after the activation flush like every consumer.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::crime::jail_bed_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_core::ecs::components::Furniture>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<byroredux_scripting::obscript_more::PendingSleep>()
            .writes_resource::<byroredux_scripting::PlayerMoveRequest>()
            .writes_resource::<crate::notifications::PlayerNotifications>(),
    );
    // Confiscated goods reach the jail's `StolenGoods` chest, loaded or parked.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::crime::evidence_deposit_system,
        Access::new()
            .writes_resource::<byroredux_scripting::jail::PendingEvidenceDeposits>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes::<byroredux_core::ecs::components::Inventory>(),
    );
}
