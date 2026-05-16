//! Periodic-update subscription — the ECS substrate for Papyrus's
//! `Self.RegisterForUpdate(intervalSec)` / `Self.UnregisterForUpdate()` /
//! `Event OnUpdate()` triad.
//!
//! Lands as part of the R5 follow-up (the `RegisterForUpdate` half of
//! the Papyrus quest-prototype evaluation; see
//! [`docs/r5-evaluation.md`](../../../docs/r5-evaluation.md)).
//!
//! ## The Papyrus surface this replaces
//!
//! ```papyrus
//! Event OnInit()
//!   Self.RegisterForUpdate(5 as Float)  ; fire OnUpdate every 5 seconds
//! EndEvent
//!
//! Event OnUpdate()
//!   ; ...do work...
//!   Self.UnregisterForUpdate()         ; one-shot termination
//! EndEvent
//! ```
//!
//! Papyrus's runtime tracks the subscription in a per-script-instance
//! table; the engine ticks every subscription each frame, fires
//! `OnUpdate` when the interval elapses, and re-arms. Our ECS shape:
//!
//! - **Subscription**: insert a [`RecurringUpdate`] component on the
//!   script entity with `{ interval_secs, seconds_until_next:
//!   interval_secs }`. The subscription IS the component.
//! - **Cancellation**: remove the component. Same lifetime as Papyrus's
//!   "while the subscription is alive in the table".
//! - **Firing**: [`recurring_update_tick_system`] runs every frame,
//!   counts down `seconds_until_next`, emits an [`OnUpdateEvent`]
//!   marker on the entity when it crosses zero, and re-arms via
//!   `seconds_until_next += interval_secs` (accumulating not resetting
//!   so a long-frame dt that pushes through multiple intervals still
//!   only fires once per tick — the standard "missed fire" behaviour
//!   matching Papyrus's runtime).
//! - **Handler dispatch**: per-script systems query for
//!   `(RecurringUpdate, OnUpdateEvent, MyScriptComponent)` and run
//!   their `OnUpdate` body.
//!
//! ## Tick accuracy notes
//!
//! Papyrus's `RegisterForUpdate(N)` documents the firing as
//! "approximately every N seconds" — the Bethesda runtime ticks at
//! the script-engine's own cadence (~50ms in vanilla), so a 1.0s
//! subscription fires no more often than 1.0s but often a few tens of
//! ms later. The ECS shape inherits the same approximation: a 1.0s
//! subscription with `dt = 0.016` will fire on the first frame
//! where the cumulative dt has crossed 1.0s, NOT exactly at t=1.0s.
//! Match the Papyrus contract.
//!
//! ## `UnregisterForUpdate` from inside an `OnUpdate` body
//!
//! The DLC2TTR4a fixture demonstrates the "fire-once-then-cancel"
//! idiom: the handler removes the script's own `RecurringUpdate`
//! component during its OnUpdate body. Standard ECS pattern — the
//! handler runs against this frame's marker (already inserted by
//! the tick), removes the subscription, and the next frame's tick
//! finds no component to count down. Sound and matches Papyrus.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;

/// "Fire an [`OnUpdateEvent`] on this entity every `interval_secs`."
///
/// Insert on the script's entity to subscribe; remove to
/// unsubscribe. Same semantic as Papyrus's
/// `Self.RegisterForUpdate(N)` / `Self.UnregisterForUpdate()`.
///
/// Default constructor matches the most common Papyrus idiom — a
/// 5-second interval (the modal value across the ~200 vanilla
/// scripts that call `RegisterForUpdate`; longer intervals like 30s
/// also occur, sub-second intervals are rare and usually
/// problematic).
#[derive(Debug, Clone, Copy)]
pub struct RecurringUpdate {
    /// Cadence between OnUpdate fires. Papyrus argument to
    /// `RegisterForUpdate(intervalSec)`.
    pub interval_secs: f32,
    /// Time remaining until the next fire. Initialised to
    /// `interval_secs` at subscription time (matches Papyrus —
    /// `RegisterForUpdate(N)` does NOT fire immediately, it waits
    /// `N` seconds before the first OnUpdate). Decremented by `dt`
    /// each frame; when ≤ 0, fires + accumulates `interval_secs`.
    pub seconds_until_next: f32,
}

impl RecurringUpdate {
    /// Idiomatic constructor — schedules the first OnUpdate after
    /// `interval_secs` seconds, matching Papyrus's
    /// `RegisterForUpdate(N)` "first fire is N seconds out" contract.
    pub fn every(interval_secs: f32) -> Self {
        Self {
            interval_secs,
            seconds_until_next: interval_secs,
        }
    }
}

impl Component for RecurringUpdate {
    type Storage = SparseSetStorage<Self>;
}

/// Marker emitted by [`recurring_update_tick_system`] when a
/// [`RecurringUpdate`] subscription's interval elapses.
///
/// Same lifecycle as [`crate::events::ActivateEvent`] — inserted by
/// the tick system, consumed by per-script handlers in the same
/// frame, swept by [`crate::event_cleanup_system`] at end of frame.
///
/// Why a dedicated event type and not a re-purposed
/// [`TimerExpired`]?
///
/// [`TimerExpired`] is the one-shot pair to [`crate::ScriptTimer`]
/// — fires once, marker removed (along with the timer) on
/// expiration. `RecurringUpdate` fires repeatedly without removing
/// the subscription. Mixing the two on the same marker type would
/// require callers to disambiguate "this was a one-shot, the timer
/// is gone" vs "this was a recurring, the subscription is still
/// alive" via a separate query. Distinct event types makes the
/// lifecycle explicit at the type level.
#[derive(Debug, Clone, Copy)]
pub struct OnUpdateEvent;

impl Component for OnUpdateEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Tick every [`RecurringUpdate`] subscription by `dt`. When a
/// counter hits or crosses zero, inserts an [`OnUpdateEvent`]
/// marker on the entity and re-arms the counter.
///
/// Cumulative `dt`-overshoot handling: if a long frame pushes the
/// counter from 0.5 to -1.5 on a 1.0s subscription, the next fire
/// re-arms to `-1.5 + 1.0 = -0.5` (still negative, would fire
/// again next frame). We DON'T loop within the tick — same as
/// Papyrus: "missed" fires drop. Two reasons:
///
/// 1. Burst-fire on resume from a long frame stall (debugger
///    breakpoint, alt-tab, vsync hiccup) is uniformly worse than
///    a single skipped fire.
/// 2. The Papyrus runtime documents the same contract; matching it
///    keeps existing-script behaviour predictable.
///
/// Sibling to [`crate::timer_tick_system`] (the one-shot
/// [`ScriptTimer`] tick system); both are dt-driven and run before
/// per-script handler systems in the scripting stage.
///
/// [`ScriptTimer`]: crate::ScriptTimer
pub fn recurring_update_tick_system(world: &World, dt: f32) {
    let Some(mut updates) = world.query_mut::<RecurringUpdate>() else {
        return;
    };
    let mut to_fire: Vec<EntityId> = Vec::new();
    for (entity, ru) in updates.iter_mut() {
        ru.seconds_until_next -= dt;
        if ru.seconds_until_next <= 0.0 {
            // Accumulate instead of reset so a dt overshoot
            // doesn't lose the partial second on the next interval.
            // For dt > 2 × interval this still fires only once
            // (the missed-fire contract — see module doc).
            ru.seconds_until_next += ru.interval_secs;
            to_fire.push(entity);
        }
    }
    drop(updates);

    if to_fire.is_empty() {
        return;
    }
    let Some(mut events) = world.query_mut::<OnUpdateEvent>() else {
        return;
    };
    for entity in to_fire {
        events.insert(entity, OnUpdateEvent);
    }
}

/// Register the recurring-update component + event storages with
/// the world. Sibling to [`crate::register`].
///
/// Called from the main scripting [`crate::register`] —
/// `RecurringUpdate` / `OnUpdateEvent` are core scripting primitives,
/// not demo-specific, so they live in the top-level registration.
pub fn register(world: &mut World) {
    world.register::<RecurringUpdate>();
    world.register::<OnUpdateEvent>();
}

#[cfg(test)]
mod tests;
