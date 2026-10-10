//! Build a Rapier **multibody** ragdoll from an engine-native
//! [`RagdollSpec`] (M41.x Phase 3).
//!
//! The spec is the glam-native form of the NIF importer's
//! `ImportedRagdoll`, resolved against the live bone world transforms at
//! activation time (the byroredux binary does that translation — this
//! crate never sees `byroredux-nif`). Here we turn it into Rapier rigid
//! bodies + colliders, orient the constraint graph into a kinematic tree,
//! and connect it with reduced-coordinate **multibody joints**.
//!
//! Why multibody, not impulse joints: a ragdoll is a pelvis-rooted tree
//! (no loops), exactly Rapier's reduced-coordinate sweet spot. Multibody
//! joints are constraint-by-construction, so the links can't visibly
//! stretch/separate under stress at the mass ratios and chain depth of a
//! humanoid — the artifact that makes the original Havok ragdolls feel
//! clunky. The trade-off (no closed loops) doesn't bite: humanoid
//! ragdolls are pure trees.
//!
//! Joint-limit fidelity is approximate for slice 1: Havok's cone + two
//! plane-angle model doesn't map 1:1 onto Rapier's per-axis angular
//! limits, so we apply twist→twist-axis and cone→both swing axes. Good
//! enough to switch an actor from bind-pose to a plausible ragdoll;
//! refinement is a follow-up.

use crate::components::Ragdoll;
use crate::config::ContactConfig;
use crate::convert::{collision_shape_to_parts, pose_from_trs};
use crate::world::PhysicsWorld;
use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::math::{Mat3, Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::VecDeque;

/// #5161 — sanity bounds for a ragdoll body's world-space seed pose.
///
/// Authored worldspace coordinates top out around ±3e5 BU, so a seed beyond
/// [`SEED_SANE_ABS_BOUND_BU`] is corruption with certainty — the same
/// argument as the keyframe-target bound in `world.rs`
/// (`KEYFRAME_TARGET_SANE_BOUND_BU`). (The bound was sized under rapier
/// 0.22 for a ~2600× margin under the multi-SAP grid boundary, ≈2.68e11,
/// which an insane pose tripped through the collider's predictive AABB;
/// that broad phase is gone since 0.35's BVH rework and the bound now
/// stands on the corruption argument alone.) The second failure class
/// is *relative*: bones planted millions of BU from their own actor root
/// (the "million-unit bone coordinates despite correct actor-root
/// placement" instability, slice doc §corpse) stay under the absolute bound
/// yet still explode the joint solver, which derives link velocities from
/// the root-to-link separation. [`SEED_SANE_MAX_ROOT_OFFSET_BU`] caps that:
/// the largest authored skeleton reach (dragon-class) is ~1e4 BU, so 1e5
/// keeps a decade of headroom while catching the million-unit class
/// outright.
pub const SEED_SANE_ABS_BOUND_BU: f32 = 1.0e8;
pub const SEED_SANE_MAX_ROOT_OFFSET_BU: f32 = 1.0e5;

/// Why a ragdoll body's seed pose was rejected by [`seed_pose_is_sane`].
#[derive(Debug, Clone, PartialEq)]
pub enum SeedInsanity {
    NonFinite,
    BeyondWorldBound { translation_norm: f32 },
    BeyondActorReach { distance: f32 },
}

impl std::fmt::Display for SeedInsanity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => write!(f, "non-finite seed pose"),
            Self::BeyondWorldBound { translation_norm } => write!(
                f,
                "seed |t|={translation_norm:.3e} beyond the sane world bound \
                 {SEED_SANE_ABS_BOUND_BU:.0e}"
            ),
            Self::BeyondActorReach { distance } => write!(
                f,
                "seed {distance:.3e} BU from the actor root, beyond the \
                 {SEED_SANE_MAX_ROOT_OFFSET_BU:.0e} BU actor-reach bound"
            ),
        }
    }
}

/// The body whose seed pose failed validation, as [`build_ragdoll`] and
/// `activate_ragdoll` report it. An articulation is all-or-nothing: no body
/// of a rejected spec reaches Rapier, so the actor keeps its last animated
/// pose instead of an exploding multibody.
#[derive(Debug, Clone, PartialEq)]
pub struct SeedRejection {
    pub body_index: usize,
    pub entity: EntityId,
    pub cause: SeedInsanity,
}

/// #5161 — validate one ragdoll body's world-space seed pose before it can
/// reach Rapier. `actor_root` is the activating actor's world translation;
/// `None` skips the relative check (the caller had no root
/// `GlobalTransform`). Checked at both boundaries that compose a seed: the
/// bin-side activator validates against the actor root (the relative class),
/// and [`build_ragdoll`] re-validates absolutely as the construction-side
/// backstop for any future builder.
pub fn seed_pose_is_sane(
    translation: Vec3,
    rotation: Quat,
    actor_root: Option<Vec3>,
) -> Result<(), SeedInsanity> {
    if !translation.is_finite() || !rotation.is_finite() {
        return Err(SeedInsanity::NonFinite);
    }
    let translation_norm = translation.length();
    if translation_norm > SEED_SANE_ABS_BOUND_BU {
        return Err(SeedInsanity::BeyondWorldBound { translation_norm });
    }
    if let Some(root) = actor_root {
        let distance = (translation - root).length();
        if distance > SEED_SANE_MAX_ROOT_OFFSET_BU {
            return Err(SeedInsanity::BeyondActorReach { distance });
        }
    }
    Ok(())
}

/// One rigid body of a ragdoll, already resolved to engine world space.
#[derive(Debug, Clone)]
pub struct RagdollBodySpec {
    /// The skeleton bone entity this body drives (for writeback).
    pub entity: EntityId,
    /// World-space seed pose (bone world × body-local offset).
    pub translation: Vec3,
    pub rotation: Quat,
    /// The bone's `GlobalTransform.scale` at the moment this spec was
    /// seeded — the *same* value `translation` was composed with
    /// (`bone_t + bone_r * (local_t * scale)`). The writeback inverse
    /// (`ragdoll_writeback_system`) must decompose using this exact
    /// snapshot, not a fresh live read, or a bone whose scale changes
    /// after activation decomposes against the wrong scale and
    /// displaces by `local_translation * Δscale`. See #1852.
    pub scale: f32,
    /// Collider shape in body-local space (Y-up, havok-scaled).
    pub shape: CollisionShape,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
}

/// Joint geometry in engine space. Pivots are body-local positions; axes
/// are body-local unit directions; angles are radians.
#[derive(Debug, Clone)]
pub enum RagdollJointSpec {
    Ragdoll {
        twist_a: Vec3,
        plane_a: Vec3,
        pivot_a: Vec3,
        twist_b: Vec3,
        plane_b: Vec3,
        pivot_b: Vec3,
        cone_max: f32,
        twist_min: f32,
        twist_max: f32,
    },
    LimitedHinge {
        axis_a: Vec3,
        /// Authored zero-angle reference direction for side A — the plane
        /// `min_angle`/`max_angle` are measured from. Threaded straight
        /// through from the NIF importer's `ImportedJointKind::LimitedHinge`
        /// (this crate never depends on `byroredux-nif`, so no re-derivation
        /// happens here). #2448 / PHYS-02.
        perp_a: Vec3,
        pivot_a: Vec3,
        axis_b: Vec3,
        /// Authored zero-angle reference direction for side B — may be a
        /// zero vector on Oblivion/Morrowind content (not authored in that
        /// era's layout); [`frame_rot`] falls back to a synthesized
        /// perpendicular for a degenerate input.
        perp_b: Vec3,
        pivot_b: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
    /// 1-DOF sliding rail (`bhkPrismaticConstraint`, #3792). All three
    /// rotation axes and the two non-sliding translation axes are
    /// locked; the body translates along `axis_a`/`axis_b` between
    /// `min_distance`/`max_distance`.
    Prismatic {
        axis_a: Vec3,
        /// Authored zero-angle reference direction for side A — same
        /// role as `LimitedHinge::perp_a`, threaded through from
        /// `ImportedJointKind::Prismatic` with no re-derivation here.
        perp_a: Vec3,
        pivot_a: Vec3,
        axis_b: Vec3,
        perp_b: Vec3,
        pivot_b: Vec3,
        min_distance: f32,
        max_distance: f32,
    },
}

impl RagdollJointSpec {
    /// Scale the body-local **pivot** vectors, leaving every axis untouched.
    ///
    /// #2868 — the pivots arrive from the NIF importer in authored bind-space
    /// units, but a scaled actor's bodies are seeded a scaled distance apart.
    /// Because a ragdoll joint is a *multibody* (reduced-coordinate) joint,
    /// the child link's translation is not a soft constraint that the seeded
    /// separation can win: forward kinematics *defines* it as
    /// `parent_pose ∘ frame1 ∘ joint_rot ∘ frame2⁻¹`, so bind-scale pivots
    /// overwrite the seeded pose on the very first step and the ragdoll
    /// collapses to bind proportions. Scaling the pivots to match the seed is
    /// what keeps the two consistent.
    ///
    /// `twist_*` / `plane_*` / `axis_*` / `perp_*` are unit **directions**
    /// defining frame orientation — scaling them would be meaningless at best
    /// and would corrupt the frame basis at worst. They stay as authored.
    ///
    /// Each side takes its own body's scale because each pivot lives in its
    /// own body's local frame; `build_joint`'s `flip` swaps the already-scaled
    /// pair, so orientation and scaling stay independent.
    pub fn scaled_pivots(&self, scale_a: f32, scale_b: f32) -> Self {
        let factor = |scale: f32| {
            if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            }
        };
        let (scale_a, scale_b) = (factor(scale_a), factor(scale_b));
        match self {
            Self::Ragdoll {
                twist_a,
                plane_a,
                pivot_a,
                twist_b,
                plane_b,
                pivot_b,
                cone_max,
                twist_min,
                twist_max,
            } => Self::Ragdoll {
                twist_a: *twist_a,
                plane_a: *plane_a,
                pivot_a: *pivot_a * scale_a,
                twist_b: *twist_b,
                plane_b: *plane_b,
                pivot_b: *pivot_b * scale_b,
                cone_max: *cone_max,
                twist_min: *twist_min,
                twist_max: *twist_max,
            },
            Self::LimitedHinge {
                axis_a,
                perp_a,
                pivot_a,
                axis_b,
                perp_b,
                pivot_b,
                min_angle,
                max_angle,
            } => Self::LimitedHinge {
                axis_a: *axis_a,
                perp_a: *perp_a,
                pivot_a: *pivot_a * scale_a,
                axis_b: *axis_b,
                perp_b: *perp_b,
                pivot_b: *pivot_b * scale_b,
                min_angle: *min_angle,
                max_angle: *max_angle,
            },
            Self::Prismatic {
                axis_a,
                perp_a,
                pivot_a,
                axis_b,
                perp_b,
                pivot_b,
                min_distance,
                max_distance,
            } => Self::Prismatic {
                axis_a: *axis_a,
                perp_a: *perp_a,
                pivot_a: *pivot_a * scale_a,
                axis_b: *axis_b,
                perp_b: *perp_b,
                pivot_b: *pivot_b * scale_b,
                // Linear travel limits, not directions — scale like a
                // pivot (in side A's frame, matching axis_a/pivot_a).
                min_distance: *min_distance * scale_a,
                max_distance: *max_distance * scale_a,
            },
        }
    }
}

/// One joint linking two bodies by index into [`RagdollSpec::bodies`].
#[derive(Debug, Clone)]
pub struct RagdollConstraintSpec {
    pub body_a: usize,
    pub body_b: usize,
    pub joint: RagdollJointSpec,
}

/// The full articulation, ready to build.
#[derive(Debug, Clone)]
pub struct RagdollSpec {
    pub bodies: Vec<RagdollBodySpec>,
    pub constraints: Vec<RagdollConstraintSpec>,
}

/// #1540 — map a ragdoll body's collision shape to one safe to attach to a
/// *dynamic* Rapier body.
///
/// A [`CollisionShape::TriMesh`] (possible when a bone hosts a
/// `bhkPackedNiTriStripsShape` / compressed mesh rather than a primitive)
/// is an open triangle soup with no well-defined enclosed volume, so
/// Rapier's shape-derived inertia tensor is degenerate even after the
/// `.mass()` override in [`build_ragdoll`] — the link can spin
/// pathologically. Substitute a convex hull of the same vertices: a closed
/// solid with a finite, non-degenerate inertia tensor that still bounds the
/// authored geometry. [`collision_shape_to_parts`] already falls back to a
/// tiny ball if the hull itself is degenerate (< 4 non-coplanar points), so
/// the inertia is finite in every case. Vanilla FNV ragdoll bones author
/// capsules/boxes so this rarely fires; it guards modded / creature
/// skeletons. Primitives and convex hulls pass through unchanged; compounds
/// recurse so a nested trimesh leaf is substituted too.
///
/// Only the dynamic *ragdoll* path uses this — static world colliders keep
/// their trimeshes (they need no inertia), so `collision_shape_to_parts`
/// stays untouched.
fn ragdoll_dynamic_shape(shape: &CollisionShape) -> CollisionShape {
    match shape {
        CollisionShape::TriMesh { vertices, .. } => CollisionShape::ConvexHull {
            vertices: vertices.clone(),
        },
        CollisionShape::Compound { children } => CollisionShape::Compound {
            children: children
                .iter()
                .map(|(t, r, child)| (*t, *r, Box::new(ragdoll_dynamic_shape(child))))
                .collect(),
        },
        other => other.clone(),
    }
}

/// Build the ragdoll into `pw` and return the [`Ragdoll`] component for
/// the actor. Creates one dynamic body + collider per spec body, orients
/// the constraint graph into a tree, and inserts a multibody joint per
/// edge. Calls [`PhysicsWorld::wake`] so the first step simulates it.
///
/// #5161 — refuses the whole spec when any body's seed pose is absolutely
/// insane ([`seed_pose_is_sane`]; the caller adds the actor-relative check)
/// *before* touching the world, so a rejected activation leaves no bodies,
/// colliders or joints behind and counts one refusal. Each built body is
/// registered with a default `body_labels` entry ("entity N") for the
/// invalid-solve evidence log; the bin-side activator enriches it.
pub fn build_ragdoll(
    pw: &mut PhysicsWorld,
    spec: &RagdollSpec,
    cfg: &ContactConfig,
) -> Result<Ragdoll, SeedRejection> {
    for (body_index, b) in spec.bodies.iter().enumerate() {
        if let Err(cause) = seed_pose_is_sane(b.translation, b.rotation, None) {
            pw.note_ragdoll_seed_refusal();
            return Err(SeedRejection {
                body_index,
                entity: b.entity,
                cause,
            });
        }
    }
    // 1. Rigid bodies + colliders.
    let mut handles: Vec<RigidBodyHandle> = Vec::with_capacity(spec.bodies.len());
    // Every collider built here, queued for the scene-query BVH below (#3968).
    let mut ragdoll_colliders: Vec<ColliderHandle> = Vec::new();
    // #3492 — the buoyancy scan cannot recover either of these from the ECS:
    // ragdoll bodies never get a `RapierHandles` row, and `activate_ragdoll`
    // deletes the bone entities' `RigidBodyData` under #1772. Record them
    // here, where both are in hand.
    let mut buoyancy: Vec<crate::components::RagdollBuoyancy> =
        Vec::with_capacity(spec.bodies.len());
    for b in &spec.bodies {
        // Effective, not authored: the extra angular damping below is part
        // of what the body actually runs with, so it is also what buoyancy
        // must restore on exit (#3492).
        let effective_linear_damping = b.linear_damping.max(0.0);
        let effective_angular_damping =
            b.angular_damping.max(0.0) + cfg.ragdoll_extra_angular_damping.max(0.0);
        let body = RigidBodyBuilder::dynamic()
            .pose(pose_from_trs(b.translation, b.rotation))
            // Authored humanoid mass ratios, joint limits, and simultaneous
            // floor contacts need more than the default four iterations:
            // the FNV restore fixture diverged on a single flat floor at
            // tick 75. Sixteen iterations kept the unchanged articulation
            // bounded. Scope the extra work to ragdoll contact/joint islands
            // rather than increasing the budget for the whole world.
            .additional_solver_iterations(12)
            .linear_damping(effective_linear_damping)
            // "less floppy than Havok" lever — extra angular damping on top
            // of the authored value (inert at the 0.0 default). See
            // ContactConfig::ragdoll_extra_angular_damping.
            .angular_damping(effective_angular_damping)
            .build();
        let h = pw.bodies.insert(body);
        // #5161 — default evidence-log label; the bin-side activator
        // overwrites it with the actor + bone name it resolved at seed time.
        pw.set_body_label(h, format!("entity {}", b.entity));
        // #4682 — ragdoll bodies are dynamic; index them for the per-substep
        // recovery snapshot (a freshly activated ragdoll's first solve is
        // exactly the case the snapshot exists to cover).
        pw.dynamic_bodies.push(h);

        // #1540 — substitute a convex hull for any TriMesh on this *dynamic*
        // ragdoll body; a raw trimesh gives Rapier a degenerate inertia
        // tensor even with the `.mass()` override below. See
        // `ragdoll_dynamic_shape`.
        // #2860 sibling — the ragdoll builder is the third collider producer
        // and had the same drop. `b.scale` is the bone's `GlobalTransform`
        // scale at seed time, and `RagdollSpec` already scales the *joint
        // pivots* by it (`scaled_pivots`); leaving the shape unscaled meant a
        // scaled actor's limb colliders were sized for bind proportions while
        // their articulation was sized for the seed — a self-inconsistent rig.
        let parts = collision_shape_to_parts(&ragdoll_dynamic_shape(&b.shape), b.scale, cfg);
        let part_mass = b.mass.max(1e-3) / parts.len() as f32;
        // #2861 — the anti-leak contact margin `register_newcomers` applies to
        // every other collider in the engine. Rapier sums the skin of both
        // colliders in a pair, so an unskinned ragdoll limb got half the
        // intended margin against skinned world geometry and *zero* against
        // another ragdoll — exactly the mixed-skin seam `ContactConfig` exists
        // to eliminate. `config.rs`'s module doc enumerates the unification
        // sites and simply never listed this one.
        let contact_skin = cfg.default_contact_skin_bu.max(0.0);
        let PhysicsWorld {
            ref mut bodies,
            ref mut colliders,
            ..
        } = *pw;
        let mut first_collider = None;
        for (iso, shape) in parts {
            // Havok-parity coefficient combine: see
            // `config::CONTACT_COEFFICIENT_COMBINE_RULE`.
            let col = ColliderBuilder::new(shape)
                .position(iso)
                .friction(b.friction.max(0.0))
                .restitution(b.restitution.clamp(0.0, 1.0))
                .friction_combine_rule(crate::config::CONTACT_COEFFICIENT_COMBINE_RULE)
                .restitution_combine_rule(crate::config::CONTACT_COEFFICIENT_COMBINE_RULE)
                .mass(part_mass)
                .contact_skin(contact_skin)
                .build();
            let ch = colliders.insert_with_parent(col, h, bodies);
            first_collider.get_or_insert(ch);
            ragdoll_colliders.push(ch);
        }
        // `collision_shape_to_parts` never yields zero parts (see #3067 and
        // its own `out.is_empty()` fallback), so this is the first of at
        // least one.
        if let Some(collider) = first_collider {
            buoyancy.push(crate::components::RagdollBuoyancy {
                collider,
                linear_damping: effective_linear_damping,
                angular_damping: effective_angular_damping,
            });
        }
        handles.push(h);
    }

    // 2. Orient the (undirected) constraint graph into a parent→child tree
    //    via BFS, handling a forest if the graph is disconnected. Back-edges
    //    (which would form a loop multibody can't represent) are dropped.
    let oriented = orient_tree(spec);

    // #1539 — a humanoid ragdoll is a single pelvis-rooted tree. A spanning
    // forest (>1 connected component) means an articulation edge was dropped
    // upstream — e.g. an unsupported `Other` constraint dropped in
    // `extract_ragdoll` (`crates/nif/src/import/collision.rs`) — so the
    // disconnected limbs build here as independent free-floating multibodies
    // that free-fall. Surface it rather than producing a broken ragdoll
    // silently. A spanning forest over `n` bodies has `n - components` edges,
    // so `components = bodies - edges`.
    let components = spec.bodies.len().saturating_sub(oriented.len());
    if components > 1 {
        log::warn!(
            "build_ragdoll: constraint graph is a forest — {components} disconnected \
             components across {} bodies ({} joint edges; a single tree needs {}). \
             Detached limbs will free-fall; an articulation constraint was likely \
             dropped upstream (#1539).",
            spec.bodies.len(),
            oriented.len(),
            spec.bodies.len().saturating_sub(1),
        );
    }

    // 3. Insert one multibody joint per tree edge (parent already in the
    //    multibody from BFS order; the root is the multibody base).
    let mut joints = Vec::with_capacity(oriented.len());
    for edge in &oriented {
        let parent_seed = pose_from_trs(
            spec.bodies[edge.parent].translation,
            spec.bodies[edge.parent].rotation,
        );
        let child_seed = pose_from_trs(
            spec.bodies[edge.child].translation,
            spec.bodies[edge.child].rotation,
        );
        let joint = build_joint(&spec.constraints[edge.constraint].joint, edge.flip);
        if let Some(jh) =
            pw.multibody_joints
                .insert(handles[edge.parent], handles[edge.child], joint, true)
        {
            if let Some((multibody, link_id)) = pw.multibody_joints.get_mut(jh) {
                // #2338 — suppress contacts only between links of this one
                // articulation. World geometry and other actors remain in
                // different multibodies, so their contacts are unaffected.
                multibody.set_self_contacts_enabled(false);

                if let Some(link) = multibody.link_mut(link_id) {
                    // #2337 — multibody forward kinematics owns every
                    // non-root pose. Seed its reduced coordinates from the
                    // animated body poses before the first physics step can
                    // replace them with the joint's zero/rest coordinates.
                    seed_joint_from_body_poses(
                        &mut link.joint,
                        &spec.constraints[edge.constraint].joint,
                        parent_seed,
                        child_seed,
                    );
                } else {
                    log::warn!(
                        "ragdoll: inserted multibody joint {}→{} has no child link — \
                         animated seed pose could not be retained",
                        edge.parent,
                        edge.child,
                    );
                }
            }
            joints.push(jh);
            // #5161 — the velocity clamp walks this index every substep;
            // the set itself exposes no mutable whole-set iteration.
            pw.articulation_joints.push(jh);
        } else {
            log::warn!(
                "ragdoll: multibody joint {}→{} rejected (would form a loop?) — skipped",
                edge.parent,
                edge.child,
            );
        }
    }

    // #3968 — `wake()` does not guarantee a substep (the `accumulator >=
    // PHYSICS_DT` gate is independent), and `queue_query_refresh` is the
    // only mechanism that reaches the query BVH on a no-substep frame
    // (#2864). Without this, a ragdoll built on a >60 fps frame is absent
    // from the query BVH for up to one banked tick. The behaviour was
    // historically safe only by accident — `activate_ragdoll`'s #1772
    // keyframed teardown calls `remove_body` per bone, which marks dirty as
    // a side effect — and that accident has a hole: the teardown is guarded
    // by `if !bone_handles.is_empty()`.
    pw.queue_query_refresh(ragdoll_colliders);
    pw.wake();

    Ok(Ragdoll {
        bodies: spec
            .bodies
            .iter()
            .zip(handles)
            .map(|(b, h)| (b.entity, h, b.scale))
            .collect(),
        joints,
        buoyancy,
    })
}

/// A tree edge after orientation: `flip` is true when the constraint's
/// `body_a` is the *child* (so frame A/B and pivots must swap).
struct TreeEdge {
    parent: usize,
    child: usize,
    constraint: usize,
    flip: bool,
}

/// BFS-orient the constraint graph into a rooted tree (forest-safe).
fn orient_tree(spec: &RagdollSpec) -> Vec<TreeEdge> {
    let n = spec.bodies.len();
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (ci, c) in spec.constraints.iter().enumerate() {
        if c.body_a < n && c.body_b < n {
            adj[c.body_a].push((c.body_b, ci));
            adj[c.body_b].push((c.body_a, ci));
        }
    }
    let mut visited = vec![false; n];
    let mut used = vec![false; spec.constraints.len()];
    let mut out = Vec::new();
    let mut queue = VecDeque::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        while let Some(p) = queue.pop_front() {
            for &(child, ci) in &adj[p] {
                if used[ci] || visited[child] {
                    continue;
                }
                used[ci] = true;
                visited[child] = true;
                out.push(TreeEdge {
                    parent: p,
                    child,
                    constraint: ci,
                    // parent p is body_b ⇒ a is the child ⇒ flip.
                    flip: spec.constraints[ci].body_a == child,
                });
                queue.push_back(child);
            }
        }
    }
    out
}

/// All three linear DOF — locked for both ragdoll and hinge joints.
fn lin_locked() -> JointAxesMask {
    JointAxesMask::LIN_X | JointAxesMask::LIN_Y | JointAxesMask::LIN_Z
}

/// All three rotational DOF plus the two non-sliding linear DOF — locked
/// for a prismatic joint (#3792). nif.xml's own description: "All three
/// rotation axes and the remaining two translation axes are fixed." The
/// local frame's rotation (`frame_rot`) maps the authored sliding axis
/// onto local X, so `LinX` is the one axis left out here — free but
/// bounded via `.limits(JointAxis::LinX, [min, max])`, the same
/// "lock the frame, limit the remaining DOF" shape `LimitedHinge` uses
/// for `AngX`.
fn prismatic_locked() -> JointAxesMask {
    JointAxesMask::LIN_Y
        | JointAxesMask::LIN_Z
        | JointAxesMask::ANG_X
        | JointAxesMask::ANG_Y
        | JointAxesMask::ANG_Z
}

fn build_joint(j: &RagdollJointSpec, flip: bool) -> GenericJoint {
    match j {
        RagdollJointSpec::Ragdoll {
            twist_a,
            plane_a,
            pivot_a,
            twist_b,
            plane_b,
            pivot_b,
            cone_max,
            twist_min,
            twist_max,
        } => {
            // Orient so frame1 is the parent's. Under flip the twist axis
            // direction reverses, so the twist limit range negates+swaps.
            let (t1, p1, pv1, t2, p2, pv2, tmin, tmax) = if !flip {
                (
                    *twist_a, *plane_a, *pivot_a, *twist_b, *plane_b, *pivot_b, *twist_min,
                    *twist_max,
                )
            } else {
                (
                    *twist_b,
                    *plane_b,
                    *pivot_b,
                    *twist_a,
                    *plane_a,
                    *pivot_a,
                    -*twist_max,
                    -*twist_min,
                )
            };
            let cone = cone_max.abs();
            GenericJointBuilder::new(lin_locked())
                .local_frame1(pose_from_trs(pv1, frame_rot(t1, p1)))
                .local_frame2(pose_from_trs(pv2, frame_rot(t2, p2)))
                .limits(JointAxis::AngX, [tmin, tmax]) // twist
                .limits(JointAxis::AngY, [-cone, cone]) // swing
                .limits(JointAxis::AngZ, [-cone, cone]) // swing
                .build()
        }
        RagdollJointSpec::LimitedHinge {
            axis_a,
            perp_a,
            pivot_a,
            axis_b,
            perp_b,
            pivot_b,
            min_angle,
            max_angle,
        } => {
            let (a1, p1, pv1, a2, p2, pv2, amin, amax) = if !flip {
                (
                    *axis_a, *perp_a, *pivot_a, *axis_b, *perp_b, *pivot_b, *min_angle, *max_angle,
                )
            } else {
                (
                    *axis_b,
                    *perp_b,
                    *pivot_b,
                    *axis_a,
                    *perp_a,
                    *pivot_a,
                    -*max_angle,
                    -*min_angle,
                )
            };
            // #2448 / PHYS-02 — the authored perp axis IS the zero-angle
            // reference `min_angle`/`max_angle` are measured from.
            // `frame_rot` orthogonalises it against the axis and falls back
            // to a synthesized perpendicular only for a degenerate (zero /
            // parallel) input — e.g. Oblivion's zeroed `perp_axis_in_b1`.
            GenericJointBuilder::new(lin_locked() | JointAxesMask::ANG_Y | JointAxesMask::ANG_Z)
                .local_frame1(pose_from_trs(pv1, frame_rot(a1, p1)))
                .local_frame2(pose_from_trs(pv2, frame_rot(a2, p2)))
                .limits(JointAxis::AngX, [amin, amax])
                .build()
        }
        RagdollJointSpec::Prismatic {
            axis_a,
            perp_a,
            pivot_a,
            axis_b,
            perp_b,
            pivot_b,
            min_distance,
            max_distance,
        } => {
            // #3792 — same flip treatment as LimitedHinge: swapping which
            // side is frame1 reverses the sign convention of the
            // along-axis coordinate, so the distance limits negate+swap.
            let (a1, p1, pv1, a2, p2, pv2, dmin, dmax) = if !flip {
                (
                    *axis_a,
                    *perp_a,
                    *pivot_a,
                    *axis_b,
                    *perp_b,
                    *pivot_b,
                    *min_distance,
                    *max_distance,
                )
            } else {
                (
                    *axis_b,
                    *perp_b,
                    *pivot_b,
                    *axis_a,
                    *perp_a,
                    *pivot_a,
                    -*max_distance,
                    -*min_distance,
                )
            };
            // `frame_rot` maps the authored sliding axis onto local X, so
            // `JointAxis::LinX` is the one DOF `prismatic_locked` leaves
            // out — free but bounded by the authored travel range.
            GenericJointBuilder::new(prismatic_locked())
                .local_frame1(pose_from_trs(pv1, frame_rot(a1, p1)))
                .local_frame2(pose_from_trs(pv2, frame_rot(a2, p2)))
                .limits(JointAxis::LinX, [dmin, dmax])
                .build()
        }
    }
}

/// Initialise a child link's reduced coordinates from the world-space body
/// poses captured at ragdoll activation.
///
/// Rapier stores non-root multibody poses in the joint, not in the attached
/// rigid body's Cartesian `position`. The joint transform in joint space is
/// `frame1⁻¹ ∘ (parent⁻¹ ∘ child) ∘ frame2`; whichever component of it the
/// joint's free axis names is what the reduced coordinates must carry.
///
/// # Dispatch
///
/// This matches on [`RagdollJointSpec`] — **not** on `joint.ndofs()`, which
/// is what it used to do and what #3962 was filed for. `ndofs()` is
/// `6 - locked_axes.count_ones()`, so `LimitedHinge` (free `AngX`) and
/// `Prismatic` (free `LinX`) are both `1` and indistinguishable by it. The
/// prismatic edges #3792 added were consequently seeded by writing a rotation
/// vector's X component — radians — into a slide distance in engine units,
/// and `apply_displacement` walks the linear axes first so it landed on
/// exactly the wrong coordinate.
///
/// Matching the enum means the compiler, not a reviewer, is what forces a
/// fourth variant to be considered here — the property the `ndofs()` form
/// lacked. `build_joint` and `scaled_pivots` already had it.
fn seed_joint_from_body_poses(
    joint: &mut MultibodyJoint,
    spec: &RagdollJointSpec,
    parent_pose: Pose,
    child_pose: Pose,
) {
    let parent_to_child = parent_pose.inverse() * child_pose;
    // Rotation and translation of one isometry product — the rotation half is
    // bit-for-bit the quantity the pre-#3962 code computed.
    let joint_transform =
        joint.data.local_frame1.inverse() * parent_to_child * joint.data.local_frame2;
    let angular_displacement = crate::convert::rotation_scaled_axis(joint_transform.rotation);

    match spec {
        // Local angular X/Y/Z are all free.
        RagdollJointSpec::Ragdoll { .. } => {
            joint.apply_displacement(&angular_displacement.to_array())
        }
        // Only local angular X is free.
        RagdollJointSpec::LimitedHinge { .. } => {
            joint.apply_displacement(&[angular_displacement.x])
        }
        // Only local *linear* X is free — `frame_rot` maps the authored
        // sliding axis onto it. The correct seed is the along-rail
        // component of the same joint transform.
        RagdollJointSpec::Prismatic { .. } => {
            let mut slide = joint_transform.translation.x;
            // `apply_displacement` does not clamp against the joint's own
            // limits (`MultibodyJoint::integrate` has no clamp), and an
            // authored bind pose can legitimately sit outside the authored
            // travel range. Opening in violation makes the solver correct it
            // positionally, and a multibody spreads that correction across
            // the chain — measured: a 1000-unit overshoot on a ±1 rail drags
            // the *root* ~500 units off its authored origin in one step.
            // Clamp to the nearest valid rail position instead.
            if let Some(limits) = joint.data.limits(JointAxis::LinX) {
                slide = slide.clamp(limits.min, limits.max);
            }

            joint.apply_displacement(&[slide])
        }
    }
}

/// Build a rotation whose local X = `primary`, local Y = `secondary`
/// orthogonalised against X, local Z = X×Y. Falls back to identity-ish
/// bases for degenerate (zero / parallel) inputs so a corrupt joint can't
/// produce a NaN frame.
fn frame_rot(primary: Vec3, secondary: Vec3) -> Quat {
    let x = norm_or(primary, Vec3::X);
    let mut y = secondary - x * x.dot(secondary);
    y = norm_or(y, any_perp(x));
    let z = x.cross(y).normalize_or_zero();
    if z.length_squared() < 1e-8 {
        return Quat::IDENTITY;
    }
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

#[inline]
fn norm_or(v: Vec3, fallback: Vec3) -> Vec3 {
    let n = v.normalize_or_zero();
    if n.length_squared() < 1e-8 {
        fallback
    } else {
        n
    }
}

/// Any unit vector orthogonal to `axis`.
fn any_perp(axis: Vec3) -> Vec3 {
    let a = axis.normalize_or_zero();
    let seed = if a.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    (seed - a * a.dot(seed)).normalize_or_zero()
}

impl PhysicsWorld {
    /// Tear down a ragdoll: remove every member body (which cascades its
    /// colliders + multibody joints out of the sets). Mirrors the #1520
    /// no-leak discipline so a cell unload mid-ragdoll doesn't strand
    /// bodies in the broad-phase. Safe to call with stale handles.
    pub fn remove_ragdoll(&mut self, ragdoll: &Ragdoll) {
        for (_, h, _) in &ragdoll.bodies {
            self.remove_body(*h);
        }
    }
}

/// Helper for callers/tests: a body's current world translation, if live.
pub fn body_translation(pw: &PhysicsWorld, h: RigidBodyHandle) -> Option<Vec3> {
    pw.bodies.get(h).map(|b| b.translation())
}

/// Helper for the per-frame writeback: a body's current world
/// (translation, rotation), if live.
pub fn body_pose(pw: &PhysicsWorld, h: RigidBodyHandle) -> Option<(Vec3, Quat)> {
    pw.bodies.get(h).map(|b| {
        let iso = b.position();
        (
            iso.translation,
            iso.rotation,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::PHYSICS_DT;

    fn ball_body(entity_idx: EntityId, x: f32, y: f32) -> RagdollBodySpec {
        RagdollBodySpec {
            entity: entity_idx,
            translation: Vec3::new(x, y, 0.0),
            rotation: Quat::IDENTITY,
            scale: 1.0,
            shape: CollisionShape::Ball { radius: 5.0 },
            mass: 4.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            friction: 0.5,
            restitution: 0.0,
        }
    }

    fn loose_ragdoll(a: usize, b: usize) -> RagdollConstraintSpec {
        RagdollConstraintSpec {
            body_a: a,
            body_b: b,
            joint: RagdollJointSpec::Ragdoll {
                twist_a: Vec3::X,
                plane_a: Vec3::Y,
                pivot_a: Vec3::new(25.0, 0.0, 0.0),
                twist_b: Vec3::X,
                plane_b: Vec3::Y,
                pivot_b: Vec3::new(-25.0, 0.0, 0.0),
                cone_max: std::f32::consts::PI,
                twist_min: -std::f32::consts::PI,
                twist_max: std::f32::consts::PI,
            },
        }
    }

    /// #5161 — the seed gate must accept the authored pose classes: an
    /// interior at any authored worldspace coordinate (±3e5 BU), bones
    /// within dragon-class reach of their actor root.
    #[test]
    fn seed_pose_is_sane_accepts_authored_poses() {
        let far_exterior = Vec3::new(3.0e5, -2.0e5, 4.0e3);
        let bone = far_exterior + Vec3::new(30.0, 90.0, -10.0);
        assert_eq!(seed_pose_is_sane(bone, Quat::IDENTITY, Some(far_exterior)), Ok(()));
        // Without a known actor root only the absolute check applies.
        assert_eq!(seed_pose_is_sane(bone, Quat::IDENTITY, None), Ok(()));
    }

    /// #5161 — a NaN/∞ seed (decomposed bone or template-local garbage)
    /// must be refused outright.
    #[test]
    fn seed_pose_is_sane_rejects_non_finite_poses() {
        assert_eq!(
            seed_pose_is_sane(Vec3::new(1.0, f32::NAN, 0.0), Quat::IDENTITY, None),
            Err(SeedInsanity::NonFinite)
        );
        assert_eq!(
            seed_pose_is_sane(
                Vec3::ZERO,
                Quat::from_array([f32::INFINITY, 0.0, 0.0, 1.0]),
                None,
            ),
            Err(SeedInsanity::NonFinite)
        );
    }

    /// #5161 — beyond the absolute sane world bound, with and without an
    /// actor root: the absolute class always rejects.
    #[test]
    fn seed_pose_is_sane_rejects_beyond_world_bound() {
        let insane = Vec3::new(2.7e13, 0.0, 0.0);
        assert_eq!(
            seed_pose_is_sane(insane, Quat::IDENTITY, None),
            Err(SeedInsanity::BeyondWorldBound {
                translation_norm: 2.7e13
            })
        );
        assert!(matches!(
            seed_pose_is_sane(insane, Quat::IDENTITY, Some(insane)),
            Err(SeedInsanity::BeyondWorldBound { .. })
        ));
    }

    /// #5161 — the relative class: a bone planted a million BU from its own
    /// actor root passes the absolute bound but explodes the joint solver
    /// anyway. A known root must reject it; an unknown root must not.
    #[test]
    fn seed_pose_is_sane_rejects_beyond_actor_reach() {
        let root = Vec3::new(1.0e5, 0.0, 0.0);
        let million_unit_bone = root + Vec3::new(1.0e6, 0.0, 0.0);
        assert_eq!(
            seed_pose_is_sane(million_unit_bone, Quat::IDENTITY, Some(root)),
            Err(SeedInsanity::BeyondActorReach { distance: 1.0e6 })
        );
        assert_eq!(seed_pose_is_sane(million_unit_bone, Quat::IDENTITY, None), Ok(()));
    }

    /// #5161 — a spec with one insane seed must be rejected wholesale
    /// BEFORE any body, collider or joint reaches the world (no partial
    /// articulation), and the refusal must be counted.
    #[test]
    fn build_ragdoll_rejects_an_insane_spec_without_touching_the_world() {
        let mut pw = PhysicsWorld::new();
        let pre_bodies = pw.body_count();
        let pre_colliders = pw.colliders.len();
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 1000.0),
                ball_body(2, 2.7e13, 1000.0),
                ball_body(3, 50.0, 1000.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let rejection = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT)
            .expect_err("an absolutely insane seed must be rejected");
        assert_eq!(rejection.body_index, 1);
        assert_eq!(rejection.entity, 2u32);
        assert!(matches!(
            rejection.cause,
            SeedInsanity::BeyondWorldBound { .. }
        ));
        assert_eq!(
            pw.body_count(),
            pre_bodies,
            "no body of a rejected spec may reach the world"
        );
        assert_eq!(pw.colliders.len(), pre_colliders, "no collider may leak");
        assert_eq!(
            pw.ragdoll_seed_refusals_total(),
            1,
            "construction-side rejection must count exactly one refusal"
        );
    }

    /// #5161 — every built ragdoll body carries a default evidence-log
    /// label naming its bone entity, and `remove_body` (via
    /// `remove_ragdoll`) drops it again so the map cannot accumulate one
    /// stale entry per despawned corpse.
    #[test]
    fn ragdoll_bodies_are_labelled_for_the_evidence_log_and_unlabelled_on_teardown() {
        let mut pw = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![ball_body(42, 0.0, 1000.0), ball_body(43, 50.0, 1000.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        for &(entity, handle, _) in &rag.bodies {
            assert_eq!(
                pw.body_label(handle),
                Some(format!("entity {entity}").as_str()),
                "default label must name the bone entity"
            );
        }
        pw.remove_ragdoll(&rag);
        for &(_, handle, _) in &rag.bodies {
            assert_eq!(
                pw.body_label(handle),
                None,
                "torn-down bodies must not leave stale labels"
            );
        }
    }

    /// #2861 — `build_ragdoll` is the only production collider site that
    /// omitted `default_contact_skin_bu`. Rapier sums the skin of both
    /// colliders in a pair, so an unskinned limb got half the intended margin
    /// against skinned world geometry and zero against another ragdoll —
    /// precisely the mixed-skin seam `ContactConfig` exists to close.
    #[test]
    fn ragdoll_colliders_carry_the_engine_contact_skin() {
        let mut pw = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 0.0), ball_body(2, 50.0, 0.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let cfg = ContactConfig::DEFAULT;
        let rag = build_ragdoll(&mut pw, &spec, &cfg).expect("sane ragdoll seed");

        assert!(cfg.default_contact_skin_bu > 0.0, "config precondition");
        for (_, body, _) in &rag.bodies {
            let rb = pw.bodies.get(*body).expect("body");
            assert!(!rb.colliders().is_empty(), "each limb has a collider");
            for handle in rb.colliders() {
                assert_eq!(
                    pw.colliders.get(*handle).expect("collider").contact_skin(),
                    cfg.default_contact_skin_bu,
                    "ragdoll limb must carry the same skin register_newcomers applies"
                );
            }
        }
    }

    #[test]
    fn ragdoll_solver_budget_is_local_and_preserves_authored_mass_and_limits() {
        let mut pw = PhysicsWorld::new();
        let global_iterations = pw.integration_parameters.num_solver_iterations;
        let unrelated = pw.bodies.insert(RigidBodyBuilder::dynamic().build());
        let mut heavy = ball_body(1, 0.0, 100.0);
        heavy.mass = 40.0;
        let spec = RagdollSpec {
            bodies: vec![heavy, ball_body(2, 50.0, 100.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        assert_eq!(
            pw.integration_parameters.num_solver_iterations,
            global_iterations
        );
        assert_eq!(pw.bodies[unrelated].additional_solver_iterations(), 0);
        for (i, (_, handle, _)) in rag.bodies.iter().enumerate() {
            let body = &pw.bodies[*handle];
            assert_eq!(body.additional_solver_iterations(), 12);
            let mass: f32 = body
                .colliders()
                .iter()
                .map(|h| pw.colliders[*h].mass())
                .sum();
            assert!((mass - spec.bodies[i].mass).abs() < 0.001);
        }
        let (mb, link) = pw.multibody_joints.get(rag.joints[0]).unwrap();
        let actual = &mb.link(link).unwrap().joint.data;
        let expected = build_joint(&spec.constraints[0].joint, false);
        assert_eq!(actual.limit_axes, expected.limit_axes);
        for axis in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ] {
            let actual = actual.limits(axis).unwrap();
            let expected = expected.limits(axis).unwrap();
            assert_eq!([actual.min, actual.max], [expected.min, expected.max]);
        }
    }

    /// #3968 — a ragdoll built on a >60 fps frame (`step` runs zero
    /// substeps: `wake()` armed, `accumulator < PHYSICS_DT`) must still be
    /// queryable: `queue_query_refresh()` is the only mechanism that
    /// reaches the query BVH on a no-substep frame (#2864), and pre-fix
    /// `build_ragdoll` relied on `activate_ragdoll`'s teardown marking the
    /// colliders dirty as a side effect — an accident with a hole
    /// (`bone_handles.is_empty()` skips the teardown).
    #[test]
    fn build_ragdoll_is_queryable_on_a_zero_substep_frame() {
        let mut pw = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 0.0)],
            constraints: vec![],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        let handle = rag.bodies[0].1;

        // The >60 fps shape: wake armed, but the accumulator gate runs no
        // substep — the #2856 pin's exact state.
        assert_eq!(pw.step(PHYSICS_DT / 2.0), 0, "half a tick cannot step yet");

        // The freshly built part's collider must be in the query BVH.
        let pos = body_translation(&pw, handle).unwrap();
        let hit = pw.cast_ray(
            byroredux_core::math::Vec3::new(pos.x, pos.y + 100.0, pos.z),
            byroredux_core::math::Vec3::new(0.0, -1.0, 0.0),
            200.0,
            None,
        );
        assert_eq!(
            hit.and_then(|hit| hit.body),
            Some(handle),
            "a freshly built ragdoll must be queryable on a 0-substep frame (#3968)"
        );
    }

    /// #2860 sibling — `RagdollSpec` already scales joint *pivots* by the
    /// bone's seed scale (`scaled_pivots`), but the limb *shape* was built at
    /// bind size, so a scaled actor's colliders and its articulation
    /// disagreed. Both must follow `b.scale`.
    #[test]
    fn ragdoll_collider_shape_follows_the_bone_seed_scale() {
        let mut pw = PhysicsWorld::new();
        let mut scaled = ball_body(1, 0.0, 0.0);
        scaled.scale = 2.0;
        let spec = RagdollSpec {
            bodies: vec![scaled, ball_body(2, 50.0, 0.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");

        let radius_of = |index: usize| {
            let rb = pw.bodies.get(rag.bodies[index].1).expect("body");
            let handle = rb.colliders()[0];
            pw.colliders
                .get(handle)
                .expect("collider")
                .shape()
                .as_ball()
                .expect("ball")
                .radius
        };
        // `ball_body` authors radius 5.0.
        assert_eq!(radius_of(0), 10.0, "2× bone must build a 2× limb collider");
        assert_eq!(radius_of(1), 5.0, "an unscaled sibling is untouched");
    }

    /// A 3-body horizontal chain hung from a pinned root: under gravity it
    /// swings down (the far body falls) but the multibody joints keep it
    /// connected (its distance from the root stays bounded by the chain
    /// length — a free body would fall away unboundedly). Proves the
    /// build + joints + solver are structurally sound, headless.
    #[test]
    fn ragdoll_chain_swings_but_stays_jointed() {
        let mut pw = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 1000.0),
                ball_body(2, 50.0, 1000.0),
                ball_body(3, 100.0, 1000.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        assert_eq!(rag.bodies.len(), 3);
        assert_eq!(rag.joints.len(), 2, "two multibody joints created");

        // Pin the root so the chain hangs/swings instead of free-falling
        // as a rigid unit (which would preserve distances trivially).
        let h_root = rag.bodies[0].1;
        pw.bodies[h_root].set_body_type(RigidBodyType::Fixed, true);
        pw.wake();

        let root = body_translation(&pw, h_root).unwrap();
        let h_far = rag.bodies[2].1;
        let init_far = body_translation(&pw, h_far).unwrap();
        let init_dist = (init_far - root).length();

        for _ in 0..180 {
            pw.step(PHYSICS_DT);
        }

        let end_far = body_translation(&pw, h_far).unwrap();
        assert!(end_far.is_finite(), "solver exploded: {end_far:?}");
        // Swung/fell under gravity.
        assert!(
            end_far.y < init_far.y - 1.0,
            "far body should fall under gravity: {} → {}",
            init_far.y,
            end_far.y
        );
        // Still jointed — can't drift far beyond the rest chain length.
        let end_dist = (end_far - root).length();
        assert!(
            end_dist < init_dist * 1.5 + 20.0,
            "chain separated (joints not holding): {init_dist} → {end_dist}"
        );
    }

    /// #5353 — a free ragdoll falls at gravity. The articulation-DOF clamp
    /// (#5161) walks every generalized-velocity entry, and a dynamic root's
    /// first six entries are its free joint — three linear BU/s, then three
    /// angular rad/s — not authored joint axes. Capping them at the
    /// joint-axis limit of 100 held every falling corpse to 100 BU/s
    /// (gravity is 686.7 BU/s², so it hit the cap within 0.15 s) and counted
    /// a clamp on every substep.
    #[test]
    fn a_free_ragdoll_falls_at_gravity_without_tripping_the_dof_clamp() {
        let mut pw = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 1000.0), ball_body(2, 50.0, 1000.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        let root = rag.bodies[0].1;
        let start = body_translation(&pw, root).unwrap().y;

        // Half a second of free fall: v = g·t ≈ 343 BU/s, far past the old cap.
        for _ in 0..30 {
            pw.step(PHYSICS_DT);
        }

        let speed = -pw.bodies[root].linvel().y;
        assert!(
            speed > 300.0,
            "a free-falling ragdoll root must reach gravity speed, got {speed} BU/s"
        );
        let fallen = start - body_translation(&pw, root).unwrap().y;
        assert!(fallen > 60.0, "the root should have fallen ~86 BU, fell {fallen}");
        assert_eq!(
            pw.velocity_clamps_total(),
            0,
            "ordinary free fall is not a solver explosion"
        );
    }

    /// A 1-DOF sliding rail along X with pivots ±25, so the rail coordinate
    /// is zero when the bodies sit 50 apart. `travel` bounds the authored
    /// `min_distance`/`max_distance`.
    fn prismatic_rail(a: usize, b: usize, travel: f32) -> RagdollConstraintSpec {
        RagdollConstraintSpec {
            body_a: a,
            body_b: b,
            joint: RagdollJointSpec::Prismatic {
                axis_a: Vec3::X,
                perp_a: Vec3::Y,
                pivot_a: Vec3::new(25.0, 0.0, 0.0),
                axis_b: Vec3::X,
                perp_b: Vec3::Y,
                pivot_b: Vec3::new(-25.0, 0.0, 0.0),
                min_distance: -travel,
                max_distance: travel,
            },
        }
    }

    /// Separation along the rail after one step, for a child seeded `slide`
    /// units past the joint's zero position and twisted `twist` radians about
    /// the rail axis. The twist is what makes this test falsifiable: it is
    /// the quantity the pre-#3962 `ndofs()` dispatch wrote into the linear
    /// coordinate. With `twist == 0` both the broken and the fixed code seed
    /// from a zero angular X and the bug is invisible.
    fn prismatic_separation_after_one_step(slide: f32, twist: f32, travel: f32) -> f32 {
        let mut pw = PhysicsWorld::new();
        pw.gravity = Vector::ZERO;
        let mut child = ball_body(2, 50.0 + slide, 1000.0);
        child.rotation = Quat::from_rotation_x(twist);
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 1000.0), child],
            constraints: vec![prismatic_rail(0, 1, travel)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        pw.step(PHYSICS_DT);
        let root = body_translation(&pw, rag.bodies[0].1).unwrap();
        let child = body_translation(&pw, rag.bodies[1].1).unwrap();
        (child - root).length()
    }

    /// #3962 — `seed_joint_from_body_poses` dispatched on `joint.ndofs()`,
    /// and `LimitedHinge` (free `AngX`) and `Prismatic` (free `LinX`) are
    /// both 1-DOF. Prismatic edges were therefore seeded by writing the twist
    /// angle in radians into the slide coordinate in engine units —
    /// `apply_displacement` walks the linear axes first, so it landed exactly
    /// on the wrong one.
    ///
    /// #3792's own measurement puts 2 of the Protectron skeleton's 12 joints
    /// on this path, and the crate had zero `Prismatic` test coverage: the
    /// production arm in `build_joint` was the only construction site in the
    /// whole crate.
    #[test]
    fn prismatic_seed_uses_the_slide_distance_not_the_twist_angle() {
        // Seeded 10 units down the rail, twisted 0.6 rad about it. Pivots put
        // the rail zero at 50 apart, so the correct answer is 60.
        let preserved = prismatic_separation_after_one_step(10.0, 0.6, 100.0);
        assert!(
            (preserved - 60.0).abs() < 1e-2,
            "the seeded 10-unit slide must survive the first step — got {preserved}. The pre-fix code seeded 0.6 (the twist in radians) and produced ~50.6."
        );

        // The control that makes the assertion above load-bearing: with no
        // twist, the broken dispatch happens to seed 0.0 and the child
        // collapses to the rail zero. Different quantity, same wrong axis.
        let untwisted = prismatic_separation_after_one_step(10.0, 0.0, 100.0);
        assert!(
            (untwisted - 60.0).abs() < 1e-2,
            "the slide must be seeded from the pose regardless of twist — got {untwisted}"
        );
    }

    /// `apply_displacement` writes the reduced coordinate with no regard for
    /// the joint's own limits — `MultibodyJoint::integrate` has no clamp — so
    /// a bind pose outside the authored travel range opens the articulation
    /// already in violation. Rapier resolves that positionally, and a
    /// multibody splits the correction across the whole chain: the *root* is
    /// dragged toward the offending link. Seeding the nearest valid rail
    /// position instead means there is nothing to resolve.
    ///
    /// Measured on the unclamped code with a 1000-unit seed on a ±1 rail: the
    /// root leaves its authored origin for x ≈ 499.5 in a single step. An
    /// actor's ragdoll teleporting ~500 units on activation is the visible
    /// symptom.
    ///
    /// **This asserts the root pose, not the separation.** The separation
    /// converges to 51.0 in one step either way — the two links are pulled
    /// together correctly, just in the wrong place — so a separation
    /// assertion here would pass against the unclamped code and prove
    /// nothing. (Likewise the magnitude: a 10-unit overshoot is absorbed
    /// whole in one step and displaces the root by a fraction of a unit.)
    #[test]
    fn prismatic_seed_clamps_to_the_authored_travel_range() {
        let mut pw = PhysicsWorld::new();
        pw.gravity = Vector::ZERO;
        let mut child = ball_body(2, 1050.0, 1000.0);
        child.rotation = Quat::from_rotation_x(0.6);
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 1000.0), child],
            // Authored travel ±1, seeded 1000 past the rail zero.
            constraints: vec![prismatic_rail(0, 1, 1.0)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        pw.step(PHYSICS_DT);

        let root = body_translation(&pw, rag.bodies[0].1).unwrap();
        assert!(
            (root - Vec3::new(0.0, 1000.0, 0.0)).length() < 1e-2,
            "an out-of-range seed must be clamped, not left for the solver to \
             drag the whole articulation through — root moved to {root:?}"
        );

        // And the clamped rail position is the authored limit: 50 + 1.
        let sep = (body_translation(&pw, rag.bodies[1].1).unwrap() - root).length();
        assert!((sep - 51.0).abs() < 1e-2, "expected 51.0, got {sep}");
    }

    /// #2337 — Rapier's first forward-kinematics pass must start from the
    /// animated child pose, not reset every non-root link to zero joint
    /// coordinates (the authored rest pose).
    #[test]
    fn first_step_preserves_seeded_child_pose() {
        let mut pw = PhysicsWorld::new();
        pw.gravity = Vector::ZERO;

        let root = ball_body(1, 0.0, 1000.0);
        let mut child = ball_body(2, 25.0, 1025.0);
        child.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let expected_translation = child.translation;
        let expected_rotation = child.rotation;
        let spec = RagdollSpec {
            bodies: vec![root, child],
            constraints: vec![loose_ragdoll(0, 1)],
        };

        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        let child_handle = rag.bodies[1].1;
        pw.step(PHYSICS_DT);

        let (actual_translation, actual_rotation) = body_pose(&pw, child_handle).unwrap();
        assert!(
            (actual_translation - expected_translation).length() < 1e-3,
            "first step replaced the seeded child translation: \
             {expected_translation:?} → {actual_translation:?}"
        );
        assert!(
            actual_rotation.dot(expected_rotation).abs() > 1.0 - 1e-4,
            "first step replaced the seeded child rotation: \
             {expected_rotation:?} → {actual_rotation:?}"
        );
    }

    /// #2868 — reproduces the audit's measured probe. A 2x actor seeds its
    /// bodies 100 apart, but the authored pivots are ±25 in bind units (bind
    /// separation 50). Because a ragdoll joint is a *multibody* joint, the
    /// child's translation is not a soft constraint the seed can win: forward
    /// kinematics recomputes it from the frames, so unscaled pivots discard
    /// the seeded pose on the very first step and the ragdoll snaps to bind
    /// proportions.
    ///
    /// Both directions are asserted in one test deliberately. The collapse
    /// arm is the evidence that the scaled arm is actually load-bearing —
    /// `first_step_preserves_seeded_child_pose` passes only because its
    /// hand-written pivots happen to match its body separation at scale 1, so
    /// an assertion on the scaled arm alone would look identical to a test
    /// that never exercised the scaling at all.
    #[test]
    fn scaled_pivots_preserve_a_scaled_actors_seeded_separation() {
        fn separation_after_one_step(joint: RagdollJointSpec) -> f32 {
            let mut pw = PhysicsWorld::new();
            pw.gravity = Vector::ZERO;
            let spec = RagdollSpec {
                bodies: vec![ball_body(1, 0.0, 1000.0), ball_body(2, 100.0, 1000.0)],
                constraints: vec![RagdollConstraintSpec {
                    body_a: 0,
                    body_b: 1,
                    joint,
                }],
            };
            let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
            pw.step(PHYSICS_DT);
            let root = body_translation(&pw, rag.bodies[0].1).unwrap();
            let child = body_translation(&pw, rag.bodies[1].1).unwrap();
            (child - root).length()
        }

        let authored = loose_ragdoll(0, 1).joint;
        let collapsed = separation_after_one_step(authored.clone());
        assert!(
            (collapsed - 50.0).abs() < 1.0,
            "bind-scale pivots should pull the seeded 100 apart back to the \
             authored 50 — got {collapsed}. If this changed, the mechanism \
             #2868 fixes is gone and the assertion below proves nothing."
        );

        let preserved = separation_after_one_step(authored.scaled_pivots(2.0, 2.0));
        assert!(
            (preserved - 100.0).abs() < 1.0,
            "scaled pivots must hold the 2x actor's seeded 100-unit separation \
             through the first step — got {preserved}"
        );
    }

    /// Axes are unit directions defining frame ORIENTATION; scaling them would
    /// corrupt the frame basis. Only the pivots — body-local positions — move.
    #[test]
    fn scaled_pivots_leaves_axes_and_limits_untouched() {
        let scaled = loose_ragdoll(0, 1).joint.scaled_pivots(3.0, 4.0);
        let RagdollJointSpec::Ragdoll {
            twist_a,
            plane_a,
            pivot_a,
            pivot_b,
            cone_max,
            ..
        } = scaled
        else {
            panic!("variant changed");
        };
        assert!(
            (twist_a - Vec3::X).length() < 1e-6,
            "twist axis must not scale"
        );
        assert!(
            (plane_a - Vec3::Y).length() < 1e-6,
            "plane axis must not scale"
        );
        assert!((pivot_a - Vec3::new(75.0, 0.0, 0.0)).length() < 1e-5);
        assert!(
            (pivot_b - Vec3::new(-100.0, 0.0, 0.0)).length() < 1e-5,
            "each side scales by its OWN body's factor"
        );
        assert!(
            (cone_max - std::f32::consts::PI).abs() < 1e-6,
            "limits are angles"
        );
    }

    /// A degenerate scale must leave pivots alone rather than collapsing every
    /// joint onto its parent's origin.
    #[test]
    fn scaled_pivots_rejects_non_positive_factors() {
        for bad in [0.0, -1.0, f32::NAN] {
            let RagdollJointSpec::Ragdoll { pivot_a, .. } =
                loose_ragdoll(0, 1).joint.scaled_pivots(bad, bad)
            else {
                panic!("variant changed");
            };
            assert!(
                (pivot_a - Vec3::new(25.0, 0.0, 0.0)).length() < 1e-6,
                "factor {bad} altered the pivot"
            );
        }
    }

    /// #2338 — overlapping links in one ragdoll must not generate contact
    /// impulses against each other, while the same links still collide with
    /// ordinary world geometry.
    #[test]
    fn ragdoll_self_contacts_are_disabled_but_world_contacts_remain() {
        let mut pw = PhysicsWorld::new();
        pw.gravity = Vector::ZERO;

        let mut root = ball_body(1, 0.0, 1000.0);
        root.shape = CollisionShape::Ball { radius: 30.0 };
        let mut child = ball_body(2, 50.0, 1000.0);
        child.shape = CollisionShape::Ball { radius: 30.0 };
        let spec = RagdollSpec {
            bodies: vec![root, child],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");

        let (multibody, _) = pw.multibody_joints.get(rag.joints[0]).unwrap();
        assert!(
            !multibody.self_contacts_enabled(),
            "ragdoll multibody must reject all same-articulation contacts"
        );

        let root_handle = rag.bodies[0].1;
        let child_handle = rag.bodies[1].1;
        let root_collider = pw.bodies[root_handle].colliders()[0];
        let child_collider = pw.bodies[child_handle].colliders()[0];

        // A fixed floor overlaps the root sphere by one unit. This contact
        // must remain active: disabling self-contact is per multibody, not a
        // broad collision-group mask on every ragdoll collider.
        let floor_body = pw.bodies.insert(
            RigidBodyBuilder::fixed()
                .translation(Vector::new(0.0, 970.0, 0.0))
                .build(),
        );
        let floor_collider = pw.colliders.insert_with_parent(
            ColliderBuilder::cuboid(100.0, 1.0, 100.0).build(),
            floor_body,
            &mut pw.bodies,
        );
        pw.wake();
        pw.step(PHYSICS_DT);

        let self_contact_active = pw
            .narrow_phase
            .contact_pair(root_collider, child_collider)
            .is_some_and(|pair| pair.has_any_active_contact());
        assert!(
            !self_contact_active,
            "overlapping links in one ragdoll generated an active self-contact"
        );
        let world_contact_active = pw
            .narrow_phase
            .contact_pair(root_collider, floor_collider)
            .is_some_and(|pair| pair.has_any_active_contact());
        assert!(
            world_contact_active,
            "self-contact suppression must not disable ragdoll/world contacts"
        );
    }

    /// Forest orientation: two disjoint 2-body chains both build without a
    /// shared root, producing 2 joints total and no panic.
    #[test]
    fn disconnected_forest_orients_each_component() {
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 0.0),
                ball_body(2, 50.0, 0.0),
                ball_body(3, 500.0, 0.0),
                ball_body(4, 550.0, 0.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(2, 3)],
        };
        let edges = orient_tree(&spec);
        assert_eq!(edges.len(), 2, "both components contribute one edge");
    }

    /// A cyclic graph (triangle) drops the back-edge so the multibody tree
    /// stays acyclic.
    #[test]
    fn cycle_drops_back_edge() {
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 0.0),
                ball_body(2, 50.0, 0.0),
                ball_body(3, 100.0, 0.0),
            ],
            constraints: vec![
                loose_ragdoll(0, 1),
                loose_ragdoll(1, 2),
                loose_ragdoll(2, 0),
            ],
        };
        let edges = orient_tree(&spec);
        assert_eq!(edges.len(), 2, "3-body cycle → spanning tree of 2 edges");
    }

    /// #1539 — the forest-detection arithmetic `build_ragdoll` warns on: a
    /// graph with a dropped articulation edge resolves to >1 connected
    /// component, computed as `bodies - tree_edges`. A connected 3-chain is
    /// one component; splitting the middle link (so two disjoint pieces
    /// remain) is two.
    #[test]
    fn forest_is_detected_by_edge_deficit() {
        // Connected: 3 bodies, 2 edges → 1 component (a single tree).
        let connected = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 0.0),
                ball_body(2, 50.0, 0.0),
                ball_body(3, 100.0, 0.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let comps = connected.bodies.len() - orient_tree(&connected).len();
        assert_eq!(comps, 1, "a connected chain is a single tree");

        // Fragmented: the sole link to body 2 is gone → {0-1} and {2}.
        let forest = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 0.0),
                ball_body(2, 50.0, 0.0),
                ball_body(3, 100.0, 0.0),
            ],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let comps = forest.bodies.len() - orient_tree(&forest).len();
        assert_eq!(comps, 2, "a dropped sole-link edge surfaces as a forest");
        assert!(comps > 1, "build_ragdoll warns when components > 1");
    }

    fn tetra(scale: f32) -> CollisionShape {
        CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(scale, 0.0, 0.0),
                Vec3::new(0.0, scale, 0.0),
                Vec3::new(0.0, 0.0, scale),
            ],
            indices: vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        }
    }

    /// #1540 — a `TriMesh` ragdoll-body shape is substituted with a convex
    /// hull (closed solid → well-defined inertia); primitives and compounds
    /// pass through, with nested trimesh leaves substituted.
    #[test]
    fn trimesh_ragdoll_shape_substituted_with_convex_hull() {
        match ragdoll_dynamic_shape(&tetra(10.0)) {
            CollisionShape::ConvexHull { vertices } => assert_eq!(vertices.len(), 4),
            other => panic!("TriMesh must become ConvexHull, got {other:?}"),
        }
        // Primitives untouched.
        assert!(matches!(
            ragdoll_dynamic_shape(&CollisionShape::Ball { radius: 5.0 }),
            CollisionShape::Ball { .. }
        ));
        // Compound recurses: a trimesh leaf inside a compound is substituted.
        let compound = CollisionShape::Compound {
            children: vec![(Vec3::ZERO, Quat::IDENTITY, Box::new(tetra(10.0)))],
        };
        match ragdoll_dynamic_shape(&compound) {
            CollisionShape::Compound { children } => {
                assert!(matches!(
                    children[0].2.as_ref(),
                    CollisionShape::ConvexHull { .. }
                ));
            }
            other => panic!("Compound must stay a Compound, got {other:?}"),
        }
    }

    /// #1540 — building a ragdoll body from a `TriMesh` shape yields a
    /// finite, non-degenerate principal-inertia tensor (via the convex-hull
    /// substitution), not the degenerate one a raw open trimesh would give.
    #[test]
    fn trimesh_ragdoll_body_has_finite_nondegenerate_inertia() {
        let mut pw = PhysicsWorld::new();
        let mut body = ball_body(1, 0.0, 0.0);
        body.shape = tetra(20.0);
        let spec = RagdollSpec {
            bodies: vec![body],
            constraints: vec![],
        };
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        let h = rag.bodies[0].1;
        let pi = pw.bodies[h]
            .mass_properties()
            .local_mprops
            .principal_inertia();
        assert!(
            pi.x.is_finite() && pi.y.is_finite() && pi.z.is_finite(),
            "principal inertia must be finite: {pi:?}"
        );
        assert!(
            pi.x > 0.0 && pi.y > 0.0 && pi.z > 0.0,
            "principal inertia must be non-degenerate: {pi:?}"
        );
    }

    /// Regression for #2448 / PHYS-02: `build_joint`'s `LimitedHinge` arm
    /// must build its angle-limit frame from the authored `perp_a`/`perp_b`
    /// zero-reference, not a synthesized-arbitrary one. Picks a perp
    /// deliberately different from what `any_perp` would choose for the
    /// same axis, so a regression back to the old `any_perp(axis)` call
    /// would produce a visibly different frame and fail this assertion.
    #[test]
    fn limited_hinge_frame_uses_authored_perp_not_synthesized() {
        let axis = Vec3::X;
        // `any_perp(X)`: `a.x.abs() = 1.0 >= 0.9` ⇒ seed = Y ⇒ picks ~Y.
        // Author a perp that's ~Z instead — orthogonal to both X and the
        // `any_perp` fallback, so the two frames are unambiguously distinct.
        let authored_perp = Vec3::Z;

        let joint = build_joint(
            &RagdollJointSpec::LimitedHinge {
                axis_a: axis,
                perp_a: authored_perp,
                pivot_a: Vec3::ZERO,
                axis_b: axis,
                perp_b: authored_perp,
                pivot_b: Vec3::ZERO,
                min_angle: -1.0,
                max_angle: 1.0,
            },
            false,
        );

        let built_rotation = joint.local_frame1.rotation;
        let expected = frame_rot(axis, authored_perp);
        let synthesized = frame_rot(axis, any_perp(axis));

        // Same-or-flipped-sign match against the authored frame (quaternion
        // double-cover — same rotation can have either sign).
        let matches_expected = built_rotation.dot(expected).abs() > 1.0 - 1e-5;
        assert!(
            matches_expected,
            "local_frame1 must be built from the authored perp: {built_rotation:?} vs \
             {expected:?}"
        );
        // And it must NOT match the arbitrary synthesized fallback — proves
        // the authored perp is actually reaching `build_joint`, not silently
        // falling through to `any_perp` regardless of input.
        let matches_synthesized = built_rotation.dot(synthesized).abs() > 1.0 - 1e-5;
        assert!(
            !matches_synthesized,
            "local_frame1 must NOT match the any_perp-synthesized frame — the authored perp \
             isn't reaching build_joint: {built_rotation:?}"
        );
    }

    /// #2884 — `remove_ragdoll` had **zero** test coverage: three grep hits
    /// (definition, one doc reference, one production call site in
    /// `cell_loader/unload.rs`) and nothing exercising it. The one adjacent
    /// test, `reactivating_ragdoll_does_not_leak_previous_bodies`, covers the
    /// #2083 double-*activate* path, not the build→remove cycle #1531 was
    /// filed against.
    ///
    /// A **branching** tree (not a chain) is deliberate: the multibody joint
    /// set indexes differently for a body with two children, so a linear spec
    /// would miss a whole class of arena drift. Repeated so a leak that only
    /// shows on reuse of freed arena slots surfaces.
    #[test]
    fn build_remove_cycle_leaves_no_bodies_colliders_or_multibodies() {
        let mut pw = PhysicsWorld::new();

        // root ─┬─ 1 ─┬─ 3        two children at two levels, so at least one
        //       │     └─ 4        body carries >1 outgoing joint
        //       └─ 2 ─┬─ 5
        //             └─ 6
        let spec = RagdollSpec {
            bodies: (0..7)
                .map(|i| ball_body(i as EntityId, i as f32 * 30.0, 100.0))
                .collect(),
            constraints: vec![
                loose_ragdoll(0, 1),
                loose_ragdoll(0, 2),
                loose_ragdoll(1, 3),
                loose_ragdoll(1, 4),
                loose_ragdoll(2, 5),
                loose_ragdoll(2, 6),
            ],
        };

        for cycle in 0..3 {
            let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
            assert_eq!(
                rag.bodies.len(),
                7,
                "cycle {cycle}: every spec body must be built"
            );
            assert_eq!(pw.body_count(), 7, "cycle {cycle}: bodies live after build");

            pw.step(PHYSICS_DT);
            pw.remove_ragdoll(&rag);

            assert_eq!(
                pw.body_count(),
                0,
                "cycle {cycle}: remove_ragdoll must drop every body (#1531)"
            );
            assert_eq!(
                pw.colliders.len(),
                0,
                "cycle {cycle}: colliders must cascade out with their bodies"
            );
            assert_eq!(
                pw.multibody_joints.multibodies().count(),
                0,
                "cycle {cycle}: multibody joints must cascade out too — a \
                 stranded multibody is the #1531 leak shape"
            );
        }
    }

    /// #2884 — `ragdoll_extra_angular_damping` is documented in
    /// `physal.md` §4 as the biggest "less floppy than Havok" lever, and it is
    /// added in the *body* loop. Nothing pinned that it lands once per body
    /// rather than once per constraint, so a refactor moving it into the joint
    /// loop would silently double it on any body with two joints.
    #[test]
    fn extra_angular_damping_is_added_once_per_body_not_once_per_joint() {
        let mut pw = PhysicsWorld::new();
        let cfg = ContactConfig {
            ragdoll_extra_angular_damping: 0.75,
            ..ContactConfig::DEFAULT
        };

        // Body 0 carries two joints; bodies 1 and 2 carry one each. If the
        // extra were applied per joint, body 0 would land at authored + 1.50.
        let spec = RagdollSpec {
            bodies: (0..3)
                .map(|i| ball_body(i as EntityId, i as f32 * 30.0, 100.0))
                .collect(),
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(0, 2)],
        };

        let rag = build_ragdoll(&mut pw, &spec, &cfg).expect("sane ragdoll seed");
        let authored = 0.05_f32; // `ball_body`'s angular_damping

        for (idx, (_, h, _)) in rag.bodies.iter().enumerate() {
            let actual = pw.bodies[*h].angular_damping();
            assert!(
                (actual - (authored + 0.75)).abs() < 1e-5,
                "body {idx}: expected authored {authored} + extra 0.75 = {}, got {actual} \
                 (per-joint application would give {} on the two-joint body)",
                authored + 0.75,
                authored + 1.5,
            );
        }
    }
    /// #5161 — an articulation's exploding REDUCED-COORDINATE velocity is
    /// invisible to the body-speed cap (forward kinematics integrates it
    /// before any body-level clamp runs, and rapier's in-step speed cap
    /// covers rigid bodies only) and teleports its links out of the world in
    /// one step. The DOF clamp must cap it — and
    /// zero non-finite DOFs — before the next substep can integrate it.
    /// An articulation that blows up while RESTING ON A FLOOR is quarantined
    /// by rapier (its multibody solve goes non-finite), restored once, and
    /// stays restored. The P1 smoke run caught the regression this pins: the
    /// restore re-enabled the bodies in the same frame rapier disabled them,
    /// so the disable was never processed, the bodies kept their stale
    /// per-body solver state, and every later step that woke them through
    /// their floor contact quarantined all of them again — 253 restores in
    /// ten seconds on a Whiterun corpse, each one forfeiting the frame's
    /// physics catch-up. In free space (no contact to wake them) the same
    /// bug is invisible, hence the floor.
    #[test]
    fn a_quarantined_rig_resting_on_a_floor_recovers_once() {
        let mut w = PhysicsWorld::new();
        let cfg = ContactConfig::DEFAULT;
        let floor = w.bodies.insert(
            RigidBodyBuilder::fixed().translation(Vector::new(50.0, 1000.0 - 15.0 - 10.0, 0.0)),
        );
        w.colliders.insert_with_parent(
            ColliderBuilder::cuboid(500.0, 10.0, 500.0).build(),
            floor,
            &mut w.bodies,
        );
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 1000.0),
                ball_body(2, 50.0, 1000.0),
                ball_body(3, 100.0, 1000.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let rag = build_ragdoll(&mut w, &spec, &cfg).expect("sane ragdoll seed");
        for _ in 0..30 {
            w.wake();
            w.step(PHYSICS_DT);
        }
        assert_eq!(w.recovery_counts().0, 0, "fixture: the rig settles cleanly");

        // 150 rad/s on every DOF: rapier 0.36's multibody solve goes
        // non-finite inside the step and quarantines all three links.
        {
            let (multibody, _) = w.multibody_joints.get_mut(rag.joints[0]).expect("live joint");
            for v in multibody.generalized_velocity_mut().iter_mut() {
                *v = 150.0;
            }
        }
        for _ in 0..8 {
            // Wake the rig every step, as a walking player brushing past
            // does through the shared contact.
            for &(_, h, _) in &rag.bodies {
                w.bodies.get_mut(h).expect("live link").wake_up(true);
            }
            w.wake();
            w.step(PHYSICS_DT);
        }
        assert_eq!(
            w.recovery_counts().0,
            1,
            "one explosion, one recovery — a re-quarantine every woken step is the regression"
        );
        for &(_, h, _) in &rag.bodies {
            let body = &w.bodies[h];
            assert!(body.is_enabled(), "every link is back in the simulation");
            assert!(body.translation().is_finite() && body.linvel().is_finite());
        }
    }

    /// #5353 — the free root's linear DOFs still have a ceiling: the
    /// body-level `VELOCITY_SANITY_CAP_BU_PER_S`, not the 100 of the joint
    /// axes. A root launched past it is clamped and counted.
    #[test]
    fn an_exploding_free_root_dof_is_capped_at_the_body_speed_cap() {
        use crate::world::VELOCITY_SANITY_CAP_BU_PER_S;
        let mut w = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 1000.0), ball_body(2, 50.0, 1000.0)],
            constraints: vec![loose_ragdoll(0, 1)],
        };
        let rag = build_ragdoll(&mut w, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        // Root linear X: over the body cap (20 000) but under rapier's
        // in-step cap (28 000), so it survives the step to reach the clamp.
        let launched = VELOCITY_SANITY_CAP_BU_PER_S * 1.25;
        {
            let (multibody, _) = w.multibody_joints.get_mut(rag.joints[0]).expect("live joint");
            multibody.generalized_velocity_mut()[0] = launched;
        }
        w.step(PHYSICS_DT);
        assert!(w.velocity_clamps_total() >= 1, "the exploding root DOF must be clamped and counted");
        let (multibody, _) = w.multibody_joints.get_mut(rag.joints[0]).expect("live joint");
        let root_x = multibody.generalized_velocity_mut()[0];
        assert!(
            root_x.abs() <= VELOCITY_SANITY_CAP_BU_PER_S,
            "the root's linear DOF must end the substep at or under the body cap, got {root_x}"
        );
    }

    /// #5531 — the DOF clamp names the rig it clamped. The warning used to
    /// carry only a count, so a per-substep flood could not be traced to a
    /// body; the walk now reports `(root link body, clamped DOFs)` once per
    /// multibody (a rig's joints all resolve to the same multibody), and the
    /// root body is the key into the labels the body-level clamp already uses.
    #[test]
    fn the_dof_clamp_attributes_each_rig_by_its_root_body() {
        use crate::world::VELOCITY_SANITY_CAP_BU_PER_S;
        let mut w = PhysicsWorld::new();
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 1000.0),
                ball_body(2, 50.0, 1000.0),
                ball_body(3, 100.0, 1000.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let rag = build_ragdoll(&mut w, &spec, &ContactConfig::DEFAULT).expect("sane ragdoll seed");
        assert_eq!(rag.joints.len(), 2, "fixture: two joints, one multibody");
        assert!(w.clamp_articulation_dofs().is_empty(), "a rig at rest clamps nothing");

        {
            let (multibody, _) = w.multibody_joints.get_mut(rag.joints[0]).expect("live joint");
            multibody.generalized_velocity_mut()[0] = VELOCITY_SANITY_CAP_BU_PER_S * 1.25;
        }
        let clamped = w.clamp_articulation_dofs();
        assert_eq!(
            clamped,
            vec![(rag.bodies[0].1, 1)],
            "one multibody, attributed to its root link, with the one DOF it clamped"
        );
        assert!(
            w.clamp_articulation_dofs().is_empty(),
            "the clamp is idempotent: nothing is left to report"
        );
    }

    #[test]
    fn exploding_articulation_dofs_are_capped_before_forward_kinematics() {
        let mut w = PhysicsWorld::new();
        let cfg = ContactConfig::DEFAULT;
        let spec = RagdollSpec {
            bodies: vec![
                ball_body(1, 0.0, 1000.0),
                ball_body(2, 50.0, 1000.0),
                ball_body(3, 100.0, 1000.0),
            ],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let rag = build_ragdoll(&mut w, &spec, &cfg).expect("sane ragdoll seed");
        assert_eq!(rag.joints.len(), 2, "both joints built");

        // Inject an explosive generalized velocity straight into the
        // multibody state — the same state forward kinematics integrates.
        // ONE DOF (the leaf joint's last axis) at 600 rad/s: over the DOF
        // cap, and finite through the step. Writing 600 rad/s into EVERY
        // DOF of this chain is a different case under rapier 0.36 — its
        // multibody solve goes non-finite inside the step and the bodies are
        // quarantined, which the restore path owns
        // (`world::tests::a_quarantined_body_is_re_enabled_and_parked`) — so
        // it would not reach the clamp under test at all.
        {
            let (multibody, _) = w
                .multibody_joints
                .get_mut(rag.joints[0])
                .expect("live joint");
            let mut vels = multibody.generalized_velocity_mut();
            let leaf_dof = vels.len() - 1;
            vels[leaf_dof] = 600.0;
        }

        // The first substep still integrates the pre-clamp value — the
        // clamp runs at the end of the substep and bounds the NEXT one.
        w.step(PHYSICS_DT);
        assert!(
            w.velocity_clamps_total() >= 1,
            "the exploding DOF must be clamped (and counted)"
        );
        let pre: Vec<Vec3> = rag
            .bodies
            .iter()
            .filter_map(|&(_, h, _)| body_translation(&w, h))
            .collect();
        w.step(PHYSICS_DT);
        for (i, &(_, h, _)) in rag.bodies.iter().enumerate() {
            let post = body_translation(&w, h).expect("live link");
            let moved = (post - pre[i]).length();
            let bound = 200.0; // 50-BU limb radius × 100 rad/s × dt ≈ 83; 200 is headroom.
            let pre_dbg = pre[i];
            assert!(
                moved <= bound,
                "link {i} must not teleport under a capped DOF: {pre_dbg:?} → {post:?}"
            );
        }
    }
}
