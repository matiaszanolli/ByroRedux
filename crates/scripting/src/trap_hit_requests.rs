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
//!
//! #5860 — `Weapon.Fire` projectile hits ride the same queue, and a queued
//! hit whose target already holds this frame's `HitEvent` (a melee strike, an
//! earlier trap or projectile hit) waits a frame instead of replacing it: the
//! hold `npc_combat_ai_system` applies since #4324.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};

/// One `ProcessTrapHit` call, or a projectile hit (`trap` is then the shooter).
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

/// Land every queued hit as its target's [`crate::HitEvent`], holding the
/// ones whose target was already hit this frame for the next. Registered
/// after the script systems that raise the requests and the projectile
/// flight, before the `HitEvent` consumers (see
/// `hit_event_consumers_run_after_every_producer`).
pub fn pending_trap_hit_flush_system(world: &World, _dt: f32) {
    let hits = match world.try_resource_mut::<PendingTrapHits>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    let mut held = Vec::new();
    {
        let Some(mut events) = world.query_mut::<crate::HitEvent>() else {
            return;
        };
        for hit in hits {
            let TrapHit { target, trap, damage } = hit;
            if events.get(target).is_some() {
                held.push(hit);
                continue;
            }
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
    if !held.is_empty() {
        if let Some(mut pending) = world.try_resource_mut::<PendingTrapHits>() {
            pending.0.extend(held);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(aggressor: EntityId, damage: f32) -> crate::HitEvent {
        crate::HitEvent {
            aggressor,
            source: aggressor,
            projectile: 0,
            damage,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        }
    }

    /// #5860 — a queued hit on a target already struck this frame waits a
    /// frame instead of replacing that hit, and two queued hits on one target
    /// land one per frame.
    #[test]
    fn a_queued_hit_never_overwrites_a_same_frame_hit() {
        let mut world = World::new();
        crate::register(&mut world);
        let (target, melee, trap_a, trap_b) = (world.spawn(), world.spawn(), world.spawn(), world.spawn());
        world.insert(target, hit(melee, 10.0));
        world.resource_mut::<PendingTrapHits>().0 = vec![
            TrapHit { target, trap: trap_a, damage: 3.0 },
            TrapHit { target, trap: trap_b, damage: 4.0 },
        ];
        let landed = |world: &World| {
            world
                .query::<crate::HitEvent>()
                .unwrap()
                .get(target)
                .map(|event| (event.aggressor, event.damage))
        };

        pending_trap_hit_flush_system(&world, 0.0);
        assert_eq!(landed(&world), Some((melee, 10.0)), "the melee hit stands");
        assert_eq!(world.resource::<PendingTrapHits>().0.len(), 2, "both trap hits wait");

        // `event_cleanup_system` drains the frame's `HitEvent`s.
        world.remove::<crate::HitEvent>(target);
        pending_trap_hit_flush_system(&world, 0.0);
        assert_eq!(landed(&world), Some((trap_a, 3.0)));
        assert_eq!(world.resource::<PendingTrapHits>().0, vec![TrapHit { target, trap: trap_b, damage: 4.0 }]);

        world.remove::<crate::HitEvent>(target);
        pending_trap_hit_flush_system(&world, 0.0);
        assert_eq!(landed(&world), Some((trap_b, 4.0)));
        assert!(world.resource::<PendingTrapHits>().0.is_empty());
    }
}
