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
