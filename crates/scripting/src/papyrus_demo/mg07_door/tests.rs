//! Behavioural tests for the MG07LabyrinthianDoor translation.
//!
//! Each test pins one branch of the .psc source. The headline test
//! ([`success_path_fires_cross_reference_activate_on_my_door`])
//! exercises R5's third-axis criterion end-to-end: a script's
//! handler dispatches a method call to another reference, the
//! ECS-side translation collapses the call into an `ActivateEvent`
//! insert on the target entity, and the target's own systems pick
//! it up on the next frame.

use super::*;
use crate::events::ActivateEvent;
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

const MG07_QUEST: QuestFormId = QuestFormId(0x000A610C);
const DENIAL_MSG_FORM_ID: u32 = 0x000A6111;
const DELAY: f32 = 1.0;

/// Spawn: a world with scripting + papyrus_demo registered, an
/// empty QuestStageState, a player entity with a `KeystoneInventory`,
/// the MG07 keystone-door entity, and a "secret door" target entity
/// (with no script — purely an ActivateEvent sink for the test).
fn setup_world() -> (World, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    crate::papyrus_demo::register(&mut world);
    world.insert_resource(QuestStageState::default());

    let player = world.spawn();
    world.insert(player, KeystoneInventory::default());
    world.insert_resource(PlayerEntity(player));

    let secret_door = world.spawn(); // "myDoor" in the Papyrus source

    let keystone_door = world.spawn();
    world.insert(
        keystone_door,
        MG07LabyrinthianDoor {
            mg07_quest: MG07_QUEST,
            my_door: secret_door,
            delay_after_insert: DELAY,
            denial_message_form_id: DENIAL_MSG_FORM_ID,
            been_opened: false,
            state: MG07State::Uninitialized,
            activation_blocked: false,
            disabled: false,
        },
    );

    (world, player, keystone_door, secret_door)
}

fn fire_activate(world: &World, target: EntityId, activator: EntityId) {
    world
        .query_mut::<ActivateEvent>()
        .unwrap()
        .insert(target, ActivateEvent { activator });
}

fn give_keystone(world: &World, player: EntityId, give: bool) {
    world
        .query_mut::<KeystoneInventory>()
        .unwrap()
        .get_mut(player)
        .unwrap()
        .has_mg07_keystone = give;
}

fn advance_quest_to_stage(world: &World, stage: u16) {
    world
        .resource_mut::<QuestStageState>()
        .set_stage(MG07_QUEST, stage);
}

// ── OnLoad branches ────────────────────────────────────────────

#[test]
fn on_load_blocks_activation_when_not_yet_opened() {
    let (world, _player, keystone_door, _secret_door) = setup_world();

    mg07_on_load_system(&world);

    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert_eq!(door.state, MG07State::Waiting, "OnLoad transitions to waiting");
    assert!(
        door.activation_blocked,
        "fresh `beenOpened=False` arm sets blockActivation(true)"
    );
    assert!(!door.disabled);
}

#[test]
fn on_load_disables_when_already_opened() {
    let (world, _player, keystone_door, _secret_door) = setup_world();
    // Pretend we loaded from save — beenOpened persisted as true.
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().been_opened = true;
    }

    mg07_on_load_system(&world);

    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert_eq!(door.state, MG07State::Waiting);
    assert!(
        door.disabled,
        "post-open arm sets Self.disable() so the door doesn't re-render"
    );
    assert!(!door.activation_blocked);
}

#[test]
fn on_load_is_idempotent() {
    // Papyrus's OnLoad fires once per cell-load, not every frame.
    // Our shape is "Uninitialized → first run" — subsequent runs
    // are no-ops because state is no longer Uninitialized.
    let (world, _player, keystone_door, _secret_door) = setup_world();

    mg07_on_load_system(&world);
    let state_after_first = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap().state;

    // Force an artificial perturbation between runs to prove
    // re-invocation doesn't reset.
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }

    mg07_on_load_system(&world);
    let state_after_second = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap().state;
    assert_eq!(state_after_first, state_after_second);
    // Perturbation wasn't undone — re-run was a no-op.
    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert!(!door.activation_blocked);
}

// ── BlockActivation enforcement ────────────────────────────────

#[test]
fn blocked_activation_swallows_events_pre_unblock() {
    // While activation_blocked=true (immediately after OnLoad with
    // beenOpened=false), even the player with stage 10 done + the
    // keystone in hand can't progress. Pin this — the gate is
    // explicit, not just predicate-driven.
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);

    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert_eq!(
        door.state,
        MG07State::Waiting,
        "blocked door must stay Waiting — no Inserting transition"
    );
    // No UI message either — the predicate path didn't run.
    assert!(!world.has::<UiMessageCommand>(player));
}

// ── Denial-message branch ──────────────────────────────────────

#[test]
fn denial_branch_emits_ui_message_when_quest_stage_not_done() {
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    // Unblock — so the predicate is what gates.
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    // Stage 10 not done. Even if the keystone is present, the
    // predicate fails on the stage check.
    give_keystone(&world, player, true);

    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    let msg = world
        .get::<UiMessageCommand>(player)
        .expect("denial branch must emit a UI message");
    assert_eq!(msg.message_form_id, DENIAL_MSG_FORM_ID);
}

#[test]
fn denial_branch_emits_ui_message_when_keystone_missing() {
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    // Keystone NOT given.

    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    assert!(world.has::<UiMessageCommand>(player));
}

#[test]
fn denial_branch_emits_ui_message_when_activator_not_player() {
    let (mut world, _player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }

    let some_npc = world.spawn();
    fire_activate(&world, keystone_door, some_npc);
    mg07_on_activate_system(&world);

    // Activator is not the player — predicate fails on the first
    // conjunct, denial path fires. (Note Papyrus shows the UI
    // message to the player regardless of who activated; matches
    // here.)
    assert!(world.has::<UiMessageCommand>(world.resource::<PlayerEntity>().0));
    assert_eq!(
        world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap().state,
        MG07State::Waiting
    );
}

// ── Success path: the R5 cross-reference call demonstration ────

#[test]
fn success_path_consumes_keystone_and_transitions_to_inserting() {
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);

    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    // Pre-wait state — door has transitioned to Inserting +
    // keystone removed (Papyrus's pre-wait `RemoveItem` call).
    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    match door.state {
        MG07State::Inserting { wait_remaining_secs } => {
            assert_eq!(wait_remaining_secs, DELAY);
        }
        other => panic!("expected Inserting, got {:?}", other),
    }
    assert!(
        !world
            .get::<KeystoneInventory>(player)
            .unwrap()
            .has_mg07_keystone,
        "RemoveItem must have flipped the inventory flag"
    );
    // No cross-reference activate yet — that fires from the tick.
    assert!(
        !world.has::<ActivateEvent>(_secret_door),
        "myDoor must NOT be activated until the wait elapses"
    );
}

/// The headline R5 test. Drives the full lifecycle from successful
/// activation through the wait barrier and verifies the
/// cross-reference activate fires correctly on the target entity.
#[test]
fn success_path_fires_cross_reference_activate_on_my_door() {
    let (world, player, keystone_door, secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);

    // Activation → Inserting.
    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);
    crate::event_cleanup_system(&world, 0.0);

    // Wait elapses → tick fires the cross-ref ActivateEvent on
    // `secret_door`.
    mg07_tick_system(&world, DELAY);

    let cross_ref_event = world
        .get::<ActivateEvent>(secret_door)
        .expect("cross-reference activate must land on `myDoor`");
    // The activator threaded through the call is the player —
    // matches Papyrus's `myDoor.activate(actronaut, False)` semantic
    // (actronaut == player by the head-of-handler gate).
    assert_eq!(cross_ref_event.activator, player);

    // Door bookkeeping: disabled + Inactive after the wait.
    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert!(door.disabled, "post-wait Self.disable() must take effect");
    assert_eq!(door.state, MG07State::Inactive);
}

#[test]
fn inserting_state_swallows_re_activation_during_wait() {
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);

    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);
    crate::event_cleanup_system(&world, 0.0);

    // Mid-wait re-activation — Papyrus's `State waiting` is no
    // longer the active state, so the OnActivate body's gate
    // doesn't run. Same shape as `RumbleState::Busy` swallowing
    // re-activations during the post-shake wait.
    mg07_tick_system(&world, DELAY * 0.5);
    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    // No new UI message + no early cross-ref fire.
    assert!(!world.has::<UiMessageCommand>(player));
    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    match door.state {
        MG07State::Inserting { wait_remaining_secs } => {
            assert!((wait_remaining_secs - DELAY * 0.5).abs() < 1e-6);
        }
        other => panic!("expected mid-wait Inserting, got {:?}", other),
    }
}

#[test]
fn inactive_state_swallows_re_activation_post_open() {
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);

    // Full lifecycle.
    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);
    mg07_tick_system(&world, DELAY);
    crate::event_cleanup_system(&world, 0.0);

    // Door is Inactive. Re-give keystone (gameplay can't actually
    // do this, but we verify the state gates rather than the
    // predicate gates).
    give_keystone(&world, player, true);
    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);

    // No UI message (the Waiting-only branch is what emits it),
    // no new Inserting transition.
    assert!(!world.has::<UiMessageCommand>(player));
    assert_eq!(
        world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap().state,
        MG07State::Inactive
    );
}

// ── Faithful-to-source bug preservation ───────────────────────

#[test]
fn beenopened_is_never_flipped_due_to_source_typo() {
    // The .psc has `beenOpened == False` (comparison) where the
    // author plainly meant `beenOpened = False`. Papyrus compiles
    // this as a discarded expression — silent no-op. Vanilla
    // shipped this typo; the lockout-after-open behaviour relies
    // on `Self.disable()` (which DOES fire post-wait) rather than
    // the `beenOpened` flag (which never flips).
    //
    // Pin the BUG, not a fix. R5's contract is faithful
    // translation; if a future M47.2 transpiler "auto-corrects"
    // typos like this it would diverge from shipped behaviour.
    let (world, player, keystone_door, _secret_door) = setup_world();
    mg07_on_load_system(&world);
    {
        let mut q = world.query_mut::<MG07LabyrinthianDoor>().unwrap();
        q.get_mut(keystone_door).unwrap().activation_blocked = false;
    }
    advance_quest_to_stage(&world, 10);
    give_keystone(&world, player, true);
    fire_activate(&world, keystone_door, player);
    mg07_on_activate_system(&world);
    mg07_tick_system(&world, DELAY);

    let door = world.get::<MG07LabyrinthianDoor>(keystone_door).unwrap();
    assert!(
        !door.been_opened,
        "shipping Papyrus bug: been_opened is never flipped — Self.disable() carries the lockout instead"
    );
    assert!(door.disabled, "Self.disable() is the actual lockout — and DOES fire");
}
