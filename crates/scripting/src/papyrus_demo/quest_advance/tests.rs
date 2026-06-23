//! Behavioural tests for `quest_advance_system`.
//!
//! Each test pins one Papyrus semantic from `DA10MainDoorScript.psc`
//! against the ECS-side translation. The integration boundary is
//! the [`crate::events::ActivateEvent`] marker + the
//! [`crate::quest_stages::QuestStageState`] resource — the system
//! reads one, writes the other, and emits a
//! [`crate::quest_stages::QuestStageAdvanced`] marker.

use super::*;
use crate::events::{ActivateEvent, OnTriggerEnterEvent};
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageAdvanced, QuestStageState};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

const DA10_QUEST_FORM_ID: QuestFormId = QuestFormId(0x000DEAD0);

/// Spin up a world with scripting + papyrus_demo registered + a
/// player entity wired into [`PlayerEntity`] + an empty
/// [`QuestStageState`] resource, plus a "DA10 main door" entity
/// carrying the DA10 component preset.
fn setup_da10_world() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    crate::papyrus_demo::register(&mut world);
    world.insert_resource(QuestStageState::default());

    let player = world.spawn();
    world.insert_resource(PlayerEntity(player));

    let door = world.spawn();
    world.insert(door, da10_main_door(DA10_QUEST_FORM_ID));

    (world, player, door)
}

fn fire_activate(world: &World, target: EntityId, activator: EntityId) {
    let mut q = world.query_mut::<ActivateEvent>().unwrap();
    q.insert(target, ActivateEvent { activator });
}

fn fire_trigger_enter(world: &World, target: EntityId, triggerer: EntityId) {
    let mut q = world.query_mut::<OnTriggerEnterEvent>().unwrap();
    q.insert(target, OnTriggerEnterEvent { triggerer });
}

// ── Stage-predicate gating ────────────────────────────────────

/// Both pre-conditions wrong — `GetStageDone(37) == 0`, so the
/// `(GetStageDone(37) == 1) && …` predicate fails at the first
/// conjunct. SetStage must NOT fire.
#[test]
fn da10_no_advance_when_required_stage_not_done() {
    let (world, player, door) = setup_da10_world();

    fire_activate(&world, door, player);
    quest_advance_system(&world);

    // Quest never had stage 37 set; state unchanged.
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 0);
    assert!(!stage_state.get_stage_done(DA10_QUEST_FORM_ID, 40));
    drop(stage_state);
    // No QuestStageAdvanced emitted.
    assert!(!world.has::<QuestStageAdvanced>(player));
}

/// Required stage done, forbidden stage NOT done → predicates
/// satisfied. SetStage(40) must fire.
#[test]
fn da10_advances_when_stage_37_done_and_stage_40_not_done() {
    let (world, player, door) = setup_da10_world();
    // Simulate prior quest progression: SetStage(37) happened
    // before the player ever touched this door.
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }

    fire_activate(&world, door, player);
    quest_advance_system(&world);

    // Quest advanced to 40.
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 40);
    assert!(stage_state.get_stage_done(DA10_QUEST_FORM_ID, 37));
    assert!(stage_state.get_stage_done(DA10_QUEST_FORM_ID, 40));
    drop(stage_state);
    // Marker event emitted on the player entity with the right
    // pre-image / post-image.
    let ev = world
        .get::<QuestStageAdvanced>(player)
        .expect("QuestStageAdvanced marker must land on player");
    assert_eq!(ev.quest, DA10_QUEST_FORM_ID);
    assert_eq!(ev.previous_stage, 37);
    assert_eq!(ev.new_stage, 40);
}

/// Required stage done AND forbidden stage already done → the
/// `(GetStageDone(40) == 0)` predicate fails. SetStage must NOT
/// fire (the Papyrus idempotency idiom). This pins the
/// "don't advance twice" guard.
#[test]
fn da10_idempotent_when_target_stage_already_done() {
    let (world, player, door) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
        stage_state.set_stage(DA10_QUEST_FORM_ID, 40);
    }
    // Clear the marker the second set_stage emitted in setup.
    if world.has::<QuestStageAdvanced>(player) {
        world
            .query_mut::<QuestStageAdvanced>()
            .unwrap()
            .remove(player);
    }

    fire_activate(&world, door, player);
    quest_advance_system(&world);

    // current_stage stays at 40 (no re-write because the predicate
    // gated the call). stages_done still has both 37 and 40.
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 40);
    assert!(stage_state.get_stage_done(DA10_QUEST_FORM_ID, 40));
    drop(stage_state);
    // No new marker emitted.
    assert!(!world.has::<QuestStageAdvanced>(player));
}

// ── Activator gate ────────────────────────────────────────────

/// DA10 uses `ActivatorGate::Any` (the .psc has no player-only
/// guard). NPC activation must therefore also fire the advance.
#[test]
fn da10_any_activator_can_advance() {
    let (mut world, _player, door) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }

    let npc = world.spawn();
    fire_activate(&world, door, npc);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(
        stage_state.get_stage(DA10_QUEST_FORM_ID),
        40,
        "ActivatorGate::Any must accept non-player activators"
    );
}

/// Companion: when the component overrides to `PlayerOnly` (the
/// MG07 / TG05 pattern), non-player activation must be filtered.
#[test]
fn player_only_gate_filters_non_player_activator() {
    let (mut world, player, door) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }
    // Override the gate.
    {
        let mut q = world.query_mut::<QuestAdvanceOnActivate>().unwrap();
        q.get_mut(door).unwrap().activator_gate = ActivatorGate::PlayerOnly;
    }

    // NPC activation — must NOT advance.
    let npc = world.spawn();
    fire_activate(&world, door, npc);
    quest_advance_system(&world);
    {
        let stage_state = world.resource::<QuestStageState>();
        assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 37);
    }

    // Re-fire as player — now advances.
    fire_activate(&world, door, player);
    quest_advance_system(&world);
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 40);
}

// ── No-precondition long tail ────────────────────────────────

/// Some scripts (the auto-generated `[A-Z]F_*FragmentScript.pex`
/// scene fragments) have empty `conditions` — they advance
/// unconditionally on activate. M47.1 pin: the evaluator returns
/// `true` on an empty `ConditionList`, preserving the vacuous-true
/// reduction the bespoke `require_done` / `forbid_done` vecs had.
#[test]
fn empty_predicates_advance_unconditionally() {
    let (world, player, door) = setup_da10_world();
    {
        let mut q = world.query_mut::<QuestAdvanceOnActivate>().unwrap();
        let comp = q.get_mut(door).unwrap();
        comp.conditions.clear();
        comp.target_stage = 20;
    }

    fire_activate(&world, door, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 20);
}

// ── Trigger-enter dispatch ────────────────────────────────────

/// The trigger-volume path: an `OnTriggerEnterEvent` advances the
/// quest through the same component + system as `ActivateEvent`. This
/// is the `default*Trigger` family's runtime contract — an actor
/// crossing a trigger volume fires the advance.
#[test]
fn trigger_enter_advances_quest() {
    let (world, player, trigger) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }

    fire_trigger_enter(&world, trigger, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(
        stage_state.get_stage(DA10_QUEST_FORM_ID),
        40,
        "an OnTriggerEnterEvent must drive the advance like an activate"
    );
}

/// The activator gate applies on the trigger path too: a `PlayerOnly`
/// volume ignores a non-player triggerer (an NPC patrol crossing it).
#[test]
fn trigger_enter_respects_player_only_gate() {
    let (mut world, player, trigger) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }
    {
        let mut q = world.query_mut::<QuestAdvanceOnActivate>().unwrap();
        q.get_mut(trigger).unwrap().activator_gate = ActivatorGate::PlayerOnly;
    }

    // NPC crosses the volume — must NOT advance.
    let npc = world.spawn();
    fire_trigger_enter(&world, trigger, npc);
    quest_advance_system(&world);
    {
        let stage_state = world.resource::<QuestStageState>();
        assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 37);
    }

    // Player crosses — advances.
    fire_trigger_enter(&world, trigger, player);
    quest_advance_system(&world);
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 40);
}

/// Unconditional advance on the trigger path: a trigger volume whose
/// component has empty `conditions` advances on enter (the empty list is
/// vacuously satisfied), mirroring the activate-side unconditional case.
#[test]
fn trigger_enter_unconditional_advances() {
    let (world, player, trigger) = setup_da10_world();
    {
        let mut q = world.query_mut::<QuestAdvanceOnActivate>().unwrap();
        let comp = q.get_mut(trigger).unwrap();
        comp.conditions.clear();
        comp.target_stage = 12;
    }

    fire_trigger_enter(&world, trigger, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 12);
}

/// Both signals in one frame on two different entities: an activated door
/// and an entered trigger, each advancing its own quest. The unified
/// system must process both event sources in a single pass.
#[test]
fn activate_and_trigger_in_same_frame_both_advance() {
    let (mut world, player, door) = setup_da10_world();
    let other_quest = QuestFormId(0x000B_EEF0);

    // A second entity: an unconditional trigger advancing a different quest.
    let trigger = world.spawn();
    {
        let mut comp = da10_main_door(other_quest);
        comp.conditions.clear();
        comp.target_stage = 5;
        world.insert(trigger, comp);
    }
    // Satisfy the door's predicate (it needs stage 37 done).
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }

    fire_activate(&world, door, player);
    fire_trigger_enter(&world, trigger, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(
        stage_state.get_stage(DA10_QUEST_FORM_ID),
        40,
        "the activated door advanced its quest"
    );
    assert_eq!(
        stage_state.get_stage(other_quest),
        5,
        "the entered trigger advanced its (separate) quest in the same pass"
    );
}

// ── Cross-quest isolation ────────────────────────────────────

/// Two separate quests must not influence each other through this
/// component — stage state is per-quest.
#[test]
fn separate_quests_do_not_alias_stage_state() {
    let (world, player, door) = setup_da10_world();
    let other_quest = QuestFormId(0x000CAFE0);
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(other_quest, 37); // wrong quest set 37
    }

    fire_activate(&world, door, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    // DA10 quest has nothing done — predicate fails, no advance.
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 0);
    assert_eq!(
        stage_state.get_stage(other_quest),
        37,
        "other quest untouched"
    );
}

// ── Two doors, one quest ─────────────────────────────────────

/// If two REFRs both carry the DA10 component (e.g., the door is
/// duplicated into a "fallback" mesh during the quest), the FIRST
/// activation advances the quest, and subsequent activations are
/// guarded by the `forbid_done: [40]` idempotency. Pins that the
/// system processes all events in one pass but the second event
/// sees the already-advanced state.
///
/// Note: today the system reads stage_state once per pass — so
/// within a single system invocation both activations see the
/// SAME (pre-pass) stage_state. Both pass the predicate. Both
/// write SetStage(40). The second write is the no-op the Papyrus
/// `SetStage` idempotency idiom would also produce
/// (current_stage already 40 → re-set is a write of the same
/// value). Both emit QuestStageAdvanced markers but the second
/// has `previous_stage == 40` (the within-pass intermediate
/// value), exposing the same-frame collision.
///
/// Test exists primarily to document this edge case for the
/// M47.0 fragment dispatcher — fragments must be idempotent
/// across same-frame duplicate advances.
#[test]
fn two_doors_same_quest_advance_in_one_pass() {
    let (mut world, player, door) = setup_da10_world();
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(DA10_QUEST_FORM_ID, 37);
    }
    let door2 = world.spawn();
    world.insert(door2, da10_main_door(DA10_QUEST_FORM_ID));

    fire_activate(&world, door, player);
    fire_activate(&world, door2, player);
    quest_advance_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(DA10_QUEST_FORM_ID), 40);
    assert!(stage_state.get_stage_done(DA10_QUEST_FORM_ID, 40));
    // The marker collapsed to one — the SparseSet stores the most
    // recent insert under the same key. Documented behaviour, not
    // a bug — the cleanup-pass approach to events doesn't naturally
    // accumulate, which is fine because stage-advances are
    // idempotent.
    let ev = world.get::<QuestStageAdvanced>(player).unwrap();
    assert_eq!(ev.quest, DA10_QUEST_FORM_ID);
    assert_eq!(ev.new_stage, 40);
}
