//! The canonical player-interaction producer, with its declared access.
//!
//! Split out of `interaction_and_crime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, Transform};
use crate::interaction::{ActionState, InteractionCandidateScratch, InteractionState, InteractionTrace};

/// The canonical player-interaction producer, with its declared access.
pub(super) fn register_interaction_systems(scheduler: &mut Scheduler) {
    register_interaction_system(scheduler);
}

/// The canonical player-interaction producer and its declared access. A helper of its own (#6546) so the
/// segment function stays within its size pin; it is called at the exact point in the registration order
/// the system always held.
fn register_interaction_system(scheduler: &mut Scheduler) {
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
            // The seat and ObScript `OnActivate` arms: a seat is a `Furniture` with a sitting
            // position (none while the player is `Seated`); an activator runs an object script
            // whose table says it has the block.
            .reads::<byroredux_core::ecs::components::Seated>()
            .reads::<byroredux_scripting::obscript_objects::ObScriptInstance>()
            .reads_resource::<byroredux_scripting::obscript_objects::ObScriptObjectTable>()
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
            .writes::<byroredux_scripting::ActivateEvent>()
            // #6546 — M43: the activation filters and the bench / lock paths.
            .reads_resource::<byroredux_scripting::activate_parents::ActivateParentTable>()
            .reads::<crate::components::PlacementContentWithheld>()
            // A bench activation asks for the crafting page (`request_crafting`) and reads the book.
            .reads_resource::<crate::crafting::CraftingBook>()
            .writes_resource::<crate::crafting::CraftingRequest>()
            // `unlock_with_carried_key` -> `lockpick::bypass_of`: the terminal bases, the pick item and the
            // `LOCK` / `HACK` roots; the events are raised through `raise_story_event`
            // (a marker, or the pending queue when the actor already holds one).
            .reads_resource::<crate::lockpick::TerminalBases>()
            .reads_resource::<crate::lockpick::LockpickItem>()
            .reads_resource::<byroredux_scripting::SmTree>()
            .writes::<byroredux_scripting::StoryEvent>()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>()
            // PERK-HOOK-007 / -012 — `perk_activate`: the player's activate entries are composed (the perk
            // table, the player's perks, the condition evaluator behind their tabs), a choice opens the box
            // (the script-message queue) and holds the activation, and what an entry carries runs: the
            // embedded script on the ObScript host, a fragment report, a spell cast.
            .reads_resource::<byroredux_scripting::perk_entry_points::PerkEntryTable>()
            .reads::<byroredux_core::character::Perks>()
            .writes_resource::<byroredux_scripting::PendingScriptMessages>()
            .writes_resource::<crate::perk_activate::PendingActivateChoice>()
            .reads_resource::<byroredux_scripting::PerkFragments>()
            .writes_resource::<byroredux_scripting::PendingPerkEntries>(),
    );
}
