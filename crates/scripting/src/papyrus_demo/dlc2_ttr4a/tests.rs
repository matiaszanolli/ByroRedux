//! Behavioural tests for the DLC2TTR4aPlayerScript translation.
//!
//! Drives the full register → poll → fire → unregister lifecycle
//! against a constructed world. Each test pins one branch of the
//! Papyrus source.

use super::*;
use crate::papyrus_demo::actor_stats::ActorStats;
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};
use crate::recurring_update::{recurring_update_tick_system, RecurringUpdate};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

const QUEST_FORM_ID: QuestFormId = QuestFormId(0x02019200);
const POLL_INTERVAL: f32 = 5.0;

/// Build a world with: scripting + papyrus_demo registered, an
/// empty QuestStageState, a player entity with default ActorStats,
/// and a script-bearing entity carrying the DLC2TTR4a component.
fn setup_world() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    crate::papyrus_demo::register(&mut world);
    world.insert_resource(QuestStageState::default());

    let player = world.spawn();
    world.insert(player, ActorStats::default());
    world.insert_resource(PlayerEntity(player));

    let script_entity = world.spawn();
    world.insert(script_entity, dlc2_ttr4a_player_script(QUEST_FORM_ID));

    (world, player, script_entity)
}

/// Helper: write a value to the player's stat slot. Stand-in for
/// the eventual perk + temporary-modifier pipeline.
fn set_player_stat(world: &World, player: EntityId, name: &str, value: f32) {
    world
        .query_mut::<ActorStats>()
        .unwrap()
        .get_mut(player)
        .unwrap()
        .set(name, value);
}

// ── OnInit ────────────────────────────────────────────────────

#[test]
fn on_init_subscribes_to_recurring_update() {
    let (world, _player, script_entity) = setup_world();
    assert!(
        !world.has::<RecurringUpdate>(script_entity),
        "no subscription before on_init"
    );

    dlc2_ttr4a_on_init_system(&world);

    let sub = world
        .get::<RecurringUpdate>(script_entity)
        .expect("RecurringUpdate must be inserted by on_init");
    // Hardcoded 5-second cadence from the .psc source.
    assert_eq!(sub.interval_secs, POLL_INTERVAL);
    assert_eq!(
        sub.seconds_until_next, POLL_INTERVAL,
        "fresh subscription must wait one full interval before first fire"
    );
}

#[test]
fn on_init_is_idempotent() {
    // Papyrus's `OnInit` fires exactly once per alias instance.
    // The ECS-side "OnInit" runs as long as a subscription doesn't
    // already exist — repeated invocations must be no-ops to
    // preserve that contract.
    let (world, _player, script_entity) = setup_world();

    dlc2_ttr4a_on_init_system(&world);
    let sub = world.get::<RecurringUpdate>(script_entity).unwrap();
    let initial_remaining = sub.seconds_until_next;
    drop(sub);

    // Repeated invocation must not reset the counter or re-arm.
    dlc2_ttr4a_on_init_system(&world);
    let sub2 = world.get::<RecurringUpdate>(script_entity).unwrap();
    assert_eq!(
        sub2.seconds_until_next, initial_remaining,
        "second on_init call must be a no-op"
    );
}

// ── OnUpdate — threshold not met ──────────────────────────────

#[test]
fn polling_below_threshold_does_not_fire_setstage() {
    let (mut world, _player, script_entity) = setup_world();
    dlc2_ttr4a_on_init_system(&world);

    // Player's Variable05 is 0.0 (default). Tick across one
    // interval, then run the handler.
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(
        stage_state.get_stage(QUEST_FORM_ID),
        0,
        "stat below threshold must not advance quest"
    );
    drop(stage_state);
    // Subscription survives the unsatisfied tick — script keeps
    // polling.
    assert!(world.has::<RecurringUpdate>(script_entity));

    // Confirm the handler will fire again next cycle. Clear this
    // tick's marker.
    crate::event_cleanup_system(&world, 0.0);
    let _ = &mut world;
}

// ── OnUpdate — threshold satisfied ────────────────────────────

#[test]
fn polling_above_threshold_fires_setstage_and_unsubscribes() {
    let (world, player, script_entity) = setup_world();
    dlc2_ttr4a_on_init_system(&world);

    // Externally write the flag the script is watching.
    set_player_stat(&world, player, "Variable05", 1.0);

    // Tick to fire OnUpdate, then run the handler.
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);

    // Quest advanced.
    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(QUEST_FORM_ID), 200);
    assert!(stage_state.get_stage_done(QUEST_FORM_ID, 200));
    drop(stage_state);

    // UnregisterForUpdate fired — subscription gone.
    assert!(
        !world.has::<RecurringUpdate>(script_entity),
        "handler must remove RecurringUpdate via UnregisterForUpdate"
    );
}

#[test]
fn unsubscribed_script_no_longer_polls() {
    let (world, player, script_entity) = setup_world();
    dlc2_ttr4a_on_init_system(&world);
    set_player_stat(&world, player, "Variable05", 1.0);

    // First fire → satisfies, advances, unsubscribes.
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);
    crate::event_cleanup_system(&world, 0.0);
    assert!(!world.has::<RecurringUpdate>(script_entity));

    // Even if the player's stat changes again, the script should
    // not re-poll — the subscription is gone. Advance the stage
    // forward externally then drop it back to confirm.
    {
        let mut stage_state = world.resource_mut::<QuestStageState>();
        stage_state.set_stage(QUEST_FORM_ID, 300);
    }

    recurring_update_tick_system(&world, POLL_INTERVAL * 5.0); // huge dt
    dlc2_ttr4a_on_update_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(
        stage_state.get_stage(QUEST_FORM_ID),
        300,
        "no further script activity after self-unsubscribe"
    );
}

// ── Threshold-cross mid-poll ──────────────────────────────────

#[test]
fn threshold_cross_during_polling_fires_on_next_tick() {
    let (world, player, script_entity) = setup_world();
    dlc2_ttr4a_on_init_system(&world);

    // First tick — stat below threshold, no advance.
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);
    crate::event_cleanup_system(&world, 0.0);
    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 0);
    assert!(world.has::<RecurringUpdate>(script_entity));

    // Player crosses the threshold between ticks.
    set_player_stat(&world, player, "Variable05", 0.5);

    // Next tick — handler observes the crossed stat, fires.
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);

    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 200);
    assert!(!world.has::<RecurringUpdate>(script_entity));
}

// ── No marker = no poll ───────────────────────────────────────

#[test]
fn handler_no_ops_without_on_update_marker() {
    // Defensive: even with a satisfied threshold, the handler
    // must NOT advance the quest unless the OnUpdate marker is
    // present this frame. The marker is what gates the handler
    // body running — without it, the tick system hasn't fired
    // yet.
    let (world, player, _script_entity) = setup_world();
    dlc2_ttr4a_on_init_system(&world);
    set_player_stat(&world, player, "Variable05", 1.0);

    // No `recurring_update_tick_system` call → no OnUpdateEvent →
    // handler is a no-op.
    dlc2_ttr4a_on_update_system(&world);

    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 0);
}

// ── Custom thresholds + stat names ────────────────────────────

#[test]
fn custom_threshold_and_stat_name_are_honoured() {
    // Validates that the per-script-component fields actually
    // drive the polling. Future M47.2 transpiler emissions need
    // this to faithfully reproduce script behaviour from differing
    // .psc constants.
    let (world, player, script_entity) = setup_world();
    {
        let mut q = world.query_mut::<Dlc2Ttr4aPlayerScript>().unwrap();
        let script = q.get_mut(script_entity).unwrap();
        script.poll_actor_value = "Health";
        script.threshold = 100.0;
        script.on_satisfied_stage = 50;
    }
    dlc2_ttr4a_on_init_system(&world);

    // Variable05 stays 0 (not the polled value); Health below
    // threshold. No fire.
    set_player_stat(&world, player, "Variable05", 999.0);
    set_player_stat(&world, player, "Health", 50.0);
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 0);
    crate::event_cleanup_system(&world, 0.0);

    // Push Health past the threshold — fires + advances to 50,
    // not 200.
    set_player_stat(&world, player, "Health", 150.0);
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);
    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 50);
}

// ── Multiple scripts of the same type ─────────────────────────

#[test]
fn multiple_script_instances_poll_independently() {
    let (mut world, player, script_a) = setup_world();
    let script_b = world.spawn();
    let other_quest = QuestFormId(0x02019999);
    world.insert(script_b, dlc2_ttr4a_player_script(other_quest));

    dlc2_ttr4a_on_init_system(&world);
    assert!(world.has::<RecurringUpdate>(script_a));
    assert!(world.has::<RecurringUpdate>(script_b));

    // Both scripts poll the SAME player stat (it's the player's
    // `Variable05`, not the script's). When the threshold crosses,
    // BOTH should advance their respective quests. This matches
    // Papyrus — every alias instance polling the same global state
    // wakes up simultaneously.
    set_player_stat(&world, player, "Variable05", 1.0);
    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);

    let stage_state = world.resource::<QuestStageState>();
    assert_eq!(stage_state.get_stage(QUEST_FORM_ID), 200);
    assert_eq!(stage_state.get_stage(other_quest), 200);
    drop(stage_state);
    // Both unsubscribed independently.
    assert!(!world.has::<RecurringUpdate>(script_a));
    assert!(!world.has::<RecurringUpdate>(script_b));
}

// ── Stat is missing entirely ──────────────────────────────────

#[test]
fn missing_stat_defaults_to_zero_and_does_not_fire() {
    let (world, player, _script_entity) = setup_world();
    // Player has ActorStats but no "Variable05" key set.
    dlc2_ttr4a_on_init_system(&world);

    recurring_update_tick_system(&world, POLL_INTERVAL);
    dlc2_ttr4a_on_update_system(&world);

    // get() returns 0.0 for unknown stat → predicate (0.0 > 0.0)
    // is false → no advance. Matches Papyrus's "unknown
    // actor-value resolves to 0" contract.
    assert_eq!(world.resource::<QuestStageState>().get_stage(QUEST_FORM_ID), 0);
    let _ = player; // suppress unused-binding lint
}
