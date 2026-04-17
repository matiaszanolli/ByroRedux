//! `PhysicsWorld` resource — owns the Rapier simulation state.
//!
//! One `PhysicsWorld` per `byroredux_core::ecs::World`. Inserted as a
//! resource by `main.rs` during scene setup. The entire simulation lives
//! inside this struct — sets, broad phase, narrow phase, pipeline,
//! integration parameters, and a fixed-timestep accumulator.

use byroredux_core::ecs::resource::Resource;
use rapier3d::prelude::*;

/// Fixed physics tick in seconds. 60 Hz matches Skyrim/FO4.
pub const PHYSICS_DT: f32 = 1.0 / 60.0;
/// Cap on substeps per frame to prevent spiral-of-death.
pub const MAX_SUBSTEPS: u32 = 5;

/// Rapier simulation container + fixed-timestep accumulator.
///
/// Held as an ECS resource. Query via `world.resource_mut::<PhysicsWorld>()`.
pub struct PhysicsWorld {
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub islands: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,
    pub pipeline: PhysicsPipeline,
    pub integration_parameters: IntegrationParameters,
    pub gravity: Vector<Real>,
    /// Seconds of unsimulated time left over from the last frame.
    pub accumulator: f32,
}

impl PhysicsWorld {
    /// Create an empty world with Earth gravity (-9.81 m/s² × Bethesda-unit
    /// scale 70) and fixed 60 Hz step.
    ///
    /// Bethesda units ≈ 1.428 cm, so 1 m ≈ 70 BU, and -9.81 m/s² ≈ -686.7 BU/s².
    pub fn new() -> Self {
        let integration_parameters = IntegrationParameters {
            dt: PHYSICS_DT,
            ..Default::default()
        };

        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            pipeline: PhysicsPipeline::new(),
            integration_parameters,
            gravity: Vector::new(0.0, -686.7, 0.0),
            accumulator: 0.0,
        }
    }

    /// Number of live bodies in the simulation.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Advance the simulation by up to `MAX_SUBSTEPS` fixed steps,
    /// draining the accumulator. `frame_dt` is the wall-clock delta
    /// since the last call; anything above `MAX_SUBSTEPS * PHYSICS_DT`
    /// is dropped to avoid spiral-of-death on hitches.
    pub fn step(&mut self, frame_dt: f32) -> u32 {
        self.accumulator += frame_dt.max(0.0);
        let max_acc = MAX_SUBSTEPS as f32 * PHYSICS_DT;
        if self.accumulator > max_acc {
            self.accumulator = max_acc;
        }

        let mut steps = 0u32;
        while self.accumulator >= PHYSICS_DT && steps < MAX_SUBSTEPS {
            self.pipeline.step(
                &self.gravity,
                &self.integration_parameters,
                &mut self.islands,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.ccd_solver,
                Some(&mut self.query_pipeline),
                &(),
                &(),
            );
            self.accumulator -= PHYSICS_DT;
            steps += 1;
        }
        steps
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl Resource for PhysicsWorld {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{collision_shape_to_parts, iso_from_trs};
    use byroredux_core::ecs::components::collision::CollisionShape;
    use byroredux_core::math::{Quat, Vec3};

    /// Test helper: legacy single-`SharedShape` API. Assumes the input
    /// produces exactly one part (every primitive variant does; the
    /// tests here only feed primitives).
    fn single_shape(s: &CollisionShape) -> rapier3d::prelude::SharedShape {
        let mut parts = collision_shape_to_parts(s);
        assert_eq!(parts.len(), 1, "test helper expects a single part");
        parts.swap_remove(0).1
    }

    #[test]
    fn empty_world_has_no_bodies() {
        let w = PhysicsWorld::new();
        assert_eq!(w.body_count(), 0);
    }

    #[test]
    fn dynamic_ball_falls_under_gravity() {
        let mut w = PhysicsWorld::new();

        // Spawn a dynamic ball at y = 1000 BU, well above any floor.
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let body = RigidBodyBuilder::dynamic()
            .position(iso_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
            .build();
        let handle = w.bodies.insert(body);
        let collider = ColliderBuilder::new(shape).build();
        w.colliders
            .insert_with_parent(collider, handle, &mut w.bodies);

        // Step for 1 second of physics time.
        for _ in 0..60 {
            w.step(PHYSICS_DT);
        }

        let y = w.bodies[handle].translation().y;
        assert!(y < 1000.0, "ball did not fall; y = {}", y);
    }

    #[test]
    fn static_floor_blocks_dynamic_ball() {
        let mut w = PhysicsWorld::new();

        // Large static floor at y = 0.
        let floor_shape = single_shape(&CollisionShape::Cuboid {
            half_extents: Vec3::new(500.0, 1.0, 500.0),
        });
        let floor = RigidBodyBuilder::fixed().build();
        let fh = w.bodies.insert(floor);
        w.colliders.insert_with_parent(
            ColliderBuilder::new(floor_shape).build(),
            fh,
            &mut w.bodies,
        );

        // Dynamic ball at y = 200.
        let ball_shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let ball = RigidBodyBuilder::dynamic()
            .position(iso_from_trs(Vec3::new(0.0, 200.0, 0.0), Quat::IDENTITY))
            .build();
        let bh = w.bodies.insert(ball);
        w.colliders.insert_with_parent(
            ColliderBuilder::new(ball_shape).restitution(0.0).build(),
            bh,
            &mut w.bodies,
        );

        // Step 3 seconds.
        for _ in 0..180 {
            w.step(PHYSICS_DT);
        }

        let y = w.bodies[bh].translation().y;
        // Ball rests on top of the 1-unit-thick floor at y ≈ 11.
        assert!(y > 0.0 && y < 50.0, "ball did not settle on floor; y = {y}");
    }

    #[test]
    fn accumulator_caps_substeps() {
        let mut w = PhysicsWorld::new();
        // A huge frame_dt shouldn't run more than MAX_SUBSTEPS steps.
        let steps = w.step(100.0);
        assert!(steps <= MAX_SUBSTEPS);
    }
}
