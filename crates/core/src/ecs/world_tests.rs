//! Unit tests for `World` — spawn/despawn, query, resource access,
//! interior mutability invariants. Extracted from `world.rs` to keep
//! the production code under ~800 lines; pulled in via
//! `#[cfg(test)] #[path = "..."] mod tests;`.

use super::*;
use crate::ecs::packed::PackedStorage;
use crate::ecs::sparse_set::SparseSetStorage;

struct Health(f32);
impl Component for Health {
    type Storage = SparseSetStorage<Self>;
}

struct Position {
    x: f32,
    y: f32,
}
impl Component for Position {
    type Storage = PackedStorage<Self>;
}

struct Velocity {
    dx: f32,
    dy: f32,
}
impl Component for Velocity {
    type Storage = PackedStorage<Self>;
}

// ── Basic World operations ──────────────────────────────────────────

#[test]
fn spawn_and_insert() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    world.insert(e, Position { x: 1.0, y: 2.0 });

    assert_eq!(world.get::<Health>(e).unwrap().0, 100.0);
    assert_eq!(world.get::<Position>(e).unwrap().x, 1.0);
}

#[test]
fn despawn_removes_every_component() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();

    world.insert(a, Health(100.0));
    world.insert(a, Position { x: 1.0, y: 2.0 });
    world.insert(a, Velocity { dx: 3.0, dy: 4.0 });
    world.insert(b, Health(50.0));
    world.insert(b, Position { x: 5.0, y: 6.0 });

    world.despawn(a);

    assert!(world.get::<Health>(a).is_none(), "Health not removed");
    assert!(world.get::<Position>(a).is_none(), "Position not removed");
    assert!(world.query::<Velocity>().unwrap().get(a).is_none());

    // b is untouched.
    assert_eq!(world.get::<Health>(b).unwrap().0, 50.0);
    assert_eq!(world.get::<Position>(b).unwrap().x, 5.0);
}

#[test]
fn despawn_nonexistent_entity_is_noop() {
    let mut world = World::new();
    let a = world.spawn();
    world.insert(a, Health(100.0));

    // Entity id beyond next_entity — no-op, not a panic.
    world.despawn(12345);

    assert_eq!(world.get::<Health>(a).unwrap().0, 100.0);
}

#[test]
fn despawn_empty_storages_is_noop() {
    let mut world = World::new();
    world.register::<Health>();
    let a = world.spawn();
    // Entity was spawned but never got any component.
    world.despawn(a);
    assert!(world.get::<Health>(a).is_none());
}

#[test]
fn despawn_does_not_reclaim_entity_ids() {
    // next_entity must keep growing — reusing IDs without generation
    // tagging would cause silent component aliasing (see #36, #372).
    let mut world = World::new();
    let a = world.spawn();
    world.insert(a, Health(1.0));
    let next_before = world.next_entity_id();

    world.despawn(a);
    let c = world.spawn();

    assert_eq!(c, next_before, "spawn should advance, not reclaim");
    assert_ne!(c, a, "reclaimed id would alias stale component data");
}

#[test]
fn different_storage_backends() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();

    world.insert(a, Health(50.0));
    world.insert(b, Health(75.0));
    world.insert(a, Position { x: 0.0, y: 0.0 });

    assert_eq!(world.count::<Health>(), 2);
    assert_eq!(world.count::<Position>(), 1);

    assert!(world.has::<Health>(a));
    assert!(world.has::<Health>(b));
    assert!(world.has::<Position>(a));
    assert!(!world.has::<Position>(b));
}

#[test]
fn remove_component() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let removed = world.remove::<Health>(e).unwrap();
    assert_eq!(removed.0, 100.0);
    assert!(!world.has::<Health>(e));
}

#[test]
fn mutate_component() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    world.get_mut::<Health>(e).unwrap().0 -= 25.0;
    assert_eq!(world.get::<Health>(e).unwrap().0, 75.0);
}

#[test]
fn get_nonexistent() {
    let world = World::new();
    assert!(world.get::<Health>(0).is_none());
    assert!(world.get::<Position>(999).is_none());
}

#[test]
fn lazy_storage_init() {
    let world = World::new();
    assert_eq!(world.count::<Health>(), 0);
    assert!(!world.has::<Health>(0));
}

// ── Single-component query ──────────────────────────────────────────

#[test]
fn query_read_single() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();
    world.insert(a, Health(100.0));
    world.insert(b, Health(50.0));

    let q = world.query::<Health>().unwrap();
    assert_eq!(q.get(a).unwrap().0, 100.0);
    assert_eq!(q.get(b).unwrap().0, 50.0);
    assert_eq!(q.len(), 2);
}

#[test]
fn query_write_single() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    {
        let mut q = world.query_mut::<Health>().unwrap();
        q.get_mut(e).unwrap().0 -= 30.0;
    }

    assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
}

#[test]
fn query_write_insert_remove() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();
    world.insert(a, Health(100.0));

    {
        let mut q = world.query_mut::<Health>().unwrap();
        q.insert(b, Health(200.0));
        q.remove(a);
    }

    assert!(world.get::<Health>(a).is_none());
    assert_eq!(world.get::<Health>(b).unwrap().0, 200.0);
}

#[test]
fn query_returns_none_for_unregistered() {
    let world = World::new();
    assert!(world.query::<Health>().is_none());
    assert!(world.query_mut::<Health>().is_none());
}

#[test]
fn query_after_register() {
    let mut world = World::new();
    world.register::<Health>();

    let q = world.query::<Health>().unwrap();
    assert_eq!(q.len(), 0);
}

// ── Multiple concurrent queries ─────────────────────────────────────

#[test]
fn multiple_read_queries_coexist() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    world.insert(e, Position { x: 1.0, y: 2.0 });

    // Two reads at the same time — no deadlock, no borrow error.
    let q_health = world.query::<Health>().unwrap();
    let q_pos = world.query::<Position>().unwrap();

    assert_eq!(q_health.get(e).unwrap().0, 100.0);
    assert_eq!(q_pos.get(e).unwrap().x, 1.0);
}

#[test]
fn query_2_mut_read_and_write() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 10.0, y: 20.0 });
    world.insert(e, Velocity { dx: 5.0, dy: 3.0 });

    {
        let (q_pos, mut q_vel) = world.query_2_mut::<Position, Velocity>().unwrap();

        let pos = q_pos.get(e).unwrap();
        let vel = q_vel.get_mut(e).unwrap();
        // Apply position offset to velocity.
        vel.dx += pos.x;
        vel.dy += pos.y;
    }

    assert_eq!(world.get::<Velocity>(e).unwrap().dx, 15.0);
    assert_eq!(world.get::<Velocity>(e).unwrap().dy, 23.0);
}

#[test]
fn query_2_mut_mut_both_writable() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Velocity { dx: 10.0, dy: 20.0 });

    {
        let (mut q_pos, mut q_vel) = world.query_2_mut_mut::<Position, Velocity>().unwrap();

        let vel = q_vel.get(e).unwrap();
        let dx = vel.dx;
        let dy = vel.dy;

        let pos = q_pos.get_mut(e).unwrap();
        pos.x += dx;
        pos.y += dy;

        let vel = q_vel.get_mut(e).unwrap();
        vel.dx = 0.0;
        vel.dy = 0.0;
    }

    assert_eq!(world.get::<Position>(e).unwrap().x, 11.0);
    assert_eq!(world.get::<Position>(e).unwrap().y, 22.0);
    assert_eq!(world.get::<Velocity>(e).unwrap().dx, 0.0);
}

#[test]
#[should_panic(expected = "must be different component types")]
fn query_2_mut_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _ = world.query_2_mut::<Health, Health>();
}

#[test]
#[should_panic(expected = "must be different component types")]
fn query_2_mut_mut_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _ = world.query_2_mut_mut::<Health, Health>();
}

// ── Iteration ───────────────────────────────────────────────────────

#[test]
fn query_iter() {
    let mut world = World::new();
    for i in 0..5 {
        let e = world.spawn();
        world.insert(e, Health(i as f32 * 10.0));
    }

    let q = world.query::<Health>().unwrap();
    let sum: f32 = q.iter().map(|(_, h)| h.0).sum();
    assert_eq!(sum, 100.0); // 0 + 10 + 20 + 30 + 40
}

#[test]
fn query_iter_mut() {
    let mut world = World::new();
    for i in 0..3 {
        let e = world.spawn();
        world.insert(e, Health(i as f32 * 10.0));
    }

    {
        let mut q = world.query_mut::<Health>().unwrap();
        for (_, health) in q.iter_mut() {
            health.0 *= 2.0;
        }
    }

    let q = world.query::<Health>().unwrap();
    let mut values: Vec<f32> = q.iter().map(|(_, h)| h.0).collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(values, vec![0.0, 20.0, 40.0]);
}

// ── Intersection iteration (the real-world use case) ────────────────

#[test]
fn intersection_iteration() {
    let mut world = World::new();

    // Entity 0: has both Position + Velocity
    let e0 = world.spawn();
    world.insert(e0, Position { x: 0.0, y: 0.0 });
    world.insert(e0, Velocity { dx: 1.0, dy: 2.0 });

    // Entity 1: only Position
    let e1 = world.spawn();
    world.insert(e1, Position { x: 5.0, y: 5.0 });

    // Entity 2: has both
    let e2 = world.spawn();
    world.insert(e2, Position { x: 10.0, y: 10.0 });
    world.insert(e2, Velocity { dx: 3.0, dy: 4.0 });

    {
        let (q_vel, mut q_pos) = world.query_2_mut::<Velocity, Position>().unwrap();

        // Iterate the smaller set (velocity), look up in the larger.
        for (entity, vel) in q_vel.iter() {
            if let Some(pos) = q_pos.get_mut(entity) {
                pos.x += vel.dx;
                pos.y += vel.dy;
            }
        }
    }

    // e0 moved, e1 untouched, e2 moved.
    assert_eq!(world.get::<Position>(e0).unwrap().x, 1.0);
    assert_eq!(world.get::<Position>(e0).unwrap().y, 2.0);
    assert_eq!(world.get::<Position>(e1).unwrap().x, 5.0);
    assert_eq!(world.get::<Position>(e1).unwrap().y, 5.0);
    assert_eq!(world.get::<Position>(e2).unwrap().x, 13.0);
    assert_eq!(world.get::<Position>(e2).unwrap().y, 14.0);
}

// ── Resource tests ──────────────────────────────────────────────────

struct DeltaTime(f32);
impl Resource for DeltaTime {}

struct GameConfig {
    gravity: f32,
    max_speed: f32,
}
impl Resource for GameConfig {}

#[test]
fn resource_insert_and_read() {
    let mut world = World::new();
    world.insert_resource(DeltaTime(1.0 / 60.0));

    let dt = world.resource::<DeltaTime>();
    assert!((dt.0 - 1.0 / 60.0).abs() < f32::EPSILON);
}

#[test]
fn resource_insert_and_mutate() {
    let mut world = World::new();
    world.insert_resource(DeltaTime(1.0 / 60.0));

    {
        let mut dt = world.resource_mut::<DeltaTime>();
        dt.0 = 1.0 / 30.0;
    }

    let dt = world.resource::<DeltaTime>();
    assert!((dt.0 - 1.0 / 30.0).abs() < f32::EPSILON);
}

#[test]
fn two_resource_types_coexist() {
    let mut world = World::new();
    world.insert_resource(DeltaTime(0.016));
    world.insert_resource(GameConfig {
        gravity: -9.81,
        max_speed: 50.0,
    });

    // Both readable at the same time.
    let dt = world.resource::<DeltaTime>();
    let config = world.resource::<GameConfig>();
    assert!((dt.0 - 0.016).abs() < f32::EPSILON);
    assert!((config.gravity - -9.81).abs() < f32::EPSILON);
    assert!((config.max_speed - 50.0).abs() < f32::EPSILON);
}

#[test]
#[should_panic(expected = "Resource `")]
fn missing_resource_panics_with_type_name() {
    let world = World::new();
    let _ = world.resource::<DeltaTime>();
}

#[test]
#[should_panic(expected = "not found")]
fn missing_resource_mut_panics() {
    let world = World::new();
    let _ = world.resource_mut::<DeltaTime>();
}

#[test]
fn remove_resource_returns_value() {
    let mut world = World::new();
    world.insert_resource(DeltaTime(0.016));

    let removed = world.remove_resource::<DeltaTime>().unwrap();
    assert!((removed.0 - 0.016).abs() < f32::EPSILON);

    // Gone now.
    assert!(world.try_resource::<DeltaTime>().is_none());
}

#[test]
fn remove_nonexistent_resource_returns_none() {
    let mut world = World::new();
    assert!(world.remove_resource::<DeltaTime>().is_none());
}

#[test]
fn resource_overwrite_returns_old() {
    let mut world = World::new();
    let first = world.insert_resource(DeltaTime(0.016));
    assert!(first.is_none()); // no previous value

    let second = world.insert_resource(DeltaTime(0.033));
    assert!(second.is_some());
    assert!((second.unwrap().0 - 0.016).abs() < f32::EPSILON);

    let dt = world.resource::<DeltaTime>();
    assert!((dt.0 - 0.033).abs() < f32::EPSILON);
}

#[test]
fn resource_visible_to_system_via_scheduler() {
    use crate::ecs::scheduler::Scheduler;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    world.insert_resource(DeltaTime(0.5));

    let mut scheduler = Scheduler::new();
    scheduler.add(|world: &World, _dt: f32| {
        let dt = world.resource::<DeltaTime>();
        let mut q = world.query_mut::<Health>().unwrap();
        for (_, health) in q.iter_mut() {
            // Drain 60 HP/sec.
            health.0 -= 60.0 * dt.0;
        }
    });

    scheduler.run(&world, 0.0);
    assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
}

#[test]
fn try_resource_returns_none_when_missing() {
    let world = World::new();
    assert!(world.try_resource::<DeltaTime>().is_none());
    assert!(world.try_resource_mut::<DeltaTime>().is_none());
}

// ── Name + StringPool + find_by_name ────────────────────────────────

// ── FormIdComponent + find_by_form_id ──────────────────────────────

use crate::ecs::components::FormIdComponent;
use crate::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

#[test]
fn form_id_component_attach_and_query() {
    let mut world = World::new();
    world.insert_resource(FormIdPool::new());

    let pair = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x000014),
    };
    let fid = world.resource_mut::<FormIdPool>().intern(pair);

    let e = world.spawn();
    world.insert(e, FormIdComponent(fid));

    let got = world.get::<FormIdComponent>(e).unwrap();
    assert_eq!(got.0, fid);
}

#[test]
fn find_by_form_id_hit() {
    let mut world = World::new();
    world.insert_resource(FormIdPool::new());

    let pair = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x000014),
    };
    let fid = world.resource_mut::<FormIdPool>().intern(pair);

    let e = world.spawn();
    world.insert(e, FormIdComponent(fid));

    assert_eq!(world.find_by_form_id(fid), Some(e));
}

#[test]
fn find_by_form_id_miss() {
    let mut world = World::new();
    world.insert_resource(FormIdPool::new());

    let pair_a = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x000014),
    };
    let pair_b = FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x000015),
    };
    let fid_a = world.resource_mut::<FormIdPool>().intern(pair_a);
    let fid_b = world.resource_mut::<FormIdPool>().intern(pair_b);

    let e = world.spawn();
    world.insert(e, FormIdComponent(fid_a));

    assert!(world.find_by_form_id(fid_b).is_none());
}

#[test]
fn find_by_form_id_no_components() {
    let world = World::new();
    let mut pool = FormIdPool::new();
    let fid = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(0x001),
    });
    assert!(world.find_by_form_id(fid).is_none());
}

#[test]
fn form_id_pool_as_world_resource() {
    let mut world = World::new();
    world.insert_resource(FormIdPool::new());

    let pair = FormIdPair {
        plugin: PluginId::from_filename("Oblivion.esm"),
        local: LocalFormId(0x100),
    };

    let fid = world.resource_mut::<FormIdPool>().intern(pair);
    let pool = world.resource::<FormIdPool>();
    assert_eq!(pool.resolve(fid).unwrap().local, LocalFormId(0x100));
    assert_eq!(pool.len(), 1);
    assert!(!pool.is_empty());
}

// ── Name + StringPool + find_by_name ────────────────────────────────

use crate::ecs::components::Name;
use crate::string::StringPool;

#[test]
fn name_component_attach_and_query() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let sym = world.resource_mut::<StringPool>().intern("player");
    let e = world.spawn();
    world.insert(e, Name(sym));

    let name = world.get::<Name>(e).unwrap();
    assert_eq!(name.0, sym);

    let pool = world.resource::<StringPool>();
    assert_eq!(pool.resolve(name.0), Some("player"));
}

#[test]
fn find_by_name_hit() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let sym = world.resource_mut::<StringPool>().intern("hero");
    let e = world.spawn();
    world.insert(e, Name(sym));

    assert_eq!(world.find_by_name("hero"), Some(e));
}

#[test]
fn find_by_name_miss() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let sym = world.resource_mut::<StringPool>().intern("hero");
    let e = world.spawn();
    world.insert(e, Name(sym));

    assert!(world.find_by_name("villain").is_none());
}

#[test]
fn find_by_name_no_pool() {
    let world = World::new();
    assert!(world.find_by_name("anything").is_none());
}

#[test]
fn find_by_name_no_name_components() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());
    world.resource_mut::<StringPool>().intern("ghost");

    assert!(world.find_by_name("ghost").is_none());
}

#[test]
fn string_pool_as_world_resource() {
    let mut world = World::new();
    world.insert_resource(StringPool::new());

    let sym = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern("asset/texture.png")
    };

    let pool = world.resource::<StringPool>();
    assert_eq!(pool.resolve(sym), Some("asset/texture.png"));
}

// ── Regression: remove/get_mut must not create empty storage (#39) ──

#[test]
fn remove_nonexistent_does_not_create_storage() {
    let mut world = World::new();
    // Remove a component type that was never inserted.
    assert!(world.remove::<Health>(0).is_none());
    // query should still return None (no storage created).
    assert!(world.query::<Health>().is_none());
}

#[test]
#[should_panic(expected = "was never spawned")]
#[cfg(debug_assertions)]
fn insert_unspawned_entity_panics_debug() {
    let mut world = World::new();
    // Entity 999 was never spawned — should panic in debug mode.
    world.insert(999, Health(100.0));
}

#[test]
fn get_mut_nonexistent_does_not_create_storage() {
    let mut world = World::new();
    // get_mut on a type that was never inserted.
    assert!(world.get_mut::<Health>(0).is_none());
    // query should still return None.
    assert!(world.query::<Health>().is_none());
}

// ── resource_2_mut tests ────────────────────────────────────────

struct ResA(f32);
impl Resource for ResA {}
struct ResB(f32);
impl Resource for ResB {}

#[test]
fn resource_2_mut_both_writable() {
    let mut world = World::new();
    world.insert_resource(ResA(1.0));
    world.insert_resource(ResB(2.0));

    {
        let (mut a, mut b) = world.resource_2_mut::<ResA, ResB>();
        a.0 += 10.0;
        b.0 += 20.0;
    }

    let a = world.resource::<ResA>();
    let b = world.resource::<ResB>();
    assert!((a.0 - 11.0).abs() < 1e-6);
    assert!((b.0 - 22.0).abs() < 1e-6);
}

#[test]
fn resource_2_mut_reverse_order_same_result() {
    let mut world = World::new();
    world.insert_resource(ResA(1.0));
    world.insert_resource(ResB(2.0));

    // Acquire in reverse generic order — lock ordering should still prevent deadlock.
    let (mut b, mut a) = world.resource_2_mut::<ResB, ResA>();
    b.0 = 99.0;
    a.0 = 88.0;
    drop(b);
    drop(a);

    assert!((world.resource::<ResA>().0 - 88.0).abs() < 1e-6);
    assert!((world.resource::<ResB>().0 - 99.0).abs() < 1e-6);
}

#[test]
#[should_panic(expected = "must be different resource types")]
fn resource_2_mut_same_type_panics() {
    let mut world = World::new();
    world.insert_resource(ResA(1.0));
    let _ = world.resource_2_mut::<ResA, ResA>();
}

#[test]
fn try_resource_2_mut_returns_some_when_both_present() {
    let mut world = World::new();
    world.insert_resource(ResA(1.0));
    world.insert_resource(ResB(2.0));

    {
        let (mut a, mut b) = world
            .try_resource_2_mut::<ResA, ResB>()
            .expect("both resources present, expected Some");
        a.0 = 7.0;
        b.0 = 9.0;
    }
    assert!((world.resource::<ResA>().0 - 7.0).abs() < 1e-6);
    assert!((world.resource::<ResB>().0 - 9.0).abs() < 1e-6);
}

#[test]
fn try_resource_2_mut_returns_none_when_a_missing() {
    let mut world = World::new();
    world.insert_resource(ResB(2.0));
    assert!(world.try_resource_2_mut::<ResA, ResB>().is_none());
}

#[test]
fn try_resource_2_mut_returns_none_when_b_missing() {
    let mut world = World::new();
    world.insert_resource(ResA(1.0));
    assert!(world.try_resource_2_mut::<ResA, ResB>().is_none());
}

#[test]
fn try_resource_2_mut_returns_none_when_both_missing() {
    let world = World::new();
    assert!(world.try_resource_2_mut::<ResA, ResB>().is_none());
}

#[test]
#[should_panic(expected = "must be different resource types")]
fn try_resource_2_mut_same_type_panics() {
    let world = World::new();
    // Panics regardless of whether the resource is present — same-type
    // lock would deadlock.
    let _ = world.try_resource_2_mut::<ResA, ResA>();
}

// ── insert_batch equivalence (#512) ─────────────────────────────────

/// Regression: `insert_batch` must produce a World state bit-identical
/// to a serial `insert` loop with the same items. SparseSet backend.
#[test]
fn insert_batch_equivalent_to_serial_insert_sparse_set() {
    const N: u32 = 500;

    let mut world_a = World::new();
    let mut world_b = World::new();
    let entities_a: Vec<_> = (0..N).map(|_| world_a.spawn()).collect();
    let entities_b: Vec<_> = (0..N).map(|_| world_b.spawn()).collect();

    // World A: serial inserts.
    for (i, &e) in entities_a.iter().enumerate() {
        world_a.insert(e, Health(i as f32));
    }

    // World B: one batch insert.
    world_b.insert_batch(
        entities_b
            .iter()
            .enumerate()
            .map(|(i, &e)| (e, Health(i as f32))),
    );

    // Same entity count, same stored values.
    let qa = world_a.query::<Health>().unwrap();
    let qb = world_b.query::<Health>().unwrap();
    let collect_a: Vec<_> = qa.iter().map(|(e, h)| (e, h.0)).collect();
    let collect_b: Vec<_> = qb.iter().map(|(e, h)| (e, h.0)).collect();
    assert_eq!(collect_a, collect_b);
    assert_eq!(collect_a.len() as u32, N);
}

/// Same coverage for the PackedStorage backend — the two storage
/// impls route through different `insert` paths (sparse-set vs
/// binary-search insert-at-position).
#[test]
fn insert_batch_equivalent_to_serial_insert_packed() {
    const N: u32 = 500;

    let mut world_a = World::new();
    let mut world_b = World::new();
    let entities_a: Vec<_> = (0..N).map(|_| world_a.spawn()).collect();
    let entities_b: Vec<_> = (0..N).map(|_| world_b.spawn()).collect();

    for (i, &e) in entities_a.iter().enumerate() {
        world_a.insert(
            e,
            Position {
                x: i as f32,
                y: -(i as f32),
            },
        );
    }
    world_b.insert_batch(entities_b.iter().enumerate().map(|(i, &e)| {
        (
            e,
            Position {
                x: i as f32,
                y: -(i as f32),
            },
        )
    }));

    let qa = world_a.query::<Position>().unwrap();
    let qb = world_b.query::<Position>().unwrap();
    let a: Vec<_> = qa.iter().map(|(e, p)| (e, p.x, p.y)).collect();
    let b: Vec<_> = qb.iter().map(|(e, p)| (e, p.x, p.y)).collect();
    assert_eq!(a, b);
}

/// Empty batch is a no-op — no storage is created if it didn't exist,
/// and existing storages are untouched.
#[test]
fn insert_batch_empty_iterator_is_noop() {
    let mut world = World::new();
    world.insert_batch::<Health, _>(std::iter::empty());
    // Query must still be None — storage wasn't created by the empty
    // batch.
    // Actually — `storage_write` creates the storage eagerly. Verify
    // the contract: empty storage is fine, query returns an empty
    // iterator (not None — because storage exists but has no rows).
    let q = world.query::<Health>();
    match q {
        Some(q) => assert_eq!(q.iter().count(), 0),
        None => {} // also acceptable if future change makes it lazy
    }
}

/// Overwrite semantics — inserting the same entity twice via batch
/// must overwrite, same as serial `insert`.
#[test]
fn insert_batch_overwrites_existing_component() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(1.0));
    world.insert_batch([(e, Health(99.0))]);
    let q = world.query::<Health>().unwrap();
    let h = q.iter().find(|(eid, _)| *eid == e).map(|(_, h)| h.0);
    assert_eq!(h, Some(99.0));
}

// ── Deadlock detection (debug-only) ────────────────────────────────

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn query_read_then_write_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _read = world.query::<Health>().unwrap();
    let _write = world.query_mut::<Health>(); // deadlock → panic
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn query_write_then_read_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _write = world.query_mut::<Health>().unwrap();
    let _read = world.query::<Health>(); // deadlock → panic
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn query_write_then_write_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _w1 = world.query_mut::<Health>().unwrap();
    let _w2 = world.query_mut::<Health>(); // deadlock → panic
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn resource_read_then_write_same_type_panics() {
    let mut world = World::new();
    world.insert_resource(ResA(42.0));

    let _read = world.try_resource::<ResA>().unwrap();
    let _write = world.resource_mut::<ResA>(); // deadlock → panic
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn resource_write_then_read_same_type_panics() {
    let mut world = World::new();
    world.insert_resource(ResA(42.0));

    let _write = world.resource_mut::<ResA>();
    let _read = world.resource::<ResA>(); // deadlock → panic
}

#[test]
fn query_read_then_write_different_types_ok() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let _read = world.query::<Health>().unwrap();
    let _write = world.query_mut::<Position>().unwrap();
    // No panic — different types.
}

#[test]
fn sequential_query_after_drop_ok() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    {
        let _write = world.query_mut::<Health>().unwrap();
    }
    // Write dropped — read should succeed.
    let _read = world.query::<Health>().unwrap();
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn get_then_query_mut_same_type_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _ref = world.get::<Health>(e).unwrap();
    let _write = world.query_mut::<Health>(); // deadlock → panic
}

#[test]
#[should_panic(expected = "ECS deadlock detected")]
fn has_while_holding_write_panics() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));

    let _write = world.query_mut::<Health>().unwrap();
    let _ = world.has::<Health>(e); // deadlock → panic
}

// ── Poisoned-lock cascade reporting (issue #95) ─────────────────────

#[test]
fn poisoned_storage_lock_panics_with_type_name() {
    use std::sync::Arc;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    let world = Arc::new(world);

    // Poison the Health storage from another thread.
    let w = Arc::clone(&world);
    let _ = std::thread::spawn(move || {
        let _q = w.query_mut::<Health>().unwrap();
        panic!("intentional panic to poison the lock");
    })
    .join();

    // Now any access to Health from this thread should surface a
    // type-aware panic — not the generic "lock poisoned".
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world.query::<Health>();
    }));
    let err = result.expect_err("expected poisoned-lock panic");
    let msg = err
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("Health") && msg.contains("poisoned"),
        "panic message should name the component type and mention poisoning, got: {msg}"
    );
}

#[test]
fn poisoned_resource_lock_panics_with_type_name() {
    use std::sync::Arc;

    let mut world = World::new();
    world.insert_resource(ResA(42.0));
    let world = Arc::new(world);

    let w = Arc::clone(&world);
    let _ = std::thread::spawn(move || {
        let _r = w.resource_mut::<ResA>();
        panic!("intentional panic to poison the lock");
    })
    .join();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world.try_resource::<ResA>();
    }));
    let err = result.expect_err("expected poisoned-lock panic");
    let msg = err
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("ResA") && msg.contains("poisoned"),
        "panic message should name the resource type and mention poisoning, got: {msg}"
    );
}

/// Regression test for issue #36: `spawn()` must panic when the
/// `EntityId` counter would overflow, not wrap silently.
#[test]
#[should_panic(expected = "overflowed EntityId")]
fn spawn_panics_on_entity_id_overflow() {
    let mut world = World::new();
    // Jam the counter to u32::MAX. The next spawn() returns MAX
    // and the increment inside should panic.
    world.next_entity = EntityId::MAX;
    let last = world.spawn();
    assert_eq!(last, EntityId::MAX);
    // This call must panic — u32::MAX + 1 overflows.
    let _ = world.spawn();
}

/// Regression test for issue #137: when the poisoned-lock panic
/// path fires inside `query`/`resource`/etc., the RAII `TrackedRead`/
/// `TrackedWrite` scope guard must untrack the pending row. Before
/// the fix, `track_read` / `track_write` were called directly before
/// `lock.read()/write()`, and the stale row leaked into the
/// thread-local tracker — a subsequent `catch_unwind` recovery
/// would then see a false "deadlock detected" panic on the same type.
#[test]
fn lock_tracker_is_clean_after_poisoned_panic() {
    use super::super::lock_tracker;
    use std::sync::Arc;

    // Sanity check: tracker is empty at the start of this test. In
    // debug builds the thread-local map is per-thread and each test
    // runs on a fresh worker thread, so this must hold.
    assert!(lock_tracker::is_clean(), "lock tracker must start clean");

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    let world = Arc::new(world);

    // Poison the Health storage from another thread.
    let w = Arc::clone(&world);
    let _ = std::thread::spawn(move || {
        let _q = w.query_mut::<Health>().unwrap();
        panic!("intentional panic to poison the lock");
    })
    .join();

    // Each of the nine affected methods must leave the tracker
    // clean after its poisoned-lock panic unwinds. Iterating over
    // the representative set (single read/write, 2-read/write,
    // resource read/write, resource 2-write, try_read/try_write)
    // covers every `TrackedRead::new` / `TrackedWrite::new` call
    // site in `world.rs`.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world.query::<Health>();
    }));
    assert!(
        lock_tracker::is_clean(),
        "tracker row leaked after query<Health> poison-panic"
    );

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world.query_mut::<Health>();
    }));
    assert!(
        lock_tracker::is_clean(),
        "tracker row leaked after query_mut<Health> poison-panic"
    );

    // query_2_mut: the FIRST tracked scope must also untrack if
    // the second lock panics. Here the Health lock is poisoned —
    // the exact arm that panics depends on TypeId ordering, but
    // either way both scopes must untrack cleanly.
    let mut world2 = World::new();
    let e = world2.spawn();
    world2.insert(e, Health(100.0));
    world2.insert(e, Position { x: 0.0, y: 0.0 });
    let world2 = Arc::new(world2);
    let w = Arc::clone(&world2);
    let _ = std::thread::spawn(move || {
        let _q = w.query_mut::<Health>().unwrap();
        panic!("intentional panic to poison the lock");
    })
    .join();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world2.query_2_mut::<Position, Health>();
    }));
    assert!(
        lock_tracker::is_clean(),
        "tracker row leaked after query_2_mut poison-panic"
    );

    // Resource path mirrors the storage path.
    let mut world3 = World::new();
    world3.insert_resource(ResA(42.0));
    let world3 = Arc::new(world3);
    let w = Arc::clone(&world3);
    let _ = std::thread::spawn(move || {
        let _r = w.resource_mut::<ResA>();
        panic!("intentional panic to poison the resource lock");
    })
    .join();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = world3.try_resource::<ResA>();
    }));
    assert!(
        lock_tracker::is_clean(),
        "tracker row leaked after try_resource<ResA> poison-panic"
    );
}

/// Regression test for issue #466: when `World::despawn` walks the
/// type-erased storage map and trips a poisoned lock, the panic must
/// name the offending component type instead of the literal
/// `"<unknown>"`. Pre-fix the loop discarded the `TypeId` and passed
/// `"<unknown>"` to the helper, masking the cascade source.
#[test]
fn despawn_poisoned_lock_panics_with_type_name() {
    use std::sync::Arc;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Health(100.0));
    let world_arc = Arc::new(world);

    // Poison the Health storage from another thread.
    let w = Arc::clone(&world_arc);
    let _ = std::thread::spawn(move || {
        let _q = w.query_mut::<Health>().unwrap();
        panic!("intentional panic to poison the lock");
    })
    .join();

    // Reclaim ownership now that the worker thread has dropped its
    // Arc clone. `try_unwrap` succeeds because strong_count == 1.
    let mut world = Arc::try_unwrap(world_arc)
        .unwrap_or_else(|_| panic!("Arc still aliased after worker join"));

    // `despawn` walks all storages with `&mut self`. The poisoned
    // Health lock should now panic — and the message must name
    // `Health`, not `<unknown>`.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.despawn(e);
    }));
    let err = result.expect_err("expected poisoned-lock panic from despawn");
    let msg = err
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("Health") && msg.contains("poisoned"),
        "despawn panic should name the component type, got: {msg}"
    );
}
