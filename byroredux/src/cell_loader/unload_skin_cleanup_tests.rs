//! Regression tests for #1003 / #1004 — `unload_cell` queues victim
//! skin slots for deferred teardown and prunes the `failed_skin_slots`
//! host-side cache. Pre-fix the per-frame eviction pass at the top of
//! `draw_frame` was the only path that reclaimed either; cell-unload-
//! without-render-tick (headless smoke tests, paused world) silently
//! retained slots indefinitely AND a recycled `EntityId` on a fresh
//! NPC was suppressed by the stale "previous attempt failed" bit.
//!
//! These tests exercise the host-side state transformation in
//! isolation via the `queue_skin_unload_victims` helper — VulkanContext
//! is unavailable in `cargo test` (no Vulkan device).

use byroredux_core::ecs::storage::EntityId;
use std::collections::HashSet;

use super::unload::queue_skin_unload_victims;

#[test]
fn victims_with_skin_slots_get_queued() {
    let victims: Vec<EntityId> = vec![10, 11, 12];
    // Mock: entities 10 + 12 have live SkinSlots; 11 does not.
    let slot_present = |eid: EntityId| eid == 10 || eid == 12;
    let mut pending: Vec<EntityId> = Vec::new();
    let mut failed: HashSet<EntityId> = HashSet::new();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);

    assert_eq!(
        pending,
        vec![10, 12],
        "only entities with a live SkinSlot should land in the deferred queue"
    );
}

#[test]
fn victims_without_skin_slots_do_not_pollute_the_queue() {
    let victims: Vec<EntityId> = vec![1, 2, 3, 4, 5];
    // Mock: none of the victims hold a SkinSlot (typical static-prop
    // cell — clutter, lights, terrain — no skinned NPCs).
    let slot_present = |_eid: EntityId| false;
    let mut pending: Vec<EntityId> = Vec::new();
    let mut failed: HashSet<EntityId> = HashSet::new();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);

    assert!(
        pending.is_empty(),
        "static-prop cell unload must not touch the skin queue"
    );
}

#[test]
fn failed_skin_slots_pruned_for_victim_entities() {
    let victims: Vec<EntityId> = vec![20, 21];
    let slot_present = |_eid: EntityId| false;
    let mut pending: Vec<EntityId> = Vec::new();
    // Prior frame's pool-exhausted entities — some are about to be
    // unloaded (20, 21), some are in other cells (22, 23).
    let mut failed: HashSet<EntityId> = [20, 21, 22, 23].into_iter().collect();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);

    assert!(
        !failed.contains(&20) && !failed.contains(&21),
        "victims removed from failed cache"
    );
    assert!(
        failed.contains(&22) && failed.contains(&23),
        "non-victim cache entries preserved"
    );
}

/// #1004's "EntityId recycle hazard" — once the victim's cached
/// failed-bit is pruned, a fresh entity reusing the same id (recycled
/// from the ECS slot pool) is NOT suppressed by the prior failure.
#[test]
fn entity_id_recycle_no_longer_inherits_failed_bit() {
    let victims: Vec<EntityId> = vec![42];
    let slot_present = |_eid: EntityId| false;
    let mut pending: Vec<EntityId> = Vec::new();
    let mut failed: HashSet<EntityId> = [42].into_iter().collect();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);
    // Simulate ECS recycling EntityId 42 for a fresh NPC.
    assert!(
        !failed.contains(&42),
        "stale failed-bit must not poison a recycled EntityId"
    );
}

#[test]
fn empty_failed_cache_short_circuits_without_building_victim_set() {
    let victims: Vec<EntityId> = vec![100, 101, 102];
    let slot_present = |eid: EntityId| eid == 100;
    let mut pending: Vec<EntityId> = Vec::new();
    let mut failed: HashSet<EntityId> = HashSet::new();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);

    // Function still queues the skin victim correctly.
    assert_eq!(pending, vec![100]);
    // No-op on empty cache (the `if failed.is_empty() { return; }`
    // fast-path avoids building the victim_set HashSet).
    assert!(failed.is_empty());
}

#[test]
fn empty_victims_is_idempotent() {
    let victims: Vec<EntityId> = Vec::new();
    let slot_present = |_eid: EntityId| true;
    let mut pending: Vec<EntityId> = vec![999]; // pre-existing entry
    let mut failed: HashSet<EntityId> = [999].into_iter().collect();

    queue_skin_unload_victims(&victims, slot_present, &mut pending, &mut failed);

    // Pre-existing state preserved; empty-victim unload is a no-op.
    assert_eq!(pending, vec![999]);
    assert_eq!(failed.len(), 1);
}
