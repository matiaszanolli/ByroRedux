//! `PhysicsWorld` resource — owns the Rapier simulation state.
//!
//! One `PhysicsWorld` per `byroredux_core::ecs::World`. Inserted as a
//! resource by `main.rs` during scene setup. The entire simulation lives
//! inside this struct — sets, broad phase, narrow phase, pipeline,
//! integration parameters, and a fixed-timestep accumulator.

use byroredux_core::ecs::components::MotionType;
use byroredux_core::ecs::resource::Resource;
use rapier3d::prelude::*;

// #5311 (TD1-2026-10-05-01) — the query/KCC impl and the explosion
// recovery/containment cluster live in child modules; `step` consumes
// the recovery entry points directly.
mod queries;
mod recovery;

pub use queries::{CharacterMoveParams, CharacterMoveResult};
use recovery::{body_state_is_finite, restore_invalid_dynamic_bodies, DynamicBodySnapshot};
#[cfg(test)]
use recovery::MAX_DYNAMIC_SUBSTEP_DISPLACEMENT;
#[cfg(test)]
use queries::character_capsule;

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

/// #5161 — sanity cap on a dynamic body's speed, applied at the end of each
/// of [`PhysicsWorld::step`]'s substeps by `clamp_explosive_velocities`.
/// No engine-driven dynamic body moves legitimately at 20 000 BU/s
/// (~285 m/s): the fastest authored motion class (arrows) sits near 6 000.
/// A body found above it is exploding (the real-cell P2 route measured a
/// ragdoll articulation kicked by deep floor penetration at |v|≈7.7e15
/// BU/s), and escalation is by LIFETIME burst count (see
/// `explosion_offences`): 1 = clamp, 2 = park (velocities zeroed, slept —
/// the invalid-solve restore's containment), 3 = detach the body's whole
/// articulation (#5246).
///
/// This is the escalation policy, not the in-step bound: one
/// `pipeline.step` runs several internal substeps, so an explosion born in
/// it is integrated before this clamp can run (#5488). Rapier bounds that
/// itself — see [`IN_STEP_LINEAR_SPEED_CAP_BU_PER_S`].
pub const VELOCITY_SANITY_CAP_BU_PER_S: f32 = 20_000.0;
/// Rapier's own per-internal-substep speed cap, in BU/s
/// (`IntegrationParameters::normalized_max_linear_velocity` × length unit;
/// rapier 0.35+, ~45°/substep for rotation). This is the guard #5488 found
/// missing: it runs INSIDE `pipeline.step`, so a solve that blows up on an
/// early internal substep can move a body at most this far per second
/// before any engine code sees it — ≈467 BU per 60 Hz tick, a quarter of
/// [`MAX_DYNAMIC_SUBSTEP_DISPLACEMENT`]. The value is rapier's default (400
/// m/s) at the Bethesda scale, pinned here so it is a decision rather than
/// an accident. It sits deliberately ABOVE [`VELOCITY_SANITY_CAP_BU_PER_S`]:
/// a body rapier had to cap leaves the step faster than the sanity cap, so
/// the end-of-substep clamp still sees — and counts — the explosion.
///
/// Under rapier 0.22 the same explosion was fatal: the multi-SAP broad
/// phase panicked (`sap_axis.rs`) once an AABB neared its ≈2.68e11 grid
/// boundary. The BVH broad phase (rapier 0.27+) has no such grid and skips
/// non-finite AABBs, so that panic class no longer exists.
pub const IN_STEP_LINEAR_SPEED_CAP_BU_PER_S: f32 = 28_000.0;
/// #5161 — sanity cap on one articulation DOF's generalized velocity
/// (rad/s for the ragdoll/hinge joints' angular axes, BU/s for the
/// prismatic rail; every authored class moves far slower than this). The
/// body-speed cap above cannot see these: articulation links integrate
/// their reduced-coordinate DOF velocities (`Multibody::velocities`)
/// through forward kinematics BEFORE the body-level clamp can matter, so
/// an exploding DOF teleports its links out of the world in one step even
/// with every rigid body capped. Non-finite DOFs are zeroed.
pub(crate) const ARTICULATION_DOF_SANITY_CAP: f32 = 100.0;


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
    use rapier3d::prelude::{Group, InteractionGroups, InteractionTestMode};
    // `And`: the pre-0.31 rule (each side's filter must admit the other's
    // memberships), which rapier now asks for explicitly.
    InteractionGroups::new(Group::ALL, Group::ALL & !ACTOR_BONE_GROUP, InteractionTestMode::And)
}

/// M42.10 — the interaction-group mask a *walking actor's* KCC sweep
/// uses. What it ACTUALLY does (#4690 / PHYS-D4-2026-09-21-02 — the old
/// doc claimed "own bones" and "shoves clutter", neither of which a
/// group mask or this query can deliver):
///
/// - **Every actor's bones are masked, not just the walker's own.** All
///   bone colliders carry [`ACTOR_BONE_GROUP`] and the filter excludes
///   the group wholesale — a group mask cannot distinguish "mine" from
///   "anyone else's", so walking NPCs ghost through each other and
///   through FO4 fallback capsules. Genuine NPC-vs-NPC blocking needs a
///   `QueryFilter` predicate keyed on [`ACTOR_BONE_GROUP`]'s owner
///   (`ActorColliderOwner`), which does not exist yet.
/// - **Dynamic clutter blocks but is never shoved**: `move_character`
///   passes a no-op collision callback and never applies
///   character-collision impulses, and autostep runs with
///   `include_dynamic_bodies: false`, so a dynamic body tall enough to
///   exceed the climb limit blocks a walker outright. Wander/Patrol
///   recover via the stuck re-pick; Travel/Follow/Escort/Guard do not
///   (their gameplay routing is /audit-gameplay's).
///
/// Passed as [`CharacterMoveParams::filter_groups`] by NPC locomotion;
/// identical to the ground-probe mask because the group semantics are
/// the same statement in both query shapes. If own-bones-only exclusion
/// or impulse shoving is ever wanted, both this doc and
/// `CharacterMoveParams::filter_groups`'s doc are part of the contract
/// to update together.
pub fn actor_move_interaction_groups() -> rapier3d::prelude::InteractionGroups {
    ground_probe_groups()
}

/// The filter every *solid-world* probe must use: fixed geometry only, actor
/// bones masked out, and **sensors excluded**.
///
/// #3116 — `ground_probe_groups()` is an interaction-group mask, not a sensor
/// filter; it only masks `ACTOR_BONE_GROUP`. Since `00fc0f3b` (#2549) the
/// engine registers every Havok layer-15 body (`OL_NONCOLLIDABLE` /
/// `FOL_NONCOLLIDABLE` / `SKYL_NONCOLLIDABLE` — the same numeric value across
/// Oblivion / FO3 / FNV / Skyrim) as a Rapier **sensor**. Sensors generate no
/// contact response, but they are still returned by shape and ray casts unless
/// explicitly excluded, so a spawn probe would ground the player on geometry
/// the author marked non-collidable and the player would fall through it on the
/// first step.
///
/// Factored into one function so a fourth probe cannot drift away from the
/// other three — the drift that produced #3116 in the first place.
#[inline]
fn solid_probe_filter<'a>() -> rapier3d::prelude::QueryFilter<'a> {
    rapier3d::prelude::QueryFilter::exclude_dynamic()
        .groups(ground_probe_groups())
        .exclude_sensors()
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
    /// `"SoftFrame"` (a rapier soft-body proxy; the engine builds none) /
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
    /// Rapier 0.36's soft bodies (ropes, cloth). The engine authors none;
    /// the set exists because `PhysicsPipeline::step` and
    /// `RigidBodySet::remove` take it.
    pub soft_bodies: SoftBodySet,
    pub islands: IslandManager,
    /// Rapier's BVH broad phase. Since rapier 0.27 it is also the scene-query
    /// index: [`Self::queries`] borrows its tree, so there is no separate
    /// query pipeline to keep in sync. It never pairs two colliders that no
    /// `ActiveCollisionTypes` admits (fixed-on-fixed, by default), and a body
    /// that changes type has its colliders re-inserted so the pairs it was
    /// denied are reported afresh (rapier 0.35+).
    pub broad_phase: BroadPhaseBvh,
    pub narrow_phase: NarrowPhase,
    pub ccd_solver: CCDSolver,
    pub pipeline: PhysicsPipeline,
    pub integration_parameters: IntegrationParameters,
    pub gravity: Vector,
    /// Seconds of unsimulated time left over from the last frame.
    pub accumulator: f32,
    /// Per-frame wall-clock budget for catch-up substeps (seconds). See
    /// [`SUBSTEP_TIME_BUDGET`] for the anti-spiral rationale (#1698).
    /// Public so tests can force the cap (`0.0`) or disable it (a huge
    /// value); production keeps the [`SUBSTEP_TIME_BUDGET`] default.
    pub substep_time_budget: f32,
    /// One-shot "something changed, step at least once" flag. Set via
    /// [`PhysicsWorld::wake`] by mutations that introduce motion into an
    /// existing body (kinematic push, velocity set, impulse, ragdoll
    /// articulation) — NOT by body spawn, which is exempt by design; see
    /// [`PhysicsWorld::wake`]'s doc (#3969). Cleared the next time the
    /// pipeline actually steps. Lets [`step`](Self::step) skip the (costly)
    /// pipeline run for a fully-asleep scene without missing the first
    /// frame of newly-introduced motion. See the static-scene fast path.
    pending_wake: bool,
    /// Colliders inserted since the last pipeline step, which the broad
    /// phase's BVH — the scene-query index — does not hold yet. Registration
    /// queues them ([`Self::queue_query_refresh`]); a frame that steps
    /// inserts them through the step's own broad-phase update, and a frame
    /// that does not (the static-scene fast path, a sub-tick frame) inserts
    /// exactly these leaves via `BroadPhaseBvh::set_aabb` at its end, so a
    /// streaming frame never pays for the whole collider set (#2864, #4685).
    pending_query_leaves: Vec<ColliderHandle>,
    /// Index of dynamic-body handles for the per-substep recovery snapshot
    /// (#4682 / PHYS-D2-2026-09-21-01). Maintained at the three production
    /// mutation points — newcomer registration (`physics_sync_system`),
    /// ragdoll body construction, [`Self::set_motion_type`] — so the
    /// snapshot can iterate ~N dynamics instead of the whole body arena
    /// (measured 1.77 ms/substep on a 95 k-fixed world, ~10× the solver it
    /// protects). The index MAY hold stale handles (a removed body's slot
    /// or a body since flipped to kinematic/fixed via
    /// [`Self::set_motion_type`] / the ragdoll root pin): the snapshot
    /// re-checks liveness and type per entry and compacts the vector, so
    /// staleness costs a get() miss, never a wrong snapshot. Direct
    /// `bodies.insert` calls that bypass these points (test fixtures) are
    /// not indexed — production code has none.
    pub(crate) dynamic_bodies: Vec<RigidBodyHandle>,
    /// Shape/handle storage generations at the last scan that proved every
    /// shape had handles. Membership changes invalidate the proof (#3477).
    pub(crate) registered_shape_generations: Option<(u64, u64)>,
    /// Lifetime count of solver-explosion recoveries (`restored > 0` in
    /// `step`). #4683 (PHYS-D3-2026-09-21-01): the recovery used to report
    /// itself only through one `log::error!`, invisible to every ragdoll
    /// stability gate — the FO3 P2 pass certified a corpse whose
    /// articulation the recovery itself had detached. Surfaced via
    /// [`Self::recovery_counts`], `phys.stats` and `ragdoll.status`.
    /// Counts EVENTS — one per recovering substep, however many bodies
    /// that substep restored.
    recoveries_total: u64,
    /// BODIES restored during the most recent `step` call (reset at
    /// entry). #5127 — a body count, not an event count: `step` breaks
    /// after a recovery, so the per-frame event count is only ever 0 or 1
    /// and an 18-bone corpse restore reads `total=1 bodies=18`. The name
    /// used to be `recoveries_last_frame`, which made `total >= last_frame`
    /// look like an invariant it never was.
    bodies_restored_last_frame: u32,
    /// Lifetime count of pre-broken bodies parked by
    /// [`Self::recover_pre_broken_bodies`] (#4687a) — the same
    /// non-finite-state class, one step earlier in its lifetime.
    bodies_parked_total: u64,
    /// Lifetime count of keyframe targets refused by
    /// [`Self::accept_keyframe_target`] (#5161) — insane-but-finite bone
    /// transforms that would otherwise panic the multi-SAP broad phase or
    /// park a live actor's bones out of melee reach. Surfaced via
    /// [`Self::keyframe_targets_refused_total`] into `phys.stats`.
    keyframe_targets_refused_total: u64,
    /// Bodies already logged for a refused keyframe target. The refusal
    /// repeats every frame for as long as the animation keeps emitting the
    /// broken pose, so the `log::error!` fires once per body, not per frame.
    keyframe_refusals_logged: std::collections::HashSet<RigidBodyHandle>,
    /// Lifetime count of ragdoll activations refused at attach time (#5161)
    /// — a seed pose that failed `seed_pose_is_sane`, counted once per
    /// rejected spec whether the rejection came from `build_ragdoll`'s own
    /// absolute backstop or the bin-side actor-reach check. Surfaced via
    /// [`Self::ragdoll_seed_refusals_total`] into `phys.stats`.
    ragdoll_seed_refusals_total: u64,
    /// #5161 — human-readable label per rapier body, for the invalid-solve
    /// evidence log (`restore_invalid_dynamic_bodies`). Ragdoll bodies have
    /// no `RapierHandles` component to reverse-look them up with, so
    /// `build_ragdoll` registers "entity N" (the bone) at insert and the
    /// bin-side activator enriches it with the actor + bone name. Dropped
    /// in [`Self::remove_body`]; keyed by the full handle (index +
    /// generation), so rapier handle reuse can never alias an old label.
    body_labels: std::collections::HashMap<RigidBodyHandle, String>,
    /// Lifetime count of velocity clamps applied by
    /// [`Self::clamp_explosive_velocities`] (#5161) — one per clamped body
    /// per substep. Surfaced via [`Self::velocity_clamps_total`] into
    /// `phys.stats`.
    velocity_clamps_total: u64,
    /// Lifetime solver-explosion burst count per body (#5161/#5246).
    /// Deliberately never cleared on clean substeps — the FNV
    /// SLscorpionBurrowINT log showed the same body parked four times
    /// because every wake-and-re-explosion cycle looked like a fresh first
    /// offence when the state was a set cleared on clean substeps. The
    /// count drives the escalation ladder: 1 = clamp, 2 = park, 3+ =
    /// detach the body's whole articulation. Keyed by the full handle
    /// (index + generation), so rapier handle reuse never aliases a count.
    explosion_offences: std::collections::HashMap<RigidBodyHandle, u32>,
    /// Lifetime articulation detaches ordered by
    /// [`Self::clamp_explosive_velocities`]' third-offence escalation
    /// (#5246). Surfaced via [`Self::explosive_detaches_total`] into
    /// `phys.stats`.
    explosive_detaches_total: u64,
    /// Every multibody joint this world built (`build_ragdoll` pushes; the
    /// set has no mutable whole-set iterator and rapier's internal index is
    /// `pub(crate)`). #5355 — handles whose articulation died (ragdoll
    /// teardown via `remove_body`, the third-offence detach) are swept by
    /// the retain at the top of [`Self::clamp_explosive_velocities`], so
    /// the per-substep DOF walk below stays bounded by live joints instead
    /// of one `get_mut` miss per joint of every corpse ever built.
    pub(crate) articulation_joints: Vec<rapier3d::prelude::MultibodyJointHandle>,
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
            normalized_max_linear_velocity: IN_STEP_LINEAR_SPEED_CAP_BU_PER_S / BU_PER_METER,
            ..Default::default()
        };

        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            soft_bodies: SoftBodySet::new(),
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            ccd_solver: CCDSolver::new(),
            pipeline: PhysicsPipeline::new(),
            integration_parameters,
            gravity: Vector::new(0.0, -686.7, 0.0),
            accumulator: 0.0,
            substep_time_budget: SUBSTEP_TIME_BUDGET,
            // Step once on the first frame so any bodies present at startup
            // settle / populate the island state.
            pending_wake: true,
            pending_query_leaves: Vec::new(),
            dynamic_bodies: Vec::new(),
            registered_shape_generations: None,
            recoveries_total: 0,
            bodies_restored_last_frame: 0,
            bodies_parked_total: 0,
            keyframe_targets_refused_total: 0,
            keyframe_refusals_logged: std::collections::HashSet::new(),
            ragdoll_seed_refusals_total: 0,
            body_labels: std::collections::HashMap::new(),
            velocity_clamps_total: 0,
            explosion_offences: std::collections::HashMap::new(),
            explosive_detaches_total: 0,
            articulation_joints: Vec::new(),
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
    ///
    /// **Idempotent** (#3380): calling it again with a handle that has
    /// already been removed is a no-op returning `false`, because
    /// `RigidBodySet::remove` matches on the handle's generation and a
    /// freed slot no longer matches. Callers sweeping a victim list are
    /// therefore allowed to hand over the same handle twice — the
    /// wasted work is theirs to avoid, the arena stays correct either
    /// way. Pinned by
    /// `byroredux::cell_loader::rapier_release_tests::release_is_idempotent_over_a_duplicated_victim_list`.
    pub fn remove_body(&mut self, handle: RigidBodyHandle) -> bool {
        let removed = self
            .bodies
            .remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                &mut self.soft_bodies,
                /* remove_attached_colliders = */ true,
            )
            .is_some();
        if removed {
            // #5161 — the evidence-label dies with the body; the map must
            // not accumulate one stale entry per despawned ragdoll bone.
            self.body_labels.remove(&handle);
            // #5272 — same discipline for the other per-body #5161/#5246
            // bookkeeping: the lifetime offence ladder and the log-once
            // refusal set die with the body, or the maps grow once per
            // despawned bone for the whole session. Keyed by index +
            // generation, so a rapier handle reuse can never inherit the
            // removed body's escalation rung.
            self.explosion_offences.remove(&handle);
            self.keyframe_refusals_logged.remove(&handle);
            // Rapier processes neighbour wake-ups from removed colliders during
            // `pipeline.step()`. Re-arm the static-scene fast path so that
            // deferred cleanup and those wake-ups are not stranded when the
            // scene is otherwise asleep (#2863). The removed colliders' BVH
            // leaves need no refresh here: the next step's broad-phase update
            // drops them, and until then a query reaching one resolves its
            // slot to nothing (or to the collider now occupying it, whose own
            // leaf is queued at insertion).
            self.wake();
        }
        removed
    }

    /// `(awake dynamic bodies, active kinematic bodies)` from the last step's
    /// island state — diagnostic for the static-scene fast path.
    ///
    /// **The second element is not an awake count** (#3975). Rapier's active
    /// set holds the awake *island* — every dynamic body that failed the
    /// sleep test — plus the kinematic bodies, which rapier does not put to
    /// sleep on a sleep test the way it does dynamics. So the first element
    /// is exactly "awake", while the second counts kinematic bodies whether
    /// or not anything moved them — see the static-scene fast path's own
    /// rationale a few hundred lines below, which already knew this and
    /// deliberately does not gate on kinematics for exactly that reason.
    /// Named `active_island_counts`, not `awake_counts`, so the name stops
    /// implying a claim the second half doesn't make.
    pub fn active_island_counts(&self) -> (usize, usize) {
        self.islands
            .active_bodies()
            .filter_map(|h| self.bodies.get(h))
            .fold((0, 0), |(dynamic, kinematic), body| {
                if body.is_dynamic() {
                    (dynamic + 1, kinematic)
                } else {
                    (dynamic, kinematic + body.is_kinematic() as usize)
                }
            })
    }

    /// Whether the last step left any dynamic body awake. Rapier's active set
    /// mixes dynamics with kinematics (which never leave it), so emptiness of
    /// the set itself says nothing — only its dynamic members do.
    fn has_awake_dynamics(&self) -> bool {
        self.islands
            .active_bodies()
            .any(|h| self.bodies.get(h).is_some_and(|b| b.is_dynamic()))
    }

    /// Mark the simulation as needing at least one pipeline step on the next
    /// [`step`](Self::step) call. Must be called by every mutation that can
    /// introduce motion — pushing a kinematic target, setting a velocity,
    /// applying an impulse, re-articulating a ragdoll — so the static-scene
    /// fast path doesn't sleep through the first frame of new motion (the
    /// island lists only reflect the *previous* step, so a just-woken body
    /// isn't in them yet).
    ///
    /// **Spawning a body is the deliberate exemption**, not an omission
    /// (#3969 / PHYS-D2-2026-09-06-02 — this doc used to name it as a
    /// caller). `sync::register_newcomers` builds dynamic bodies
    /// `sleeping(true)` on purpose (the EXTERIOR-FREEZE FIX: a Skyrim
    /// exterior streaming frame measured `atw_scheduler=3005ms` with ~3000
    /// awake dynamics) and announces itself with `queue_query_refresh()`
    /// alone, which the fast path below honours without arming a step.
    /// Consumers that need first-frame visibility of a newcomer take it as
    /// an explicit argument instead — see `water.rs`'s
    /// `apply_buoyancy(world, n_new > 0)` and the `had_newcomers` term in
    /// its quiesced-scene fast path, whose own comment records that it is
    /// load-bearing *because* spawn does not arm `pending_wake`.
    ///
    /// So: do not "reconcile" this by adding a `wake()` to the spawn path.
    /// That reintroduces the measured multi-second streaming stall and
    /// simultaneously makes `had_newcomers` look redundant, inviting its
    /// removal. A new body-creating path should follow `register_newcomers`
    /// (spawn asleep + `queue_query_refresh`) and hand first-frame
    /// visibility to its consumers explicitly.
    #[inline]
    pub fn wake(&mut self) {
        self.pending_wake = true;
    }

    /// Whether a pipeline step is already pending (something was woken or
    /// re-targeted this frame). Read by the WATAL buoyancy phase to skip its
    /// per-body scan in a fully-quiesced scene: with nothing awake and
    /// nothing pending, no body's pose changed, so no dry→wet water
    /// transition can occur and the scan would be pure waste.
    ///
    /// #3969 — this doc used to say "woken / **spawned** / re-targeted",
    /// which is false and misleading exactly where it is read: a body
    /// streaming in already submerged spawns ASLEEP and does not arm this
    /// flag, so `apply_buoyancy`'s fast path must additionally consult its
    /// `had_newcomers` argument to catch that body's first-frame dry→wet
    /// float-up. Any other consumer using this as "did anything change this
    /// frame?" needs the same companion signal. See [`PhysicsWorld::wake`].
    #[inline]
    pub fn pending_wake(&self) -> bool {
        self.pending_wake
    }

    /// Add a persistent external force (engine world-space, Y-up) to a
    /// dynamic body — Bethesda-unit "Newtons" (body mass × BU/s²). The
    /// force **accumulates across frames** until cleared with
    /// [`reset_forces`](Self::reset_forces); the WATAL buoyancy / flow
    /// systems re-derive it every frame, so they call `reset_forces`
    /// first and `add_force` after.
    ///
    /// `wake_up` decides whether the force is allowed to *start* motion:
    ///
    /// - `true` — wake the body and re-arm the static-scene fast path so the
    ///   next [`step`](Self::step) runs (the island lists only reflect the
    ///   *previous* step, so a freshly-forced body isn't in them yet — same
    ///   reason [`wake`](Self::wake) exists). The right choice for a one-off
    ///   push at a body that may be asleep.
    /// - `false` — apply to an already-moving body without disturbing sleep.
    ///   Required by any *per-frame* force: waking unconditionally would keep
    ///   the whole scene stepping forever, since the force is re-derived on
    ///   every tick and would re-wake the body it just settled.
    ///
    /// That parameter is why the buoyancy phase can use this at all (#2889).
    /// Before it, the wrappers hard-coded `wake_up = true` plus `self.wake()`,
    /// which would have defeated the wake discipline
    /// [`crate::water::apply_buoyancy`] is built around — so the one consumer
    /// the API was written for reached past it to the Rapier body instead,
    /// leaving this an untested-in-production public surface.
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
        wake_up: bool,
    ) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            if b.body_type() == RigidBodyType::Dynamic {
                b.add_force(force, wake_up);
                if wake_up {
                    self.wake();
                }
                return true;
            }
        }
        false
    }

    /// Apply an instantaneous impulse (engine world-space, Y-up) to a
    /// dynamic body — changes velocity by `impulse / mass` immediately,
    /// independent of the per-frame force accumulation. Wakes the body +
    /// re-arms the fast path. No-op on dead / non-dynamic handles.
    ///
    /// Unlike its two siblings this takes no `wake_up` flag, because a
    /// one-shot impulse at a body that must stay asleep is not a meaningful
    /// request — the impulse would be integrated into a velocity nothing
    /// steps.
    ///
    /// **No production consumer yet** (#2889): the intended one is the WATAL
    /// Phase 3 splash kick / actor-jumping-out-of-water effect, which is not
    /// built. Exercised only by this module's unit tests until then — stated
    /// here so the gap is visible from the API rather than discovered by
    /// grep.
    pub fn apply_impulse(
        &mut self,
        handle: RigidBodyHandle,
        impulse: byroredux_core::math::Vec3,
    ) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            if b.body_type() == RigidBodyType::Dynamic {
                b.apply_impulse(impulse, true);
                self.wake();
                return true;
            }
        }
        false
    }

    /// Clear the accumulated external force + torque on a body. Called by
    /// the buoyancy / flow systems at the top of each frame before they
    /// re-`add_force`, so forces don't compound frame-over-frame. No-op on
    /// a dead handle. Never re-arms the fast path (the following
    /// `add_force` does that when there's still a force to apply; a body
    /// with zero net force this frame should be allowed to sleep).
    ///
    /// `wake_up` is Rapier's own body-level wake, distinct from the fast-path
    /// arming above: `false` clears the accumulator without disturbing a
    /// sleeping body, which is what a per-frame re-derivation wants (#2889).
    pub fn reset_forces(&mut self, handle: RigidBodyHandle, wake_up: bool) -> bool {
        if let Some(b) = self.bodies.get_mut(handle) {
            b.reset_forces(wake_up);
            b.reset_torques(wake_up);
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
        // A body leaving the fixed type needs its overlaps with fixed
        // colliders reported — the broad phase never paired them while both
        // were fixed. Rapier does it: a type change re-inserts the body's
        // colliders into the BVH as new leaves, which re-reports every pair
        // the `ActiveCollisionTypes` filter withheld.
        body.set_body_type(body_type, wake_up);
        if wake_up {
            self.wake();
        }
        // #4682 — keep the recovery-snapshot index informed. A flip TO
        // dynamic must be indexed or the body loses snapshot coverage; a
        // flip AWAY can stay listed (the snapshot's per-entry type check
        // filters it). The `contains` guard keeps repeated
        // dynamic↔kinematic toggling from growing the index without bound.
        if body_type == RigidBodyType::Dynamic && !self.dynamic_bodies.contains(&handle) {
            self.dynamic_bodies.push(handle);
        }
        true
    }


    /// Live dynamic bodies tracked by the recovery-snapshot index (#4682).
    /// Diagnostic only; a stale handle not yet compacted out is excluded.
    pub fn dynamic_body_count(&self) -> usize {
        self.dynamic_bodies
            .iter()
            .filter(|h| {
                self.bodies
                    .get(**h)
                    .is_some_and(|b| b.body_type() == RigidBodyType::Dynamic)
            })
            .count()
    }





    /// #5161 — register the human-readable label for one rapier body (see
    /// the `body_labels` field doc). `build_ragdoll` writes the default;
    /// the bin-side activator enriches it.
    pub fn set_body_label(&mut self, handle: RigidBodyHandle, label: String) {
        self.body_labels.insert(handle, label);
    }

    /// #5161 — a body's registered evidence-log label, if any.
    pub fn body_label(&self, handle: RigidBodyHandle) -> Option<&str> {
        self.body_labels.get(&handle).map(String::as_str)
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
    /// cost. Once the loop is entered at least one substep always runs (the
    /// budget check is *after* the step), so a genuinely slow frame still
    /// advances the simulation. See [`SUBSTEP_TIME_BUDGET`] (#1698).
    ///
    /// That guarantee is scoped to the budget bail-out and does **not** mean
    /// every call steps (#2879): whenever the accumulator has not yet reached
    /// `PHYSICS_DT` this returns `0` having done nothing — the normal case
    /// above 60 fps, where several frames bank sub-tick time before one
    /// full tick is due. Callers must not read `0` as "the simulation is
    /// idle"; that is what [`Self::pending_wake`] is for.
    ///
    /// `frame_dt` is sanitised through `f32::max`, which returns the
    /// **non-NaN** operand — so `NAN` and negatives alike contribute zero
    /// rather than poisoning the accumulator. This is deliberate, not
    /// incidental: `f32::maximum` *propagates* NaN, and a NaN accumulator
    /// can never satisfy `>= PHYSICS_DT`, wedging the simulation forever
    /// with no error. Pinned by `non_finite_frame_dt_cannot_poison_the_accumulator`.
    pub fn step(&mut self, frame_dt: f32) -> u32 {
        // Do NOT rewrite as `f32::maximum` or an `if frame_dt > 0.0` guard
        // without re-reading the NaN note above (#2879).
        self.accumulator += frame_dt.max(0.0);
        let max_acc = MAX_SUBSTEPS as f32 * PHYSICS_DT;
        if self.accumulator > max_acc {
            self.accumulator = max_acc;
        }

        // Static-scene fast path — skip the whole tick when there is no
        // simulation work to do.
        //
        // WHY IT EXISTS (history, not current cost — #2890). Before
        // `6e55b492` a radius-12 FNV exterior spent ~45 ms/frame in `step`
        // for a scene where nothing was moving, from three compounding
        // causes: `length_unit` was left at 1.0 so bodies never slept, there
        // was no fast path, and the query pipeline was rebuilt inside *every*
        // substep. That commit fixed all three and measured ~45 ms → ~0.02 ms
        // once settled. The old wording here quoted the pre-fix
        // "~8-10 ms/step × 5 substeps" figure in the present tense and
        // attributed it to `pipeline.step()`, which is wrong twice over: the
        // number predates its own commit, and the per-substep rebuild it
        // measured is removed forty lines below (`None` is passed for the
        // query pipeline).
        //
        // WHERE THE COST ACTUALLY IS, today: nowhere near here. #4685
        // (PHYS-D6-2026-09-21-02) measured the next suspect on rapier 0.22 —
        // a full query-QBVH `clear_and_rebuild` after the substep loop,
        // 9.6 ms/frame on a 95 k-collider world against 0.10 ms incremental.
        // Since rapier 0.27 there is no separate query tree to rebuild: scene
        // queries borrow the broad phase's BVH (`Self::queries`), which the
        // step maintains incrementally, and a frame that steps nothing only
        // inserts the leaves registration queued (`pending_query_leaves`).
        // Historical attribution, kept for its method: the `6e55b492`-era
        // proxy (release build, 30 000 fixed cuboids + 1 awake dynamic body,
        // 20 iterations after warmup) measured the then-design at ≈ 2.1-2.4
        // ms/frame with ≈ 2.1 ms of it the bare post-loop rebuild — "the
        // rebuild, not the solver". Caveat, still true: all-cuboid with one
        // moving body is not a real cell — real content is TriMesh-heavy
        // with real contact work, so solver costs go up on both designs.
        //
        // Skip conditions:
        //
        //   * No awake dynamic body (rapier's active set reflects the
        //     previous step; a body can only newly wake via a contact, which
        //     requires something else to have moved — covered by `wake()`).
        //   * Nothing was explicitly woken this frame (`pending_wake`): a
        //     set velocity, an applied impulse, or a kinematic push. NOT a
        //     spawn — `register_newcomers` deliberately leaves `pending_wake`
        //     clear and spawns dynamics asleep (see `wake`'s doc for why, and
        //     for the consumer-side `had_newcomers` contract that depends on
        //     it). A streaming frame with no other motion therefore lands
        //     HERE, in the fast path, and is served by the queued-leaf
        //     insertion just below rather than by a pipeline step (#3969).
        //
        // NOTE: we deliberately do NOT gate on kinematic bodies. Rapier keeps
        // kinematic bodies in its active set without a sleep test (idle ones
        // are just skipped in the solver via a zero-velocity check), so the
        // set is never empty in a cell with authored-keyframed clutter —
        // testing it would defeat the fast path entirely.
        // Real kinematic *motion* is captured by `pending_wake` instead
        // (`push_kinematic` / `set_kinematic_translation` call `wake()`).
        self.bodies_restored_last_frame = 0;
        self.recover_pre_broken_bodies();
        if !self.has_awake_dynamics() && !self.pending_wake {
            self.insert_pending_query_leaves();
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
            if matches!(
                self.run_substep(loop_start, budget),
                SubstepOutcome::Stop
            ) {
                steps += 1;
                break;
            }
            steps += 1;
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
                .active_bodies()
                .filter(|h| {
                    self.bodies
                        .get(*h)
                        .is_some_and(|b| b.is_dynamic() && b.translation().y < KILL_PLANE_Y)
                })
                .collect();
            for h in fallen {
                if let Some(b) = self.bodies.get_mut(h) {
                    b.set_linvel(Vector::ZERO, false);
                    b.set_angvel(AngVector::ZERO, false);
                    b.sleep();
                }
            }
        }

        // Scene-query refresh, post-substeps. A frame that stepped needs
        // nothing here: the step's broad-phase update inserted every queued
        // collider into the BVH the queries read. A frame that registered
        // colliders but ran no substep (accumulator below one tick) inserts
        // just those leaves (#2864's deferred registration).
        if steps > 0 {
            self.pending_query_leaves.clear();
        } else {
            self.insert_pending_query_leaves();
        }
        steps
    }

    /// One catch-up substep: snapshot every dynamic body, run rapier's
    /// pipeline, then contain any solver explosion the substep produced
    /// (restore + clamp, #5161/#5246). Extracted from `step`'s loop
    /// (#5311) — the loop body had grown past the function that drives
    /// it. Returns [`SubstepOutcome::Stop`] when the catch-up loop must
    /// not run another substep this frame: either a body was restored
    /// (do not spend further substeps on the same freshly-invalidated
    /// contact island) or the wall-clock budget ran out (#1698).
    fn run_substep(&mut self, loop_start: std::time::Instant, budget: f32) -> SubstepOutcome {
        // Newly activated ragdolls are absent from Rapier's active
        // islands until *after* their first pipeline step. Snapshot all
        // dynamics so their first solve is recoverable too. #4682 — the
        // dynamics come from the maintained index, not an arena walk:
        // iterating every slot (fixed bodies included) cost 1.77 ms per
        // substep on a 95 k-body world, ~10× the solver it protects.
        // The per-entry liveness + type re-check keeps the index's
        // staleness tolerance honest (see the field doc), and waking by
        // contact mid-step needs no index update — the body was indexed
        // at insert regardless of sleep state.
        self.dynamic_bodies.retain(|h| self.bodies.get(*h).is_some());
        let snapshots: Vec<_> = self
            .dynamic_bodies
            .iter()
            .filter_map(|&handle| {
                let body = self.bodies.get(handle)?;
                (body.body_type() == RigidBodyType::Dynamic && body_state_is_finite(body))
                    .then_some(DynamicBodySnapshot {
                        handle,
                        position: *body.position(),
                    })
            })
            .collect();
        self.pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            &mut self.ccd_solver,
            &(),
            &(),
        );
        self.accumulator -= PHYSICS_DT;
        // Rapier quarantines a body whose pose or velocity went non-finite
        // during the step: rolled back to its last valid pose, zeroed and
        // DISABLED (no collisions, no simulation). The engine's containment
        // is to park, not to disable — the restore below re-enables these
        // and treats them as invalid alongside its own displacement check.
        let quarantine = self.pipeline.quarantine();
        let quarantined: Vec<RigidBodyHandle> = quarantine.bodies().to_vec();
        if !quarantine.colliders().is_empty() {
            log::error!(
                "physics: rapier disabled {} collider(s) whose geometry went non-finite: {:?}",
                quarantine.colliders().len(),
                quarantine.colliders()
            );
        }
        let (restored, invalid_handles) = restore_invalid_dynamic_bodies(
            &mut self.bodies,
            &mut self.multibody_joints,
            snapshots,
            &quarantined,
            &self.body_labels,
        );
        if restored > 0 {
            log::error!(
                "physics: restored {restored} dynamic body/bodies after an invalid solve; \
                 affected bodies were put to sleep at their prior pose"
            );
            self.recoveries_total = self.recoveries_total.saturating_add(1);
            self.bodies_restored_last_frame =
                self.bodies_restored_last_frame.saturating_add(restored as u32);
            self.refresh_query_geometry_after_restore(&invalid_handles);
            // Do not spend further catch-up substeps on the same
            // freshly-invalidated contact island this frame.
            self.accumulator = 0.0;
            return SubstepOutcome::Stop;
        }
        // #5161 — caps velocities before they can be integrated into an
        // insane position. The restore branch above already sanitises
        // its bodies (rolled back + slept), so skipping the clamp there
        // loses nothing.
        self.clamp_explosive_velocities();
        // Budget check AFTER the step so at least one substep always
        // runs (a slow frame must still advance the sim). When physics
        // has spent its per-frame wall-time, forfeit the leftover
        // accumulator — catching up is futile once a single substep
        // already costs more wall-time than the sim-time it produces.
        if loop_start.elapsed().as_secs_f32() >= budget {
            self.accumulator = 0.0;
            return SubstepOutcome::Stop;
        }
        SubstepOutcome::Continue
    }
}

/// Follow-up decision of one [`PhysicsWorld::run_substep`] for the
/// catch-up loop in `step`.
enum SubstepOutcome {
    /// Run another substep if accumulator and substep budget allow.
    Continue,
    /// Stop the catch-up loop this frame.
    Stop,
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
    use crate::convert::{collision_shape_to_parts, pose_from_trs};
    use byroredux_core::ecs::components::collision::CollisionShape;
    use byroredux_core::math::{Quat, Vec3};

    /// #4614 — the stack capsule is the same shape the `SharedShape` form
    /// built (same segment and radius, same `1e-3` floor for degenerate
    /// extents), and no production query goes back to the per-call `Arc`.
    #[test]
    fn character_capsule_matches_the_shared_shape_it_replaced() {
        for (half_height, radius) in [(46.0, 18.0), (0.0, 0.0), (-3.0, 12.5)] {
            let stack = character_capsule(half_height, radius);
            let shared = SharedShape::capsule_y(half_height.max(1e-3), radius.max(1e-3));
            let shared = shared.as_capsule().expect("capsule");
            assert_eq!(stack.segment.a, shared.segment.a);
            assert_eq!(stack.segment.b, shared.segment.b);
            assert_eq!(stack.radius, shared.radius);
        }

        // Production now spans three files (#5311) — scan them all, so a
        // per-call `SharedShape::capsule_y` cannot hide in a split-out module.
        fn production(src: &str) -> &str {
            src.split_once("#[cfg(test)]\nmod ").map_or(src, |(head, _)| head)
        }
        for file in [include_str!("mod.rs"), include_str!("queries.rs"), include_str!("recovery.rs")] {
            assert!(
                !production(file).contains("SharedShape::capsule_y("),
                "character sweeps and ground probes must use character_capsule, not a per-call Arc"
            );
        }
    }

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

    /// #5160 — a melee swing must resolve through a corridor, not a line.
    /// Authored actor bone colliders are small boxes with real gaps between
    /// them; a zero-width ray aimed dead-centre on an actor can pass between
    /// two bones (the p2-melee-core GSSettlerCM repro: hit vs miss decided
    /// by 1.6° of pitch). The corridor cast must catch what the line threads.
    #[test]
    fn corridor_cast_hits_a_bone_the_zero_width_ray_threads() {
        let mut w = PhysicsWorld::new();

        // Two bone-sized kinematic boxes either side of +Z, mirroring the
        // measured FNV humanoid bone extents (16-18 BU): centres at x=±16,
        // each 16 BU wide, leaving a 16 BU gap the aim line travels through
        // but a 12 BU-radius corridor (24 BU across) cannot.
        for x in [-16.0f32, 16.0f32] {
            let shape = single_shape(&CollisionShape::Cuboid {
                half_extents: Vec3::new(8.0, 9.0, 8.0),
            });
            let body = RigidBodyBuilder::kinematic_position_based()
                .pose(pose_from_trs(Vec3::new(x, 50.0, 100.0), Quat::IDENTITY))
                .build();
            let handle = w.bodies.insert(body);
            w.colliders.insert_with_parent(
                ColliderBuilder::new(shape).build(),
                handle,
                &mut w.bodies,
            );
        }
        w.update_query_pipeline();

        let origin = Vec3::new(0.0, 50.0, 0.0);
        let direction = Vec3::new(0.0, 0.0, 1.0);

        // The zero-width ray travels the 16 BU gap between the boxes.
        assert!(
            w.cast_ray(origin, direction, 200.0, None).is_none(),
            "test geometry: the plain ray must thread the gap or the fixture \
             no longer reproduces the #5160 miss"
        );

        // A 12 BU corridor (the melee swing's blade sweep) reaches the boxes.
        let hit = w
            .cast_ray_corridor(origin, direction, 200.0, 12.0, None)
            .expect("the corridor must catch a bone the line threads (#5160)");
        // Box x-face at 8 from the centre-line, ball radius 12: contact when
        // sqrt(8² + dz²) = 12 → dz = sqrt(80) ≈ 8.944 ahead of the z-face at
        // 92, so time of impact ≈ 83.056. Assert the band, not the digit —
        // parry's CCD rounding owns the last ulp.
        assert!(
            (83.0..=83.2).contains(&hit.distance),
            "unexpected corridor contact distance {}",
            hit.distance
        );
        assert!(
            hit.body.is_some(),
            "the corridor hit must resolve to its parent body for ownership"
        );
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
                .translation(Vector::new(0.0, 0.0, -10.0))
                .build(),
            player,
            &mut world.bodies,
        );
        let wall = world.bodies.insert(RigidBodyBuilder::fixed().build());
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(20.0, 20.0, 2.0)
                .translation(Vector::new(0.0, 0.0, -100.0))
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
                .translation(Vector::new(0.0, 0.0, -20.0))
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
        .pose(pose_from_trs(pos, Quat::IDENTITY))
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

        // parry 0.31's capsule cast stops a hair short of the face
        // (0.99992 here): compare within its tolerance, not bit-exact.
        let surface = hit.expect("the walkable floor must be accepted");
        assert!((surface - 1.0).abs() < 1e-3, "surface y = {surface}");
    }

    #[test]
    fn capsule_clearance_rejects_door_overlap_and_can_exclude_self() {
        let mut w = PhysicsWorld::new();
        insert_slab(&mut w, Vec3::ZERO, false);
        let door = w
            .bodies
            .insert(RigidBodyBuilder::kinematic_position_based().build());
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(2.0, 100.0, 20.0).build(),
            door,
            &mut w.bodies,
        );
        w.update_query_pipeline();
        let center = Vec3::new(0.0, 20.0, 0.0);
        assert!(w.capsule_overlaps_solid(center, 10.0, 5.0, None));
        assert!(!w.capsule_overlaps_solid(center, 10.0, 5.0, Some(door)));
        assert!(!w.capsule_overlaps_solid(Vec3::new(30.0, 20.0, 0.0), 10.0, 5.0, None));
    }

    #[test]
    fn capsule_clearance_ignores_sensors_and_live_actor_bones() {
        let mut w = PhysicsWorld::new();
        let body = w.bodies.insert(RigidBodyBuilder::fixed().build());
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(20.0, 100.0, 20.0)
                .sensor(true)
                .build(),
            body,
            &mut w.bodies,
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(20.0, 100.0, 20.0)
                .collision_groups(InteractionGroups::new(ACTOR_BONE_GROUP, Group::ALL, InteractionTestMode::And))
                .build(),
            body,
            &mut w.bodies,
        );
        w.update_query_pipeline();
        assert!(!w.capsule_overlaps_solid(Vec3::ZERO, 10.0, 5.0, None));
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
            .pose(pose_from_trs(Vec3::ZERO, Quat::IDENTITY))
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
            .pose(pose_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
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

    /// #5161 — a solver explosion's velocity is capped at the end of the
    /// substep that produced it, so the next substep's integration cannot
    /// carry the body's AABB anywhere near the multi-SAP grid boundary; a
    /// body that keeps exploding on the next substep is parked (zeroed
    /// velocities, slept) instead of vibrating at the cap. The test
    /// velocity sits in the clamp-only window — above the sanity cap but
    /// under the per-substep displacement the invalid-solve restore would
    /// otherwise claim first (20 000 < 100 000 ≤ 122 880 = 2048·60).
    #[test]
    fn explosive_velocities_are_capped_and_persistent_exploders_are_parked() {
        let mut w = PhysicsWorld::new();
        let handle = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.dynamic_bodies.push(handle);
        w.set_body_label(handle, "actor 2484 bone RArm_Palm".to_owned());

        w.bodies[handle]
            .set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.step(PHYSICS_DT);
        let speed = w.bodies[handle].linvel().length();
        assert!(
            speed <= VELOCITY_SANITY_CAP_BU_PER_S,
            "explosive speed must be capped, got {speed}"
        );
        assert_eq!(w.velocity_clamps_total(), 1);
        assert!(!w.bodies[handle].is_sleeping(), "first clamp only watches");

        // Still exploding on the next substep → parked.
        w.bodies[handle]
            .set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.step(PHYSICS_DT);
        assert!(
            w.bodies[handle].is_sleeping(),
            "a second consecutive clamped substep must park the body"
        );
        assert_eq!(w.bodies[handle].linvel().length(), 0.0);
    }

    /// #5246 — the offence count is LIFETIME, not cleared by clean substeps:
    /// the FNV SLscorpionBurrowINT log showed the same body parked four
    /// times because every wake-and-re-explosion cycle looked like a fresh
    /// first offence. Second burst parks even after a clean substep.
    #[test]
    fn a_second_burst_parks_even_after_a_clean_substep() {
        let mut w = PhysicsWorld::new();
        let handle = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.dynamic_bodies.push(handle);

        w.bodies[handle]
            .set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.step(PHYSICS_DT);
        assert_eq!(w.velocity_clamps_total(), 1);
        assert!(!w.bodies[handle].is_sleeping());

        // The capped velocity decays under gravity but stays clean.
        w.step(PHYSICS_DT);
        assert_eq!(w.velocity_clamps_total(), 1);

        // A fresh burst — the second lifetime offence parks the body.
        w.bodies[handle]
            .set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.step(PHYSICS_DT);
        assert_eq!(w.velocity_clamps_total(), 2);
        assert!(
            w.bodies[handle].is_sleeping(),
            "the second lifetime offence must park the body"
        );
    }

    /// #5246 — a NaN velocity is ZEROED on its first clamp, never passed
    /// through: NaN fails both the `<=` cap and the `>` rescale
    /// comparisons, so the pre-#5246 clamp counted it without writing
    /// anything — and one integration step later the NaN position reached
    /// the broad phase, whose grid clamp turns NaN AABBs into corner-
    /// clamped finite ones that poison the multi-SAP layers.
    #[test]
    fn nan_velocities_are_zeroed_on_the_first_clamp() {
        let mut w = PhysicsWorld::new();
        let handle = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.dynamic_bodies.push(handle);

        w.bodies[handle]
            .set_linvel(Vector::new(f32::NAN, 1.0, 0.0), false);
        w.bodies[handle]
            .set_angvel(Vector::new(0.0, f32::NAN, 0.0), false);
        // Called directly, not through `step`: the per-substep restore's
        // finiteness check covers velocities too, so a NaN state that
        // reaches a substep boundary is restored (rolled back + detached)
        // before the clamp ever runs. The clamp's NaN branch exists for
        // the states it DOES see between calls — the ones the restore's
        // break-on-restore skipped.
        w.clamp_explosive_velocities();

        let linvel = w.bodies[handle].linvel();
        let angvel = w.bodies[handle].angvel();
        assert!(
            linvel.is_finite() && linvel.length() <= 1.0,
            "NaN linear velocity must be zeroed, got {linvel:?}"
        );
        assert!(
            angvel.is_finite(),
            "NaN angular velocity must be zeroed, got {angvel:?}"
        );
        assert_eq!(w.velocity_clamps_total(), 1);
    }

    /// #5246/#5356 — the third lifetime burst escalates to the top rung
    /// (park + articulation detach), but a body with no articulation must
    /// not count a "detach": `explosive_detaches_total` reads
    /// "articulations detached", not "bursts past the third". Driven on a
    /// plain dynamic body because a multibody LINK's velocity is
    /// solver-owned — `set_linvel` on it is replaced by forward kinematics
    /// before the clamp could ever see it; the real-articulation leg is
    /// `the_third_burst_detaches_a_live_articulation` below, and the
    /// joint-removal mechanics themselves are pinned by the #4687 restore
    /// tests.
    #[test]
    fn the_third_burst_parks_a_jointless_body_without_counting_a_detach() {
        let mut w = PhysicsWorld::new();
        let victim = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.dynamic_bodies.push(victim);
        w.set_body_label(victim, "actor 295 bone bip01 neck1".to_owned());

        for burst in 1..=3 {
            // A re-explosion starts with the parked body being woken (a
            // contact, a hit). Rapier 0.36 does not carry a velocity written
            // into a sleeping island into the step, so the fixture wakes the
            // body with it; rapier's in-step cap then bounds it to
            // `IN_STEP_LINEAR_SPEED_CAP_BU_PER_S`, still over the sanity cap.
            w.bodies[victim]
                .set_linvel(Vector::new(100_000.0, 0.0, 0.0), true);
            // Re-arm the world: a parked rig sleeps, the static-scene fast
            // path takes zero substeps unless the wake flag is armed —
            // exactly the wake-and-re-explosion cycle the escalation
            // exists to terminate.
            w.wake();
            w.step(PHYSICS_DT);
            assert_eq!(
                w.velocity_clamps_total(),
                burst as u64,
                "burst {burst} must clamp"
            );
        }
        assert_eq!(
            w.explosive_detaches_total(),
            0,
            "a joint-less body must reach the park rung without counting an \
             articulation detach (#5356)"
        );
        // And the victim is parked: finite, zeroed, asleep.
        assert!(w.bodies[victim].is_sleeping());
        assert!(w.bodies[victim].linvel().length() == 0.0);
    }

    /// #5356 — the positive leg: a body that genuinely belongs to a live
    /// articulation counts exactly one detach on the third lifetime burst,
    /// the joint is really gone afterwards, and later bursts on the freed
    /// body must not recount it. The offence ladder is seeded at 2 and the
    /// clamp called by hand for the same solver-owned-velocity reason the
    /// jointless test records above.
    #[test]
    fn the_third_burst_detaches_a_live_articulation() {
        use rapier3d::dynamics::{GenericJoint, GenericJointBuilder, JointAxesMask};

        let mut w = PhysicsWorld::new();
        let root = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        let child = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        let fixed = || -> GenericJoint {
            GenericJointBuilder::new(
                JointAxesMask::LIN_X
                    | JointAxesMask::LIN_Y
                    | JointAxesMask::LIN_Z
                    | JointAxesMask::ANG_X
                    | JointAxesMask::ANG_Y
                    | JointAxesMask::ANG_Z,
            )
            .into()
        };
        let jh = w.multibody_joints.insert(root, child, fixed(), true).unwrap();
        assert!(
            w.multibody_joints.rigid_body_link(root).is_some(),
            "the root must start as a live articulation link"
        );
        w.dynamic_bodies.push(root);
        w.explosion_offences.insert(root, 2);

        w.bodies[root].set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.clamp_explosive_velocities();
        assert_eq!(
            w.explosive_detaches_total(),
            1,
            "the third lifetime burst on a live articulation must count the detach"
        );
        assert!(
            w.multibody_joints.get(jh).is_none(),
            "the articulation must actually be removed, not just counted"
        );
        assert!(w.bodies[root].is_sleeping());

        // A fourth burst on the now-free root parks it again but must not
        // recount the detach.
        w.bodies[root].set_linvel(Vector::new(100_000.0, 0.0, 0.0), false);
        w.clamp_explosive_velocities();
        assert_eq!(
            w.explosive_detaches_total(),
            1,
            "bursts after the articulation is freed must not recount (#5356)"
        );
    }

    /// #5355 — `articulation_joints` is swept of dead handles at the top of
    /// `clamp_explosive_velocities`, so the per-substep DOF walk stays
    /// bounded by live joints instead of paying one `get_mut` miss per
    /// joint of every corpse ever built.
    #[test]
    fn articulation_joints_are_swept_when_their_articulation_dies() {
        use rapier3d::dynamics::{GenericJoint, GenericJointBuilder, JointAxesMask};

        let mut w = PhysicsWorld::new();
        let a = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        let b = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        let fixed = || -> GenericJoint {
            GenericJointBuilder::new(
                JointAxesMask::LIN_X
                    | JointAxesMask::LIN_Y
                    | JointAxesMask::LIN_Z
                    | JointAxesMask::ANG_X
                    | JointAxesMask::ANG_Y
                    | JointAxesMask::ANG_Z,
            )
            .into()
        };
        let jh = w.multibody_joints.insert(a, b, fixed(), true).unwrap();
        w.articulation_joints.push(jh);

        // `remove_body` cascades the joint out of the set but (by design,
        // #5355) not out of the walk index — the sweep owns that.
        assert!(w.remove_body(a));
        assert_eq!(w.articulation_joints.len(), 1);
        assert!(w.multibody_joints.get(jh).is_none());
        w.clamp_explosive_velocities();
        assert!(
            w.articulation_joints.is_empty(),
            "the dead joint handle must be swept (#5355)"
        );
    }

    /// #5272 — `remove_body` retires the body's #5161/#5246 bookkeeping
    /// with it: the lifetime offence ladder and the log-once refusal set
    /// die with the body instead of accumulating one stale entry per
    /// despawned bone for the whole session.
    #[test]
    fn remove_body_prunes_the_offence_ladder_and_refusal_set() {
        let mut w = PhysicsWorld::new();
        let h = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.explosion_offences.insert(h, 2);
        w.keyframe_refusals_logged.insert(h);

        assert!(w.remove_body(h));
        assert!(
            !w.explosion_offences.contains_key(&h),
            "the offence ladder must die with the body (#5272)"
        );
        assert!(
            !w.keyframe_refusals_logged.contains(&h),
            "the log-once refusal set must die with the body (#5272)"
        );
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
            .pose(pose_from_trs(Vec3::new(0.0, 200.0, 0.0), Quat::IDENTITY))
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
                .pose(pose_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
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

    /// Regression for #2879. Every `step()` call in this suite passed either
    /// exactly `PHYSICS_DT` or `100.0` — the one dt at which the accumulator
    /// always reaches a substep on the first call, and a hitch. The
    /// `accumulator < PHYSICS_DT` branch (the whole above-60 fps regime, the
    /// project's own target) was never exercised, which is how #2856's total
    /// stall shipped green.
    ///
    /// Sub-tick frames must **bank** time rather than forfeit it: four
    /// quarter-tick frames owe exactly one substep, and the wake that armed
    /// them has to survive all four.
    #[test]
    fn sub_tick_frames_bank_time_until_a_full_tick_is_due() {
        let mut w = PhysicsWorld::new();
        w.step(PHYSICS_DT); // settle
        assert_eq!(w.step(PHYSICS_DT), 0, "asleep");

        w.wake();
        let quarter = PHYSICS_DT / 4.0;
        // Three sub-tick frames: no substep is due yet, and the wake must
        // still be armed — consuming it here is exactly #2856, and the
        // static-scene fast path would then zero the banked accumulator.
        for frame in 0..3 {
            assert_eq!(w.step(quarter), 0, "frame {frame} owes no full tick yet");
            assert!(
                w.pending_wake(),
                "the wake must survive sub-tick frame {frame} — clearing it \
                 lets the fast path zero the accumulator and freeze the scene"
            );
        }
        assert_eq!(w.step(quarter), 1, "the fourth quarter completes one tick");
        assert!(!w.pending_wake(), "a substep ran, so the wake is consumed");
    }

    /// Companion for #2879: the quiesced fast path must reach the same
    /// verdict at a sub-tick `dt` as at exactly `PHYSICS_DT`. With nothing
    /// awake and no pending wake there is no work at any frame rate.
    #[test]
    fn static_scene_skips_step_at_sub_tick_frame_rates_too() {
        let mut w = PhysicsWorld::new();
        let floor = single_shape(&CollisionShape::Cuboid {
            half_extents: Vec3::new(500.0, 1.0, 500.0),
        });
        let fh = w.bodies.insert(RigidBodyBuilder::fixed().build());
        w.colliders
            .insert_with_parent(ColliderBuilder::new(floor).build(), fh, &mut w.bodies);

        assert!(w.step(PHYSICS_DT) > 0, "first step settles initial state");
        let half = PHYSICS_DT / 2.0;
        for _ in 0..8 {
            assert_eq!(w.step(half), 0, "no dynamics awake → step skipped");
        }
        // Eight half-ticks is four ticks of wall time; a quiesced scene must
        // not have banked any of it, or it would burst four substeps the
        // instant anything woke.
        assert_eq!(
            w.accumulator, 0.0,
            "the quiesced fast path zeroes the accumulator every frame"
        );
    }

    /// Regression for #2879. `frame_dt.max(0.0)` is NaN-safe only because
    /// Rust's `f32::max` returns the non-NaN operand. Nothing stated that was
    /// intentional and no test pinned it, so a refactor to `f32::maximum`
    /// (which *propagates* NaN) or to an `if frame_dt > 0.0` guard would turn
    /// the accumulator into NaN — which can never satisfy `>= PHYSICS_DT`,
    /// wedging the simulation forever with no error and no panic.
    #[test]
    fn non_finite_frame_dt_cannot_poison_the_accumulator() {
        for bad in [f32::NAN, -1.0, f32::NEG_INFINITY] {
            let mut w = PhysicsWorld::new();
            w.step(PHYSICS_DT); // settle
            w.wake();
            let before = w.accumulator;

            assert_eq!(w.step(bad), 0, "{bad} owes no substep");
            assert!(
                w.accumulator.is_finite(),
                "frame_dt={bad} poisoned the accumulator ({}) — it can never \
                 reach PHYSICS_DT again and the simulation is wedged",
                w.accumulator
            );
            assert_eq!(w.accumulator, before, "a bad frame_dt must contribute 0");

            // And the world must still be steppable afterwards.
            assert_eq!(w.step(PHYSICS_DT), 1, "recovers on the next good frame");
        }
    }

    /// #4682 (PHYS-D2-2026-09-21-01) — the recovery snapshot draws from the
    /// maintained dynamic-body index, not an arena walk. Timing is too
    /// flaky to pin directly, so this pins the SHAPE the cost follows:
    /// the index tracks dynamics only (30 k fixed bodies cost nothing),
    /// and set_motion_type keeps it informed in both directions.
    #[test]
    fn recovery_snapshot_index_tracks_dynamics_not_the_arena() {
        let mut w = PhysicsWorld::new();
        for i in 0..2000 {
            let body = RigidBodyBuilder::fixed()
                .translation(Vector::new(i as f32, 0.0, 0.0))
                .build();
            let h = w.bodies.insert(body);
            w.colliders.insert_with_parent(
                ColliderBuilder::cuboid(1.0, 1.0, 1.0).build(),
                h,
                &mut w.bodies,
            );
        }
        let d1 = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        let d2 = w.bodies.insert(RigidBodyBuilder::dynamic().build());
        w.dynamic_bodies.push(d1);
        w.dynamic_bodies.push(d2);
        assert_eq!(
            w.dynamic_body_count(),
            2,
            "2000 fixed bodies must not enter the snapshot index"
        );

        // A keyframed flip takes the body out of the effective set (stale
        // entry tolerated, filtered by the type check), and back — the
        // flip back re-indexes without duplicating (the `contains` guard
        // keeps dynamic↔kinematic toggling from growing the index).
        assert!(w.set_motion_type(d1, MotionType::CharacterKinematic, false));
        assert_eq!(w.dynamic_body_count(), 1);
        assert!(w.set_motion_type(d1, MotionType::Dynamic, false));
        assert_eq!(w.dynamic_body_count(), 2);

        // Removal leaves a stale entry that compaction (step's retain)
        // drops, never a wrong snapshot.
        assert!(w.remove_body(d2));
        assert_eq!(
            w.dynamic_body_count(),
            1,
            "the count filters the removed body's handle via get() == None"
        );
        w.step(PHYSICS_DT);
        assert_eq!(
            w.dynamic_body_count(),
            1,
            "compaction drops the removed handle; d1 remains indexed"
        );
    }

    /// #4685 (PHYS-D6-2026-09-21-02) — with the query pipeline advanced
    /// incrementally inside `pipeline.step` (no post-loop full rebuild on
    /// stepped frames), a post-step ray must still find a dynamic body at
    /// its MOVED pose, not where a stale tree would leave it. A stale QP
    /// fails the discriminating assertion: the ball drops ~34 BU from its
    /// spawn, so a ray through its XZ would first hit nothing near the top
    /// (spawn-pose tree has no collider up there) or the FLOOR at ~1 BU
    /// depth instead of the ball's crown.
    #[test]
    fn post_step_ray_finds_a_moved_dynamic_body_with_the_incremental_query_path() {
        let mut w = PhysicsWorld::new();
        let fh = w
            .bodies
            .insert(RigidBodyBuilder::fixed().translation(Vector::new(0.0, -1.0, 0.0)).build());
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(500.0, 1.0, 500.0).build(),
            fh,
            &mut w.bodies,
        );
        let bh = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 40.0, 0.0)).build());
        w.colliders.insert_with_parent(
            ColliderBuilder::ball(2.0).build(),
            bh,
            &mut w.bodies,
        );
        w.update_query_pipeline();

        w.wake();
        for _ in 0..30 {
            assert_eq!(w.step(PHYSICS_DT), 1);
        }
        let y = w.bodies.get(bh).unwrap().translation().y;
        assert!(y < 10.0, "the ball must have fallen well below spawn: {y}");

        // Straight down through the ball's XZ from high above: the first
        // hit must be the BALL (crown at y + 2), not the floor at 0 —
        // proving the incremental updates tracked the moved body.
        let hit = w
            .cast_ray(
                byroredux_core::math::Vec3::new(0.0, 100.0, 0.0),
                byroredux_core::math::Vec3::new(0.0, -1.0, 0.0),
                200.0,
                None,
            )
            .expect("the falling ball or the floor must be hit");
        let hit_y = 100.0 - hit.distance;
        assert!(
            (hit_y - (y + 2.0)).abs() < 3.0,
            "the ray must first meet the ball near its current pose (ball y \
             {y}, hit at {hit_y}) — a stale query tree would return the spawn \
             pose (~42) or the floor (0)"
        );
    }

    #[test]
    fn invalid_dynamic_body_reverts_to_its_last_valid_substep_pose() {
        let mut bodies = RigidBodySet::new();
        let handle = bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(12.0, 34.0, 56.0))
                .linvel(Vector::new(7.0, 8.0, 9.0))
                .angvel(Vector::new(1.0, 2.0, 3.0))
                .build(),
        );
        let body = &bodies[handle];
        let snapshot = DynamicBodySnapshot {
            handle,
            position: *body.position(),
        };

        bodies
            .get_mut(handle)
            .unwrap()
            .set_linvel(Vector::new(f32::NAN, 0.0, 0.0), false);
        assert!(!body_state_is_finite(&bodies[handle]));

        assert_eq!(
            restore_invalid_dynamic_bodies(&mut bodies, &mut MultibodyJointSet::new(), [snapshot], &[], &std::collections::HashMap::new())
                .0,
            1
        );
        let body = &bodies[handle];
        assert!(body_state_is_finite(body));
        assert_eq!(body.translation(), Vector::new(12.0, 34.0, 56.0));
        assert_eq!(body.linvel(), Vector::ZERO);
        assert_eq!(body.angvel(), Vector::ZERO);
        assert!(body.is_sleeping());
    }

    #[test]
    fn finite_but_impossible_dynamic_jump_reverts_to_its_last_valid_pose() {
        let mut bodies = RigidBodySet::new();
        let handle = bodies.insert(RigidBodyBuilder::dynamic().build());
        let snapshot = DynamicBodySnapshot {
            handle,
            position: *bodies[handle].position(),
        };
        bodies.get_mut(handle).unwrap().set_translation(
            Vector::new(MAX_DYNAMIC_SUBSTEP_DISPLACEMENT + 1.0, 0.0, 0.0),
            false,
        );

        assert_eq!(
            restore_invalid_dynamic_bodies(&mut bodies, &mut MultibodyJointSet::new(), [snapshot], &[], &std::collections::HashMap::new())
                .0,
            1
        );
        assert_eq!(bodies[handle].translation(), Vector::ZERO);
        assert!(bodies[handle].is_sleeping());
    }

    /// #5161 — the keyframed-bone counterpart of the recovery above. A
    /// live actor bone's ECS pose is animation-authored; when the animation
    /// emits an insane-but-finite transform, `push_kinematic` must refuse
    /// the target instead of letting Rapier derive an insane kinematic
    /// velocity from it (whose predictive AABB trips the multi-SAP grid
    /// bound — a panic the Dynamic-only substep recovery cannot intercept).
    #[test]
    fn accept_keyframe_target_refuses_insane_but_finite_poses() {
        let mut w = PhysicsWorld::new();
        let handle = w.bodies.insert(RigidBodyBuilder::kinematic_position_based().build());

        // A sane worldspace pose is accepted unchanged.
        assert!(w.accept_keyframe_target(
            handle,
            &pose_from_trs(Vec3::new(-67_763.0, 8_386.0, -3_567.0), Quat::IDENTITY)
        ));
        assert_eq!(w.keyframe_targets_refused_total(), 0);

        // The #5161 class exactly: finite, but far outside any authored
        // worldspace. This is the value class the live Skyrim P2 fight fed
        // `set_next_kinematic_position` before the broad-phase panic.
        assert!(!w.accept_keyframe_target(
            handle,
            &pose_from_trs(
                Vec3::new(268_435_460_000.0, 0.0, 0.0),
                Quat::IDENTITY
            )
        ));
        // Non-finite components in either part are refused too.
        assert!(!w.accept_keyframe_target(
            handle,
            &pose_from_trs(Vec3::ZERO, Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0))
        ));
        // Every refusal counts; the once-per-body log dedup does not.
        assert_eq!(w.keyframe_targets_refused_total(), 2);

        // Refusal leaves the caller free to skip the push; a later sane
        // target for the same body is accepted (the animation recovered).
        assert!(w.accept_keyframe_target(
            handle,
            &pose_from_trs(Vec3::new(0.0, 3_456.0, 884.0), Quat::IDENTITY)
        ));
    }

    /// #4687(a) — a dynamic body that is already non-finite when a substep
    /// starts is zeroed and put to sleep at the top of `step`, instead of
    /// staying in the active set forever (its NaN also fails every
    /// kill-plane comparison, so nothing else would ever park it).
    #[test]
    fn pre_broken_dynamic_body_is_parked_not_pinned_awake() {
        let mut w = PhysicsWorld::new();
        let h = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(Vector::new(5.0, 6.0, 7.0)).build());
        w.dynamic_bodies.push(h);
        w.bodies
            .get_mut(h)
            .unwrap()
            .set_linvel(Vector::new(f32::NAN, 0.0, 0.0), true);
        w.wake();
        assert!(w.step(PHYSICS_DT) >= 1);
        let body = w.bodies.get(h).unwrap();
        assert!(
            body.linvel().is_finite(),
            "the NaN velocity must be zeroed: {:?}",
            body.linvel()
        );
        assert!(
            body.is_sleeping(),
            "the pre-broken body must be asleep — pre-#4687 it stayed in the \
             active set forever with the fast path permanently off"
        );
        // #4683 — the parking is counted, so a gate can assert on it.
        assert_eq!(w.recovery_counts().2, 1);
    }

    /// Lift rapier's in-step speed cap ([`IN_STEP_LINEAR_SPEED_CAP_BU_PER_S`])
    /// so a velocity-driven "explosion" on a plain rigid body can jump past
    /// the restore's displacement bound in one tick. Under rapier 0.36 that
    /// cap stops a rigid body first; what still reaches the restore uncapped
    /// is multibody link motion (forward kinematics integrates outside the
    /// solver's per-body cap). These fixtures stand in for that motion with
    /// the simplest body that can carry it.
    fn uncap_rigid_body_speed(w: &mut PhysicsWorld) {
        w.integration_parameters.normalized_max_linear_velocity = Real::MAX;
    }

    /// #4683 — the recovery counter increments through a REAL substep
    /// explosion (a 1e9 BU/s solve jumps the body past the displacement
    /// bound in one tick), the per-frame body count resets on the next
    /// step, and the total persists.
    #[test]
    fn recovery_counter_counts_a_real_substep_explosion() {
        let mut w = PhysicsWorld::new();
        uncap_rigid_body_speed(&mut w);
        let h = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 10.0, 0.0)).build());
        w.dynamic_bodies.push(h);
        w.bodies
            .get_mut(h)
            .unwrap()
            .set_linvel(Vector::new(1.0e9, 0.0, 0.0), true);
        w.wake();
        assert_eq!(w.recovery_counts(), (0, 0, 0));

        assert!(w.step(PHYSICS_DT) >= 1);
        assert_eq!(
            w.recovery_counts(),
            (1, 1, 0),
            "the exploding substep must count exactly one recovery"
        );
        let body = w.bodies.get(h).unwrap();
        assert!(body.is_sleeping() && body.translation().x.is_finite());

        // A later step reports no recovery of its own; the total persists.
        w.wake();
        w.step(PHYSICS_DT);
        assert_eq!(w.recovery_counts(), (1, 0, 0));
    }

    /// #5127 — the two recovery counters have different units: the total
    /// counts recovery EVENTS, the per-frame figure counts BODIES. Two
    /// bodies exploding in the same substep are one event that restores
    /// two bodies.
    #[test]
    fn multi_body_recovery_counts_one_event_and_every_restored_body() {
        let mut w = PhysicsWorld::new();
        uncap_rigid_body_speed(&mut w);
        for z in [0.0, 50.0] {
            let h = w
                .bodies
                .insert(RigidBodyBuilder::dynamic().translation(Vector::new(0.0, 10.0, z)).build());
            w.dynamic_bodies.push(h);
            w.bodies
                .get_mut(h)
                .unwrap()
                .set_linvel(Vector::new(1.0e9, 0.0, 0.0), true);
        }
        w.wake();

        assert!(w.step(PHYSICS_DT) >= 1);
        assert_eq!(
            w.recovery_counts(),
            (1, 2, 0),
            "one recovery event, two bodies restored"
        );
    }

    /// #5488 — rapier 0.36's in-step speed cap is the guard that runs INSIDE
    /// `pipeline.step`, where the explosion is born: a rigid body launched at
    /// 1e9 BU/s moves at most `IN_STEP_LINEAR_SPEED_CAP_BU_PER_S × dt` in the
    /// tick, so there is nothing for the restore to roll back, and the
    /// end-of-substep ladder still sees — and counts — the burst, because
    /// the in-step cap sits above `VELOCITY_SANITY_CAP_BU_PER_S`.
    #[test]
    fn in_step_speed_cap_bounds_an_explosion_inside_one_step() {
        let mut w = PhysicsWorld::new();
        let start = Vector::new(0.0, 10.0, 0.0);
        let h = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(start).build());
        w.dynamic_bodies.push(h);
        w.bodies
            .get_mut(h)
            .unwrap()
            .set_linvel(Vector::new(1.0e9, 0.0, 0.0), true);
        w.wake();

        assert_eq!(w.step(PHYSICS_DT), 1);
        let moved = (w.bodies[h].translation() - start).length();
        assert!(
            moved <= IN_STEP_LINEAR_SPEED_CAP_BU_PER_S * PHYSICS_DT * 1.01,
            "rapier's cap must bound the tick's travel: moved {moved} BU"
        );
        assert!(moved < MAX_DYNAMIC_SUBSTEP_DISPLACEMENT);
        assert_eq!(
            w.recovery_counts(),
            (0, 0, 0),
            "a capped burst leaves nothing for the restore"
        );
        assert_eq!(
            w.velocity_clamps_total(),
            1,
            "the escalation ladder must still count the burst"
        );
    }

    /// Rapier 0.35+ quarantines a body whose state goes non-finite during
    /// the step: rolled back to its last valid pose, zeroed and DISABLED —
    /// no collider in the broad phase, never simulated again. The engine
    /// parks instead, so the restore re-enables the body, sleeps it, and
    /// counts it like any other recovery.
    #[test]
    fn a_quarantined_body_is_re_enabled_and_parked() {
        let mut w = PhysicsWorld::new();
        let start = Vector::new(0.0, 10.0, 0.0);
        let h = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(start).build());
        w.colliders
            .insert_with_parent(ColliderBuilder::ball(2.0).build(), h, &mut w.bodies);
        w.dynamic_bodies.push(h);
        // A non-finite force passes every check the engine runs before the
        // step (they inspect pose and velocity) and turns the velocity NaN
        // during integration — where rapier's end-of-step quarantine
        // catches it.
        assert!(w.add_force(h, Vec3::new(f32::NAN, 0.0, 0.0), true));

        assert_eq!(w.step(PHYSICS_DT), 1);
        assert_eq!(
            w.pipeline.quarantine().bodies(),
            &[h],
            "fixture: rapier must have quarantined the body"
        );
        let body = &w.bodies[h];
        assert!(
            body.is_enabled(),
            "a quarantined body must not be left disabled"
        );
        assert!(body.is_sleeping(), "it is parked, not left awake");
        assert!(body_state_is_finite(body));
        assert!((body.translation() - start).length() < 1.0);
        assert_eq!(w.recovery_counts(), (1, 1, 0));
    }

    /// #4687(b) — after a restore, the query pipeline must reflect the
    /// RESTORED pose within the same frame, not the exploded pose the step
    /// itself had just indexed.
    ///
    /// #5126 — driven through a REAL recovery in `step`: the step's
    /// broad-phase update leaves the tree holding the exploded AABB, which is
    /// exactly the state the refresh has to undo. The pre-#5126 guard staged
    /// the "explosion" by hand on rapier 0.22's separate query tree, so its
    /// tree never held the exploded AABB and it stayed green with the
    /// production refresh deleted.
    #[test]
    fn restored_pose_is_visible_to_ray_queries_same_frame() {
        let mut w = PhysicsWorld::new();
        uncap_rigid_body_speed(&mut w);
        // Floor whose top face is y = 0, under the ball.
        w.colliders.insert(
            ColliderBuilder::cuboid(500.0, 1.0, 500.0)
                .translation(Vector::new(100.0, -1.0, 0.0))
                .build(),
        );
        let h = w
            .bodies
            .insert(RigidBodyBuilder::dynamic().translation(Vector::new(100.0, 50.0, 0.0)).build());
        w.colliders.insert_with_parent(
            ColliderBuilder::ball(2.0).build(),
            h,
            &mut w.bodies,
        );
        w.dynamic_bodies.push(h);
        w.update_query_pipeline();

        // A 3e5 BU/s solve moves the body ~5000 BU in one substep — past
        // the displacement bound, so the recovery restores it to y = 50.
        w.bodies
            .get_mut(h)
            .unwrap()
            .set_linvel(Vector::new(3.0e5, 0.0, 0.0), true);
        w.wake();
        assert!(w.step(PHYSICS_DT) >= 1);
        assert_eq!(w.recovery_counts().0, 1, "the explosion must be recovered");
        assert!(
            (w.bodies.get(h).unwrap().translation() - Vector::new(100.0, 50.0, 0.0)).length() < 1e-3,
            "the body must be back at its snapshot pose"
        );

        let hit = w
            .cast_ray(
                byroredux_core::math::Vec3::new(100.0, 60.0, 0.0),
                byroredux_core::math::Vec3::new(0.0, -1.0, 0.0),
                100.0,
                None,
            )
            .expect("the ray must hit the restored ball or the floor");
        let hit_y = 60.0 - hit.distance;
        assert!(
            (hit_y - 52.0).abs() < 0.5,
            "the ray must meet the ball at its RESTORED crown (~52) in the \
             same frame, got {hit_y} — y ≈ 0 is the floor under a ball whose \
             leaf still holds the exploded AABB (#5126)"
        );
    }

    /// #4687(c) — every simultaneously-invalidated body's articulation is
    /// detached, not just the first invalid body's. Both roots here are
    /// corrupted past the displacement bound; the returned detach list
    /// must contain BOTH of them.
    #[test]
    fn two_simultaneously_invalidated_articulations_are_both_detached() {
        use rapier3d::dynamics::JointAxesMask;

        let mut bodies = RigidBodySet::new();
        let mut multibody_joints = MultibodyJointSet::new();
        let mk = |bodies: &mut RigidBodySet| {
            bodies.insert(RigidBodyBuilder::dynamic().build())
        };
        let (a, b, c, d) = (mk(&mut bodies), mk(&mut bodies), mk(&mut bodies), mk(&mut bodies));
        let fixed = || -> GenericJoint {
            GenericJointBuilder::new(
                JointAxesMask::LIN_X
                    | JointAxesMask::LIN_Y
                    | JointAxesMask::LIN_Z
                    | JointAxesMask::ANG_X
                    | JointAxesMask::ANG_Y
                    | JointAxesMask::ANG_Z,
            )
            .into()
        };
        multibody_joints.insert(a, b, fixed(), true);
        multibody_joints.insert(c, d, fixed(), true);

        // Corrupt one body per articulation past the displacement bound.
        bodies.get_mut(a).unwrap().set_translation(
            Vector::new(MAX_DYNAMIC_SUBSTEP_DISPLACEMENT + 10.0, 0.0, 0.0),
            false,
        );
        bodies.get_mut(c).unwrap().set_translation(
            Vector::new(0.0, MAX_DYNAMIC_SUBSTEP_DISPLACEMENT + 10.0, 0.0),
            false,
        );
        let snapshots = [a, c].map(|handle| DynamicBodySnapshot {
            handle,
            position: Pose::IDENTITY,
        });

        let (restored, detached) =
            restore_invalid_dynamic_bodies(&mut bodies, &mut multibody_joints, snapshots, &[], &std::collections::HashMap::new());
        assert_eq!(restored, 2);
        assert!(
            detached.contains(&a) && detached.contains(&c),
            "both articulations must be scheduled for detach: {detached:?}"
        );
        // And the removals actually detached both articulations: no live
        // multibody survives (the same cascade assertion shape the ragdoll
        // release tests use).
        assert_eq!(
            multibody_joints.multibodies().count(),
            0,
            "both articulations must be detached — pre-#4687 the second one \
             survived and re-emitted its corrupt pose next substep"
        );
    }

    /// A falling dynamic body is awake, so the fast path must NOT skip it —
    /// guards against the gate freezing legitimate motion.
    #[test]
    fn falling_dynamic_keeps_stepping() {
        let mut w = PhysicsWorld::new();
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .pose(pose_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
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
                .pose(pose_from_trs(
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
                .pose(pose_from_trs(Vec3::new(0.0, 1000.0, 0.0), Quat::IDENTITY))
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
        assert!(
            w.add_force(h, up, true),
            "force applies to the sleeping body"
        );
        assert!(w.step(PHYSICS_DT) > 0, "applied force re-engages the sim");
    }

    // ── WATAL Phase 2: external-force API (buoyancy/flow prerequisite) ──

    /// Regression for #2890. The comment justifying the static-scene fast
    /// path quoted "~8-10 ms/step × up to 5 substeps, ~40 ms/frame" in the
    /// present tense and attributed it to `pipeline.step()`. Both halves were
    /// wrong: the figure predates `6e55b492` (the commit that introduced it,
    /// which also removed the per-substep query-pipeline rebuild the number
    /// measured).
    ///
    /// #4685 then moved the remaining cost: on rapier 0.22 the post-loop
    /// full query-QBVH rebuild (9.6 ms on a 95 k world). Since the rapier 0.36
    /// upgrade there is no separate query tree at all — queries borrow the
    /// broad phase's BVH — so the guard now pins that `step` never walks the
    /// whole collider set: a stepped frame drops the queued leaves (the
    /// step's broad-phase update inserted them), a frame that steps nothing
    /// inserts only those. Source-inspection guard, since neither claim is
    /// observable from behaviour.
    #[test]
    fn step_cost_rationale_is_scoped_to_history_and_names_the_real_cost_centre() {
        let src = crate::source_scan::production_text(include_str!("mod.rs"));
        let start = src
            .find("        // Static-scene fast path")
            .expect("the fast path rationale is still here");
        let rationale = &src[start..start + 2600];

        assert!(
            !rationale.contains("~8-10 ms/step × up to 5"),
            "the pre-fix per-step figure must not be restated as current cost \
             (#2890) — it predates the commit that removed the rebuild it \
             measured"
        );
        assert!(
            rationale.contains("6e55b492"),
            "the rationale must attribute its historical numbers to the \
             commit they came from, so the next reader can date them"
        );
        assert!(
            rationale.contains("Self::queries"),
            "the rationale must say where scene queries read from now — the \
             broad phase's BVH — or the next reader goes looking for a query \
             tree to rebuild"
        );
        let step_start = src.find("    pub fn step(&mut self, frame_dt: f32) -> u32 {").expect("step");
        let step_end = src[step_start..].find("\n    }\n").expect("step's end") + step_start;
        let step_body = &src[step_start..step_end];
        assert!(
            step_body.contains("self.pending_query_leaves.clear();")
                && step_body.contains("self.insert_pending_query_leaves();"),
            "a stepped frame must drop the queued leaves and a no-step frame \
             insert only them (#2864, #4685)"
        );
        assert!(
            !step_body.contains("update_query_pipeline"),
            "the whole-set refresh must never run from `step` — it is the \
             explicit cold-start/test entry point, O(all colliders)"
        );
        assert!(
            !rationale.contains("the rebuild accounts for essentially all of it"),
            "that attribution described the pre-#4685 design; leaving it in the \
             present tense would misdirect the next budgeting exercise"
        );
    }

    /// Regression for #3975. `active_island_counts`'s doc and the
    /// static-scene fast path's own rationale (the "deliberately do NOT
    /// gate on kinematic bodies" note) must keep making the
    /// same claim about what the kinematic count means — the accessor's
    /// doc drifted from that rationale once already (it called the count
    /// "awake" for a set Rapier never drains). Source-inspection guard,
    /// since the two are hundreds of lines apart and nothing else keeps
    /// them in sync.
    #[test]
    fn kinematic_count_doc_agrees_with_the_fast_paths_own_rationale() {
        let src = crate::source_scan::production_text(include_str!("mod.rs"));
        let accessor_start = src
            .find("pub fn active_island_counts")
            .expect("the accessor must still exist under this name");
        // Walk back to the start of its doc comment block.
        let doc_start = src[..accessor_start]
            .rfind("/// `(awake dynamic bodies")
            .expect("the accessor's doc block is still here");
        let doc = &src[doc_start..accessor_start];

        assert!(
            doc.contains("not an awake count"),
            "the accessor's own doc must state the kinematic half is not \
             an awake count, not just the fast-path rationale below it"
        );

        let rationale_start = src
            .find("// NOTE: we deliberately do NOT gate on kinematic bodies.")
            .expect("the fast-path rationale is still here");
        let rationale = &src[rationale_start..rationale_start + 500];
        assert!(
            rationale.contains("never empty"),
            "the fast-path rationale must still explain why the kinematic \
             set can't be used as an awake signal"
        );
    }

    /// Regression for #2889. The three force wrappers hard-coded
    /// `wake_up = true` plus `self.wake()`, so the one consumer they were
    /// built for — `water::apply_buoyancy`, whose whole design is a wake
    /// discipline — could not use them and reached past to the Rapier body
    /// instead. A per-frame force that wakes unconditionally re-wakes the
    /// body it just settled, pinning the scene awake forever.
    ///
    /// Pin both directions of the flag: the wake must be opt-in, and opting
    /// out must leave both the body and the fast path undisturbed.
    #[test]
    fn per_frame_forces_can_be_applied_without_arming_the_fast_path() {
        let up = byroredux_core::math::Vec3::new(0.0, 1.0e7, 0.0);

        let mut w = PhysicsWorld::new();
        let h = spawn_ball(&mut w, 0.0);
        // Settle so the scene is genuinely quiesced.
        for _ in 0..8 {
            w.step(PHYSICS_DT);
        }
        while w.step(PHYSICS_DT) > 0 {}
        assert_eq!(
            w.active_island_counts().0,
            0,
            "fixture precondition: settled"
        );
        assert!(!w.pending_wake(), "fixture precondition: nothing pending");

        assert!(w.add_force(h, up, false), "no-wake force still applies");
        assert!(
            !w.pending_wake(),
            "wake_up=false must NOT re-arm the static-scene fast path — that \
             is exactly what made this API unusable from the buoyancy phase"
        );
        assert!(
            w.reset_forces(h, false),
            "the paired per-frame reset takes the same flag"
        );
        assert!(!w.pending_wake(), "a no-wake reset must not arm it either");

        // And the opt-in path still behaves as before.
        assert!(w.add_force(h, up, true), "waking force applies");
        assert!(w.pending_wake(), "wake_up=true re-arms the fast path");
    }

    /// Companion for #2889: the module doc for `water` points readers at
    /// these wrappers as *the* force-application path. Hold that claim by
    /// checking `apply_buoyancy` actually goes through them rather than
    /// mutating `pw.bodies` directly — the state the audit found.
    #[test]
    fn buoyancy_applies_forces_through_the_public_wrappers() {
        let src = include_str!("../water.rs");
        let start = src
            .find("pub(crate) fn apply_buoyancy")
            .expect("the buoyancy phase is still here");
        let body = &src[start..];
        for raw in ["b.add_force(", "b.reset_forces(", "body.add_force("] {
            assert!(
                !body.contains(raw),
                "apply_buoyancy must route `{raw}` through PhysicsWorld's own \
                 force API (`pw.add_force(handle, f, false)`), not the raw \
                 Rapier body — the water module doc names those wrappers as \
                 the force path, and #2889 was that claim being false"
            );
        }
        assert!(
            body.contains("pw.add_force(") && body.contains("pw.reset_forces("),
            "…and it must actually call them"
        );
    }

    /// Helper: spawn a dynamic ball at `y` and return its handle.
    fn spawn_ball(w: &mut PhysicsWorld, y: f32) -> rapier3d::prelude::RigidBodyHandle {
        let shape = single_shape(&CollisionShape::Ball { radius: 10.0 });
        let h = w.bodies.insert(
            RigidBodyBuilder::dynamic()
                .pose(pose_from_trs(Vec3::new(0.0, y, 0.0), Quat::IDENTITY))
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
            w.reset_forces(h, true);
            assert!(
                w.add_force(h, up, true),
                "force applies to a live dynamic body"
            );
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
            w.reset_forces(h, true);
            w.add_force(h, up, true);
            w.step(PHYSICS_DT);
        }
        let y_held = w.bodies[h].translation().y;

        // Clear the force and stop re-applying → must fall.
        w.reset_forces(h, true);
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
        assert!(
            !w.add_force(fh, up, true),
            "static body must reject add_force"
        );
        assert!(
            !w.apply_impulse(fh, up),
            "static body must reject apply_impulse"
        );
        // Dead handle → all no-op.
        let mut w2 = PhysicsWorld::new();
        let dead = w2.bodies.insert(RigidBodyBuilder::dynamic().build());
        w2.remove_body(dead);
        assert!(!w.add_force(dead, up, true), "dead handle must be a no-op");
    }

    /// Test helper: a 100×40×4 BU wall at `pos`, optionally a sensor.
    /// Mirrors what `register_newcomers` builds for a Havok layer-15
    /// (`*_NONCOLLIDABLE`) body since #2549.
    fn insert_wall(w: &mut PhysicsWorld, pos: Vec3, sensor: bool) {
        use rapier3d::prelude::*;
        let body = w.bodies.insert(
            RigidBodyBuilder::fixed()
                .pose(pose_from_trs(pos, Quat::IDENTITY))
                .build(),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(50.0, 20.0, 2.0)
                .sensor(sensor)
                .build(),
            body,
            &mut w.bodies,
        );
    }

    fn walk_into_wall(w: &PhysicsWorld) -> f32 {
        w.move_character(CharacterMoveParams {
            capsule_half_height: 30.0,
            capsule_radius: 15.0,
            position: Vec3::new(0.0, 0.0, 40.0),
            desired_translation: Vec3::new(0.0, 0.0, -60.0),
            dt: 1.0 / 60.0,
            max_slope_climb_deg: 50.0,
            step_height: 32.0,
            step_min_width: 8.0,
            snap_to_ground: 0.0,
            exclude_collider: None,
            filter_groups: None,
            kcc_offset_bu: 2.0,
        })
        .translation
        .z
    }

    /// #3116 — #2549 made every Havok layer-15 body a Rapier **sensor**, on the
    /// grounds that sensors are "already excluded from ray queries elsewhere in
    /// this crate". That was true of exactly one of five query entry points.
    /// Rapier 0.22's `KinematicCharacterController` never sets
    /// `EXCLUDE_SENSORS` for you — it only ORs in `EXCLUDE_DYNAMIC` — so the
    /// KCC still treated the sensor as solid and the player was walled off by
    /// geometry the author marked non-collidable. For the character controller
    /// #2549 was a no-op.
    #[test]
    fn character_walks_through_a_noncollidable_sensor_wall() {
        let mut solid = PhysicsWorld::new();
        insert_wall(&mut solid, Vec3::new(0.0, 0.0, 0.0), false);
        solid.update_query_pipeline();
        let blocked = walk_into_wall(&solid);

        let mut sensor = PhysicsWorld::new();
        insert_wall(&mut sensor, Vec3::new(0.0, 0.0, 0.0), true);
        sensor.update_query_pipeline();
        let through = walk_into_wall(&sensor);

        // Non-vacuity: the solid wall must actually stop the capsule, otherwise
        // the sensor assertion below proves nothing about sensor filtering.
        assert!(
            blocked > -60.0 * 0.5,
            "the solid wall did not block the capsule (moved {blocked} of -60) \
             — fixture is wrong, so the sensor case proves nothing"
        );
        assert!(
            (through - (-60.0)).abs() < 1.0,
            "capsule moved {through} of a desired -60 through a SENSOR wall — \
             the KCC filter is not excluding sensors, so a non-collidable \
             Havok body is still an invisible wall (#3116)"
        );
    }

    /// M42.10 — a walking NPC's KCC sweep must skip its *own* keyframed
    /// ragdoll bones. Each bone is a separate `KinematicPositionBased` body
    /// carrying `ACTOR_BONE_GROUP` membership, so `exclude_collider` (one
    /// handle) can never cover them — the group mask
    /// (`filter_groups: Some(actor_move_interaction_groups())`) is the
    /// multi-body analogue of the `cast_ray_down` self-hit fix (#2873).
    /// With the mask the bone wall is pass-through (a bone in the path must
    /// not stop the actor's own walk); without it the same wall blocks,
    /// proving the fixture and the mask are both load-bearing.
    #[test]
    fn kcc_filter_groups_mask_actor_bone_colliders() {
        fn insert_actor_bone_wall(w: &mut PhysicsWorld) {
            use rapier3d::prelude::*;
            let body = w.bodies.insert(
                RigidBodyBuilder::kinematic_position_based()
                    .pose(pose_from_trs(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY))
                    .build(),
            );
            w.colliders.insert_with_parent(
                ColliderBuilder::cuboid(50.0, 20.0, 2.0)
                    .collision_groups(
                        InteractionGroups::new(crate::ACTOR_BONE_GROUP, Group::ALL, InteractionTestMode::And),
                    )
                    .build(),
                body,
                &mut w.bodies,
            );
        }

        fn walk(w: &PhysicsWorld, filter_groups: Option<rapier3d::prelude::InteractionGroups>) -> f32 {
            w.move_character(CharacterMoveParams {
                capsule_half_height: 30.0,
                capsule_radius: 15.0,
                position: Vec3::new(0.0, 0.0, 40.0),
                desired_translation: Vec3::new(0.0, 0.0, -60.0),
                dt: 1.0 / 60.0,
                max_slope_climb_deg: 50.0,
                step_height: 32.0,
                step_min_width: 8.0,
                snap_to_ground: 0.0,
                exclude_collider: None,
                filter_groups,
                kcc_offset_bu: 2.0,
            })
            .translation
            .z
        }

        let mut masked = PhysicsWorld::new();
        insert_actor_bone_wall(&mut masked);
        masked.update_query_pipeline();
        let through = walk(&masked, Some(actor_move_interaction_groups()));

        let mut unmasked = PhysicsWorld::new();
        insert_actor_bone_wall(&mut unmasked);
        unmasked.update_query_pipeline();
        let blocked = walk(&unmasked, None);

        // Non-vacuity: without the mask the bone wall must actually stop
        // the capsule, otherwise the masked assertion proves nothing.
        assert!(
            blocked > -60.0 * 0.5,
            "the actor-bone wall did not block an unmasked sweep (moved \
             {blocked} of -60) — fixture is wrong"
        );
        assert!(
            (through - (-60.0)).abs() < 1.0,
            "capsule moved {through} of a desired -60 through its own \
             ACTOR_BONE_GROUP bone with the group mask applied — an NPC \
             would be stopped by its own ragdoll bones (M42.10)"
        );
    }

    /// #3116 — the spawn ground probes shared the same blind spot. Grounding
    /// the player on a non-solid marker is worse than finding no floor: the
    /// player is placed, then falls through it on the first step, which looks
    /// exactly like the door-threshold spawn gap and would be misattributed
    /// to it.
    #[test]
    fn ground_probes_do_not_accept_a_sensor_as_a_floor() {
        let mut w = PhysicsWorld::new();
        // A sensor slab where a floor would be.
        let body = w.bodies.insert(
            rapier3d::prelude::RigidBodyBuilder::fixed()
                .pose(pose_from_trs(Vec3::ZERO, Quat::IDENTITY))
                .build(),
        );
        w.colliders.insert_with_parent(
            rapier3d::prelude::ColliderBuilder::cuboid(50.0, 1.0, 50.0)
                .sensor(true)
                .build(),
            body,
            &mut w.bodies,
        );
        w.update_query_pipeline();

        assert!(
            w.cast_ray_down(Vec3::new(0.0, 100.0, 0.0), 200.0, None)
                .is_none(),
            "cast_ray_down grounded the player on a sensor (#3116)"
        );
        assert!(
            w.cast_capsule_down_onto_walkable_surface(
                Vec3::new(0.0, 100.0, 0.0),
                10.0,
                5.0,
                200.0,
                50.0_f32.to_radians().cos(),
                None,
            )
            .is_none(),
            "the walkable-surface probe accepted a sensor as a floor (#3116)"
        );

        // Non-vacuity: the same slab as a solid collider MUST be found, so a
        // filter that rejected everything would fail here.
        let mut solid = PhysicsWorld::new();
        insert_slab(&mut solid, Vec3::ZERO, false);
        solid.update_query_pipeline();
        assert!(
            solid
                .cast_ray_down(Vec3::new(0.0, 100.0, 0.0), 200.0, None)
                .is_some(),
            "the solid control slab was not found — the probe filter is too strict"
        );
    }

    /// #3116 — a sensor sitting where the floor should be is not a floor, so it
    /// must not count toward "the collision world is populated". Mirrors the
    /// discrimination `NearbyCollider::is_sensor` already carries (#2874).
    #[test]
    fn static_collider_census_excludes_sensors() {
        let mut w = PhysicsWorld::new();
        let body = w
            .bodies
            .insert(rapier3d::prelude::RigidBodyBuilder::fixed().build());
        w.colliders.insert_with_parent(
            rapier3d::prelude::ColliderBuilder::cuboid(50.0, 1.0, 50.0)
                .sensor(true)
                .build(),
            body,
            &mut w.bodies,
        );
        assert!(
            w.static_colliders_aabb().is_none(),
            "a sensor-only world reported a populated static collision census (#3116)"
        );

        insert_slab(&mut w, Vec3::new(0.0, -50.0, 0.0), false);
        let (_, _, count) = w
            .static_colliders_aabb()
            .expect("the solid slab must be counted");
        assert_eq!(
            count, 1,
            "the sensor was counted alongside the solid slab (#3116)"
        );
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
                .translation(Vector::new(0.0, top_y - half_thickness, 0.0))
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
                .translation(Vector::new(centre.x, centre.y, centre.z))
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

    /// #4414 — walls block sight; actor bones (anyone's) and the excluded
    /// player capsule do not.
    #[test]
    fn line_of_sight_is_blocked_by_walls_but_not_by_actors() {
        let mut w = PhysicsWorld::new();
        let wall = w.bodies.insert(RigidBodyBuilder::fixed().build());
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(4.0, 200.0, 200.0).build(),
            wall,
            &mut w.bodies,
        );
        actor_bone(&mut w, Vec3::new(-100.0, 52.0, 0.0), true);
        let player = player_capsule(&mut w, Vec3::new(-200.0, 64.0, 0.0));
        w.update_query_pipeline();

        let eye = Vec3::new(-300.0, 52.0, 0.0);
        assert!(w.line_of_sight_blocked(eye, Vec3::new(300.0, 52.0, 0.0), None));
        // Through a bystander's bone to the player, whose capsule the ray
        // ends inside: clear once the player body is excluded.
        let player_eye = Vec3::new(-200.0, 64.0, 0.0);
        assert!(w.line_of_sight_blocked(eye, player_eye, None));
        assert!(!w.line_of_sight_blocked(eye, player_eye, Some(player)));
        assert!(!w.line_of_sight_blocked(eye, eye, None));
    }

    // ── #2873 — ground probes must not see an actor's own bones ──────

    /// Register a keyframed ragdoll-bone body the way `physics_sync_system`
    /// does for a live actor: `KinematicPositionBased`, real collider,
    /// membership in [`ACTOR_BONE_GROUP`].
    fn actor_bone(w: &mut PhysicsWorld, centre: Vec3, tagged: bool) -> RigidBodyHandle {
        use rapier3d::prelude::{Group, InteractionGroups};
        let body = w.bodies.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(Vector::new(centre.x, centre.y, centre.z))
                .build(),
        );
        let groups = if tagged {
            InteractionGroups::new(ACTOR_BONE_GROUP, Group::ALL, InteractionTestMode::And)
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
                .translation(Vector::new(0.0, 0.0, 0.0))
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
                            filter_groups: None,
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

/// #3969 / PHYS-D2-2026-09-06-02 — the wake-discipline contract.
///
/// `wake`'s docstring is the subsystem contract, and it used to name
/// "spawning a body" as one of the three mutations that must call it. Two of
/// the three were true; the third was false for the path that spawns
/// essentially every body in the engine. `sync::register_newcomers` calls
/// only `queue_query_refresh()`, and builds its dynamic bodies
/// `sleeping(true)` on purpose (the EXTERIOR-FREEZE FIX — a measured
/// `atw_scheduler=3005ms` on a Skyrim exterior streaming frame with ~3000
/// awake dynamics). The crate contained both the false claim and its own
/// refutation: `water.rs`'s quiesced-scene fast path grew a `had_newcomers`
/// parameter *because* spawn does not arm `pending_wake`.
///
/// The hazard was asymmetric and silent in both directions — a new
/// body-creating path written on the strength of "spawn arms it" inherits a
/// body that never moves and errors nowhere, while a maintainer reconciling
/// comment with code the wrong way reintroduces the multi-second stall and
/// makes `had_newcomers` look redundant. Neither is observable from
/// behaviour, so these are source-inspection pins, matching the convention in
/// `sync.rs`'s `tick_documentation_tests`.
#[cfg(test)]
mod wake_contract_tests {
    const WORLD_RS: &str = include_str!("mod.rs");
    const SYNC_RS: &str = include_str!("../sync.rs");
    const WATER_RS: &str = include_str!("../water.rs");

    /// `wake`'s doc, from the start of its doc block to the `pub fn wake`.
    fn wake_doc() -> &'static str {
        let end = WORLD_RS
            .find("    pub fn wake(&mut self) {")
            .expect("PhysicsWorld::wake must still exist");
        let start = WORLD_RS[..end]
            .rfind("    /// Mark the simulation as needing")
            .expect("wake's doc block must still open with its summary line");
        &WORLD_RS[start..end]
    }

    #[test]
    fn the_spawn_path_still_does_not_arm_pending_wake() {
        let start = SYNC_RS
            .find("fn register_newcomers(world: &World, newcomers: Vec<Newcomer>) {")
            .expect("register_newcomers must still exist");
        let end = SYNC_RS[start..]
            .find("\nfn push_kinematic(")
            .expect("register_newcomers' following sibling must still exist")
            + start;
        let body = &SYNC_RS[start..end];

        assert!(
            body.contains("body_builder.sleeping(true)"),
            "fixture precondition: dynamic newcomers still spawn asleep (the \
             EXTERIOR-FREEZE FIX) — if that changed, the whole wake exemption \
             needs re-deciding, not just re-documenting (#3969)"
        );
        assert!(
            body.contains("pw.queue_query_refresh(inserted);"),
            "fixture precondition: the spawn path still announces itself with \
             queue_query_refresh alone (#3969)"
        );
        assert!(
            !body.contains(".wake()"),
            "register_newcomers must NOT arm `pending_wake`: its dynamics spawn \
             asleep by design, and waking every streaming frame reintroduces the \
             measured atw_scheduler=3005ms exterior stall. Consumers needing \
             first-frame visibility of a newcomer take it as an explicit argument \
             instead — see water.rs's `had_newcomers` (#3969)"
        );
    }

    #[test]
    fn wake_doc_records_the_spawn_exemption_instead_of_claiming_spawn_calls_it() {
        let doc = wake_doc();
        assert!(
            !doc.contains("introduce motion — spawning a body"),
            "wake's doc must not list spawning among its required callers — the \
             production spawn path deliberately does not call it (#3969)"
        );
        assert!(
            doc.contains("deliberate exemption"),
            "wake's doc must state that spawn is an exemption by design, or a \
             maintainer reconciling doc with code 'restores' the wake and \
             reintroduces the exterior streaming stall (#3969)"
        );
        assert!(
            doc.contains("had_newcomers"),
            "wake's doc must point at the consumer-side contract that replaces \
             the missing wake, or the exemption reads as a bug (#3969)"
        );
    }

    /// The accessor doc matters more than the setter's: it is what the WATAL
    /// buoyancy phase reads, and it claimed `pending_wake` covered spawns
    /// while sitting one call away from the workaround for it not doing so.
    #[test]
    fn pending_wake_accessor_doc_does_not_claim_to_cover_spawns() {
        let end = WORLD_RS
            .find("    pub fn pending_wake(&self) -> bool {")
            .expect("the pending_wake accessor must still exist");
        let start = WORLD_RS[..end]
            .rfind("    /// Whether a pipeline step is already pending")
            .expect("the accessor's doc block must still open with its summary line");
        let doc = &WORLD_RS[start..end];

        assert!(
            !doc.contains("woken / spawned / re-targeted"),
            "the pending_wake accessor doc must not claim a spawn arms this flag \
             — it is read by apply_buoyancy, which needs `had_newcomers` \
             precisely because a spawn does not (#3969)"
        );
        assert!(
            doc.contains("had_newcomers"),
            "the accessor doc must name the companion signal a consumer needs to \
             see newcomers, since this flag alone does not report them (#3969)"
        );
    }

    /// The refutation the doc now cross-references must still be there: if the
    /// `had_newcomers` term is ever removed, the exemption stops being safe and
    /// the docs above become wrong in the other direction.
    #[test]
    fn the_buoyancy_fast_path_still_carries_the_newcomer_term() {
        let start = WATER_RS
            .find("pub(crate) fn apply_buoyancy")
            .expect("the buoyancy phase must still exist");
        let body = &WATER_RS[start..];
        assert!(
            body.contains("!had_newcomers"),
            "apply_buoyancy's quiesced-scene fast path must keep consulting \
             `had_newcomers`: a body that streams in already submerged spawns \
             ASLEEP and does not arm `pending_wake`, so without this term its \
             first-frame dry→wet float-up is skipped (#3969)"
        );
    }
}

#[cfg(test)]
mod broad_phase_tests;
