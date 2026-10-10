//! `PhysicsWorld` explosion recovery and containment (#5161 / #5246).
//!
//! Split from `world.rs` (TD1-2026-10-05-01 / #5311): the snapshot /
//! restore / clamp cluster plus its counters live here so the lifecycle
//! and `step` file stays under the 2 000-production-LOC gate. The
//! sanity-cap constants this cluster exists for moved with it; the two
//! `pub`/`pub(crate)` caps stay in [`super`] so their documented paths
//! are unchanged.

use rapier3d::prelude::*;

use super::{PhysicsWorld, ARTICULATION_DOF_SANITY_CAP, VELOCITY_SANITY_CAP_BU_PER_S};

/// Largest plausible dynamic-body movement in one fixed 60 Hz substep.
///
/// This is a *delta*, never an absolute coordinate: exterior worlds may be
/// far from the origin, while an object moving 2,048 BU (about 29 m) in
/// 1/60th second is already far beyond ordinary character, ragdoll, debris,
/// or projectile motion. It is a backstop for finite solver explosions — a
/// NaN check alone cannot catch a body launched billions of BU by one bad
/// contact.
pub(super) const MAX_DYNAMIC_SUBSTEP_DISPLACEMENT: f32 = 2_048.0;

/// #5161 — sanity cap on angular speed (≈16 rev/s); explosions reach 1e10+.
const ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S: f32 = 100.0;

/// DOF count of a dynamic multibody root: rapier's free joint, linear
/// DOFs first then angular (#5353).
const FREE_ROOT_DOFS: usize = 6;

/// #5161 — sanity bound for a keyframed body target pushed from an ECS
/// GlobalTransform. Authored worldspace coordinates top out around ±3e5 BU,
/// so a translation beyond 1e8 is corruption with certainty. Accepting it
/// would park a live actor's bone collider somewhere no melee sweep, ray or
/// contact can reach, and its derived velocity (`(target − current)/dt`)
/// would fling whatever it touched on the way. Rapier's own quarantine only
/// catches NON-finite poses, the substep recovery only snapshots `Dynamic`
/// bodies, and live actor skeleton bones are keyframed
/// (`keyframe_live_ragdoll_bones`) — so this boundary check in
/// `accept_keyframe_target` is the only guard a finite-but-insane target
/// meets. (Under rapier 0.22 it also kept that velocity's predictive AABB
/// off the multi-SAP grid boundary, a broad-phase panic the BVH broad phase
/// no longer has.)
const KEYFRAME_TARGET_SANE_BOUND_BU: f32 = 1.0e8;

#[derive(Clone)]
pub(super) struct DynamicBodySnapshot {
    pub(super) handle: RigidBodyHandle,
    pub(super) position: Pose,
}

pub(super) fn body_state_is_finite(body: &RigidBody) -> bool {
    body.translation().is_finite()
        && body.rotation().is_finite()
        && body.linvel().is_finite()
        && body.angvel().is_finite()
}

fn body_needs_recovery(body: &RigidBody, snapshot: &DynamicBodySnapshot) -> bool {
    !body_state_is_finite(body)
        || (body.translation() - snapshot.position.translation).length()
            > MAX_DYNAMIC_SUBSTEP_DISPLACEMENT
}

/// Roll back every snapshotted dynamic body the substep invalidated, and
/// take over the bodies rapier quarantined.
///
/// `quarantined` is `PhysicsPipeline::quarantine().bodies()` for the same
/// step: bodies whose state went non-finite, which rapier (0.35+) rolled back
/// to its own last valid pose, zeroed and DISABLED. A disabled body has no
/// collider in the broad phase and is never simulated again unless someone
/// re-enables it — the wrong terminal state for a corpse limb or a crate.
/// A dynamic one is parked here exactly like a body the displacement check
/// caught (rolled back to the engine's snapshot when there is one, slept,
/// articulation detached); a kinematic one keeps rapier's rolled-back pose,
/// which is the last-accepted-target behaviour
/// [`PhysicsWorld::accept_keyframe_target`] gives a refused pose. Both stay
/// disabled until the caller re-enables them after the next step (the
/// `quarantine_cooldown` field doc says why it must not be sooner).
pub(super) fn restore_invalid_dynamic_bodies(
    bodies: &mut RigidBodySet,
    multibody_joints: &mut MultibodyJointSet,
    snapshots: impl IntoIterator<Item = DynamicBodySnapshot>,
    quarantined: &[RigidBodyHandle],
    body_labels: &std::collections::HashMap<RigidBodyHandle, String>,
) -> (usize, Vec<RigidBodyHandle>) {
    let mut restored = 0;
    let mut detached_articulations: Vec<RigidBodyHandle> = Vec::new();
    // #5161 — the recovery log alone cannot say WHAT went insane. Record the
    // pre-restore state of the first few bodies per event (translation
    // magnitude + velocity magnitude + the body's registered label) so the
    // next investigation reads the explosion's class — and WHICH actor's
    // articulation produced it — straight off the log instead of
    // re-instrumenting.
    let mut evidence = Vec::new();
    let mut snapshotted: Vec<RigidBodyHandle> = Vec::new();
    for snapshot in snapshots {
        snapshotted.push(snapshot.handle);
        let Some(body) = bodies.get(snapshot.handle) else {
            continue;
        };
        let was_quarantined = quarantined.contains(&snapshot.handle);
        if !was_quarantined && !body_needs_recovery(body, &snapshot) {
            continue;
        }
        if evidence.len() < 3 {
            let label = body_labels
                .get(&snapshot.handle)
                .map(String::as_str)
                .unwrap_or("unlabelled");
            // A quarantined body already reads rapier's rolled-back, zeroed
            // state here; the tag says why its numbers look sane.
            let tag = if was_quarantined { " quarantined" } else { "" };
            evidence.push(format!(
                "{:?} [{label}]{tag} at |t|={:.3e} |v|={:.3e}",
                snapshot.handle,
                body.translation().length(),
                body.linvel().length(),
            ));
        }
        // Rapier's get_mut marks a body modified even when the caller only
        // reads it. Keep healthy snapshots out of the next step's dirty list.
        let body = bodies
            .get_mut(snapshot.handle)
            .expect("recovery body was just read under exclusive set access");
        body.set_position(snapshot.position, false);
        // Multibody links must remain dynamic in Rapier. Sleep the damaged
        // island rather than changing its motion type, which would turn a
        // recoverable solver error into a structural multibody panic.
        body.sleep();
        // #4687(c) — collect EVERY invalid handle instead of only the
        // first. `remove_multibody_articulations` detaches the single
        // articulation containing its argument, so the old
        // `get_or_insert(first)` left every OTHER simultaneously-invalidated
        // articulation intact: its un-invalidated links stayed awake and
        // re-emitted the corrupt pose next substep (a second error log and
        // one more forfeited backlog per extra articulation). A repeat
        // handle is a no-op — removal is per-articulation and idempotent.
        detached_articulations.push(snapshot.handle);
        restored += 1;
    }
    // Quarantined dynamics the snapshot did not cover (already non-finite at
    // substep entry, or never indexed): rapier's rollback pose is the only
    // valid one left, so park them there.
    for &handle in quarantined {
        if snapshotted.contains(&handle) {
            continue;
        }
        let Some(body) = bodies.get_mut(handle) else {
            continue;
        };
        if !body.is_dynamic() {
            continue;
        }
        body.sleep();
        if evidence.len() < 3 {
            let label = body_labels
                .get(&handle)
                .map(String::as_str)
                .unwrap_or("unlabelled");
            evidence.push(format!("{handle:?} [{label}] quarantined, no snapshot"));
        }
        detached_articulations.push(handle);
        restored += 1;
    }
    for handle in &detached_articulations {
        // Detach broken articulations through Rapier's supported API. This
        // keeps the restored bodies as sleeping dynamics instead of letting
        // the next contact solve re-enter the known-bad constraint graph.
        multibody_joints.remove_multibody_articulations(*handle, false);
    }
    if !evidence.is_empty() {
        log::error!(
            "physics: invalid-solve evidence (first of {}): {}",
            restored,
            evidence.join(", ")
        );
    }
    (restored, detached_articulations)
}

impl PhysicsWorld {
    /// #4683 (PHYS-D3-2026-09-21-01) — solver-explosion recovery counts:
    /// `(lifetime recovery EVENTS, BODIES restored in the most recent
    /// `step` call, lifetime pre-broken bodies parked at step entry)`.
    /// #5127 — the first two are different units: an event restores one
    /// or more bodies, so `.1` may exceed `.0`. The recovery's only
    /// pre-#4683 signal was one `log::error!`; every ragdoll stability
    /// gate read post-recovery state and could not see it happen.
    pub fn recovery_counts(&self) -> (u64, u32, u64) {
        (
            self.recoveries_total,
            self.bodies_restored_last_frame,
            self.bodies_parked_total,
        )
    }

    /// #4687(b) (PHYS-D2-2026-09-21-02) — `set_position` defers collider
    /// sync to the next pipeline step, so straight after a restore the
    /// broad phase's BVH — the scene-query index, just updated by the step
    /// above — holds the restored bodies' colliders at their EXPLODED pose:
    /// one frame of ray/shape queries against geometry that was already
    /// rolled back. Propagate the restored poses into the colliders and
    /// re-insert exactly those leaves.
    ///
    /// #5126 found the rapier 0.22 form of this (`update_incremental` with
    /// `refit = false` only marked leaves dirty, so the restored body was
    /// invisible to queries until the next step). `BroadPhaseBvh::set_aabb`
    /// applies the leaf immediately; the guard
    /// `restored_pose_is_visible_to_ray_queries_same_frame` drives a real
    /// recovery through `step` and asserts it.
    pub(super) fn refresh_query_geometry_after_restore(&mut self, invalid_handles: &[RigidBodyHandle]) {
        self.bodies
            .propagate_modified_body_positions_to_colliders(&mut self.colliders);
        let touched: Vec<ColliderHandle> = invalid_handles
            .iter()
            .filter_map(|&h| self.bodies.get(h))
            .flat_map(|body| body.colliders().iter().copied())
            .collect();
        for c in touched {
            self.set_query_leaf(c);
        }
    }

    /// #4687(a) (PHYS-D2-2026-09-21-02) — put dynamics that are ALREADY
    /// non-finite before any substep to sleep. The per-substep recovery
    /// snapshot filters such a body out (it has no valid prior pose to
    /// roll back to), and its NaN coordinates also fail every comparison
    /// against the kill plane — so pre-#4687 it stayed in the active set
    /// forever, kept the static-scene fast path permanently off, and was
    /// recoverable by nothing. Zeroing the velocities and sleeping it
    /// parks the corruption in place (the same terminal state the
    /// restore path produces) instead of paying for it every frame.
    /// Called at the top of `step`, before the fast-path gate, from the
    /// cheap dynamic index (#4682).
    pub(super) fn recover_pre_broken_bodies(&mut self) {
        let mut parked = 0usize;
        for &handle in &self.dynamic_bodies {
            let Some(body) = self.bodies.get(handle) else {
                continue;
            };
            if body.body_type() != RigidBodyType::Dynamic
                || body_state_is_finite(body)
                || body.is_sleeping()
            {
                continue;
            }
            // The finite-state check is read-only; enqueue a Rapier user
            // change only for the rare body that actually needs parking.
            let body = self
                .bodies
                .get_mut(handle)
                .expect("pre-broken body was just read under exclusive set access");
            body.set_linvel(Vector::ZERO, false);
            body.set_angvel(AngVector::ZERO, false);
            body.sleep();
            parked += 1;
        }
        if parked > 0 {
            log::error!(
                "physics: parked {parked} dynamic body/bodies whose state was \
                 already non-finite before the step (corrupt seed or contact); \
                 they were zeroed and put to sleep — the recovery snapshot has \
                 no prior pose to roll them back to"
            );
            self.bodies_parked_total = self.bodies_parked_total.saturating_add(parked as u64);
        }
    }

    /// #5161 — gate one `push_kinematic` target. Returns `false` (and
    /// records the refusal) when the target is non-finite or its
    /// translation lies beyond [`KEYFRAME_TARGET_SANE_BOUND_BU`]: live
    /// actor bones are keyframed, the per-substep recovery only covers
    /// `Dynamic` bodies, and rapier's quarantine only catches non-finite
    /// state — see [`KEYFRAME_TARGET_SANE_BOUND_BU`] for what a finite but
    /// insane target would do. The body is left at its last accepted pose; the
    /// animation-side source of the broken transform stays visible (and
    /// open) as the rendering-side corruption it already is.
    pub fn accept_keyframe_target(
        &mut self,
        handle: RigidBodyHandle,
        target: &Pose,
    ) -> bool {
        let t = target.translation;
        let sane = t.is_finite()
            && t.abs().max_element() <= KEYFRAME_TARGET_SANE_BOUND_BU
            && target.rotation.is_finite();
        if sane {
            return true;
        }
        self.keyframe_targets_refused_total =
            self.keyframe_targets_refused_total.saturating_add(1);
        if self.keyframe_refusals_logged.insert(handle) {
            log::error!(
                "physics: refused a keyframed target for body {handle:?} at \
                 ({:.1}, {:.1}, {:.1}) — non-finite or beyond the sane world bound \
                 ({} BU); the body keeps its last accepted pose (#5161)",
                t.x,
                t.y,
                t.z,
                KEYFRAME_TARGET_SANE_BOUND_BU as u64
            );
        }
        false
    }

    /// Lifetime refused-keyframe-target count, for `phys.stats` (#5161).
    pub fn keyframe_targets_refused_total(&self) -> u64 {
        self.keyframe_targets_refused_total
    }

    /// #5161 — count one refused ragdoll activation. `build_ragdoll` calls
    /// this for its own absolute rejection; the bin-side activator calls it
    /// for the actor-reach rejection that never reaches `build_ragdoll`.
    /// The paths are mutually exclusive, so the count never doubles.
    pub fn note_ragdoll_seed_refusal(&mut self) {
        self.ragdoll_seed_refusals_total = self.ragdoll_seed_refusals_total.saturating_add(1);
    }

    /// Lifetime refused-ragdoll-seed count, for `phys.stats` (#5161).
    pub fn ragdoll_seed_refusals_total(&self) -> u64 {
        self.ragdoll_seed_refusals_total
    }

    /// #5161 — cap every dynamic body's speed at
    /// [`VELOCITY_SANITY_CAP_BU_PER_S`] (and spin at
    /// [`ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S`]), called at the end of
    /// every `pipeline.step` substep. Rapier's in-step cap
    /// ([`super::IN_STEP_LINEAR_SPEED_CAP_BU_PER_S`]) already bounded how far
    /// the body could travel inside the step; this is the escalation
    /// policy on top of it. The offence count is a LIFETIME ladder
    /// (#5246, never cleared by a clean substep): the first burst clamps,
    /// the second parks (zeroed, slept), the third detaches the body's
    /// whole articulation so a persistently exploding rig cannot churn
    /// clamp → park → wake → explode forever.
    pub(super) fn clamp_explosive_velocities(&mut self) {
        // #5355 — sweep joints whose articulation died since the last
        // substep (ragdoll teardown, the third-offence detach, a
        // `remove_body` cascade). Without this the walk at the bottom pays
        // one `get_mut` miss per joint of every corpse ever built, every
        // substep, for the whole session.
        self.articulation_joints
            .retain(|j| self.multibody_joints.get(*j).is_some());
        let mut clamped: Vec<(RigidBodyHandle, Vector, AngVector)> = Vec::new();
        for &handle in &self.dynamic_bodies {
            let (linvel, angvel) = {
                let Some(body) = self.bodies.get(handle) else {
                    continue;
                };
                (body.linvel(), body.angvel())
            };
            // #5246 — a NaN velocity must never be *passed through*: one
            // integration step later it is a NaN position inside
            // pipeline.step, before any of this code can run again. Rapier
            // 0.35+ quarantines (and disables) such a body there, which the
            // restore then has to undo; zeroing it here keeps the body
            // simulated. Classify non-finite as maximally explosive.
            let lin_finite = linvel.is_finite();
            let ang_finite = angvel.is_finite();
            let speed = if lin_finite {
                linvel.length()
            } else {
                f32::INFINITY
            };
            let spin = if ang_finite {
                angvel.length()
            } else {
                f32::INFINITY
            };
            if speed <= VELOCITY_SANITY_CAP_BU_PER_S
                && spin <= ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S
            {
                continue;
            }
            let capped_linvel = if !lin_finite {
                Vector::ZERO
            } else if speed > VELOCITY_SANITY_CAP_BU_PER_S {
                linvel * (VELOCITY_SANITY_CAP_BU_PER_S / speed)
            } else {
                linvel
            };
            let capped_angvel = if !ang_finite {
                Vector::ZERO
            } else if spin > ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S {
                angvel * (ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S / spin)
            } else {
                angvel
            };
            clamped.push((handle, capped_linvel, capped_angvel));
        }
        for (handle, capped_linvel, capped_angvel) in clamped {
            {
                let body = self
                    .bodies
                    .get_mut(handle)
                    .expect("clamped body was just read under exclusive set access");
                body.set_linvel(capped_linvel, false);
                body.set_angvel(capped_angvel, false);
            }
            self.velocity_clamps_total = self.velocity_clamps_total.saturating_add(1);
            // #5246 — lifetime offence count, NOT a set cleared on clean
            // substeps: the SLscorpionBurrowINT log showed the same body
            // parked four times because every wake-and-re-explosion cycle
            // looked like a fresh first offence. Escalation ladder — first
            // burst clamps, second parks, third detaches the whole
            // articulation (the invalid-solve restore's own tool) so a
            // persistently exploding rig cannot churn forever.
            let offences = self.explosion_offences.entry(handle).or_insert(0);
            *offences += 1;
            let label = self
                .body_labels
                .get(&handle)
                .map(String::as_str)
                .unwrap_or("unlabelled");
            if *offences >= 3 {
                if let Some(body) = self.bodies.get_mut(handle) {
                    body.set_linvel(Vector::ZERO, false);
                    body.set_angvel(Vector::ZERO, false);
                    body.sleep();
                }
                // Detaches every multibody joint containing `handle`; a
                // free body afterwards, so no forward kinematics can
                // re-teleport it. #5356 — count (and log) only a detach
                // that detached something: `rigid_body_link` consults the
                // same rb2mb map the removal walks, so `None` means
                // clutter or a rig an earlier burst already freed, where
                // the removal is a no-op. Otherwise the counter reads
                // "bursts past the third", not "articulations detached".
                if self.multibody_joints.rigid_body_link(handle).is_some() {
                    self.multibody_joints
                        .remove_multibody_articulations(handle, false);
                    self.explosive_detaches_total =
                        self.explosive_detaches_total.saturating_add(1);
                    log::error!(
                        "physics: detached {handle:?} [{label}]'s articulation after \
                         {offences} solver-explosion bursts (#5246)"
                    );
                }
            } else if *offences == 2 {
                // Second burst: the solve is persistently exploding for
                // this body. Park it the same way the invalid-solve
                // restore does — zeroed velocities, asleep at its current
                // (still sane, cap-bounded) pose — instead of letting it
                // vibrate at the cap forever.
                if let Some(body) = self.bodies.get_mut(handle) {
                    body.set_linvel(Vector::ZERO, false);
                    body.set_angvel(Vector::ZERO, false);
                    body.sleep();
                }
                log::error!(
                    "physics: parked {handle:?} [{label}] after repeated solver-explosion \
                     velocities (still sane, slept at current pose) (#5161)"
                );
            } else {
                log::warn!(
                    "physics: clamped explosive velocity on {handle:?} [{label}] to the \
                     sanity cap (#5161)"
                );
            }
        }
        // Articulation DOFs: forward kinematics integrates these BEFORE any
        // body-level clamp can matter, so an exploding reduced-coordinate
        // velocity teleports its links out of the world in one step even
        // with every rigid body capped above (#5161).
        let clamped = self.clamp_articulation_dofs();
        if !clamped.is_empty() {
            self.velocity_clamps_total = self.velocity_clamps_total.saturating_add(1);
            // #5531 — name the rig. Several per-substep floods were
            // unattributable ("clamped 6 ... DOF velocities") because the
            // warning carried no body label, unlike the body-level clamp.
            for (root, dofs) in &clamped {
                let label = self
                    .body_labels
                    .get(root)
                    .map_or("unlabelled", String::as_str);
                log::warn!(
                    "physics: clamped {dofs} exploding articulation DOF velocities on \
                     {root:?} [{label}] to the sanity cap (#5161)"
                );
            }
        }
    }

    /// Cap the generalized velocities of every live articulation and return
    /// `(root link body, clamped DOF count)` for each multibody that needed
    /// it. A non-finite DOF is zeroed and counted.
    ///
    /// `articulation_joints` holds every joint of every rig (~17 for a
    /// humanoid), and they all resolve to the same multibody, so each
    /// multibody is walked once, keyed by its root body (#5531).
    pub(crate) fn clamp_articulation_dofs(&mut self) -> Vec<(RigidBodyHandle, usize)> {
        let mut visited = std::collections::HashSet::new();
        let mut clamped = Vec::new();
        for &joint in &self.articulation_joints {
            let Some((multibody, _)) = self.multibody_joints.get_mut(joint) else {
                continue;
            };
            let root = multibody.root().rigid_body_handle();
            if !visited.insert(root) {
                continue;
            }
            // #5353 — a dynamic root is rapier's 6-DOF free joint at offset
            // 0: three linear DOFs (BU/s) then three angular (rad/s)
            // (`Multibody::update_root_type`, `MultibodyJoint::integrate`).
            // Those are body speeds, not authored joint axes, so they take
            // the body-level caps: the joint-axis cap held every falling
            // ragdoll to 100 BU/s against 686.7 BU/s² of gravity.
            let free_root = multibody.root().joint().ndofs() == FREE_ROOT_DOFS;
            let mut vels = multibody.generalized_velocity_mut();
            let mut dofs = 0usize;
            for i in 0..vels.len() {
                let cap = match (free_root, i) {
                    (true, 0..=2) => VELOCITY_SANITY_CAP_BU_PER_S,
                    (true, 3..=5) => ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S,
                    _ => ARTICULATION_DOF_SANITY_CAP,
                };
                let v = vels[i];
                if !v.is_finite() {
                    vels[i] = 0.0;
                    dofs += 1;
                } else if v.abs() > cap {
                    vels[i] = v.signum() * cap;
                    dofs += 1;
                }
            }
            if dofs > 0 {
                clamped.push((root, dofs));
            }
        }
        clamped
    }

    /// Lifetime velocity-clamp count, for `phys.stats` (#5161).
    pub fn velocity_clamps_total(&self) -> u64 {
        self.velocity_clamps_total
    }

    /// Lifetime third-offence articulation detaches, for `phys.stats`
    /// (#5246).
    pub fn explosive_detaches_total(&self) -> u64 {
        self.explosive_detaches_total
    }
}
