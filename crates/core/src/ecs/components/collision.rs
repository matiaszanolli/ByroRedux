//! Collision shape and rigid body components — physics-agnostic.
//!
//! These components carry collision geometry and physics properties extracted
//! from NIF bhk blocks. They map 1:1 to Rapier collider/body types but use
//! only engine types (glam), keeping `core` free of Rapier dependencies.
//!
//! SparseSetStorage: not every entity has collision data.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::{Quat, Vec3};

/// Physics-agnostic collision shape.
///
/// Coordinates are in engine space (Y-up), Gamebryo units.
/// Variants map to Rapier collider constructors:
/// - `Ball` → `ColliderBuilder::ball(radius)`
/// - `Cuboid` → `ColliderBuilder::cuboid(hx, hy, hz)`
/// - `Capsule` → `ColliderBuilder::capsule_y(half_height, radius)`
/// - `ConvexHull` → `ColliderBuilder::convex_hull(&vertices)`
/// - `TriMesh` → `ColliderBuilder::trimesh(vertices, indices)`
/// - `Compound` → `ColliderBuilder::compound(children)`
#[derive(Debug, Clone)]
pub enum CollisionShape {
    Ball {
        radius: f32,
    },
    Cuboid {
        half_extents: Vec3,
    },
    Capsule {
        half_height: f32,
        radius: f32,
    },
    Cylinder {
        half_height: f32,
        radius: f32,
    },
    ConvexHull {
        vertices: Vec<Vec3>,
    },
    TriMesh {
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
    },
    Compound {
        children: Vec<(Vec3, Quat, Box<CollisionShape>)>,
    },
}

impl CollisionShape {
    /// Uniformly scale the geometry about the shape's own origin.
    ///
    /// Collision shapes are authored in a NIF's bind space, but a placed
    /// instance carries a uniform `GlobalTransform.scale` (REFR `XSCL`, or a
    /// non-unit node scale on the skeleton chain). Rapier colliders have no
    /// scale of their own — the geometry itself has to be resized — so any
    /// site that turns an authored shape into a collider for a scaled
    /// instance must pass it through here first, or the collider silently
    /// keeps bind-scale proportions (#2868).
    ///
    /// `Compound` child *offsets* scale along with the child geometry;
    /// child rotations are unaffected. A non-finite or non-positive factor is
    /// treated as "don't scale" rather than collapsing the shape into a
    /// zero-volume collider, which parry cannot build a sensible inertia
    /// tensor from.
    pub fn scaled(&self, scale: f32) -> Self {
        if !scale.is_finite() || scale <= 0.0 {
            return self.clone();
        }
        match self {
            Self::Ball { radius } => Self::Ball {
                radius: radius * scale,
            },
            Self::Cuboid { half_extents } => Self::Cuboid {
                half_extents: *half_extents * scale,
            },
            Self::Capsule {
                half_height,
                radius,
            } => Self::Capsule {
                half_height: half_height * scale,
                radius: radius * scale,
            },
            Self::Cylinder {
                half_height,
                radius,
            } => Self::Cylinder {
                half_height: half_height * scale,
                radius: radius * scale,
            },
            Self::ConvexHull { vertices } => Self::ConvexHull {
                vertices: vertices.iter().map(|v| *v * scale).collect(),
            },
            Self::TriMesh { vertices, indices } => Self::TriMesh {
                vertices: vertices.iter().map(|v| *v * scale).collect(),
                indices: indices.clone(),
            },
            Self::Compound { children } => Self::Compound {
                children: children
                    .iter()
                    .map(|(translation, rotation, child)| {
                        (
                            *translation * scale,
                            *rotation,
                            Box::new(child.scaled(scale)),
                        )
                    })
                    .collect(),
            },
        }
    }
}

impl Component for CollisionShape {
    type Storage = SparseSetStorage<Self>;
}

/// Rigid body motion type — controls how the physics engine treats the body.
///
/// Maps onto Rapier's `RigidBodyType` at the physics layer; the
/// `CharacterKinematic` variant maps to the same `KinematicPositionBased`
/// type as `Keyframed` but signals to the spawn site that the body
/// represents an upright character capsule (rotations locked, driven
/// manually by `character_controller_system` via KCC `move_shape`
/// rather than tracking a `GlobalTransform`). The split exists so
/// `physics_sync_system::push_kinematic` doesn't try to write a
/// transform-derived pose onto a character body each frame — only
/// `Keyframed` bodies (doors, platforms, scripted props) take that path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum MotionType {
    /// Fixed in place, infinite mass (walls, floors, static architecture).
    Static,
    /// Moved by animation/script, not by physics forces (doors, platforms).
    /// Pushed each frame from the entity's `GlobalTransform`.
    Keyframed,
    /// Fully simulated by the physics engine (crates, bottles, debris).
    Dynamic,
    /// Kinematic body whose pose is driven by the character-controller
    /// system, not by the ECS transform. Rotations locked. The sync
    /// system registers it but does NOT push poses from
    /// `GlobalTransform`; the controller calls
    /// `set_kinematic_translation` explicitly each frame.
    CharacterKinematic,
}

/// Rigid body properties extracted from bhkRigidBody.
///
/// These feed Rapier `RigidBodyBuilder` configuration.
/// `motion_type` determines the Rapier body type:
/// - `Static` → `RigidBodyBuilder::fixed()`
/// - `Keyframed` → `RigidBodyBuilder::kinematic_position_based()`
/// - `Dynamic` → `RigidBodyBuilder::dynamic()`
///
/// #2379 (SAVE-D1-14) — `motion_type` is mutated at runtime by Papyrus
/// `.SetMotionType()` (`scripted_motion_type_system`), not just seeded
/// once from static bhkRigidBody data as this doc previously implied.
/// Registered in `byroredux::save_io::build_save_registry` (and
/// `MUTABLE_DELTA_COLUMNS`) so a scripted motion-type change survives a
/// save/load instead of reverting to the ESM-derived default.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct RigidBodyData {
    pub motion_type: MotionType,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
}

impl RigidBodyData {
    /// Default static body (walls, architecture).
    pub const STATIC: Self = Self {
        motion_type: MotionType::Static,
        mass: 0.0,
        friction: 0.5,
        restitution: 0.3,
        linear_damping: 0.0,
        angular_damping: 0.0,
    };
}

impl Default for RigidBodyData {
    fn default() -> Self {
        Self::STATIC
    }
}

impl Component for RigidBodyData {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_shape_ball() {
        let shape = CollisionShape::Ball { radius: 1.5 };
        match shape {
            CollisionShape::Ball { radius } => assert!((radius - 1.5).abs() < 1e-6),
            _ => panic!("wrong variant"),
        }
    }

    /// #2868 — every variant's extents must scale, or a scaled instance gets
    /// bind-size colliders. Covered variant-by-variant because a `match` arm
    /// that forgets one field (a `Capsule`'s radius, say) still compiles and
    /// still produces a plausible-looking shape.
    #[test]
    fn scaled_resizes_every_primitive_variant() {
        match (CollisionShape::Ball { radius: 3.0 }).scaled(2.0) {
            CollisionShape::Ball { radius } => assert!((radius - 6.0).abs() < 1e-6),
            other => panic!("variant changed: {other:?}"),
        }
        match (CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 4.0),
        })
        .scaled(3.0)
        {
            CollisionShape::Cuboid { half_extents } => {
                assert!((half_extents - Vec3::new(3.0, 6.0, 12.0)).length() < 1e-5);
            }
            other => panic!("variant changed: {other:?}"),
        }
        match (CollisionShape::Capsule {
            half_height: 10.0,
            radius: 2.0,
        })
        .scaled(2.5)
        {
            CollisionShape::Capsule {
                half_height,
                radius,
            } => {
                assert!((half_height - 25.0).abs() < 1e-5);
                assert!((radius - 5.0).abs() < 1e-5, "radius must scale too");
            }
            other => panic!("variant changed: {other:?}"),
        }
        match (CollisionShape::Cylinder {
            half_height: 10.0,
            radius: 2.0,
        })
        .scaled(0.5)
        {
            CollisionShape::Cylinder {
                half_height,
                radius,
            } => {
                assert!((half_height - 5.0).abs() < 1e-5);
                assert!((radius - 1.0).abs() < 1e-5);
            }
            other => panic!("variant changed: {other:?}"),
        }
    }

    /// Vertex soups scale their points; the index buffer is topology and must
    /// pass through untouched.
    #[test]
    fn scaled_resizes_vertices_and_preserves_topology() {
        let mesh = CollisionShape::TriMesh {
            vertices: vec![
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 2.0, 0.0),
            ],
            indices: vec![[0, 1, 2]],
        };
        match mesh.scaled(4.0) {
            CollisionShape::TriMesh { vertices, indices } => {
                assert!((vertices[1] - Vec3::new(4.0, 0.0, 0.0)).length() < 1e-5);
                assert!((vertices[2] - Vec3::new(0.0, 8.0, 0.0)).length() < 1e-5);
                assert_eq!(indices, vec![[0, 1, 2]]);
            }
            other => panic!("variant changed: {other:?}"),
        }
    }

    /// A compound's child OFFSETS are positions in the parent frame, so they
    /// scale with the geometry; child rotations are orientation and do not.
    /// Scaling only the leaves would leave the parts correctly sized but
    /// bunched at bind-scale spacing.
    #[test]
    fn scaled_compound_scales_child_offsets_and_recurses() {
        let compound = CollisionShape::Compound {
            children: vec![(
                Vec3::new(10.0, 0.0, 0.0),
                Quat::from_rotation_y(0.5),
                Box::new(CollisionShape::Ball { radius: 1.0 }),
            )],
        };
        match compound.scaled(2.0) {
            CollisionShape::Compound { children } => {
                let (translation, rotation, child) = &children[0];
                assert!((*translation - Vec3::new(20.0, 0.0, 0.0)).length() < 1e-5);
                assert!(rotation.dot(Quat::from_rotation_y(0.5)).abs() > 1.0 - 1e-6);
                match **child {
                    CollisionShape::Ball { radius } => assert!((radius - 2.0).abs() < 1e-6),
                    _ => panic!("child variant changed"),
                }
            }
            other => panic!("variant changed: {other:?}"),
        }
    }

    /// A degenerate factor must leave the shape alone rather than collapse it
    /// to zero volume — parry cannot derive a usable inertia tensor from that,
    /// and the caller would get a silently broken collider instead of a
    /// bind-scale one.
    #[test]
    fn scaled_rejects_non_positive_and_non_finite_factors() {
        for bad in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            match (CollisionShape::Ball { radius: 3.0 }).scaled(bad) {
                CollisionShape::Ball { radius } => {
                    assert!(
                        (radius - 3.0).abs() < 1e-6,
                        "factor {bad} altered the shape"
                    );
                }
                other => panic!("variant changed: {other:?}"),
            }
        }
    }

    #[test]
    fn collision_shape_compound() {
        let child = CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        };
        let compound = CollisionShape::Compound {
            children: vec![(Vec3::ZERO, Quat::IDENTITY, Box::new(child))],
        };
        match compound {
            CollisionShape::Compound { children } => assert_eq!(children.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rigid_body_default_is_static() {
        let body = RigidBodyData::default();
        assert_eq!(body.motion_type, MotionType::Static);
        assert_eq!(body.mass, 0.0);
    }

    #[test]
    fn motion_type_equality() {
        assert_eq!(MotionType::Static, MotionType::Static);
        assert_ne!(MotionType::Dynamic, MotionType::Keyframed);
    }
}
