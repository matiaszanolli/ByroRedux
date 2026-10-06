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

/// #5161 — sanity bound for a keyframed body target pushed from an ECS
/// GlobalTransform. Authored worldspace coordinates top out around ±3e5 BU,
/// so a translation beyond 1e8 is corruption with certainty — while still
/// ~2600× below rapier 0.22's multi-SAP grid boundary (≈2.68e11), which an
/// insane kinematic target's derived velocity (`(target − current)/dt`) can
/// trip through the collider's predictive AABB as a broad-phase panic. The
/// substep recovery only snapshots `Dynamic` bodies, and live actor skeleton
/// bones are keyframed (`keyframe_live_ragdoll_bones`) — so this boundary
/// check in `accept_keyframe_target` is the only guard they have.
const KEYFRAME_TARGET_SANE_BOUND_BU: f32 = 1.0e8;

#[derive(Clone)]
pub(super) struct DynamicBodySnapshot {
    pub(super) handle: RigidBodyHandle,
    pub(super) position: Isometry<Real>,
}

pub(super) fn body_state_is_finite(body: &RigidBody) -> bool {
    body.translation().iter().all(|v| v.is_finite())
        && body.rotation().coords.iter().all(|v| v.is_finite())
        && body.linvel().iter().all(|v| v.is_finite())
        && body.angvel().iter().all(|v| v.is_finite())
}

fn body_needs_recovery(body: &RigidBody, snapshot: &DynamicBodySnapshot) -> bool {
    !body_state_is_finite(body)
        || (body.translation() - snapshot.position.translation.vector).norm()
            > MAX_DYNAMIC_SUBSTEP_DISPLACEMENT
}

pub(super) fn restore_invalid_dynamic_bodies(
    bodies: &mut RigidBodySet,
    multibody_joints: &mut MultibodyJointSet,
    snapshots: impl IntoIterator<Item = DynamicBodySnapshot>,
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
    for snapshot in snapshots {
        let Some(body) = bodies.get(snapshot.handle) else {
            continue;
        };
        if !body_needs_recovery(body, &snapshot) {
            continue;
        }
        if evidence.len() < 3 {
            let label = body_labels
                .get(&snapshot.handle)
                .map(String::as_str)
                .unwrap_or("unlabelled");
            evidence.push(format!(
                "{:?} [{label}] at |t|={:.3e} |v|={:.3e}",
                snapshot.handle,
                body.translation().norm(),
                body.linvel().norm(),
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
    /// query pipeline (just advanced incrementally by the step above)
    /// indexes the restored bodies' colliders at their EXPLODED or NaN
    /// pose: one frame of ray/shape queries against geometry that was
    /// already rolled back. Propagate the restored poses into the
    /// colliders and refresh exactly those leaves.
    ///
    /// #5126 — `refit_and_rebalance` MUST be `true`. In rapier 0.22 the
    /// `false` form only marks the leaves dirty (`pre_update_or_insert`);
    /// leaf AABBs are refit only under `true`, which `PhysicsPipeline::step`
    /// passes on its final substep — so the tree we inherit holds the
    /// EXPLODED AABB, and a dirty-but-unrefit leaf left the restored body
    /// invisible to every ray/KCC/LOS query until the next frame's step.
    /// The refit + rebalance runs only on recovery frames.
    pub(super) fn refresh_query_geometry_after_restore(&mut self, invalid_handles: &[RigidBodyHandle]) {
        self.bodies
            .propagate_modified_body_positions_to_colliders(&mut self.colliders);
        let mut touched_colliders: Vec<ColliderHandle> = Vec::new();
        for &h in invalid_handles {
            if let Some(body) = self.bodies.get(h) {
                touched_colliders.extend(body.colliders().iter().copied());
            }
        }
        self.query_pipeline
            .update_incremental(&self.colliders, &touched_colliders, &[], true);
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
            body.set_linvel(Vector::zeros(), false);
            body.set_angvel(Vector::zeros(), false);
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
    /// `Dynamic` bodies, and an insane kinematic target's derived velocity
    /// (`(target − current)/dt`) trips rapier's multi-SAP grid boundary as
    /// a broad-phase panic — the class that killed the live Skyrim P2
    /// fight. The body is left at its last accepted pose; the
    /// animation-side source of the broken transform stays visible (and
    /// open) as the rendering-side corruption it already is.
    pub fn accept_keyframe_target(
        &mut self,
        handle: RigidBodyHandle,
        target: &Isometry<Real>,
    ) -> bool {
        let t = target.translation.vector;
        let q = target.rotation.coords;
        let sane = [t.x, t.y, t.z].into_iter().all(|v| {
            v.is_finite() && v.abs() <= KEYFRAME_TARGET_SANE_BOUND_BU
        }) && q.iter().all(|v| v.is_finite());
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
    /// every `pipeline.step` substep. See the cap constant's doc for why
    /// this is the only guard that runs *before* an explosion's positions
    /// reach the broad phase. A body clamped on consecutive substeps is
    /// parked; a clean substep returns it to watch-list absence.
    pub(super) fn clamp_explosive_velocities(&mut self) {
        let mut clamped: Vec<(RigidBodyHandle, nalgebra::Vector3<f32>, nalgebra::Vector3<f32>)> =
            Vec::new();
        for &handle in &self.dynamic_bodies {
            let (linvel, angvel) = {
                let Some(body) = self.bodies.get(handle) else {
                    continue;
                };
                (*body.linvel(), *body.angvel())
            };
            // #5246 — NaN is the containment hole's fingerprint: rapier's
            // broad-phase clamps a NaN-positioned collider's AABB to the
            // multi-SAP grid corners (na::clamp(NaN, ±max) lands finite),
            // and those corner AABBs pass the finite rejection and poison
            // the layer structure. A NaN velocity therefore must never be
            // *passed through* — one integration step later it is a NaN
            // position inside pipeline.step, before any of this code can
            // run again. Classify non-finite as maximally explosive and
            // zero it outright.
            let lin_finite = linvel.iter().all(|v| v.is_finite());
            let ang_finite = angvel.iter().all(|v| v.is_finite());
            let speed = if lin_finite {
                linvel.norm()
            } else {
                f32::INFINITY
            };
            let spin = if ang_finite {
                angvel.norm()
            } else {
                f32::INFINITY
            };
            if speed <= VELOCITY_SANITY_CAP_BU_PER_S
                && spin <= ANGULAR_VELOCITY_SANITY_CAP_RAD_PER_S
            {
                continue;
            }
            let capped_linvel = if !lin_finite {
                nalgebra::zero()
            } else if speed > VELOCITY_SANITY_CAP_BU_PER_S {
                linvel * (VELOCITY_SANITY_CAP_BU_PER_S / speed)
            } else {
                linvel
            };
            let capped_angvel = if !ang_finite {
                nalgebra::zero()
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
                    body.set_linvel(nalgebra::zero(), false);
                    body.set_angvel(nalgebra::zero(), false);
                    body.sleep();
                }
                // Detaches every multibody joint containing `handle`; a
                // free body afterwards, so no forward kinematics can
                // re-teleport it. Idempotent for bodies without joints.
                self.multibody_joints
                    .remove_multibody_articulations(handle, false);
                self.explosive_detaches_total = self.explosive_detaches_total.saturating_add(1);
                log::error!(
                    "physics: detached {handle:?} [{label}]'s articulation after \
                     {offences} solver-explosion bursts (#5246)"
                );
            } else if *offences == 2 {
                // Second burst: the solve is persistently exploding for
                // this body. Park it the same way the invalid-solve
                // restore does — zeroed velocities, asleep at its current
                // (still sane, cap-bounded) pose — instead of letting it
                // vibrate at the cap forever.
                if let Some(body) = self.bodies.get_mut(handle) {
                    body.set_linvel(nalgebra::zero(), false);
                    body.set_angvel(nalgebra::zero(), false);
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
        // velocity teleports its links through the broad-phase grid in one
        // step even with every rigid body capped above (#5161).
        let mut clamped_dofs = 0usize;
        for &joint in &self.articulation_joints {
            let Some((multibody, _)) = self.multibody_joints.get_mut(joint) else {
                continue;
            };
            let mut vels = multibody.generalized_velocity_mut();
            for i in 0..vels.len() {
                let v = vels[i];
                if !v.is_finite() {
                    vels[i] = 0.0;
                    clamped_dofs += 1;
                } else if v.abs() > ARTICULATION_DOF_SANITY_CAP {
                    vels[i] = v.signum() * ARTICULATION_DOF_SANITY_CAP;
                    clamped_dofs += 1;
                }
            }
        }
        if clamped_dofs > 0 {
            self.velocity_clamps_total = self.velocity_clamps_total.saturating_add(1);
            log::warn!(
                "physics: clamped {clamped_dofs} exploding articulation DOF velocities to the \
                 sanity cap (#5161)"
            );
        }
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
