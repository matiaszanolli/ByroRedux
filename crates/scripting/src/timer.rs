//! Script timer system.
//!
//! `ScriptTimer` is a component that counts down each frame. When it
//! reaches zero, the timer system inserts a `TimerExpired` marker
//! component on the same entity and removes the timer.
//!
//! **No producer, no consumer** (#5608, honest since 2026-10-13): nothing
//! in production inserts a `ScriptTimer` or reads the `TimerExpired` it
//! fires — the only constructions are this module's own tests. The
//! M47.0-era plan was to lower Papyrus's `StartTimer(time, id)` →
//! `OnTimer(id)` onto this pair, but the FO4 timers that actually shipped
//! (2026-10-13) ride the update-registration registry instead
//! (`fragment/updates.rs`: `ScriptUpdates` + `UpdateRegistration`,
//! real-time and game-time clocks, saved) — a second ECS lowering would
//! duplicate that path. The pair stays registered and round-trip-tested
//! as the one-shot shape beside `RecurringUpdate`; retire it if no
//! consumer ever lands.

use crate::events::TimerExpired;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;

/// A countdown timer attached to an entity.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptTimer {
    /// Caller-assigned ID, echoed in the `TimerExpired` event.
    pub id: u32,
    /// Seconds remaining. Decremented by dt each frame.
    pub remaining: f32,
}

impl Component for ScriptTimer {
    type Storage = SparseSetStorage<Self>;
}

/// System: tick all ScriptTimer components, fire TimerExpired when done.
pub fn timer_tick_system(world: &World, dt: f32) {
    let Some(mut timers) = world.query_mut::<ScriptTimer>() else {
        return;
    };

    // Collect expired timers (can't mutate two storages while iterating one)
    let mut expired: Vec<(EntityId, u32)> = Vec::new();

    for (entity, timer) in timers.iter_mut() {
        timer.remaining -= dt;
        if timer.remaining <= 0.0 {
            expired.push((entity, timer.id));
        }
    }

    // Remove expired timers
    for &(entity, _) in &expired {
        timers.remove(entity);
    }
    drop(timers);

    // Insert TimerExpired markers
    if !expired.is_empty() {
        let Some(mut markers) = world.query_mut::<TimerExpired>() else {
            return;
        };
        for (entity, timer_id) in expired {
            markers.insert(entity, TimerExpired { timer_id });
        }
    }
}

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_component::<ScriptTimer>("ScriptTimer");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TimerExpired;
    use byroredux_core::ecs::world::World;

    fn setup_world() -> World {
        let mut world = World::new();
        crate::register(&mut world);
        world
    }

    #[test]
    fn timer_fires_after_duration() {
        let mut world = setup_world();
        let e = world.spawn();
        world.insert(
            e,
            ScriptTimer {
                id: 42,
                remaining: 1.0,
            },
        );

        // Frame 1: 0.5s elapsed — timer still running
        timer_tick_system(&world, 0.5);
        assert!(world.has::<ScriptTimer>(e));
        assert!(!world.has::<TimerExpired>(e));

        // Frame 2: another 0.5s — timer fires
        timer_tick_system(&world, 0.6);
        assert!(!world.has::<ScriptTimer>(e));
        assert!(world.has::<TimerExpired>(e));

        let marker = world.get::<TimerExpired>(e).unwrap();
        assert_eq!(marker.timer_id, 42);
    }

    #[test]
    fn timer_zero_duration_fires_immediately() {
        let mut world = setup_world();
        let e = world.spawn();
        world.insert(
            e,
            ScriptTimer {
                id: 1,
                remaining: 0.0,
            },
        );

        timer_tick_system(&world, 0.016);
        assert!(!world.has::<ScriptTimer>(e));
        assert!(world.has::<TimerExpired>(e));
    }

    #[test]
    fn multiple_timers_independent() {
        let mut world = setup_world();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(
            a,
            ScriptTimer {
                id: 10,
                remaining: 0.5,
            },
        );
        world.insert(
            b,
            ScriptTimer {
                id: 20,
                remaining: 1.5,
            },
        );

        // After 0.6s: a fires, b still running
        timer_tick_system(&world, 0.6);
        assert!(world.has::<TimerExpired>(a));
        assert!(!world.has::<TimerExpired>(b));
        assert!(world.has::<ScriptTimer>(b));

        // Clean up a's marker
        crate::cleanup::event_cleanup_system(&world, 0.0);
        assert!(!world.has::<TimerExpired>(a));

        // After another 1.0s: b fires
        timer_tick_system(&world, 1.0);
        assert!(world.has::<TimerExpired>(b));
        assert!(!world.has::<ScriptTimer>(b));
    }

    #[test]
    fn cleanup_removes_expired_markers() {
        let mut world = setup_world();
        let e = world.spawn();
        world.insert(
            e,
            ScriptTimer {
                id: 1,
                remaining: 0.0,
            },
        );

        timer_tick_system(&world, 0.016);
        assert!(world.has::<TimerExpired>(e));

        crate::cleanup::event_cleanup_system(&world, 0.0);
        assert!(!world.has::<TimerExpired>(e));
    }

    #[test]
    fn no_timers_is_safe() {
        let mut world = setup_world();
        let bystander = world.spawn();
        timer_tick_system(&world, 0.016);
        // Nothing to tick: no marker appears anywhere.
        assert_eq!(world.count::<TimerExpired>(), 0);
        assert_eq!(world.count::<ScriptTimer>(), 0);
        assert!(!world.has::<TimerExpired>(bystander));
    }

    #[test]
    fn cleanup_with_no_events_is_safe() {
        let mut world = setup_world();
        let e = world.spawn();
        world.insert(
            e,
            ScriptTimer {
                id: 7,
                remaining: 5.0,
            },
        );
        crate::cleanup::event_cleanup_system(&world, 0.0);
        // A live timer is not an event: cleanup leaves it alone.
        assert_eq!(world.count::<ScriptTimer>(), 1);
        assert_eq!(world.count::<TimerExpired>(), 0);
    }
}
