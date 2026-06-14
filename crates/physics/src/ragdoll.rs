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
use crate::convert::{collision_shape_to_parts, iso_from_trs, quat_from_na, vec3_from_na};
use crate::world::PhysicsWorld;
use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::math::{Mat3, Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::VecDeque;

/// One rigid body of a ragdoll, already resolved to engine world space.
#[derive(Debug, Clone)]
pub struct RagdollBodySpec {
    /// The skeleton bone entity this body drives (for writeback).
    pub entity: EntityId,
    /// World-space seed pose (bone world × body-local offset).
    pub translation: Vec3,
    pub rotation: Quat,
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
        pivot_a: Vec3,
        axis_b: Vec3,
        pivot_b: Vec3,
        min_angle: f32,
        max_angle: f32,
    },
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

/// Build the ragdoll into `pw` and return the [`Ragdoll`] component for
/// the actor. Creates one dynamic body + collider per spec body, orients
/// the constraint graph into a tree, and inserts a multibody joint per
/// edge. Calls [`PhysicsWorld::wake`] so the first step simulates it.
pub fn build_ragdoll(pw: &mut PhysicsWorld, spec: &RagdollSpec, cfg: &ContactConfig) -> Ragdoll {
    // 1. Rigid bodies + colliders.
    let mut handles: Vec<RigidBodyHandle> = Vec::with_capacity(spec.bodies.len());
    for b in &spec.bodies {
        let body = RigidBodyBuilder::dynamic()
            .position(iso_from_trs(b.translation, b.rotation))
            .linear_damping(b.linear_damping.max(0.0))
            // "less floppy than Havok" lever — extra angular damping on top
            // of the authored value (inert at the 0.0 default). See
            // ContactConfig::ragdoll_extra_angular_damping.
            .angular_damping(b.angular_damping.max(0.0) + cfg.ragdoll_extra_angular_damping.max(0.0))
            .build();
        let h = pw.bodies.insert(body);

        let parts = collision_shape_to_parts(&b.shape, cfg);
        let part_mass = b.mass.max(1e-3) / parts.len() as f32;
        let PhysicsWorld {
            ref mut bodies,
            ref mut colliders,
            ..
        } = *pw;
        for (iso, shape) in parts {
            let col = ColliderBuilder::new(shape)
                .position(iso)
                .friction(b.friction.max(0.0))
                .restitution(b.restitution.clamp(0.0, 1.0))
                .mass(part_mass)
                .build();
            colliders.insert_with_parent(col, h, bodies);
        }
        handles.push(h);
    }

    // 2. Orient the (undirected) constraint graph into a parent→child tree
    //    via BFS, handling a forest if the graph is disconnected. Back-edges
    //    (which would form a loop multibody can't represent) are dropped.
    let oriented = orient_tree(spec);

    // 3. Insert one multibody joint per tree edge (parent already in the
    //    multibody from BFS order; the root is the multibody base).
    let mut joints = Vec::with_capacity(oriented.len());
    for edge in &oriented {
        let joint = build_joint(&spec.constraints[edge.constraint].joint, edge.flip);
        if let Some(jh) =
            pw.multibody_joints
                .insert(handles[edge.parent], handles[edge.child], joint, true)
        {
            joints.push(jh);
        } else {
            log::warn!(
                "ragdoll: multibody joint {}→{} rejected (would form a loop?) — skipped",
                edge.parent,
                edge.child,
            );
        }
    }

    pw.wake();

    Ragdoll {
        bodies: spec
            .bodies
            .iter()
            .map(|b| b.entity)
            .zip(handles)
            .collect(),
        joints,
    }
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
                    *twist_a, *plane_a, *pivot_a, *twist_b, *plane_b, *pivot_b, *twist_min, *twist_max,
                )
            } else {
                (
                    *twist_b, *plane_b, *pivot_b, *twist_a, *plane_a, *pivot_a, -*twist_max,
                    -*twist_min,
                )
            };
            let cone = cone_max.abs();
            GenericJointBuilder::new(lin_locked())
                .local_frame1(iso_from_trs(pv1, frame_rot(t1, p1)))
                .local_frame2(iso_from_trs(pv2, frame_rot(t2, p2)))
                .limits(JointAxis::AngX, [tmin, tmax]) // twist
                .limits(JointAxis::AngY, [-cone, cone]) // swing
                .limits(JointAxis::AngZ, [-cone, cone]) // swing
                .build()
        }
        RagdollJointSpec::LimitedHinge {
            axis_a,
            pivot_a,
            axis_b,
            pivot_b,
            min_angle,
            max_angle,
        } => {
            let (a1, pv1, a2, pv2, amin, amax) = if !flip {
                (*axis_a, *pivot_a, *axis_b, *pivot_b, *min_angle, *max_angle)
            } else {
                (*axis_b, *pivot_b, *axis_a, *pivot_a, -*max_angle, -*min_angle)
            };
            // No authored perp on the hinge spec (slice 1) — synthesize one
            // orthogonal to the axis. The hinge still rotates about the
            // correct axis; only the limit's zero-reference is offset.
            GenericJointBuilder::new(
                lin_locked() | JointAxesMask::ANG_Y | JointAxesMask::ANG_Z,
            )
            .local_frame1(iso_from_trs(pv1, frame_rot(a1, any_perp(a1))))
            .local_frame2(iso_from_trs(pv2, frame_rot(a2, any_perp(a2))))
            .limits(JointAxis::AngX, [amin, amax])
            .build()
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
        for (_, h) in &ragdoll.bodies {
            self.remove_body(*h);
        }
    }
}

/// Helper for callers/tests: a body's current world translation, if live.
pub fn body_translation(pw: &PhysicsWorld, h: RigidBodyHandle) -> Option<Vec3> {
    pw.bodies.get(h).map(|b| vec3_from_na(*b.translation()))
}

/// Helper for the per-frame writeback: a body's current world
/// (translation, rotation), if live.
pub fn body_pose(pw: &PhysicsWorld, h: RigidBodyHandle) -> Option<(Vec3, Quat)> {
    pw.bodies.get(h).map(|b| {
        let iso = b.position();
        (vec3_from_na(iso.translation.vector), quat_from_na(iso.rotation))
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
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT);
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
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2), loose_ragdoll(2, 0)],
        };
        let edges = orient_tree(&spec);
        assert_eq!(edges.len(), 2, "3-body cycle → spanning tree of 2 edges");
    }
}
