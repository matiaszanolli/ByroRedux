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
    // Physics caches the absence of unregistered shapes using membership generations.
    const TRACK_CHANGES: bool = true;
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
    /// `false` when the source content authored this body on Havok's
    /// non-collidable layer (`OL_NONCOLLIDABLE` / `FOL_NONCOLLIDABLE` /
    /// `SKYL_NONCOLLIDABLE` — value 15 in every per-game
    /// `HavokFilter.Layer` enum nif.xml declares, confirmed for
    /// Oblivion/FO3+FNV/Skyrim). `true` (collidable) for every other
    /// layer and for every non-NIF-derived body (synthesized colliders,
    /// water planes, the player's own kinematic capsule, …), none of
    /// which carry Havok layer authoring at all. #2549.
    pub collidable: bool,
    /// The Havok surface material the collision authoring carries, as the
    /// per-game material enum's numeric value (Oblivion/Fallout3/Skyrim
    /// HavokMaterial — nif.xml's per-game `bhkMaterial` wrappers). A u32,
    /// not a byte: Oblivion (0–31) and FO3/FNV (0–127) share the
    /// STONE-is-0 start, but Skyrim's `SkyrimHavokMaterial` is the CRC32
    /// of the Creation Kit material name (`SKY_HAV_MAT_STONE` =
    /// 3741512247) — truncating it to a byte collided 8 material pairs
    /// and made the value unjoinable to MATT records (#5542). Body-level
    /// NIFAL parser-to-canonical reduction of the shape tree's first
    /// authored leaf (`body_surface_material`, over the sequence produced by
    /// `resolve_shape_with_materials`). `0` is authored STONE on
    /// Oblivion/FO3/FNV, but `SKY_HAV_MAT_NONE` on Skyrim and later; `0` is
    /// also the default for every non-NIF-derived body. Gameplay consumers
    /// are surface-aware footsteps and impacts, which need "what kind of
    /// ground is this", not the per-triangle split.
    ///
    /// Deliberately the raw per-game id, not a canonical surface: no
    /// sourced Havok-material → surface table exists yet, so the
    /// translation is parked (`docs/engine/nifal.md` "Havok surface
    /// material", #5574). Any map keyed on it is per-game data in the
    /// loaded game's id space; consumers must not branch on the game.
    pub surface_material: u32,
    /// Per-part surface materials, leaf-aligned with
    /// `collision_shape_to_parts`' output order — one entry per collider
    /// part the shape flattens to, so a compound body whose bhk children
    /// carry different materials puts each part's own material into its
    /// collider's user-data. Derived from the NIF collision tree by
    /// `resolve_shape_with_materials` and NOT saved: the tree is re-parsed from
    /// content on load and re-derived at import, so a snapshot never
    /// carries it (`serde(skip)` — the #3470 class; the save-shape
    /// fingerprint moves without a FORMAT_MAJOR bump). The live-load
    /// overlay registers this component *preserving* — the apply hook
    /// (`keep_reimported_part_surface_materials`, #5686) carries the
    /// freshly re-imported slice across instead of letting the
    /// deserialized row's empty default wipe it. Consumers treat a
    /// short or empty slice as "fall back to [`Self::surface_material`]"
    /// for every part beyond it.
    #[cfg_attr(feature = "inspect", serde(skip))]
    pub part_surface_materials: Box<[u32]>,
    /// The Havok collision layer the body was authored on — the low byte of
    /// its packed `HavokFilter` (Skyrim: 8 Biped, 10 Props, 14 Trap, 20 Debris
    /// Large, …); `0` (Unidentified) for every non-NIF-derived body.
    /// `ApplyHavokImpulse` scales its impulse by a per-layer game setting.
    /// Derived from the NIF at import and NOT saved (`serde(skip)`, the
    /// `part_surface_materials` class: the save-shape fingerprint moves
    /// without a FORMAT_MAJOR bump, and the live-load overlay carries the
    /// re-imported value across).
    #[cfg_attr(feature = "inspect", serde(skip))]
    pub havok_layer: u8,
}

impl RigidBodyData {
    /// Default static body (walls, architecture). Was `pub const STATIC`
    /// until `part_surface_materials` (a `Box`) made a const impossible —
    /// call sites see no difference.
    pub fn static_body() -> Self {
        Self {
            motion_type: MotionType::Static,
            mass: 0.0,
            friction: 0.5,
            restitution: 0.3,
            linear_damping: 0.0,
            angular_damping: 0.0,
            collidable: true,
            surface_material: 0,
            part_surface_materials: Box::new([]),
            havok_layer: 0,
        }
    }
}

impl Default for RigidBodyData {
    fn default() -> Self {
        Self::static_body()
    }
}

impl Component for RigidBodyData {
    type Storage = SparseSetStorage<Self>;
    // Trap contact discovery caches the Trap-layer subset and rebuilds it when
    // bodies are inserted, replaced, or removed.
    const TRACK_CHANGES: bool = true;
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
