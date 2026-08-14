//! `PhysicsWorld` resource — owns the Rapier simulation state.
//!
//! One `PhysicsWorld` per `byroredux_core::ecs::World`. Inserted as a
//! resource by `main.rs` during scene setup. The entire simulation lives
//! inside this struct — sets, broad phase, narrow phase, pipeline,
//! integration parameters, and a fixed-timestep accumulator.

use byroredux_core::ecs::components::MotionType;
use byroredux_core::ecs::resource::Resource;
use rapier3d::prelude::*;

/// Fixed physics tick in seconds. 60 Hz matches Skyrim/FO4.
pub const PHYSICS_DT: f32 = 1.0 / 60.0;
/// Cap on substeps per frame to prevent spiral-of-death.
pub const MAX_SUBSTEPS: u32 = 5;
/// Per-frame wall-clock budget (seconds) for physics catch-up substeps.
///
/// Anti-spiral guard for #1698. The fixed-timestep loop normally runs up
/// to [`MAX_SUBSTEPS`] catch-up steps so simulated time keeps tracking
/// wall time when rendering dips below 60 Hz. But on cell entry a *settle
/// storm* — hundreds of clutter bodies waking at once to resolve authored
/// spawn-interpenetration — makes each substep cost tens of ms over the
/// awake island. Running the full 5-substep catch-up budget then makes
/// the frame slow enough to demand 5 substeps *again* next frame: a stable
/// ~7 fps plateau (Whiterun Dragonsreach, measured 144 ms/frame) that only
/// clears ~28 s later when the bodies finally re-sleep.
///
/// Once the substeps run this frame have eaten this much wall-time, more
/// catch-up is futile — physics already can't keep real-time, so each
/// extra substep produces less simulated time than the wall-time it costs
/// — and [`PhysicsWorld::step`] stops, forfeiting the remaining backlog
/// (slight slow-motion) rather than collapsing the frame rate. Anchored to
/// [`PHYSICS_DT`]: one sim-tick of wall-time is exactly that break-even
/// point, so the common case (a handful of sub-millisecond substeps) never
/// approaches it and steady-state behaviour is unchanged. This amortizes
/// the same settle work across more, individually-cheap frames (the
/// "amortize across frames" resolution the issue calls for) — Dragonsreach
/// goes from a 7 fps plateau to a playable frame rate for the same ~28 s.
pub const SUBSTEP_TIME_BUDGET: f32 = PHYSICS_DT;
/// Bethesda units per metre (1 BU ≈ 1.428 cm). The whole simulation runs in
/// BU, so Rapier's length-relative thresholds — sleep velocity, contact
/// prediction distance, allowed penetration — must be told this scale via
/// `IntegrationParameters::length_unit`, or they stay at their metre-scale
/// defaults (≈70× too small) and clutter micro-jitters forever instead of
/// sleeping. Also the scalar behind the ×70 gravity (-9.81 m/s² × 70).
pub const BU_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
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

/// Collision-group bit reserved for a **live actor's keyframed ragdoll-bone**
/// colliders (#2873).
///
/// `keyframe_live_ragdoll_bones` flips each of an actor's ~18 skeleton bone
/// bodies from Dynamic to Keyframed *before* first registration, so every
/// bone lands in Rapier as a `KinematicPositionBased` body with a real
/// collider — ~480+ of them across a dense interior. `exclude_dynamic()` does
/// not filter kinematic bodies, so a ground probe cast from above an actor's
/// root meets the actor's own upper-body bone long before it reaches the
/// floor. Excluding a single rigid-body handle cannot fix that: each bone is a
/// *separate* body.
///
/// Registration puts exactly those colliders in this membership group (and
/// nothing else), and every downward floor probe filters it out via
/// [`ground_probe_groups`]. Their *filter* mask stays `Group::ALL`, so contact
/// generation against the rest of the world is unchanged — this bit is a
/// query-side label, not a solver-side layer.
pub const ACTOR_BONE_GROUP: rapier3d::prelude::Group = rapier3d::prelude::Group::GROUP_32;

/// Interaction groups every downward floor probe queries with: sees
/// everything except [`ACTOR_BONE_GROUP`].
///
/// Deliberately *not* a blanket "fixed bodies only" filter. The FO4+/Starfield
/// packed-Havok compatibility proxy (`cell_loader::spawn`) registers real
/// architecture as `MotionType::Keyframed`, so excluding the whole kinematic
/// family would blind the spawn probe to the very floors that fallback exists
/// to provide.
#[inline]
fn ground_probe_groups() -> rapier3d::prelude::InteractionGroups {
    use rapier3d::prelude::{Group, InteractionGroups};
    InteractionGroups::new(Group::ALL, Group::ALL & !ACTOR_BONE_GROUP)
}

/// One collider found by [`PhysicsWorld::colliders_near_xz`] (#2202).
///
/// `body_type` is the Rapier label of the *parent* body, because that is the
/// discriminator the spawn-probe blind spots turn on: `QueryFilter::
/// exclude_dynamic` (used by `cast_ray_down` / `cast_capsule_down`) and the
/// `RigidBodyType::Fixed` census in `static_colliders_aabb` both skip the
/// Dynamic family entirely, so a floor authored as Dynamic is simultaneously
/// un-probeable and uncounted.
#[derive(Debug, Clone, Copy)]
pub struct NearbyCollider {
    /// Parent rigid body, for resolving back to an ECS entity via
    /// `RapierHandles`. `None` for a parentless (orphan) collider.
    pub body: Option<rapier3d::prelude::RigidBodyHandle>,
    /// `"Fixed"` / `"Dynamic"` / `"KinematicPos"` / `"KinematicVel"` /
    /// `"orphan"`.
    pub body_type: &'static str,
    /// Sensors (trigger volumes) never generate contacts — a sensor sitting
    /// where the floor should be is not a floor.
    pub is_sensor: bool,
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
}

/// First solid collider hit by a gameplay ray query.
///
/// The parent body is intentionally exposed instead of a physics-internal
/// user-data convention: engine callers can resolve it back to the owning ECS
/// entity through [`crate::RapierHandles`]. Parentless colliders remain valid
/// occluders and report `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsRayHit {
    pub body: Option<rapier3d::prelude::RigidBodyHandle>,
    /// Time of impact in world units along the normalized ray direction.
    pub distance: f32,
}

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
    /// Per-frame wall-clock budget for catch-up substeps (seconds). See
    /// [`SUBSTEP_TIME_BUDGET`] for the anti-spiral rationale (#1698).
    /// Public so tests can force the cap (`0.0`) or disable it (a huge
    /// value); production keeps the [`SUBSTEP_TIME_BUDGET`] default.
    pub substep_time_budget: f32,
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
            substep_time_budget: SUBSTEP_TIME_BUDGET,
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

    /// Whether a pipeline step is already pending (something was woken /
    /// spawned / re-targeted this frame). Read by the WATAL buoyancy phase
    /// to skip its per-body scan in a fully-quiesced scene: with nothing
    /// awake and nothing pending, no body's pose changed, so no dry→wet
    /// water transition can occur and the scan would be pure waste.
    #[inline]
    pub fn pending_wake(&self) -> bool {
        self.pending_wake
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
    pub fn add_force(
        &mut self,
        handle: RigidBodyHandle,
        force: byroredux_core::math::Vec3,
    ) -> bool {
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

    /// Change a live body's motion type in place for Papyrus
    /// `ObjectReference.SetMotionType`.
    ///
    /// The ECS-side `RigidBodyData` is updated by the caller; this method
    /// updates the already-registered Rapier body so the change takes effect
    /// immediately instead of waiting for newcomer registration (which only
    /// runs before a `RapierHandles` component exists).
    pub fn set_motion_type(
        &mut self,
        handle: RigidBodyHandle,
        motion_type: MotionType,
        wake_up: bool,
    ) -> bool {
        let body_type = match motion_type {
            MotionType::Static => RigidBodyType::Fixed,
            MotionType::Keyframed | MotionType::CharacterKinematic => {
                RigidBodyType::KinematicPositionBased
            }
            MotionType::Dynamic => RigidBodyType::Dynamic,
        };
        let Some(body) = self.bodies.get_mut(handle) else {
            return false;
        };
        body.set_body_type(body_type, wake_up);
        if wake_up {
            self.wake();
        }
        true
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
    ///
    /// A second, finer guard caps the catch-up loop by *wall time*: once
    /// the substeps run this frame have consumed [`SUBSTEP_TIME_BUDGET`],
    /// the loop stops and forfeits the remaining backlog instead of
    /// running the full 5-substep budget at tens-of-ms-per-substep settle
    /// cost. At least one substep always runs (the check is after the
    /// step), so a genuinely slow frame still advances the simulation.
    /// See [`SUBSTEP_TIME_BUDGET`] (#1698).
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

        // Anti-spiral wall-clock budget (#1698). Time the catch-up loop so a
        // settle storm can't pin the frame at the full 5-substep cost; once
        // the substeps run this frame have eaten `substep_time_budget`, drop
        // the rest of the backlog (slight slow-motion) rather than re-arming
        // the same demand next frame. See `SUBSTEP_TIME_BUDGET`.
        let budget = self.substep_time_budget.max(0.0);
        let loop_start = std::time::Instant::now();

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
            // Budget check AFTER the step so at least one substep always
            // runs (a slow frame must still advance the sim). When physics
            // has spent its per-frame wall-time, forfeit the leftover
            // accumulator — catching up is futile once a single substep
            // already costs more wall-time than the sim-time it produces.
            if loop_start.elapsed().as_secs_f32() >= budget {
                self.accumulator = 0.0;
                break;
            }
        }
        // Consume the wake only once a substep has actually run (#2856).
        // Clearing it before the loop dropped one-shot wakes on any frame
        // where `accumulator < PHYSICS_DT` — i.e. every frame above 60 fps.
        // The next frame then saw `pending_wake == false` with the island
        // lists still stale (they only update inside `pipeline.step`), took
        // the fast path above, and ZEROED the accumulator — so the banked
        // sub-tick time could never reach `PHYSICS_DT` and the scene stayed
        // frozen. Ragdoll activation was the worst case: the debug server is
        // a `Stage::Late` exclusive, so its `wake()` always landed on the
        // next frame's `step()`. Keeping the flag armed until work happens
        // makes the wake survive however many sub-tick frames it takes to
        // accumulate one full tick (2 frames at 120 fps, 17 at 1000 fps).
        if steps > 0 {
            self.pending_wake = false;
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
    ///
    /// `excluded_body` must be passed whenever the origin can lie inside a
    /// body — the player capsule, or an actor's own keyframed ragdoll bones.
    /// `exclude_dynamic()` does NOT cover those: both are
    /// `KinematicPositionBased`, and with `solid = true` rapier returns an
    /// impact at `toi = 0` for a ray starting inside a shape, which always
    /// wins the closest-hit search. The parameter is deliberately mandatory
    /// (rather than a defaulted sibling method) so every call site has to
    /// decide — a silent self-hit is invisible, since the returned
    /// `origin.y` is exactly what most callers use as their fallback (#2859).
    ///
    /// A live actor's keyframed ragdoll bones need no handle here: they carry
    /// [`ACTOR_BONE_GROUP`] and are masked out wholesale by
    /// [`ground_probe_groups`] (#2873). That matters because each bone is a
    /// separate body — `excluded_body` could never cover all ~18 of them.
    pub fn cast_ray_down(
        &self,
        origin: byroredux_core::math::Vec3,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
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
        let mut filter = QueryFilter::exclude_dynamic().groups(ground_probe_groups());
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
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

    /// Cast a normalized gameplay ray against solid colliders.
    ///
    /// Sensors are excluded because trigger volumes do not obstruct sight.
    /// `excluded_body` is normally the player capsule: a camera ray can begin
    /// inside that body, which would otherwise return an immediate self-hit.
    /// The query pipeline must have been refreshed after collider insertion,
    /// matching [`cast_ray_down`](Self::cast_ray_down)'s contract.
    pub fn cast_ray(
        &self,
        origin: byroredux_core::math::Vec3,
        direction: byroredux_core::math::Vec3,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<PhysicsRayHit> {
        if max_distance <= 0.0 || !max_distance.is_finite() {
            return None;
        }
        let direction = direction.normalize_or_zero();
        if direction.length_squared() == 0.0 {
            return None;
        }

        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector![direction.x, direction.y, direction.z],
        );
        let mut filter = QueryFilter::default().exclude_sensors();
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_ray(
                &self.bodies,
                &self.colliders,
                &ray,
                max_distance,
                /* solid = */ true,
                filter,
            )
            .map(|(collider, distance)| PhysicsRayHit {
                body: self.colliders.get(collider).and_then(|hit| hit.parent()),
                distance,
            })
    }

    /// Like [`cast_ray_down`](Self::cast_ray_down), but sweeps a capsule of
    /// the given dimensions instead of a zero-width ray. A bare ray can pass
    /// clean through a gap beside a sloped or narrow piece of architecture
    /// that a real capsule of nonzero radius would still clip — #2013 traced
    /// exactly this gap: the M28.5 door-spawn nudge picks an XZ a fixed
    /// distance into the room, and a ray straight down from that single
    /// point can miss the actual walkable floor a capsule spawned there
    /// would rest on (or, conversely, clip a sloped decoration a bare ray
    /// slips past — either way the ray and the KCC's own shape disagree).
    ///
    /// Returns the world-space Y of the surface a capsule of this size would
    /// rest on (equivalent contract to `cast_ray_down`: the caller still adds
    /// `half_height + radius + offset` to place the capsule centre above it).
    ///
    /// Same **caller must have called [`update_query_pipeline`]** and
    /// fixed-bodies-only caveats as `cast_ray_down`.
    pub fn cast_capsule_down(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<f32> {
        self.cast_capsule_down_surface_and_normal(
            origin,
            capsule_half_height,
            capsule_radius,
            max_distance,
            excluded_body,
        )
        .map(|(surface_y, _)| surface_y)
    }

    /// Capsule floor probe that rejects walls and other non-walkable hits.
    ///
    /// A vertical capsule sweep can hit nearby door frames or shell walls
    /// before its bottom reaches the floor. Those hits have a near-horizontal
    /// normal and must not be used as a character spawn surface (#2193).
    /// Normal orientation is deliberately ignored: legacy Havok architecture
    /// can be consistently inward-wound, but its geometric slope is still a
    /// valid basis for deciding whether a surface is walkable.
    pub fn cast_capsule_down_onto_walkable_surface(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        min_walkable_normal_y: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<f32> {
        self.cast_capsule_down_surface_and_normal(
            origin,
            capsule_half_height,
            capsule_radius,
            max_distance,
            excluded_body,
        )
        .and_then(|(surface_y, normal_y)| {
            (normal_y.abs() >= min_walkable_normal_y.clamp(0.0, 1.0)).then_some(surface_y)
        })
    }

    /// The unfiltered form of [`cast_capsule_down_onto_walkable_surface`]:
    /// returns `(surface_y, normal1.y)` for the first hit, walkable or not.
    ///
    /// Public because the walkable wrapper collapses two very different
    /// outcomes into `None` — "the swept capsule hit nothing" and "it hit
    /// something whose slope failed the walkable test" — and a spawn that
    /// misses every rung needs to tell those apart. Re-running the probe
    /// through this entry point on the failure path is what lets
    /// `dump_spawn_collider_census` report *"unfiltered sweep hit y=… with
    /// normal_y=… → REJECTED as non-walkable"* instead of mis-attributing a
    /// 60° ramp to a transform-composition bug (#2874).
    ///
    /// [`cast_capsule_down_onto_walkable_surface`]: Self::cast_capsule_down_onto_walkable_surface
    pub fn cast_capsule_down_surface_and_normal(
        &self,
        origin: byroredux_core::math::Vec3,
        capsule_half_height: f32,
        capsule_radius: f32,
        max_distance: f32,
        excluded_body: Option<rapier3d::prelude::RigidBodyHandle>,
    ) -> Option<(f32, f32)> {
        use rapier3d::parry::query::ShapeCastOptions;
        use rapier3d::prelude::*;
        let shape = SharedShape::capsule_y(capsule_half_height.max(1e-3), capsule_radius.max(1e-3));
        let pos = Isometry::translation(origin.x, origin.y, origin.z);
        let mut filter = QueryFilter::exclude_dynamic().groups(ground_probe_groups());
        if let Some(body) = excluded_body {
            filter = filter.exclude_rigid_body(body);
        }
        self.query_pipeline
            .cast_shape(
                &self.bodies,
                &self.colliders,
                &pos,
                &-Vector::y_axis(),
                shape.as_ref(),
                ShapeCastOptions {
                    target_distance: 0.0,
                    stop_at_penetration: false,
                    max_time_of_impact: max_distance,
                    compute_impact_geometry_on_penetration: true,
                },
                filter,
            )
            .map(|(_handle, hit)| {
                (
                    origin.y - hit.time_of_impact - capsule_half_height - capsule_radius,
                    hit.normal1.y,
                )
            })
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

    /// Diagnostic — every collider whose AABB overlaps the vertical column
    /// of half-width `radius` around `(x, z)`, whatever its body type
    /// (#2202).
    ///
    /// [`static_colliders_aabb`](Self::static_colliders_aabb) answers
    /// "is the collision world populated and does it overlap this cell?" —
    /// cell-wide bounds and a Fixed-only count. That reads healthy for a
    /// cell with 2560 fixed colliders and a hole exactly under the player's
    /// spawn, which is why it cannot discriminate between a collider that
    /// is absent, a collider that exists but is Dynamic (and so invisible
    /// to both that census and the `exclude_dynamic` spawn probe), and a
    /// collider that exists as Fixed but composed to the wrong Y.
    ///
    /// This one is deliberately unfiltered: the *point* is to see colliders
    /// the spawn probe cannot. Returned entries carry the parent body handle
    /// so the caller can resolve it back to an entity and its
    /// `PhysicsSourceForm`.
    ///
    /// Sorted by **distance from `probe_y`**, nearest first (#2875). The
    /// pre-fix ordering sorted by absolute AABB centre Y descending and took
    /// only the first N, which inverted the diagnostic: the question is "is
    /// there a floor at or below the spawn?", whose answer lives at the low
    /// end of the column, while a two-storey inn's roof beams and upper
    /// landing monopolise the high end. In the dense-interior case this
    /// census exists for, the evidence was exactly what got truncated away.
    pub fn colliders_near_xz(
        &self,
        x: f32,
        probe_y: f32,
        z: f32,
        radius: f32,
    ) -> Vec<NearbyCollider> {
        use rapier3d::prelude::*;
        let mut out = Vec::new();
        for (_h, c) in self.colliders.iter() {
            let aabb = c.compute_aabb();
            // Column overlap test — a wall whose AABB straddles the column
            // counts even if its centre is far away.
            if aabb.maxs.x < x - radius
                || aabb.mins.x > x + radius
                || aabb.maxs.z < z - radius
                || aabb.mins.z > z + radius
            {
                continue;
            }
            let parent = c.parent();
            let body_type = parent
                .and_then(|p| self.bodies.get(p))
                .map(|rb| match rb.body_type() {
                    RigidBodyType::Fixed => "Fixed",
                    RigidBodyType::Dynamic => "Dynamic",
                    RigidBodyType::KinematicPositionBased => "KinematicPos",
                    RigidBodyType::KinematicVelocityBased => "KinematicVel",
                })
                .unwrap_or("orphan");
            out.push(NearbyCollider {
                body: parent,
                body_type,
                is_sensor: c.is_sensor(),
                aabb_min: [aabb.mins.x, aabb.mins.y, aabb.mins.z],
                aabb_max: [aabb.maxs.x, aabb.maxs.y, aabb.maxs.z],
            });
        }
        // Distance from the probe height, nearest first. Ties (a floor slab
        // and a ceiling slab equidistant from the probe) break downward, so
        // the one that could actually be a floor reads first.
        let distance = |c: &NearbyCollider| {
            let centre = 0.5 * (c.aabb_min[1] + c.aabb_max[1]);
            ((centre - probe_y).abs(), centre > probe_y)
        };
        out.sort_by(|a, b| {
            let (ad, a_above) = distance(a);
            let (bd, b_above) = distance(b);
            ad.partial_cmp(&bd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a_above.cmp(&b_above))
        });
        out
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
        let mut parts = collision_shape_to_parts(s, 1.0, &crate::config::ContactConfig::DEFAULT);
        assert_eq!(parts.len(), 1, "test helper expects a single part");
        parts.swap_remove(0).1
    }

    #[test]
    fn empty_world_has_no_bodies() {
        let w = PhysicsWorld::new();
        assert_eq!(w.body_count(), 0);
    }

    #[test]
    fn scripted_motion_type_updates_a_live_body() {
        let mut world = PhysicsWorld::new();
        let handle = world.bodies.insert(RigidBodyBuilder::dynamic().build());

        assert!(world.set_motion_type(handle, MotionType::Keyframed, true));
        assert_eq!(
            world.bodies[handle].body_type(),
            RigidBodyType::KinematicPositionBased
        );
        assert!(world.pending_wake());
    }

    #[test]
    fn gameplay_ray_reports_owner_and_can_exclude_player_body() {
        let mut world = PhysicsWorld::new();
        let player = world.bodies.insert(RigidBodyBuilder::fixed().build());
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(5.0, 5.0, 5.0)
                .translation(vector![0.0, 0.0, -10.0])
                .build(),
            player,
            &mut world.bodies,
        );
        let wall = world.bodies.insert(RigidBodyBuilder::fixed().build());
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(20.0, 20.0, 2.0)
                .translation(vector![0.0, 0.0, -100.0])
                .build(),
            wall,
            &mut world.bodies,
        );
        world.update_query_pipeline();

        let self_hit = world
            .cast_ray(Vec3::ZERO, Vec3::NEG_Z, 200.0, None)
            .expect("player collider is the first hit");
        assert_eq!(self_hit.body, Some(player));

        let wall_hit = world
            .cast_ray(Vec3::ZERO, Vec3::NEG_Z, 200.0, Some(player))
            .expect("excluding the player exposes the wall");
        assert_eq!(wall_hit.body, Some(wall));
        assert!((wall_hit.distance - 98.0).abs() < 0.01);
    }

    #[test]
    fn gameplay_ray_ignores_trigger_sensors() {
        let mut world = PhysicsWorld::new();
        let trigger = world.bodies.insert(RigidBodyBuilder::fixed().build());
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(20.0, 20.0, 2.0)
                .translation(vector![0.0, 0.0, -20.0])
                .sensor(true)
                .build(),
            trigger,
            &mut world.bodies,
        );
        world.update_query_pipeline();

        assert!(world
            .cast_ray(Vec3::ZERO, Vec3::NEG_Z, 100.0, None)
            .is_none());
    }

    /// Test helper: insert a 100×2×100 BU slab at `(x, y, z)` with the
    /// given body type, and return nothing — the census is queried by
    /// position, not handle.
    fn insert_slab(w: &mut PhysicsWorld, pos: Vec3, dynamic: bool) {
        let shape = single_shape(&CollisionShape::Cuboid {
            half_extents: Vec3::new(50.0, 1.0, 50.0),
        });
        let body = if dynamic {
            RigidBodyBuilder::dynamic()
        } else {
            RigidBodyBuilder::fixed()
        }
        .position(iso_from_trs(pos, Quat::IDENTITY))
        .build();
        let h = w.bodies.insert(body);
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);
    }

    #[test]
    fn walkable_capsule_probe_accepts_floor() {
        let mut w = PhysicsWorld::new();
        insert_slab(&mut w, Vec3::ZERO, false);
        w.update_query_pipeline();

        let hit = w.cast_capsule_down_onto_walkable_surface(
            Vec3::new(0.0, 100.0, 0.0),
            10.0,
            5.0,
            200.0,
            50.0_f32.to_radians().cos(),
            None,
        );

        assert_eq!(hit, Some(1.0));
    }

    #[test]
    fn walkable_capsule_probe_rejects_door_frame_wall() {
        let mut w = PhysicsWorld::new();
        let wall = single_shape(&CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(20.0, 100.0, -50.0),
                Vec3::new(20.0, 100.0, 50.0),
                Vec3::new(0.0, -100.0, 50.0),
                Vec3::new(0.0, -100.0, -50.0),
            ],
            indices: vec![[0, 1, 2], [0, 2, 3]],
        });
        let body = RigidBodyBuilder::fixed()
            // This steep panel crosses the capsule column as Y decreases, so
            // the downward cast meets its wall-like face before any floor.
            .position(iso_from_trs(Vec3::ZERO, Quat::IDENTITY))
            .build();
        let handle = w.bodies.insert(body);
        w.colliders
            .insert_with_parent(ColliderBuilder::new(wall).build(), handle, &mut w.bodies);
        w.update_query_pipeline();

        assert!(
            w.cast_capsule_down(Vec3::new(0.0, 150.0, 0.0), 10.0, 5.0, 300.0, None)
                .is_some(),
            "the unfiltered capsule sweep must observe the wall contact"
        );
        assert_eq!(
            w.cast_capsule_down_onto_walkable_surface(
                Vec3::new(0.0, 150.0, 0.0),
                10.0,
                5.0,
                300.0,
                50.0_f32.to_radians().cos(),
                None,
            ),
            None,
            "a horizontal door-frame contact is not a spawn floor"
        );
    }

    /// #2202 — the census must see a Dynamic-family floor. This is the
    /// blind spot that makes candidate (B) indistinguishable from "no
    /// collider": `cast_capsule_down` filters with `exclude_dynamic` and
    /// `static_colliders_aabb` counts only `Fixed`, so a Dynamic slab is
    /// reported by neither.
    #[test]
    fn census_sees_a_dynamic_floor_that_both_existing_probes_miss() {
        let mut w = PhysicsWorld::new();
        insert_slab(&mut w, Vec3::new(0.0, 0.0, 0.0), true);
        w.update_query_pipeline();

        assert!(
            w.static_colliders_aabb().is_none(),
            "Fixed-only census must not see a Dynamic floor — that blindness \
             is the premise of this diagnostic"
        );
        assert!(
            w.cast_capsule_down(Vec3::new(0.0, 100.0, 0.0), 60.0, 20.0, 500.0, None)
                .is_none(),
            "exclude_dynamic probe must not see a Dynamic floor"
        );

        let near = w.colliders_near_xz(0.0, 0.0, 0.0, 64.0);
        assert_eq!(near.len(), 1, "census must see it");
        assert_eq!(near[0].body_type, "Dynamic");
        assert!(!near[0].is_sensor);
    }

    /// A genuinely empty column reports empty — distinguishing candidate
    /// (A)/(D) from (B)/(C).
    #[test]
    fn census_of_an_empty_column_is_empty() {
        let mut w = PhysicsWorld::new();
        // Floor exists, but 5000 BU away in X: the cell is populated, the
        // spawn column is not. That is exactly the 2560-colliders-with-a-hole
        // shape `static_colliders_aabb` reads as healthy.
        insert_slab(&mut w, Vec3::new(5000.0, 0.0, 0.0), false);
        w.update_query_pipeline();

        assert!(
            w.static_colliders_aabb().is_some(),
            "cell-wide census still reports a populated collision world"
        );
        assert!(w.colliders_near_xz(0.0, 0.0, 0.0, 64.0).is_empty());
    }

    /// Column overlap is an AABB test, not a centre-distance test: a slab
    /// whose centre is outside the radius but whose extent straddles the
    /// column still counts (a wall beside the spawn is load-bearing
    /// evidence).
    #[test]
    fn census_column_test_uses_aabb_overlap_not_centre_distance() {
        let mut w = PhysicsWorld::new();
        // Centre 60 BU away in X, half-extent 50 ⇒ AABB spans x ∈ [10, 110].
        insert_slab(&mut w, Vec3::new(60.0, 0.0, 0.0), false);
        w.update_query_pipeline();
        assert_eq!(w.colliders_near_xz(0.0, 0.0, 0.0, 16.0).len(), 1);
        // Same slab, column now well clear of x ∈ [10, 110].
        assert!(w.colliders_near_xz(-100.0, 0.0, 0.0, 16.0).is_empty());
    }

    /// #2875 — results come back nearest-to-the-probe first, so the entry
    /// most likely to be the missing floor reads at the top of the dump.
    #[test]
    fn census_sorts_by_distance_from_the_probe_height() {
        let mut w = PhysicsWorld::new();
        insert_slab(&mut w, Vec3::new(0.0, -500.0, 0.0), false);
        insert_slab(&mut w, Vec3::new(0.0, 300.0, 0.0), false);
        insert_slab(&mut w, Vec3::new(0.0, 0.0, 0.0), false);
        w.update_query_pipeline();

        // Probe at y=40: the floor at 0 is 40 away, the ceiling at 300 is
        // 260, the basement at -500 is 540.
        let near = w.colliders_near_xz(0.0, 40.0, 0.0, 16.0);
        assert_eq!(near.len(), 3);
        let centres: Vec<f32> = near
            .iter()
            .map(|n| 0.5 * (n.aabb_min[1] + n.aabb_max[1]))
            .collect();
        assert_eq!(
            centres,
            vec![0.0, 300.0, -500.0],
            "expected nearest-to-probe ordering, got {centres:?}"
        );
    }

    /// #2875 — the ordering fix only matters because the census truncates,
    /// and the pre-fix absolute-Y-descending sort put the floor *past* the
    /// cut in exactly the dense column the diagnostic was written for.
    ///
    /// Models a two-storey interior: a stack of ceiling/beam/upper-landing
    /// slabs above the probe, and one floor slab just below it. With
    /// `SPAWN_CENSUS_DETAIL_CAP` entries printed, the floor must survive.
    #[test]
    fn census_keeps_the_floor_inside_the_detail_cap_in_a_dense_column() {
        const DETAIL_CAP: usize = 24;
        let mut w = PhysicsWorld::new();
        let probe_y = 100.0;
        // 40 slabs stacked above the probe — roof, rafters, upper floor.
        for i in 0..40 {
            insert_slab(
                &mut w,
                Vec3::new(0.0, probe_y + 200.0 * (i + 1) as f32, 0.0),
                false,
            );
        }
        // The floor the spawn is actually looking for, 20 BU under the probe.
        insert_slab(&mut w, Vec3::new(0.0, probe_y - 20.0, 0.0), false);
        w.update_query_pipeline();

        let near = w.colliders_near_xz(0.0, probe_y, 0.0, 64.0);
        assert_eq!(near.len(), 41);
        let printed: Vec<f32> = near
            .iter()
            .take(DETAIL_CAP)
            .map(|n| 0.5 * (n.aabb_min[1] + n.aabb_max[1]))
            .collect();
        assert_eq!(
            printed[0],
            probe_y - 20.0,
            "the floor must be the FIRST entry printed, got {printed:?}"
        );
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

    /// The default per-frame substep budget is one sim-tick of wall-time —
    /// the break-even point past which catch-up substeps are futile (#1698).
    #[test]
    fn default_substep_budget_is_one_tick() {
        let w = PhysicsWorld::new();
        assert_eq!(w.substep_time_budget, SUBSTEP_TIME_BUDGET);
        assert_eq!(SUBSTEP_TIME_BUDGET, PHYSICS_DT);
    }

    /// #1698 anti-spiral regression: with the wall-clock budget exhausted
    /// (forced to 0), a frame whose backlog would otherwise demand the full
    /// `MAX_SUBSTEPS` collapses to a single catch-up substep — the storm
    /// settle is amortized across frames instead of pinning one frame at the
    /// 5× cost. At least one substep still runs so the sim always advances.
    #[test]
    fn zero_budget_caps_settle_storm_to_one_substep() {
        let mut w = PhysicsWorld::new();
        w.substep_time_budget = 0.0; // force the anti-spiral cap

        // An awake dynamic body so the catch-up loop actually runs.
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
                .build(),
        );
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);

        // A frame_dt large enough to fill the accumulator to the 5-substep cap.
        let steps = w.step(100.0);
        assert_eq!(
            steps, 1,
            "an exhausted wall-clock budget must collapse catch-up to one substep"
        );
        // Backlog is forfeited (slow-motion), not banked — the accumulator is
        // drained so the next frame doesn't immediately demand 5 again.
        assert_eq!(w.accumulator, 0.0, "budget-bail must drain the backlog");
    }

    /// The budget is a no-op in the common case: a cheap scene (sub-ms
    /// substeps) with the budget effectively disabled runs the full
    /// `MAX_SUBSTEPS` catch-up, so steady-state simulation speed is
    /// unchanged by the #1698 guard.
    #[test]
    fn ample_budget_allows_full_catchup() {
        let mut w = PhysicsWorld::new();
        w.substep_time_budget = f32::INFINITY; // never trips
                                               // Empty world still steps on the first frame (constructor arms
                                               // `pending_wake`); cheap substeps all fit, so the accumulator cap is
                                               // the only limit.
        let steps = w.step(100.0);
        assert_eq!(steps, MAX_SUBSTEPS, "ample budget must allow full catch-up");
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

    /// EXTERIOR-FREEZE FIX: a dynamic body spawned ASLEEP (as
    /// `register_newcomers` now does for all `MotionType::Dynamic` newcomers)
    /// must NOT free-fall and must NOT pin the simulation awake — even when
    /// the sim is poked. This is the core of the fix for the measured
    /// `atw_scheduler=3005ms` / ~3000-awake-dynamics exterior stall: streamed
    /// NPC ragdoll bones (no terrain collider beneath them) used to free-fall
    /// forever, keeping thousands of bodies in the active set every frame.
    #[test]
    fn sleeping_dynamic_newcomer_does_not_fall_or_pin_sim() {
        let mut w = PhysicsWorld::new();
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .position(iso_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
                .sleeping(true) // the new spawn state for dynamic newcomers
                .build(),
        );
        w.colliders
            .insert_with_parent(ColliderBuilder::new(shape).build(), h, &mut w.bodies);

        // Poke the sim (as a kinematic newcomer / registration would) and run
        // 2 s. With no contact or applied force, the asleep body stays put.
        w.wake();
        for _ in 0..120 {
            w.step(PHYSICS_DT);
        }
        let y = w.bodies[h].translation().y;
        assert!(
            (y - 1000.0).abs() < 1.0,
            "asleep dynamic must not free-fall; y={y}"
        );
        assert!(
            w.bodies[h].is_sleeping(),
            "must remain asleep without contact/force"
        );
        // And the scene quiesces — asleep newcomers don't keep the sim stepping.
        assert_eq!(
            w.step(PHYSICS_DT),
            0,
            "asleep dynamic newcomers must not pin physics_sync_system awake"
        );

        // Sanity: the WATAL force API still wakes it (buoyancy/interaction path).
        let up = byroredux_core::math::Vec3::new(0.0, 1.0e7, 0.0);
        assert!(w.add_force(h, up), "force applies to the sleeping body");
        assert!(w.step(PHYSICS_DT) > 0, "applied force re-engages the sim");
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
        assert!(
            !w.apply_impulse(fh, up),
            "static body must reject apply_impulse"
        );
        // Dead handle → all no-op.
        let mut w2 = PhysicsWorld::new();
        let dead = w2.bodies.insert(RigidBodyBuilder::dynamic().build());
        w2.remove_body(dead);
        assert!(!w.add_force(dead, up), "dead handle must be a no-op");
    }
}

#[cfg(test)]
mod audit_2026_08_13_regressions {
    use super::*;
    use crate::config::ContactConfig;
    use byroredux_core::math::Vec3;

    /// Insert a Fixed cuboid slab whose TOP surface is at `top_y`.
    fn floor_slab(w: &mut PhysicsWorld, top_y: f32, half_extent: f32) {
        let half_thickness = 4.0;
        let body = w.bodies.insert(
            RigidBodyBuilder::fixed()
                .translation(vector![0.0, top_y - half_thickness, 0.0])
                .build(),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(half_extent, half_thickness, half_extent)
                .contact_skin(ContactConfig::DEFAULT.default_contact_skin_bu)
                .build(),
            body,
            &mut w.bodies,
        );
    }

    /// A `KinematicPositionBased` capsule standing in for the player body.
    fn player_capsule(w: &mut PhysicsWorld, centre: Vec3) -> RigidBodyHandle {
        let body = w.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(vector![centre.x, centre.y, centre.z])
                .build(),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::capsule_y(46.0, 18.0).build(),
            body,
            &mut w.bodies,
        );
        body
    }

    // ── #2856 — one-shot wake must survive sub-tick frames ───────────

    /// The regression itself: above 60 fps every frame is sub-tick, so
    /// clearing `pending_wake` before the substep loop consumed the wake
    /// without stepping, and the next frame's fast path then zeroed the
    /// accumulator — an absorbing state the sim could never leave.
    #[test]
    fn one_shot_wake_survives_sub_tick_frames_and_eventually_steps() {
        let mut w = PhysicsWorld::new();
        // Quiesce: no awake dynamic body at all.
        let _ = w.step(PHYSICS_DT);
        assert_eq!(
            w.step(PHYSICS_DT),
            0,
            "scene must be quiesced for this test"
        );

        w.wake();
        let mut total = 0u32;
        for _ in 0..600 {
            total += w.step(PHYSICS_DT / 2.0);
        }
        assert!(
            total > 0,
            "a one-shot wake() was swallowed across 600 sub-tick frames (#2856)"
        );
    }

    /// The wake must not be spent by a frame that ran no substep, but it
    /// *must* be spent once one does — otherwise the fast path could never
    /// re-engage and the static-scene optimisation would be dead.
    #[test]
    fn wake_is_consumed_once_a_substep_actually_runs() {
        let mut w = PhysicsWorld::new();
        let _ = w.step(PHYSICS_DT);
        assert_eq!(w.step(PHYSICS_DT), 0);

        w.wake();
        assert_eq!(w.step(PHYSICS_DT / 2.0), 0, "half a tick cannot step yet");
        assert!(
            w.pending_wake(),
            "wake must still be armed after a 0-substep frame"
        );
        assert_eq!(
            w.step(PHYSICS_DT / 2.0),
            1,
            "the banked half-ticks must now step"
        );
        assert!(
            !w.pending_wake(),
            "wake must be consumed once work happened"
        );
        assert_eq!(w.step(PHYSICS_DT), 0, "fast path must re-engage afterwards");
    }

    // ── #2859 — casts must be able to exclude their caster ───────────

    /// The camera sits *inside* the player capsule by design, and the
    /// capsule is `KinematicPositionBased`, which `exclude_dynamic()` does
    /// not filter. With `solid = true` rapier returns a `toi = 0` self-hit,
    /// so the cast returned `origin.y` — numerically identical to the
    /// caller's fallback, making the whole height-fog fix a silent no-op.
    #[test]
    fn cast_ray_down_self_hits_without_exclusion_and_finds_the_floor_with_it() {
        let mut w = PhysicsWorld::new();
        floor_slab(&mut w, 0.0, 500.0);
        let eye = Vec3::new(0.0, 152.0, 0.0);
        // Capsule centred at 100 spans [36, 164] — the eye is inside it.
        let body = player_capsule(&mut w, Vec3::new(0.0, 100.0, 0.0));
        w.update_query_pipeline();

        let unfiltered = w.cast_ray_down(eye, 1000.0, None);
        assert_eq!(
            unfiltered,
            Some(eye.y),
            "unfiltered ray must self-hit at toi 0 — this is the #2859 bug"
        );

        let filtered = w.cast_ray_down(eye, 1000.0, Some(body));
        assert!(
            filtered.is_some_and(|y| (y - 0.0).abs() < 0.5),
            "excluding the player body must reach the floor, got {filtered:?}"
        );
    }

    // ── #2873 — ground probes must not see an actor's own bones ──────

    /// Register a keyframed ragdoll-bone body the way `physics_sync_system`
    /// does for a live actor: `KinematicPositionBased`, real collider,
    /// membership in [`ACTOR_BONE_GROUP`].
    fn actor_bone(w: &mut PhysicsWorld, centre: Vec3, tagged: bool) -> RigidBodyHandle {
        use rapier3d::prelude::{Group, InteractionGroups};
        let body = w.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(vector![centre.x, centre.y, centre.z])
                .build(),
        );
        let groups = if tagged {
            InteractionGroups::new(ACTOR_BONE_GROUP, Group::ALL)
        } else {
            InteractionGroups::all()
        };
        w.colliders.insert_with_parent(
            ColliderBuilder::ball(6.0).collision_groups(groups).build(),
            body,
            &mut w.bodies,
        );
        body
    }

    /// The defect: `step_toward` casts from `current.y + 256` straight down
    /// through the actor it is trying to ground-snap. Its ~18 bones are
    /// separate `KinematicPositionBased` bodies, which `exclude_dynamic()`
    /// keeps, so the ray reports the actor's own upper body as the floor —
    /// and since the bones follow the root, the next tick casts from higher
    /// still. `ACTOR_BONE_GROUP` masks all of them at once.
    #[test]
    fn ground_ray_skips_actor_bones_and_reaches_the_floor() {
        let mut w = PhysicsWorld::new();
        floor_slab(&mut w, 0.0, 500.0);
        // Stand-in for a head/spine bone at chest height, and the
        // locomotion ray origin 256 BU above the actor's root.
        let origin = Vec3::new(0.0, 256.0, 0.0);

        let mut untagged = PhysicsWorld::new();
        floor_slab(&mut untagged, 0.0, 500.0);
        actor_bone(&mut untagged, Vec3::new(0.0, 120.0, 0.0), false);
        untagged.update_query_pipeline();
        assert_eq!(
            untagged.cast_ray_down(origin, 1000.0, None),
            Some(126.0),
            "pre-fix behaviour: the ray stops on the actor's own bone, not the floor"
        );

        actor_bone(&mut w, Vec3::new(0.0, 120.0, 0.0), true);
        w.update_query_pipeline();
        let hit = w.cast_ray_down(origin, 1000.0, None);
        assert!(
            hit.is_some_and(|y| y.abs() < 0.5),
            "a tagged actor bone must be invisible to the ground ray, got {hit:?}"
        );
    }

    /// The capsule probes share the filter, so a spawn sweep cannot land the
    /// player on an NPC's shoulder either.
    #[test]
    fn capsule_floor_probe_skips_actor_bones() {
        let mut w = PhysicsWorld::new();
        floor_slab(&mut w, 0.0, 500.0);
        actor_bone(&mut w, Vec3::new(0.0, 120.0, 0.0), true);
        w.update_query_pipeline();

        let surface = w.cast_capsule_down(Vec3::new(0.0, 400.0, 0.0), 46.0, 18.0, 1000.0, None);
        assert!(
            surface.is_some_and(|y| y.abs() < 1.0),
            "capsule sweep must pass through a tagged actor bone, got {surface:?}"
        );
    }

    /// The mask is query-side only. Non-bone kinematic colliders — above all
    /// the FO4+/Starfield packed-Havok proxy, which registers real
    /// architecture as `MotionType::Keyframed` — must stay probeable, or the
    /// fix would blind the spawn probe to the floors that fallback provides.
    #[test]
    fn ground_probe_still_sees_untagged_kinematic_architecture() {
        let mut w = PhysicsWorld::new();
        let body = w.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(vector![0.0, 0.0, 0.0])
                .build(),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(500.0, 10.0, 500.0).build(),
            body,
            &mut w.bodies,
        );
        w.update_query_pipeline();

        let hit = w.cast_ray_down(Vec3::new(0.0, 300.0, 0.0), 1000.0, None);
        assert!(
            hit.is_some_and(|y| (y - 10.0).abs() < 0.5),
            "an untagged Keyframed floor (the packed-Havok proxy) must stay \
             probeable, got {hit:?}"
        );
    }

    /// The capsule probes needed the same parameter — the #2857 support
    /// probe casts from the player's own position, so without exclusion it
    /// would measure a zero gap against itself every frame.
    #[test]
    fn cast_capsule_down_can_exclude_the_probing_body() {
        let mut w = PhysicsWorld::new();
        floor_slab(&mut w, 0.0, 500.0);
        let centre = Vec3::new(0.0, 68.0, 0.0);
        let body = player_capsule(&mut w, centre);
        w.update_query_pipeline();

        let surface = w.cast_capsule_down(centre, 46.0, 18.0, 200.0, Some(body));
        assert!(
            surface.is_some_and(|y| (y - 0.0).abs() < 0.5),
            "excluded probe must find the floor, got {surface:?}"
        );
    }

    // ── #2858 — the walkable probe must start ABOVE the target floor ──

    /// The audit's truth table: with the old `+50` origin the probe capsule's
    /// bottom (`half_height + radius` = 64 BU below the origin) started
    /// *below* the door's own floor, so rungs 1 and 2 were structurally blind
    /// to every floor within ~15 BU of door height — the normal case.
    #[test]
    fn walkable_probe_sees_a_floor_at_door_height_with_capsule_clearance() {
        let (hh, r) = (46.0_f32, 18.0_f32);
        let min_walkable = 50.0_f32.to_radians().cos();
        let door_y = 0.0_f32;

        for floor_top in [0.0_f32, -2.0, -5.0, -10.0, -14.0] {
            let mut w = PhysicsWorld::new();
            floor_slab(&mut w, floor_top, 500.0);
            w.update_query_pipeline();

            // Old behaviour: origin only 50 BU above the door, so the probe
            // capsule's bottom started at `door_y - 14`. It fails in one of
            // two ways depending on where the floor sits relative to that
            // penetrating start — either no hit at all, or a PHANTOM surface
            // pinned at the capsule's own bottom (`origin.y - 64`) rather
            // than the real floor. Both are the #2858 defect; assert only
            // that it does not report the truth.
            let old = w.cast_capsule_down_onto_walkable_surface(
                Vec3::new(0.0, door_y + 50.0, 0.0),
                hh,
                r,
                150.0,
                min_walkable,
                None,
            );
            if floor_top > -14.0 {
                assert!(
                    !old.is_some_and(|y| (y - floor_top).abs() < 0.5),
                    "pre-fix origin must not resolve floor_top={floor_top}, got {old:?} \
                     (documents #2858)"
                );
            }

            // Fixed behaviour: lift by the capsule's own half-extent + clearance.
            let lift = hh + r + 16.0;
            let new = w.cast_capsule_down_onto_walkable_surface(
                Vec3::new(0.0, door_y + lift, 0.0),
                hh,
                r,
                16.0 + 164.0,
                min_walkable,
                None,
            );
            assert!(
                new.is_some_and(|y| (y - floor_top).abs() < 0.5),
                "lifted origin must find floor_top={floor_top}, got {new:?}"
            );
        }
    }

    // ── #2857 — a grounded capsule must never sink through a convex floor ──

    /// Mirrors what `character_controller_system` now does each grounded
    /// frame: probe for the real support, move exactly to resting contact,
    /// then step. The old code sent a fixed `-step_height`, which rapier
    /// applied verbatim whenever its sweep reported no interference —
    /// 32 BU/frame straight through a solid `Cuboid`.
    ///
    /// Reproducing it requires the capsule to SETTLE under gravity first
    /// (which lands it at a gap of ~3.996 rather than exactly `offset`), and
    /// it then fires only at certain absolute floor Y values — which is why
    /// the bug reads as intermittent in play. Both policies are run over the
    /// same sweep so the test proves it still has teeth.
    #[test]
    fn grounded_capsule_does_not_sink_through_convex_floors_across_absolute_y() {
        let (hh, r) = (46.0_f32, 18.0_f32);
        let kcc_offset = ContactConfig::DEFAULT.kcc_offset_bu;
        let step_height = 32.0_f32;
        const SETTLE_FRAMES: usize = 3;

        let mut sank_old = 0;
        let mut sank_new = 0;
        let mut checked = 0;

        for &bounded in &[false, true] {
            // Two floor heights confirmed to trigger the pre-fix punch-through
            // (`0.0`, `137.0`) plus a spread so the sweep is not overfitted to
            // them. Which values fire is a float-precision property of the
            // resting gap, not something a content author controls.
            let mut floors = vec![0.0_f32, 137.0];
            floors.extend((0..40).map(|i| -60.0 + (i as f32) * 7.3));
            for floor_top in floors {
                for half_extent in [50.0_f32, 500.0] {
                    if bounded {
                        checked += 1;
                    }
                    let mut w = PhysicsWorld::new();
                    floor_slab(&mut w, floor_top, half_extent);
                    w.update_query_pipeline();

                    // Start above the floor and let gravity settle it, the
                    // way the real controller reaches its grounded state.
                    let mut pos = Vec3::new(0.0, floor_top + hh + r + kcc_offset + 10.0, 0.0);
                    let mut vv = 0.0f32;

                    for f in 0..120 {
                        let desired_y = if f < SETTLE_FRAMES {
                            vv += -1220.8 * PHYSICS_DT;
                            vv * PHYSICS_DT
                        } else if bounded {
                            // The fixed probe-bounded correction (#2857).
                            match w.cast_capsule_down(pos, hh, r, step_height + kcc_offset, None) {
                                Some(surface_y) => {
                                    let feet_y = pos.y - hh - r;
                                    let correction = -(feet_y - surface_y - kcc_offset);
                                    correction.clamp(-step_height, kcc_offset)
                                }
                                None => vv * PHYSICS_DT,
                            }
                        } else {
                            // The pre-fix fixed probe.
                            -step_height
                        };
                        let res = w.move_character(CharacterMoveParams {
                            capsule_half_height: hh,
                            capsule_radius: r,
                            position: pos,
                            desired_translation: Vec3::new(0.0, desired_y, 0.0),
                            dt: PHYSICS_DT,
                            max_slope_climb_deg: 50.0,
                            step_height,
                            step_min_width: 8.0,
                            snap_to_ground: step_height,
                            exclude_collider: None,
                            kcc_offset_bu: kcc_offset,
                        });
                        pos += res.translation;
                    }

                    let feet = pos.y - hh - r;
                    if feet < floor_top - 1.0 {
                        if bounded {
                            sank_new += 1;
                        } else {
                            sank_old += 1;
                        }
                    }
                }
            }
        }

        assert!(
            sank_old > 0,
            "the pre-fix fixed -step_height probe sank 0/{checked} configurations — this test \
             no longer reproduces #2857 and has lost its teeth"
        );
        assert_eq!(
            sank_new, 0,
            "{sank_new}/{checked} grounded configurations sank through a convex floor (#2857); \
             the pre-fix policy sank {sank_old}/{checked}"
        );
    }
}
