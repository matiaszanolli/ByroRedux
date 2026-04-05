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

impl Component for CollisionShape {
    type Storage = SparseSetStorage<Self>;
}

/// Rigid body motion type — controls how the physics engine treats the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionType {
    /// Fixed in place, infinite mass (walls, floors, static architecture).
    Static,
    /// Moved by animation/script, not by physics forces (doors, platforms).
    Keyframed,
    /// Fully simulated by the physics engine (crates, bottles, debris).
    Dynamic,
}

/// Rigid body properties extracted from bhkRigidBody.
///
/// These feed Rapier `RigidBodyBuilder` configuration.
/// `motion_type` determines the Rapier body type:
/// - `Static` → `RigidBodyBuilder::fixed()`
/// - `Keyframed` → `RigidBodyBuilder::kinematic_position_based()`
/// - `Dynamic` → `RigidBodyBuilder::dynamic()`
#[derive(Debug, Clone)]
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
