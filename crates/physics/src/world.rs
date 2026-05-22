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

/// Result of a [`PhysicsWorld::move_character`] step. Mirrors Rapier's
/// `EffectiveCharacterMovement` but with engine-side types so callers
/// don't pull in `rapier3d::prelude::*`. See M28.5.
#[derive(Debug, Clone, Copy)]
pub struct CharacterMoveResult {
    /// Effective translation in engine world-space (Y-up). Apply this
    /// to the character body's Transform + queue as the kinematic
    /// next-translation.
    pub translation: byroredux_core::math::Vec3,
    /// Whether the character ended the step touching the ground.
    /// Read by the controller system to gate jump triggers + zero
    /// vertical velocity on landing.
    pub grounded: bool,
    /// Whether the character is currently sliding down a steep slope
    /// (slope > `max_slope_climb_deg`). Not consumed today; surfaced
    /// for future stamina / damage hooks.
    pub is_sliding_down_slope: bool,
}

/// Movement-step parameters for [`PhysicsWorld::move_character`]. Pure
/// data so the engine-side controller stays decoupled from
/// `rapier3d::control::KinematicCharacterController` field layout.
#[derive(Debug, Clone, Copy)]
pub struct CharacterMoveParams {
    /// Capsule half-height (Y-axis), excludes caps. BU.
    pub capsule_half_height: f32,
    /// Capsule radius. BU.
    pub capsule_radius: f32,
    /// Current body position in engine world-space (Y-up).
    pub position: byroredux_core::math::Vec3,
    /// Desired translation for this step (engine world-space).
    /// Caller is responsible for combining horizontal motion with
    /// gravity-integrated vertical motion into a single vector.
    pub desired_translation: byroredux_core::math::Vec3,
    /// Time-step (seconds) for ground-detection friction.
    pub dt: f32,
    /// Max climbable slope, degrees. KCC default 50°.
    pub max_slope_climb_deg: f32,
    /// Auto-step max height, BU. KCC default 32 BU (~46 cm — covers
    /// canonical Bethesda stairs).
    pub step_height: f32,
    /// Ground-snap distance, BU. Holds the character on terrain
    /// rolls without per-step bouncing.
    pub snap_to_ground: f32,
    /// Optional rapier collider handle to exclude from the
    /// shapecast — pass the character's own collider here so the
    /// KCC doesn't self-hit.
    pub exclude_collider: Option<rapier3d::prelude::ColliderHandle>,
}

impl PhysicsWorld {
    /// Rebuild the `QueryPipeline` BVH from the current `ColliderSet`.
    ///
    /// `pipeline.step()` updates the query pipeline as a side-effect of
    /// each physics tick, but newly-inserted colliders are invisible to
    /// `cast_ray` / `intersection_with_shape` / etc. until the next
    /// step runs. M28.5 character spawn needs to ray-cast the floor
    /// BEFORE the first physics tick (the spawn position depends on
    /// the result), so we call this explicitly after newcomer
    /// registration to flush the BVH.
    pub fn update_query_pipeline(&mut self) {
        self.query_pipeline.update(&self.colliders);
    }

    /// Cast a downward ray from `origin` and return the Y-coordinate
    /// of the first solid hit (the highest solid surface below the
    /// ray's start point), if any. Used by M28.5 character spawn to
    /// place the body on the actual floor rather than at
    /// `aabb.max.y + N` which lands on the building's exterior roof
    /// — that roof has structural gaps the KCC can slip through.
    ///
    /// Ranges over fixed (static) colliders only. `max_distance` is
    /// in BU; pass the AABB height + slack.
    ///
    /// **Caller must have called [`update_query_pipeline`]** since the
    /// last collider insertion, otherwise the BVH is stale and the ray
    /// will report no hits even when colliders exist.
    ///
    /// Returns the world-space Y of the hit; the caller adds capsule
    /// `half_height + offset` to place the capsule centre above the
    /// surface.
    pub fn cast_ray_down(
        &self,
        origin: byroredux_core::math::Vec3,
        max_distance: f32,
    ) -> Option<f32> {
        use rapier3d::prelude::*;
        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector![0.0, -1.0, 0.0],
        );
        // Restrict to fixed bodies — we don't want to spawn the player
        // standing on a dropped barrel. `exclude_dynamic()` is an
        // associated-fn constructor on `QueryFilter`, not a builder
        // method; call it directly.
        let filter = QueryFilter::exclude_dynamic();
        self.query_pipeline
            .cast_ray(
                &self.bodies,
                &self.colliders,
                &ray,
                max_distance,
                /* solid = */ true,
                filter,
            )
            .map(|(_handle, toi)| origin.y - toi)
    }

    /// Diagnostic — compute the AABB of all static colliders in the
    /// world, plus the count. Returns `None` when there are no static
    /// colliders. Used by the M28.5 controller's one-shot "collider
    /// world overlaps character XZ?" sanity log.
    pub fn static_colliders_aabb(
        &self,
    ) -> Option<([f32; 3], [f32; 3], u32)> {
        use rapier3d::prelude::*;
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut count = 0u32;
        for (_h, c) in self.colliders.iter() {
            if let Some(parent) = c.parent() {
                if let Some(rb) = self.bodies.get(parent) {
                    if rb.body_type() == RigidBodyType::Fixed {
                        let aabb = c.compute_aabb();
                        min[0] = min[0].min(aabb.mins.x);
                        min[1] = min[1].min(aabb.mins.y);
                        min[2] = min[2].min(aabb.mins.z);
                        max[0] = max[0].max(aabb.maxs.x);
                        max[1] = max[1].max(aabb.maxs.y);
                        max[2] = max[2].max(aabb.maxs.z);
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            None
        } else {
            Some((min, max, count))
        }
    }

    /// Drive a kinematic character body forward one step using
    /// Rapier's `KinematicCharacterController` (M28.5). Returns the
    /// effective collide-and-slide-corrected motion + grounded status.
    ///
    /// Caller is responsible for:
    ///   1. Combining horizontal WASD-driven motion with vertical
    ///      gravity-integrated motion into `params.desired_translation`.
    ///   2. Applying `result.translation` to the character body's
    ///      `Transform` (engine-side) AND
    ///      `set_next_kinematic_translation` (Rapier-side) so the
    ///      simulation + ECS stay in lockstep.
    ///   3. Resetting `vertical_velocity` to 0 on `result.grounded`
    ///      transitions and to `jump_velocity` on jump triggers.
    pub fn move_character(&self, params: CharacterMoveParams) -> CharacterMoveResult {
        use rapier3d::control::{
            CharacterAutostep, CharacterLength, KinematicCharacterController,
        };
        use rapier3d::prelude::*;

        let mut controller = KinematicCharacterController::default();
        controller.up = Vector::y_axis();
        // M28.5 KCC offset — at Skyrim's 70 BU/m scale, 0.5 BU
        // was only 7 mm of skin between the capsule and any surface,
        // letting the KCC's swept cast graze TriMesh edges and tunnel
        // through tiny gaps (Whiterun Bannered Mare floor planks have
        // ~1-2 BU vertex-gaps where adjacent collision triangles meet;
        // the 0.5 BU offset wasn't enough margin). 4 BU (~5.7 cm) is
        // a typical Rapier KCC offset for Bethesda-scale content —
        // wide enough to keep the capsule from grazing edges, narrow
        // enough that 80 BU doorways still admit the 36 BU-diameter
        // capsule with ~22 BU of margin per side.
        controller.offset = CharacterLength::Absolute(4.0);
        controller.slide = true;
        controller.autostep = Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(params.step_height.max(0.0)),
            min_width: CharacterLength::Absolute(params.capsule_radius.max(0.1)),
            include_dynamic_bodies: false,
        });
        controller.max_slope_climb_angle = params.max_slope_climb_deg.to_radians();
        // Min slide angle: half-way between climb limit and 90° — once
        // the slope is steeper than this, the controller starts
        // sliding the character down instead of trying to hold pose.
        controller.min_slope_slide_angle =
            ((params.max_slope_climb_deg + 90.0) * 0.5).to_radians();
        controller.snap_to_ground = if params.snap_to_ground > 0.0 {
            Some(CharacterLength::Absolute(params.snap_to_ground))
        } else {
            None
        };

        let shape = SharedShape::capsule_y(
            params.capsule_half_height.max(1e-3),
            params.capsule_radius.max(1e-3),
        );
        let pos = Isometry::translation(
            params.position.x,
            params.position.y,
            params.position.z,
        );
        let desired = Vector::new(
            params.desired_translation.x,
            params.desired_translation.y,
            params.desired_translation.z,
        );

        let filter = if let Some(exclude) = params.exclude_collider {
            QueryFilter::default().exclude_collider(exclude)
        } else {
            QueryFilter::default()
        };

        let result = controller.move_shape(
            params.dt.max(1e-6),
            &self.bodies,
            &self.colliders,
            &self.query_pipeline,
            shape.as_ref(),
            &pos,
            desired,
            filter,
            |_| {},
        );

        CharacterMoveResult {
            translation: byroredux_core::math::Vec3::new(
                result.translation.x,
                result.translation.y,
                result.translation.z,
            ),
            grounded: result.grounded,
            is_sliding_down_slope: result.is_sliding_down_slope,
        }
    }
}

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
