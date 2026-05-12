//! Regression tests for `release_victim_item_instances` — issue #896
//! DROP completeness check.
//!
//! The `ItemInstancePool` resource is the bounded arena that holds per-
//! instance state for inventory rows that diverge from their base record
//! (named items, modded weapons, partial-condition armor). Without this
//! release wiring, every cell unload would leak the pool slots referenced
//! by the cell's victim entities — Bethesda's save-bloat tail revisited.

use super::*;
use byroredux_core::ecs::components::{Inventory, ItemStack};
use byroredux_core::ecs::resources::{ItemInstance, ItemInstancePool};
use byroredux_core::ecs::World;

fn world_with_pool() -> World {
    let mut world = World::new();
    world.insert_resource(ItemInstancePool::new());
    world
}

/// Two victim entities each holding an inventory row with an allocated
/// instance — release reclaims both slots, leaving `live_count()` at 0.
#[test]
fn release_drains_pool_slots_for_victim_inventories() {
    let mut world = world_with_pool();

    let id_a = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };
    let id_b = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };
    assert_eq!(world.resource::<ItemInstancePool>().live_count(), 2);

    let actor_a = world.spawn();
    let actor_b = world.spawn();
    let mut inv_a = Inventory::new();
    inv_a.push(ItemStack {
        base_form_id: 0x0001_F4A0,
        count: 1,
        instance: Some(id_a),
    });
    let mut inv_b = Inventory::new();
    inv_b.push(ItemStack {
        base_form_id: 0x0001_F4A1,
        count: 1,
        instance: Some(id_b),
    });
    world.insert(actor_a, inv_a);
    world.insert(actor_b, inv_b);

    release_victim_item_instances(&mut world, &[actor_a, actor_b]);

    assert_eq!(
        world.resource::<ItemInstancePool>().live_count(),
        0,
        "both instance slots should be released back to the free-list",
    );
}

/// Stack-only inventories (`instance: None`, the common case for stimpaks
/// + ammo) must not touch the pool — release is a no-op when no stack
/// carries an allocated instance.
#[test]
fn release_skips_stack_only_inventories() {
    let mut world = world_with_pool();

    let pre_existing = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };
    assert_eq!(world.resource::<ItemInstancePool>().live_count(), 1);

    let actor = world.spawn();
    let mut inv = Inventory::new();
    inv.push(ItemStack::new(0x0001_F4A0, 100));
    inv.push(ItemStack::new(0x0001_F4A1, 50));
    world.insert(actor, inv);

    release_victim_item_instances(&mut world, &[actor]);

    // The pre-existing slot belongs to a non-victim entity — must stay live.
    assert_eq!(
        world.resource::<ItemInstancePool>().live_count(),
        1,
        "stack-only inventories must not release unrelated pool slots",
    );
    let _ = pre_existing;
}

/// Reclaimed slots are observable on the free-list: the next allocate()
/// returns the same `ItemInstanceId` the release surfaced.
#[test]
fn released_slot_is_reclaimed_by_next_allocate() {
    let mut world = world_with_pool();

    let id = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };

    let actor = world.spawn();
    let mut inv = Inventory::new();
    inv.push(ItemStack {
        base_form_id: 0x0001_F4A0,
        count: 1,
        instance: Some(id),
    });
    world.insert(actor, inv);

    release_victim_item_instances(&mut world, &[actor]);

    let reclaimed = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };
    assert_eq!(
        reclaimed, id,
        "free-list reuse must hand back the released slot id",
    );
}

/// Missing `ItemInstancePool` resource is treated as "nothing to release"
/// — early test fixtures and reduced-setup integration tests don't
/// register the pool. The release helper must not panic.
#[test]
fn release_is_noop_when_pool_resource_absent() {
    let mut world = World::new();
    let actor = world.spawn();
    let mut inv = Inventory::new();
    // Even with an allocated-looking instance id, no pool means no
    // release path — the helper short-circuits silently.
    let fake_id = ItemInstanceId(std::num::NonZeroU32::new(1).unwrap());
    inv.push(ItemStack {
        base_form_id: 0x42,
        count: 1,
        instance: Some(fake_id),
    });
    world.insert(actor, inv);

    release_victim_item_instances(&mut world, &[actor]);
}

/// Victim list that contains entities without `Inventory` components
/// (stat-only refs, lights, doors) walks past them without trying to
/// release anything — the SparseSet `get()` returns `None`.
#[test]
fn release_tolerates_victims_without_inventory() {
    let mut world = world_with_pool();

    let id = {
        let mut pool = world.resource_mut::<ItemInstancePool>();
        pool.allocate(ItemInstance::default())
    };

    let inv_actor = world.spawn();
    let bare_actor = world.spawn();
    let mut inv = Inventory::new();
    inv.push(ItemStack {
        base_form_id: 0x0001_F4A0,
        count: 1,
        instance: Some(id),
    });
    world.insert(inv_actor, inv);

    release_victim_item_instances(&mut world, &[bare_actor, inv_actor]);

    assert_eq!(
        world.resource::<ItemInstancePool>().live_count(),
        0,
        "release walks past inventory-less victims and still reclaims the live slot",
    );
}
