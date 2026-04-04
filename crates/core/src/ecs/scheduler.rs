//! Scheduler — runs systems in registration order.
//!
//! Currently single-threaded and sequential. The design is intentionally
//! parallel-ready: systems take `&World` (not `&mut World`), and storage
//! access is mediated by RwLock, so adding parallelism is a matter of
//! replacing the loop with a thread pool — no API changes needed.

use super::system::System;
use super::world::World;

pub struct Scheduler {
    systems: Vec<Box<dyn System>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Add a system to the end of the execution list.
    ///
    /// Systems run in the order they are added.
    pub fn add<S: System + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    /// Run all systems in order, passing the shared world and delta time.
    pub fn run(&mut self, world: &World, dt: f32) {
        // TODO: replace this sequential loop with rayon::scope or a
        // dependency-graph executor for parallel system dispatch.
        // The RwLock-per-storage design already supports concurrent
        // reads across systems — the scheduler just needs to stop
        // serialising them.
        for system in &mut self.systems {
            system.run(world, dt);
        }
    }

    /// Returns the names of all registered systems, in execution order.
    pub fn system_names(&self) -> Vec<&str> {
        self.systems.iter().map(|s| s.name()).collect()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::packed::PackedStorage;
    use crate::ecs::sparse_set::SparseSetStorage;
    use crate::ecs::storage::Component;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

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

    // ── Closure as system ───────────────────────────────────────────────

    #[test]
    fn closure_system() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();
        scheduler.add(|world: &World, _dt: f32| {
            let mut q = world.query_mut::<Health>().unwrap();
            for (_, health) in q.iter_mut() {
                health.0 -= 10.0;
            }
        });

        scheduler.run(&world, 1.0 / 60.0);
        assert_eq!(world.get::<Health>(e).unwrap().0, 90.0);
    }

    // ── Struct implementing System ──────────────────────────────────────

    struct DamageOverTime {
        dps: f32,
    }

    impl System for DamageOverTime {
        fn run(&mut self, world: &World, dt: f32) {
            if let Some(mut q) = world.query_mut::<Health>() {
                for (_, health) in q.iter_mut() {
                    health.0 -= self.dps * dt;
                }
            }
        }

        fn name(&self) -> &str {
            "DamageOverTime"
        }
    }

    #[test]
    fn struct_system() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let mut scheduler = Scheduler::new();
        scheduler.add(DamageOverTime { dps: 60.0 });

        scheduler.run(&world, 0.5);
        assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
    }

    // ── Multiple systems, ordered execution ─────────────────────────────

    #[test]
    fn systems_run_in_order() {
        let order = Arc::new(AtomicU32::new(0));

        let order1 = Arc::clone(&order);
        let order2 = Arc::clone(&order);
        let order3 = Arc::clone(&order);

        let mut scheduler = Scheduler::new();

        scheduler.add(move |_world: &World, _dt: f32| {
            assert_eq!(order1.fetch_add(1, Ordering::SeqCst), 0);
        });
        scheduler.add(move |_world: &World, _dt: f32| {
            assert_eq!(order2.fetch_add(1, Ordering::SeqCst), 1);
        });
        scheduler.add(move |_world: &World, _dt: f32| {
            assert_eq!(order3.fetch_add(1, Ordering::SeqCst), 2);
        });

        let world = World::new();
        scheduler.run(&world, 0.0);

        assert_eq!(order.load(Ordering::SeqCst), 3);
    }

    // ── Mutation visible to subsequent system ───────────────────────────

    #[test]
    fn mutation_visible_to_next_system() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 0.0, y: 0.0 });
        world.insert(e, Velocity { dx: 5.0, dy: 10.0 });

        let mut scheduler = Scheduler::new();

        // System 1: apply velocity to position.
        scheduler.add(|world: &World, dt: f32| {
            let (q_vel, mut q_pos) = world.query_2_mut::<Velocity, Position>().unwrap();
            for (entity, vel) in q_vel.iter() {
                if let Some(pos) = q_pos.get_mut(entity) {
                    pos.x += vel.dx * dt;
                    pos.y += vel.dy * dt;
                }
            }
        });

        // System 2: verify position was updated by system 1.
        scheduler.add(|world: &World, _dt: f32| {
            let q = world.query::<Position>().unwrap();
            let pos = q.get(0).unwrap();
            assert!(pos.x > 0.0, "System 1 mutation not visible to System 2");
        });

        scheduler.run(&world, 1.0);

        assert_eq!(world.get::<Position>(e).unwrap().x, 5.0);
        assert_eq!(world.get::<Position>(e).unwrap().y, 10.0);
    }

    // ── Empty scheduler ─────────────────────────────────────────────────

    #[test]
    fn empty_scheduler_runs_cleanly() {
        let mut scheduler = Scheduler::new();
        let world = World::new();
        scheduler.run(&world, 1.0 / 60.0); // no panic
    }

    // ── system_names ────────────────────────────────────────────────────

    #[test]
    fn system_names_in_order() {
        let mut scheduler = Scheduler::new();
        scheduler.add(DamageOverTime { dps: 10.0 });
        scheduler.add(|_world: &World, _dt: f32| {});

        let names = scheduler.system_names();
        assert_eq!(names.len(), 2);
        assert_eq!(names[0], "DamageOverTime");
        // Closure name is the compiler-generated type name — just check it exists.
        assert!(!names[1].is_empty());
    }
}
