//! Behavioural tests for the periodic-update substrate.

use super::*;
use byroredux_core::ecs::world::World;

fn setup_world() -> World {
    let mut world = World::new();
    crate::register(&mut world);
    world
}

#[test]
fn fresh_subscription_does_not_fire_on_zero_dt() {
    // Papyrus `RegisterForUpdate(N)` does NOT fire immediately — the
    // first OnUpdate is N seconds out. Pin this so a future refactor
    // that initializes `seconds_until_next = 0` (a tempting "fire
    // first tick" simplification) doesn't break the contract.
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(5.0));

    recurring_update_tick_system(&world, 0.0);
    assert!(
        !world.has::<OnUpdateEvent>(e),
        "subscription must not fire on the same frame it was created"
    );
}

#[test]
fn subscription_fires_when_interval_elapsed() {
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(1.0));

    // Half the interval — no fire.
    recurring_update_tick_system(&world, 0.5);
    assert!(!world.has::<OnUpdateEvent>(e));

    // Cross the interval — fire.
    recurring_update_tick_system(&world, 0.6);
    assert!(world.has::<OnUpdateEvent>(e));
}

#[test]
fn subscription_re_arms_after_fire() {
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(1.0));

    // First fire.
    recurring_update_tick_system(&world, 1.0);
    assert!(world.has::<OnUpdateEvent>(e));
    // Subscription must STILL be on the entity — RecurringUpdate
    // is the long-lived subscription, OnUpdateEvent is the per-fire
    // marker.
    assert!(world.has::<RecurringUpdate>(e));

    // Clear the per-frame marker (matches what
    // event_cleanup_system would do).
    crate::event_cleanup_system(&world, 0.0);
    assert!(!world.has::<OnUpdateEvent>(e));

    // Second cycle — another full interval must pass.
    recurring_update_tick_system(&world, 0.5);
    assert!(!world.has::<OnUpdateEvent>(e));
    recurring_update_tick_system(&world, 0.5);
    assert!(world.has::<OnUpdateEvent>(e));
}

#[test]
fn dt_overshoot_fires_only_once_per_tick() {
    // Papyrus's contract: a long-frame stall (debugger, alt-tab)
    // doesn't burst-fire missed OnUpdates. Documented in module
    // doc; pin it here.
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(0.5));

    recurring_update_tick_system(&world, 5.0); // 10 × interval
    assert!(world.has::<OnUpdateEvent>(e));
    // Confirm it's a single marker, not a stack of 10. Since
    // SparseSet stores one component per entity per type, this is
    // structurally guaranteed but the test pins the contract
    // against future storage refactors.
    crate::event_cleanup_system(&world, 0.0);
    assert!(!world.has::<OnUpdateEvent>(e));
}

#[test]
fn dt_overshoot_does_not_advance_phase_artificially() {
    // The re-arm uses `seconds_until_next += interval_secs`
    // (accumulating) — verify that a dt overshoot still leaves the
    // phase consistent with the elapsed time, so the NEXT fire
    // doesn't drift later than it should.
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(1.0));

    // Overshoot the first interval by 0.5s. seconds_until_next
    // becomes 1.0 - 1.5 + 1.0 = 0.5 (post-fire). Next fire is
    // 0.5s from now, not 1.0s — accounting for the overshoot.
    recurring_update_tick_system(&world, 1.5);
    assert!(world.has::<OnUpdateEvent>(e));
    crate::event_cleanup_system(&world, 0.0);

    // 0.4s into the next cycle — not yet.
    recurring_update_tick_system(&world, 0.4);
    assert!(!world.has::<OnUpdateEvent>(e));
    // Another 0.2s (cumulative 0.6 > 0.5) — fires.
    recurring_update_tick_system(&world, 0.2);
    assert!(world.has::<OnUpdateEvent>(e));
}

#[test]
fn unregister_removes_subscription() {
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(1.0));

    // Unsubscribe — same as Papyrus `UnregisterForUpdate()`.
    world.query_mut::<RecurringUpdate>().unwrap().remove(e);
    assert!(!world.has::<RecurringUpdate>(e));

    // No future firing.
    recurring_update_tick_system(&world, 10.0);
    assert!(!world.has::<OnUpdateEvent>(e));
}

#[test]
fn unregister_inside_handler_terminates_cleanly() {
    // The DLC2TTR4a "fire-once-then-cancel" idiom — handler
    // removes the subscription during its OnUpdate body, the next
    // tick finds no component and stops firing.
    let mut world = setup_world();
    let e = world.spawn();
    world.insert(e, RecurringUpdate::every(1.0));

    // Tick → fires.
    recurring_update_tick_system(&world, 1.0);
    assert!(world.has::<OnUpdateEvent>(e));

    // "Handler" runs and unsubscribes.
    world.query_mut::<RecurringUpdate>().unwrap().remove(e);
    crate::event_cleanup_system(&world, 0.0);

    // Next tick — no fire.
    recurring_update_tick_system(&world, 10.0);
    assert!(!world.has::<OnUpdateEvent>(e));
}

#[test]
fn multiple_subscriptions_independent() {
    let mut world = setup_world();
    let fast = world.spawn();
    world.insert(fast, RecurringUpdate::every(0.5));
    let slow = world.spawn();
    world.insert(slow, RecurringUpdate::every(2.0));

    // After 0.5s — fast fires, slow doesn't.
    recurring_update_tick_system(&world, 0.5);
    assert!(world.has::<OnUpdateEvent>(fast));
    assert!(!world.has::<OnUpdateEvent>(slow));
    crate::event_cleanup_system(&world, 0.0);

    // Another 1.5s (cumulative 2.0) — fast fires three more times
    // worth of intervals but only once per tick, slow fires once.
    recurring_update_tick_system(&world, 1.5);
    assert!(world.has::<OnUpdateEvent>(fast));
    assert!(world.has::<OnUpdateEvent>(slow));
}

#[test]
fn empty_world_tick_is_safe() {
    // Defensive — make sure the tick system handles a world with
    // no subscriptions without panicking.
    let world = setup_world();
    recurring_update_tick_system(&world, 0.016);
    // Tick twice, both should be no-ops.
    recurring_update_tick_system(&world, 0.016);
}
