//! Trap contacts, projectile launch and flight, and the pending trap-hit flush.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Trap contacts, projectile launch and flight, and the pending trap-hit flush.
pub(super) fn register_projectiles_systems(scheduler: &mut Scheduler) {
    // M47.2 — `Weapon.Fire` projectiles: start the requested ones, then fly them
    // (a hit on a live actor queues the `HitEvent` the damage system consumes).
    scheduler.add_exclusive(Stage::Update, crate::systems::trap_hits::trap_contact_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_launch_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_flight_system);
    // `HitEvent` is a one-frame marker that `event_cleanup_system` drains in Late, so its
    // consumers run after EVERY producer: the melee / NPC strikes above, the VM effect flush
    // (`ProcessTrapHit`) and the projectile flight just before. (A consumer ahead of a
    // producer never sees its hits: the dart trap's darts dealt no damage that way.)
    // #5872 — `ProcessTrapHit` requests (queued by every script flush, including the Late
    // dialogue one, whose same-frame `HitEvent` would be drained unread) land as `HitEvent`s
    // here: after the script systems and projectile flight, before every consumer.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::pending_trap_hit_flush_system);
}
