//! End-to-end behavioural tests for the R5 hand-translation.
//!
//! Each test exercises one Papyrus semantic at a time, named after
//! the .psc construct it pins:
//!
//! - [`active_state_emits_camera_and_controller_commands`] —
//!   `Auto State active` + `OnActivate` body
//! - [`active_state_transitions_to_busy_after_emit`] —
//!   `Self.GotoState("busy")`
//! - [`busy_state_swallows_re_activation`] —
//!   `State busy ; empty body`
//! - [`tick_decrements_wait_and_holds_busy_until_zero`] —
//!   `Utility.wait(duration)` continuation
//! - [`repeatable_returns_to_active_after_wait`] —
//!   the `If repeatable / Self.GotoState("active")` branch
//! - [`non_repeatable_falls_through_to_inactive`] —
//!   the `Else / Self.GotoState("inactive")` branch
//! - [`inactive_state_swallows_re_activation`] —
//!   `State inactive ; empty body`
//! - [`oversized_dt_resolves_to_terminal_state_in_one_tick`] —
//!   long-frame edge case
//! - [`full_lifecycle_round_trip_via_event_then_tick`] —
//!   the canonical Active → Busy → Active loop

use super::*;
use crate::events::ActivateEvent;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// Set up a world with the scripting + demo storages registered and a
/// player entity wired into the [`PlayerEntity`] resource. Mirrors
/// the App-level setup an integration call site would do.
fn setup_world() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    crate::register(&mut world);
    super::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PlayerEntity(player));
    let lever = world.spawn();
    world.insert(lever, RumbleOnActivate::default());
    (world, player, lever)
}

/// Fire a Papyrus-style `OnActivate(actronaut=player)` on `target`.
fn fire_activate(world: &World, target: EntityId, activator: EntityId) {
    let mut q = world
        .query_mut::<ActivateEvent>()
        .expect("ActivateEvent storage must be registered");
    q.insert(target, ActivateEvent { activator });
}

// ── State A: `Auto State active` ──────────────────────────────

#[test]
fn active_state_emits_camera_and_controller_commands() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // Papyrus emitted `Game.shakeCamera(None, cameraIntensity, 0.0)` and
    // `Game.shakeController(shakeLeft, shakeRight, duration)`. Both
    // land as marker components on the player entity (the resolution
    // target of Papyrus's `Game` pseudo-singleton).
    let cam = world
        .get::<CameraShakeCommand>(player)
        .expect("CameraShakeCommand must land on player");
    assert_eq!(cam.intensity, 0.25);
    // Papyrus passes 0.0 for duration on `Game.shakeCamera` — preserved.
    assert_eq!(cam.duration_secs, 0.0);

    let rumble_cmd = world
        .get::<ControllerRumbleCommand>(player)
        .expect("ControllerRumbleCommand must land on player");
    assert_eq!(rumble_cmd.left, 0.25);
    assert_eq!(rumble_cmd.right, 0.25);
    assert_eq!(rumble_cmd.duration_secs, 0.25);
}

#[test]
fn active_state_transitions_to_busy_after_emit() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    let rumble = world.get::<RumbleOnActivate>(lever).unwrap();
    match rumble.state {
        RumbleState::Busy { wait_remaining_secs } => {
            // Wait counter started at `duration` (= 0.25 by default).
            assert_eq!(wait_remaining_secs, 0.25);
        }
        other => panic!("expected Busy after activation, got {:?}", other),
    }
}

// ── State B: `State busy` (empty OnActivate) ──────────────────

#[test]
fn busy_state_swallows_re_activation() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // Clear the ActivateEvent marker so the next fire is fresh.
    crate::event_cleanup_system(&world, 0.0);
    // Also drop the camera-shake / controller-rumble markers so we
    // can prove the re-activation didn't add a new pair.
    world
        .query_mut::<CameraShakeCommand>()
        .unwrap()
        .remove(player);
    world
        .query_mut::<ControllerRumbleCommand>()
        .unwrap()
        .remove(player);

    // Confirm we're in Busy.
    assert!(matches!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Busy { .. }
    ));

    // Second activation while Busy.
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // No new shake commands — Papyrus's empty `State busy` OnActivate
    // body translates to a no-op match arm.
    assert!(
        !world.has::<CameraShakeCommand>(player),
        "Busy state must swallow re-activations (no new shake)"
    );
    assert!(
        !world.has::<ControllerRumbleCommand>(player),
        "Busy state must swallow re-activations (no new rumble)"
    );
}

// ── Latent wait: `Utility.wait(duration)` ─────────────────────

#[test]
fn tick_decrements_wait_and_holds_busy_until_zero() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // Half the wait passes — must still be Busy.
    rumble_tick_system(&world, 0.10);
    match world.get::<RumbleOnActivate>(lever).unwrap().state {
        RumbleState::Busy { wait_remaining_secs } => {
            assert!(
                (wait_remaining_secs - 0.15).abs() < 1e-6,
                "wait_remaining_secs should be 0.25 - 0.10 = 0.15, got {}",
                wait_remaining_secs
            );
        }
        other => panic!("expected still Busy mid-wait, got {:?}", other),
    }
    // Player-targeted markers still present (we never cleaned them up).
    assert!(world.has::<CameraShakeCommand>(player));
}

#[test]
fn repeatable_returns_to_active_after_wait() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // Tick the full wait — repeatable=true → back to Active.
    rumble_tick_system(&world, 0.25);
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active
    );
}

#[test]
fn non_repeatable_falls_through_to_inactive() {
    let (world, player, lever) = setup_world();
    // Configure as one-shot (`repeatable = False`).
    {
        let mut q = world.query_mut::<RumbleOnActivate>().unwrap();
        q.get_mut(lever).unwrap().repeatable = false;
    }

    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);
    rumble_tick_system(&world, 0.25);

    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Inactive
    );
}

// ── State C: `State inactive` (empty OnActivate) ─────────────

#[test]
fn inactive_state_swallows_re_activation() {
    let (world, player, lever) = setup_world();
    {
        let mut q = world.query_mut::<RumbleOnActivate>().unwrap();
        q.get_mut(lever).unwrap().repeatable = false;
    }

    // Drive through Active → Busy → Inactive.
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);
    rumble_tick_system(&world, 0.25);
    crate::event_cleanup_system(&world, 0.0);
    world
        .query_mut::<CameraShakeCommand>()
        .unwrap()
        .remove(player);
    world
        .query_mut::<ControllerRumbleCommand>()
        .unwrap()
        .remove(player);

    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Inactive
    );

    // Re-activate while Inactive — must no-op.
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    assert!(
        !world.has::<CameraShakeCommand>(player),
        "Inactive state must swallow re-activations"
    );
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Inactive,
    );
}

// ── Edge case: oversized dt ───────────────────────────────────

#[test]
fn oversized_dt_resolves_to_terminal_state_in_one_tick() {
    let (world, player, lever) = setup_world();
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);

    // dt = 10s vs wait = 0.25s — single tick must fully resolve.
    // Papyrus's `Utility.wait()` would have suspended for exactly
    // 0.25s and then resumed; the ECS shape collapses the
    // continuation into the same tick that pushes the counter past
    // zero, which is acceptable because `repeatable`'s branching is
    // pure data (no side-effects deferred to "after the wait").
    rumble_tick_system(&world, 10.0);
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active,
    );
}

// ── Canonical lifecycle ───────────────────────────────────────

#[test]
fn full_lifecycle_round_trip_via_event_then_tick() {
    let (world, player, lever) = setup_world();

    // Cycle 1: Active → Busy → Active
    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);
    assert!(matches!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Busy { .. }
    ));
    rumble_tick_system(&world, 0.25);
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active
    );
    crate::event_cleanup_system(&world, 0.0);

    // Cycle 2 — clean state, same result.
    world
        .query_mut::<CameraShakeCommand>()
        .unwrap()
        .remove(player);
    world
        .query_mut::<ControllerRumbleCommand>()
        .unwrap()
        .remove(player);

    fire_activate(&world, lever, player);
    rumble_on_activate_system(&world);
    assert!(world.has::<CameraShakeCommand>(player));
    rumble_tick_system(&world, 0.25);
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active
    );
}

// ── M47.0 Phase 6 — end-to-end integration tests ────────────────
//
// The R5 tests above bypass the script registry by directly inserting
// `RumbleOnActivate` on the lever. The integration tests below
// exercise the FULL Phase 1-5 chain:
//   1. Build a world with `crate::register` (Phase 1 wiring).
//   2. Build a `ScriptRegistry` + `register_spawners` (Phase 2 +
//      Phase 3a/3b).
//   3. Run the spawner via the registry's lookup path → component
//      lands on the entity (simulates the cell loader's
//      `attach_script_for_refr`).
//   4. Fire `ActivateEvent` (simulates the `script.activate`
//      console command from Phase 4).
//   5. Run `rumble_on_activate_system` (Phase 1's scheduler
//      registration would do this automatically in the engine; the
//      test calls it explicitly).
//   6. Assert the post-state matches the e2e expected outcome.

/// Build a world with the FULL M47.0 plumbing in place — scripting
/// crate `register`, a `ScriptRegistry` populated by
/// `papyrus_demo::register_spawners`, and a player entity wired into
/// the [`PlayerEntity`] resource. Mirrors what the engine's main.rs
/// does at boot.
fn setup_world_with_registry() -> (World, EntityId, crate::ScriptRegistry) {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PlayerEntity(player));
    let mut registry = crate::ScriptRegistry::new();
    crate::papyrus_demo::register_spawners(&mut registry);
    (world, player, registry)
}

#[test]
fn registry_spawner_attaches_rumble_component_on_lookup_hit() {
    // Phase 2+3 contract: editor_id "defaultRumbleOnActivate" → spawner
    // → `RumbleOnActivate` lands on the supplied entity at .psc defaults.
    let (mut world, _player, registry) = setup_world_with_registry();
    let lever = world.spawn();
    assert!(
        !world.has::<RumbleOnActivate>(lever),
        "lever must not have RumbleOnActivate before spawner runs"
    );

    let spawn_fn = registry
        .lookup("defaultRumbleOnActivate")
        .expect("Phase 2 registers defaultRumbleOnActivate");
    spawn_fn(&mut world, lever);

    let rumble = world
        .get::<RumbleOnActivate>(lever)
        .expect("spawner must insert RumbleOnActivate");
    assert_eq!(rumble.state, RumbleState::Active);
    assert_eq!(rumble.camera_intensity, 0.25); // .psc default
    assert_eq!(rumble.duration, 0.25); // .psc default
    assert!(rumble.repeatable); // .psc default
}

#[test]
fn registry_lookup_miss_for_unregistered_editor_id_returns_none() {
    // Phase 3b contract: an unregistered SCPT.editor_id is silently
    // skipped by the cell loader. Verify the lookup-half of that
    // contract returns None so the loader's fall-through fires.
    let (_world, _player, registry) = setup_world_with_registry();
    assert!(registry.lookup("SomeMod_NoSuchScript").is_none());
    assert!(registry.lookup("").is_none());
    // Case sensitivity (Phase 2 contract).
    assert!(registry.lookup("DEFAULTRUMBLEONACTIVATE").is_none());
}

#[test]
fn full_e2e_pipeline_registry_spawn_then_activate_then_state_machine() {
    // The integration smoke test for M47.0: walk the entire Phase
    // 1-5 chain end-to-end on a synthetic entity.
    let (mut world, player, registry) = setup_world_with_registry();
    let lever = world.spawn();

    // Phase 3b — spawner attaches state via registry lookup.
    let spawn_fn = registry.lookup("defaultRumbleOnActivate").unwrap();
    spawn_fn(&mut world, lever);
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active
    );

    // Phase 4 — emit ActivateEvent (simulates `script.activate` cmd
    // or the gameplay use-key path).
    let mut activate_q = world.query_mut::<ActivateEvent>().unwrap();
    activate_q.insert(lever, ActivateEvent { activator: player });
    drop(activate_q);

    // Phase 1 — dispatcher system runs (engine scheduler would call
    // this; the test calls it directly).
    rumble_on_activate_system(&world);

    // Assert: Active → Busy transition fired, cross-subsystem
    // commands landed on the player.
    let post_state = world.get::<RumbleOnActivate>(lever).unwrap().state;
    assert!(
        matches!(post_state, RumbleState::Busy { .. }),
        "expected Busy state post-activate, got {:?}",
        post_state
    );
    assert!(
        world.has::<CameraShakeCommand>(player),
        "Phase 1 dispatcher must emit CameraShakeCommand on the player"
    );
    assert!(
        world.has::<ControllerRumbleCommand>(player),
        "Phase 1 dispatcher must emit ControllerRumbleCommand on the player"
    );

    // Phase 1+3 (continuation) — tick the wait counter; Active state
    // restored because `repeatable: true`.
    rumble_tick_system(&world, 0.5); // longer than .psc duration (0.25)
    assert_eq!(
        world.get::<RumbleOnActivate>(lever).unwrap().state,
        RumbleState::Active,
        "post-tick repeatable rumble must return to Active"
    );
}

#[test]
fn on_cell_load_event_storage_registered_and_insertable() {
    // Phase 5 contract: the OnCellLoadEvent storage is registered by
    // `scripting::register`. The cell-loader emit site (Phase 5 in
    // `byroredux/src/cell_loader/references.rs`) inserts the marker
    // after `spawn_fn` runs. Mirror that here to confirm the storage
    // surface works.
    let mut world = World::new();
    crate::register(&mut world);
    let entity = world.spawn();
    let mut q = world
        .query_mut::<crate::OnCellLoadEvent>()
        .expect("OnCellLoadEvent storage must be registered by crate::register");
    q.insert(entity, crate::OnCellLoadEvent);
    drop(q);
    assert!(world.has::<crate::OnCellLoadEvent>(entity));
}

#[test]
fn on_trigger_enter_event_and_on_equip_event_storages_registered() {
    // Phase 5 contract: OnTriggerEnterEvent + OnEquipEvent are
    // structurally available (storage registered, marker insertable)
    // even though their emit sites land in follow-up work
    // (Rapier sensors + M41 equip pipeline). Scripts can declare
    // these queries today and the engine will start firing them once
    // the emit sites materialize.
    let mut world = World::new();
    crate::register(&mut world);
    let trigger_volume = world.spawn();
    let player = world.spawn();
    let item = world.spawn();
    let npc = world.spawn();

    {
        let mut q = world.query_mut::<crate::OnTriggerEnterEvent>().unwrap();
        q.insert(
            trigger_volume,
            crate::OnTriggerEnterEvent { triggerer: player },
        );
    }
    assert!(world.has::<crate::OnTriggerEnterEvent>(trigger_volume));

    {
        let mut q = world.query_mut::<crate::OnEquipEvent>().unwrap();
        q.insert(item, crate::OnEquipEvent { wearer: npc });
    }
    assert!(world.has::<crate::OnEquipEvent>(item));
}
