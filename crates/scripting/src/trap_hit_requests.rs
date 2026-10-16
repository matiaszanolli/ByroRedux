//! `ObjectReference.ProcessTrapHit` requests (M47.2).
//!
//! CK *ProcessTrapHit*: "tells this reference to handle getting hit by a
//! trap". A script run records the request (target, trap, damage) when its
//! deferred effects flush; [`pending_trap_hit_flush_system`] then lands it as
//! the target's [`crate::HitEvent`] in the Update stage, between the hit
//! producers and the hit consumers.
//!
//! #5872 — the request is queued rather than inserted straight into
//! `HitEvent` because the flush also runs in Late (a spoken dialogue line's
//! fragment), where `event_cleanup_system` drains `HitEvent` before any
//! consumer has run: the hit was dropped. A Late request now waits for the
//! next frame's Update flush. Transient: drained the frame it is flushed.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};

/// One `ProcessTrapHit` call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapHit {
    pub target: EntityId,
    pub trap: EntityId,
    pub damage: f32,
}

/// The `ProcessTrapHit` calls awaiting [`pending_trap_hit_flush_system`].
#[derive(Debug, Default)]
pub struct PendingTrapHits(pub Vec<TrapHit>);

impl Resource for PendingTrapHits {}

/// Land every queued trap hit as its target's [`crate::HitEvent`]. Registered
/// after the script systems that raise the requests and before the
/// `HitEvent` consumers (see `hit_event_consumers_run_after_every_producer`).
pub fn pending_trap_hit_flush_system(world: &World, _dt: f32) {
    let hits = match world.try_resource_mut::<PendingTrapHits>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let Some(mut events) = world.query_mut::<crate::HitEvent>() else {
        return;
    };
    for TrapHit { target, trap, damage } in hits {
        events.insert(
            target,
            crate::HitEvent {
                aggressor: trap,
                source: trap,
                projectile: 0,
                damage,
                power_attack: false,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
    }
}
