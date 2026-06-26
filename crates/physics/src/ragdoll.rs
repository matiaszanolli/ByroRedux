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

        // #1540 — substitute a convex hull for any TriMesh on this *dynamic*
        // ragdoll body; a raw trimesh gives Rapier a degenerate inertia
        // tensor even with the `.mass()` override below. See
        // `ragdoll_dynamic_shape`.
        let parts = collision_shape_to_parts(&ragdoll_dynamic_shape(&b.shape), cfg);
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

    /// #1539 — the forest-detection arithmetic `build_ragdoll` warns on: a
    /// graph with a dropped articulation edge resolves to >1 connected
    /// component, computed as `bodies - tree_edges`. A connected 3-chain is
    /// one component; splitting the middle link (so two disjoint pieces
    /// remain) is two.
    #[test]
    fn forest_is_detected_by_edge_deficit() {
        // Connected: 3 bodies, 2 edges → 1 component (a single tree).
        let connected = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 0.0), ball_body(2, 50.0, 0.0), ball_body(3, 100.0, 0.0)],
            constraints: vec![loose_ragdoll(0, 1), loose_ragdoll(1, 2)],
        };
        let comps = connected.bodies.len() - orient_tree(&connected).len();
        assert_eq!(comps, 1, "a connected chain is a single tree");

        // Fragmented: the sole link to body 2 is gone → {0-1} and {2}.
        let forest = RagdollSpec {
            bodies: vec![ball_body(1, 0.0, 0.0), ball_body(2, 50.0, 0.0), ball_body(3, 100.0, 0.0)],
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
                assert!(matches!(children[0].2.as_ref(), CollisionShape::ConvexHull { .. }));
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
        let rag = build_ragdoll(&mut pw, &spec, &ContactConfig::DEFAULT);
        let h = rag.bodies[0].1;
        let pi = pw.bodies[h].mass_properties().local_mprops.principal_inertia();
        assert!(
            pi.x.is_finite() && pi.y.is_finite() && pi.z.is_finite(),
            "principal inertia must be finite: {pi:?}"
        );
        assert!(
            pi.x > 0.0 && pi.y > 0.0 && pi.z > 0.0,
            "principal inertia must be non-degenerate: {pi:?}"
        );
    }
}
