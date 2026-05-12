//! `AttachPoints` and `ChildAttachConnections` — FO4+ weapon-mod attachment graph.
//!
//! Populated from `BSConnectPoint::Parents` (the named attach points an
//! item *exposes*) and `BSConnectPoint::Children` (the names a modular
//! accessory *connects back to* on its parent item) extra-data blocks
//! attached to a NIF's root `NiNode`.
//!
//! Together they form the FO4 weapon-mod attachment graph:
//!
//! - A 10mm pistol mesh exposes `CON_Magazine`, `CON_Scope`, `CON_Stock`,
//!   `CON_Grip` attach points via `BSConnectPoint::Parents`.
//! - A reflex-sight accessory mesh references `CON_Scope` via
//!   `BSConnectPoint::Children`.
//! - The equip system composes the world transform as
//!   `parent_world * attach_point.local_transform * accessory.local`.
//!
//! Without the graph reaching the ECS, every modular FO4 weapon imports
//! as a base mesh with no discoverable attach surface — the weapon-mod
//! system can't function. See #985 / NIF-D5-ORPHAN-A3.
//!
//! Both components are `SparseSetStorage` because only modular items
//! carry them (a tiny fraction of entities).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::string::FixedString;

/// One named attach point on a parent (host) item.
///
/// `name` is the CON_xxx tag (e.g. "CON_Magazine") referenced by both
/// the modular accessory's `BSConnectPoint::Children` and the OMOD
/// record's attach-point identifier. `parent_bone` is the skeleton bone
/// the attach point hangs off — drives the equip-time world-transform
/// composition for skinned weapons. `translation` / `rotation` / `scale`
/// give the attach point's local transform relative to the parent bone
/// (or the host mesh root when `parent_bone` is empty).
///
/// Rotation is stored as a quaternion in `(w, x, y, z)` matching the
/// Gamebryo `NiQuaternion` serialization. Y-up coordinate frame (the
/// importer applies the Z-up → Y-up conversion at NIF load time).
// No `#[cfg_attr(feature = "inspect", derive(serde::*))]` — `FixedString`
// is a `SymbolU32` handle into a process-global StringPool with no
// serde impl (resolving to a string requires the pool, which isn't
// available at deserialize time). Matches the precedent set by
// `Name` (also FixedString-backed, also opts out of inspect-serde).
#[derive(Debug, Clone)]
pub struct AttachPoint {
    /// Attach point name — `CON_xxx` style identifier interned through
    /// the engine `StringPool` so equip-time lookups are integer
    /// comparisons. Examples: `CON_Magazine`, `CON_Scope`, `CON_Grip`,
    /// `CON_Stock`, `CON_MuzzleAttach`, `CON_RailAttach`.
    pub name: FixedString,
    /// Skeleton bone the attach point hangs off. Empty (`None`) for
    /// non-skinned weapons where the attach point is anchored on the
    /// host mesh root. Drives the equip-time world-transform
    /// composition: `bone_world * local_transform * accessory.local`.
    pub parent_bone: Option<FixedString>,
    /// Local translation relative to the parent bone (or host root).
    /// Y-up world units.
    pub translation: [f32; 3],
    /// Local rotation as a unit quaternion `(w, x, y, z)`. Identity is
    /// `[1.0, 0.0, 0.0, 0.0]`.
    pub rotation: [f32; 4],
    /// Local uniform scale. `1.0` is identity.
    pub scale: f32,
}

/// Named attach points an item *exposes* for modular accessories to
/// connect to. One entry per `CON_xxx` tag authored in
/// `BSConnectPoint::Parents` on the NIF root.
///
/// Attached to the entity that owns the host mesh (e.g. the equipped
/// weapon entity). The equip system queries this component on the
/// parent when composing a modular accessory's world transform.
///
/// Empty `points` means the NIF authored a `BSConnectPoint::Parents`
/// extra-data with zero entries (unusual — most authored content
/// either omits the block or carries ≥1 attach point). Distinct from
/// "no `AttachPoints` component" which means the NIF carried no
/// `BSConnectPoint::Parents` extra-data at all.
#[derive(Debug, Clone, Default)]
pub struct AttachPoints {
    pub points: Vec<AttachPoint>,
}

impl Component for AttachPoints {
    type Storage = SparseSetStorage<Self>;
}

impl AttachPoints {
    /// Look up an attach point by name. Returns `None` if no point
    /// with this name is exposed. Integer comparison on the
    /// `FixedString` handle — no string compare in the equip hot path.
    pub fn find(&self, name: FixedString) -> Option<&AttachPoint> {
        self.points.iter().find(|p| p.name == name)
    }

    /// Count of exposed attach points.
    #[inline]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// True when no attach point is exposed (rare but valid).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Named attach points a modular accessory *connects back to* on its
/// parent host. Populated from `BSConnectPoint::Children` on the NIF
/// root.
///
/// Attached to the entity that owns the accessory mesh (e.g. the
/// reflex sight that mounts on `CON_Scope`). The equip system reads
/// this when an accessory is being mounted: each name in
/// `connect_names` must resolve to an [`AttachPoints::find`] hit on
/// the parent's `AttachPoints` component, or the accessory can't
/// mount.
///
/// `skinned` flips the attach math from "rigid transform off parent
/// bone" to "skinned weight blend across the parent's skeleton" —
/// drives bone-influenced accessories (capes, cloth bits attached to
/// armor).
#[derive(Debug, Clone, Default)]
pub struct ChildAttachConnections {
    /// Attach-point names this accessory connects to on its parent.
    /// Each name must resolve to an [`AttachPoint::name`] on the
    /// parent's [`AttachPoints`] component.
    pub connect_names: Vec<FixedString>,
    /// `true` if the accessory's geometry needs skinned (bone-weighted)
    /// attachment to the parent's skeleton rather than a rigid
    /// transform-off-bone. Capes / cloth / shoulder pauldrons.
    pub skinned: bool,
}

impl Component for ChildAttachConnections {
    type Storage = SparseSetStorage<Self>;
}

impl ChildAttachConnections {
    #[inline]
    pub fn len(&self) -> usize {
        self.connect_names.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.connect_names.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string::StringPool;

    fn intern_name(pool: &mut StringPool, s: &str) -> FixedString {
        pool.intern(s)
    }

    #[test]
    fn attach_points_default_is_empty() {
        let ap = AttachPoints::default();
        assert!(ap.is_empty());
        assert_eq!(ap.len(), 0);
    }

    #[test]
    fn attach_points_find_by_name_hits() {
        let mut pool = StringPool::new();
        let con_mag = intern_name(&mut pool, "CON_Magazine");
        let con_scope = intern_name(&mut pool, "CON_Scope");
        let ap = AttachPoints {
            points: vec![
                AttachPoint {
                    name: con_mag,
                    parent_bone: None,
                    translation: [0.0, -1.5, 0.0],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                    scale: 1.0,
                },
                AttachPoint {
                    name: con_scope,
                    parent_bone: None,
                    translation: [0.0, 0.0, 2.0],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                    scale: 1.0,
                },
            ],
        };
        let hit = ap.find(con_scope).expect("CON_Scope must resolve");
        assert_eq!(hit.translation, [0.0, 0.0, 2.0]);
    }

    #[test]
    fn attach_points_find_by_name_miss() {
        let mut pool = StringPool::new();
        let con_mag = intern_name(&mut pool, "CON_Magazine");
        let con_grip = intern_name(&mut pool, "CON_Grip");
        let ap = AttachPoints {
            points: vec![AttachPoint {
                name: con_mag,
                parent_bone: None,
                translation: [0.0, 0.0, 0.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
                scale: 1.0,
            }],
        };
        assert!(ap.find(con_grip).is_none());
    }

    #[test]
    fn child_attach_connections_default_is_empty() {
        let cac = ChildAttachConnections::default();
        assert!(cac.is_empty());
        assert!(!cac.skinned);
    }
}
