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
/// Bethesda units per metre (1 BU ≈ 1.428 cm). The whole simulation runs in
/// BU, so Rapier's length-relative thresholds — sleep velocity, contact
/// prediction distance, allowed penetration — must be told this scale via
/// `IntegrationParameters::length_unit`, or they stay at their metre-scale
/// defaults (≈70× too small) and clutter micro-jitters forever instead of
/// sleeping. Also the scalar behind the ×70 gravity (-9.81 m/s² × 70).
pub const BU_PER_METER: f32 = 70.0;
/// Y (Bethesda units, renderer-space) below which a free-falling dynamic
/// body is considered "lost out of the world" and frozen. The kill-plane
/// only ever inspects *actively-falling* (awake) bodies — anything resting
/// or asleep is never touched — so this just has to sit below the lowest
/// point real clutter could legitimately come to rest. Bethesda exterior
/// terrain bottoms out a few thousand BU below sea level and the deepest
/// interiors a bit more; -25 000 BU (~350 m below the lowest world geometry)
/// clears all of it while still catching a free-faller within a few seconds
/// of leaving the playable volume. See the kill-plane in
/// [`PhysicsWorld::step`].
pub const KILL_PLANE_Y: f32 = -25_000.0;

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
    /// One-shot "something changed, step at least once" flag. Set by any
    /// mutation that can introduce motion (body spawn, kinematic push,
    /// velocity set) via [`PhysicsWorld::wake`]. Cleared the next time the
    /// pipeline actually steps. Lets [`step`](Self::step) skip the (costly)
    /// pipeline run for a fully-asleep scene without missing the first
    /// frame of newly-introduced motion. See the static-scene fast path.
    pending_wake: bool,
}

impl PhysicsWorld {
    /// Create an empty world with Earth gravity (-9.81 m/s² × Bethesda-unit
    /// scale 70) and fixed 60 Hz step.
    ///
    /// Bethesda units ≈ 1.428 cm, so 1 m ≈ 70 BU, and -9.81 m/s² ≈ -686.7 BU/s².
    pub fn new() -> Self {
        let integration_parameters = IntegrationParameters {
            dt: PHYSICS_DT,
            // Tell Rapier the world is in Bethesda units, not metres, so its
            // length-relative thresholds (sleep velocity, contact prediction,
            // allowed penetration) scale correctly. Without this, the sleep
            // threshold is ~70× too small and resting clutter never sleeps —
            // which on its own pins the static-scene fast path awake. See
            // `BU_PER_METER`.
            length_unit: BU_PER_METER,
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
            // Step once on the first frame so any bodies present at startup
            // settle / populate the island state.
            pending_wake: true,
        }
    }

    /// Number of live bodies in the simulation.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Remove a rigid body and its attached colliders from the simulation.
    ///
    /// Returns `true` if `handle` referenced a live body. The body's
    /// colliders are cascaded out via the `remove_attached_colliders =
    /// true` flag, so the caller only needs the `RigidBodyHandle` — the
    /// representative `ColliderHandle` on `RapierHandles` is freed
    /// automatically.
    ///
    /// This is the symmetric counterpart to the `bodies.insert` /
    /// `colliders.insert_with_parent` pair in `physics_sync_system`. It
    /// MUST be called when a simulated entity is despawned (cell unload):
    /// `World::despawn` only drops the `RapierHandles` ECS row, so without
    /// this the body + colliders leak into `RigidBodySet` / `ColliderSet`
    /// and stay in the broad-phase / query-pipeline BVH forever — an
    /// unbounded per-cell-crossing leak. See #1520.
    pub fn remove_body(&mut self, handle: RigidBodyHandle) -> bool {
        self.bodies
            .remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                /* remove_attached_colliders = */ true,
            )
            .is_some()
    }

    /// `(awake dynamic, awake kinematic)` body counts from the last step's
    /// island state — diagnostic for the static-scene fast path.
    pub fn awake_counts(&self) -> (usize, usize) {
        (
            self.islands.active_dynamic_bodies().len(),
            self.islands.active_kinematic_bodies().len(),
        )
    }

    /// Mark the simulation as needing at least one pipeline step on the next
    /// [`step`](Self::step) call. Must be called by every mutation that can
    /// introduce motion — spawning a body, pushing a kinematic target,
    /// setting a velocity — so the static-scene fast path doesn't sleep
    /// through the first frame of new motion (the island lists only reflect
    /// the *previous* step, so a just-woken body isn't in them yet).
    #[inline]
    pub fn wake(&mut self) {
        self.pending_wake = true;
    }

    /// Add a persistent external force (engine world-space, Y-up) to a
    /// dynamic body — Bethesda-unit "Newtons" (body mass × BU/s²). The
    /// force **accumulates across frames** until cleared with
    /// [`reset_forces`](Self::reset_forces); the WATAL buoyancy / flow
    /// systems re-derive it every frame, so they call `reset_forces`
    /// first and `add_force` after. Wakes the body and re-arms the
    /// static-scene fast path so the next [`step`](Self::step) runs (the
    /// island lists only reflect the *previous* step, so a freshly-forced
    /// body isn't in them yet — same reason [`wake`](Self::wake) exists).
    ///
    /// Returns `false` (no-op) if `handle` is dead or non-dynamic — a
    /// static water-plane or kinematic actor can't take a buoyancy force.
    ///
    /// This is the load-bearing prerequisite for water physics: pre-WATAL
    /// the only body mutation exposed was `set_linear_velocity`, so
    /// buoyancy/flow/drag had no application path (see
    /// `docs/engine/watal.md` §7 Phase 2).
    pub fn add_force(&mut self, handle: RigidBodyHandle, force: byroredux_core::math::Vec3) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            if b.body_type() == RigidBodyType::Dynamic {
                b.add_force(vector![force.x, force.y, force.z], true);
                self.wake();
                return true;
            }
        }
        false
    }

    /// Apply an instantaneous impulse (engine world-space, Y-up) to a
    /// dynamic body — changes velocity by `impulse / mass` immediately,
    /// independent of the per-frame force accumulation. Used for one-shot
    /// effects (a splash kick, an actor jumping out of water). Wakes the
    /// body + re-arms the fast path. No-op on dead / non-dynamic handles.
    pub fn apply_impulse(
        &mut self,
        handle: RigidBodyHandle,
        impulse: byroredux_core::math::Vec3,
    ) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            if b.body_type() == RigidBodyType::Dynamic {
                b.apply_impulse(vector![impulse.x, impulse.y, impulse.z], true);
                self.wake();
                return true;
            }
        }
        false
    }

    /// Clear the accumulated external force + torque on a body. Called by
    /// the buoyancy / flow systems at the top of each frame before they
    /// re-`add_force`, so forces don't compound frame-over-frame. No-op on
    /// a dead handle. Does **not** re-arm the fast path (the following
    /// `add_force` does that when there's still a force to apply; a body
    /// with zero net force this frame should be allowed to sleep).
    pub fn reset_forces(&mut self, handle: RigidBodyHandle) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            b.reset_forces(true);
            b.reset_torques(true);
            return true;
        }
        false
    }

    /// Read a dynamic body's mass (BU³ × density). Buoyancy derives the
    /// gravity-cancelling force from this; exposed so the water systems
    /// stay in engine types without reaching into `RigidBodySet`.
    pub fn body_mass(&self, handle: RigidBodyHandle) -> Option<f32> {
        self.bodies.get(handle).map(|b| b.mass())
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

        // Static-scene fast path. A `pipeline.step()` pays full broad-phase
        // + query-pipeline-rebuild cost over *every* collider regardless of
        // motion — on a radius-12 exterior that's ~8-10 ms/step × up to 5
        // substeps, ~40 ms/frame for a scene where nothing is actually
        // moving. Skip it when there's no simulation work:
        //
        //   * No awake dynamic body (`active_dynamic_bodies()` reflects the
        //     previous step; a body can only newly wake via a contact, which
        //     requires something else to have moved — covered by `wake()`).
        //   * Nothing was explicitly woken this frame (`pending_wake`): a
        //     spawned body, a set velocity, or a kinematic push.
        //
        // NOTE: we deliberately do NOT gate on `active_kinematic_bodies()`.
        // Rapier keeps every kinematic body in that set structurally for its
        // whole life (idle ones are just skipped in the solver via a
        // zero-velocity check), so it's never empty in a cell with authored-
        // keyframed clutter — testing it would defeat the fast path entirely.
        // Real kinematic *motion* is captured by `pending_wake` instead
        // (`push_kinematic` / `set_kinematic_translation` call `wake()`).
        if self.islands.active_dynamic_bodies().is_empty() && !self.pending_wake {
            self.accumulator = 0.0;
            return 0;
        }
        self.pending_wake = false;

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
                // Do NOT rebuild the query pipeline inside each substep:
                // `QueryPipeline::update` is O(all colliders) (BVH refit over
                // the whole set), so passing it here rebuilt it up to 5× per
                // frame over ~30 k static colliders — the bulk of the per-step
                // cost. The raycast/overlap accelerator only needs to reflect
                // the post-step collider poses *once* per frame; we refresh it
                // after the loop instead. (Explicit `update_query_pipeline`
                // call sites — e.g. the spawn ground-snap — are unaffected.)
                None,
                &(),
                &(),
            );
            self.accumulator -= PHYSICS_DT;
            steps += 1;
        }
        // Kill-plane. Clutter spawned without a floor beneath it (missing or
        // failed static collision under the placement) free-falls forever: it
        // never rests, so Rapier never sleeps it, so the static-scene fast
        // path above never engages and the cell pays the full per-step cost
        // indefinitely (observed on FNV grid 0,0 — ~12 bodies falling past
        // y=-120 000). Once a dynamic body has fallen unambiguously below any
        // real geometry, freeze it: zero its velocity and put it to sleep so
        // it leaves the active set. It's already invisibly far below the
        // world; this just stops it from pinning the simulation awake.
        if steps > 0 {
            let fallen: Vec<_> = self
                .islands
                .active_dynamic_bodies()
                .iter()
                .copied()
                .filter(|h| {
                    self.bodies
                        .get(*h)
                        .is_some_and(|b| b.translation().y < KILL_PLANE_Y)
                })
                .collect();
            for h in fallen {
                if let Some(b) = self.bodies.get_mut(h) {
                    b.set_linvel(Vector::zeros(), false);
                    b.set_angvel(Vector::zeros(), false);
                    b.sleep();
                }
            }
        }

        // One BVH refit per frame after all substeps, only when something
        // actually stepped (the fast-path early-return above skips this when
        // the scene is asleep and colliders haven't moved).
        if steps > 0 {
            self.query_pipeline.update(&self.colliders);
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
    /// Auto-step minimum platform width (tread depth). BU. Rapier only
    /// steps up when the surface above the obstacle is at least this
    /// wide. Smaller = more permissive. 8 BU handles FNV doorsteps
    /// whose treads are often 8-16 BU deep; using capsule_radius here
    /// blocks autostep on narrow thresholds.
    pub step_min_width: f32,
    /// Ground-snap distance, BU. Holds the character on terrain
    /// rolls without per-step bouncing.
    pub snap_to_ground: f32,
    /// Optional rapier collider handle to exclude from the
    /// shapecast — pass the character's own collider here so the
    /// KCC doesn't self-hit.
    pub exclude_collider: Option<rapier3d::prelude::ColliderHandle>,
    /// `KinematicCharacterController.offset` distance in BU. Sourced
    /// from `ContactConfig::kcc_offset_bu` by the controller system;
    /// surfaced as a param so `move_character` stays pure (no resource
    /// lookups on PhysicsWorld). Wider keeps the capsule from grazing
    /// TriMesh edges; narrower lets the player fit tighter clearances.
    pub kcc_offset_bu: f32,
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
    pub fn static_colliders_aabb(&self) -> Option<([f32; 3], [f32; 3], u32)> {
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
        use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
        use rapier3d::prelude::*;

        // M28.5 KCC offset — at Skyrim's 70 BU/m scale, 0.5 BU
        // was only 7 mm of skin between the capsule and any surface,
        // letting the KCC's swept cast graze TriMesh edges and tunnel
        // through tiny gaps (Whiterun Bannered Mare floor planks have
        // ~1-2 BU vertex-gaps where adjacent collision triangles meet;
        // the 0.5 BU offset wasn't enough margin). The value lives on
        // `ContactConfig::kcc_offset_bu` (default 4 BU ≈ 5.7 cm) and
        // is plumbed through `CharacterMoveParams` so a single resource
        // edit can re-tune every character.
        //
        // Min slide angle: half-way between climb limit and 90° — once
        // the slope is steeper than this, the controller starts
        // sliding the character down instead of trying to hold pose.
        let controller = KinematicCharacterController {
            up: Vector::y_axis(),
            offset: CharacterLength::Absolute(params.kcc_offset_bu.max(0.0)),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(params.step_height.max(0.0)),
                min_width: CharacterLength::Absolute(params.step_min_width.max(0.1)),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: params.max_slope_climb_deg.to_radians(),
            min_slope_slide_angle: ((params.max_slope_climb_deg + 90.0) * 0.5).to_radians(),
            snap_to_ground: if params.snap_to_ground > 0.0 {
                Some(CharacterLength::Absolute(params.snap_to_ground))
            } else {
                None
            },
            ..Default::default()
        };

        let shape = SharedShape::capsule_y(
            params.capsule_half_height.max(1e-3),
            params.capsule_radius.max(1e-3),
        );
        let pos = Isometry::translation(params.position.x, params.position.y, params.position.z);
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
        let mut parts = collision_shape_to_parts(s, &crate::config::ContactConfig::DEFAULT);
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

    /// Static-scene fast path: with nothing awake, `step` must skip the
    /// pipeline after the initial settle frame. This is the optimization
    /// that took a radius-12 FNV exterior from ~45 ms → ~0 ms of physics
    /// per frame (12 → 26 fps). The first step still runs (the constructor
    /// arms `pending_wake` so any startup bodies settle).
    #[test]
    fn static_scene_skips_step_when_nothing_awake() {
        let mut w = PhysicsWorld::new();
        let floor = single_shape(&CollisionShape::Cuboid {
            half_extents: Vec3::new(500.0, 1.0, 500.0),
        });
        let fh = w.bodies.insert(RigidBodyBuilder::fixed().build());
        w.colliders
            .insert_with_parent(ColliderBuilder::new(floor).build(), fh, &mut w.bodies);

        assert!(w.step(PHYSICS_DT) > 0, "first step settles initial state");
        assert_eq!(w.step(PHYSICS_DT), 0, "no dynamics awake → step skipped");
        assert_eq!(w.step(PHYSICS_DT), 0, "stays skipped while idle");
    }

    /// Once asleep, an explicit `wake()` must re-engage the pipeline for the
    /// next frame (then it sleeps again). Mirrors what `set_linear_velocity`
    /// / `set_kinematic_translation` / newcomer registration do on real
    /// motion.
    #[test]
    fn wake_re_engages_stepping() {
        let mut w = PhysicsWorld::new();
        w.step(PHYSICS_DT); // settle
        assert_eq!(w.step(PHYSICS_DT), 0, "asleep");

        w.wake();
        assert!(w.step(PHYSICS_DT) > 0, "wake() must re-engage the step");
        assert_eq!(w.step(PHYSICS_DT), 0, "sleeps again once idle");
    }

    /// A falling dynamic body is awake, so the fast path must NOT skip it —
    /// guards against the gate freezing legitimate motion.
    #[test]
    fn falling_dynamic_keeps_stepping() {
        let mut w = PhysicsWorld::new();
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
                .build(),
        );
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);

        w.step(PHYSICS_DT); // frame 1
                            // Still falling on frame 2 → must keep stepping (not gated away).
        assert!(
            w.step(PHYSICS_DT) > 0,
            "a falling (awake) body must keep the simulation stepping"
        );
    }

    /// Kill-plane: a dynamic body that has fallen far below any geometry
    /// (missing-floor clutter) is frozen so it can't pin the simulation
    /// awake forever. Without this, ~12 such bodies on FNV grid 0,0 free-fell
    /// past y=-120 000 and the fast path never engaged.
    #[test]
    fn kill_plane_freezes_fallen_body() {
        let mut w = PhysicsWorld::new();
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(
                    Vec3::new(0.0, KILL_PLANE_Y - 10_000.0, 0.0),
                    Quat::IDENTITY,
                ))
                .build(),
        );
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);

        // Fresh dynamic body is awake → the first step runs and the kill-plane
        // freezes it (it's below KILL_PLANE_Y).
        w.step(PHYSICS_DT);
        assert!(
            w.bodies[h].is_sleeping(),
            "body below the kill plane must be frozen"
        );

        // And the scene quiesces: within a couple of frames (one for the
        // island set to drop the now-sleeping body) the step is skipped.
        let mut last = w.step(PHYSICS_DT);
        for _ in 0..4 {
            last = w.step(PHYSICS_DT);
        }
        assert_eq!(last, 0, "a frozen body must not keep the sim awake");
    }

    /// `length_unit` must be set to the Bethesda-units scale, or Rapier's
    /// metre-scale sleep / contact thresholds are ~70× too small and clutter
    /// never sleeps (the root cause behind the perpetually-awake bodies).
    #[test]
    fn length_unit_is_bethesda_scale() {
        let w = PhysicsWorld::new();
        assert_eq!(w.integration_parameters.length_unit, BU_PER_METER);
    }

    // ── WATAL Phase 2: external-force API (buoyancy/flow prerequisite) ──

    /// Helper: spawn a dynamic ball at `y` and return its handle.
    fn spawn_ball(w: &mut PhysicsWorld, y: f32) -> rapier3d::prelude::RigidBodyHandle {
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(Vec3::new(0.0, y, 0.0), Quat::IDENTITY))
                .build(),
        );
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);
        h
    }

    /// A sustained upward force greater than gravity must lift a body that
    /// would otherwise fall — the buoyancy path. Force is re-applied each
    /// frame (Rapier forces persist, but re-deriving + resetting is the
    /// system contract) and derived from the body's own mass so the test
    /// makes no magic-number assumption (No-Guessing).
    #[test]
    fn add_force_lifts_body_against_gravity() {
        let mut w = PhysicsWorld::new();
        let h = spawn_ball(&mut w, 1000.0);
        let mass = w.body_mass(h).expect("dynamic body has mass");
        // 2× the gravity-cancelling force → net upward ≈ +1 g.
        let up = byroredux_core::math::Vec3::new(0.0, 2.0 * mass * 686.7, 0.0);

        for _ in 0..30 {
            w.reset_forces(h);
            assert!(w.add_force(h, up), "force applies to a live dynamic body");
            w.step(PHYSICS_DT);
        }

        let y = w.bodies[h].translation().y;
        assert!(y > 1000.0, "net-upward force must raise the body; y = {y}");
    }

    /// An upward impulse must immediately impart upward velocity (vs the
    /// downward velocity a free-falling body would have).
    #[test]
    fn apply_impulse_imparts_upward_velocity() {
        let mut w = PhysicsWorld::new();
        let h = spawn_ball(&mut w, 1000.0);
        let mass = w.body_mass(h).expect("mass");
        // Impulse = mass · Δv; aim for ~+500 BU/s upward.
        let imp = byroredux_core::math::Vec3::new(0.0, mass * 500.0, 0.0);
        assert!(w.apply_impulse(h, imp), "impulse applies to a dynamic body");
        w.step(PHYSICS_DT); // one tick: gravity barely dents +500 BU/s.

        assert!(
            w.bodies[h].linvel().y > 0.0,
            "upward impulse must yield upward velocity; vy = {}",
            w.bodies[h].linvel().y
        );
    }

    /// After `reset_forces`, with nothing re-applied, the body falls under
    /// gravity again — proving the force does not silently persist past a
    /// reset (the frame-over-frame compounding guard).
    #[test]
    fn reset_forces_lets_body_fall_again() {
        let mut w = PhysicsWorld::new();
        let h = spawn_ball(&mut w, 1000.0);
        let mass = w.body_mass(h).expect("mass");
        let up = byroredux_core::math::Vec3::new(0.0, 2.0 * mass * 686.7, 0.0);

        // Hold it up for a bit.
        for _ in 0..10 {
            w.reset_forces(h);
            w.add_force(h, up);
            w.step(PHYSICS_DT);
        }
        let y_held = w.bodies[h].translation().y;

        // Clear the force and stop re-applying → must fall.
        w.reset_forces(h);
        for _ in 0..30 {
            w.step(PHYSICS_DT);
        }
        let y_after = w.bodies[h].translation().y;
        assert!(
            y_after < y_held,
            "after reset the body must fall: held y = {y_held}, after = {y_after}"
        );
    }

    /// The force API must refuse non-dynamic bodies — a static water plane
    /// or a fixed floor can't take a buoyancy force.
    #[test]
    fn force_api_rejects_non_dynamic_bodies() {
        let mut w = PhysicsWorld::new();
        let fh = w.bodies.insert(RigidBodyBuilder::fixed().build());
        let up = byroredux_core::math::Vec3::new(0.0, 1.0, 0.0);
        assert!(!w.add_force(fh, up), "static body must reject add_force");
        assert!(!w.apply_impulse(fh, up), "static body must reject apply_impulse");
        // Dead handle → all no-op.
        let mut w2 = PhysicsWorld::new();
        let dead = w2.bodies.insert(RigidBodyBuilder::dynamic().build());
        w2.remove_body(dead);
        assert!(!w.add_force(dead, up), "dead handle must be a no-op");
    }
}
