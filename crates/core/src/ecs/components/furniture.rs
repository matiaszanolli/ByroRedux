//! `Furniture` — sit / sleep / lean entry markers lifted from a
//! `BSFurnitureMarker` block on a furniture mesh (chairs, benches, beds,
//! wall-lean spots, workbenches).
//!
//! The marker data lives in the furniture *NIF*, not the FURN ESM record
//! (the ESM side carries only the model path). The importer parses
//! `BSFurnitureMarker` / `FurniturePosition` and this component surfaces
//! those positions to the runtime — the foundation an actor uses to seat
//! itself (M41.5 Phase C, gated on an AI/linked-ref assignment signal).
//!
//! Attaching this component does NOT change how the furniture renders —
//! it still draws as its static mesh. It only makes the entry positions
//! queryable. `SparseSetStorage` because only furniture entities carry it.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// One furniture entry position — where an actor sits, sleeps, or leans.
///
/// Lifted from a `BSFurnitureMarker` `FurniturePosition`. The offset is
/// converted to renderer Y-up at import (like `BSBound` / attach points);
/// it is **entity-local** and composes with the furniture entity's world
/// `GlobalTransform` when an actor is seated.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FurnitureMarker {
    /// Entry position, renderer Y-up, local to the furniture entity.
    pub local_offset: [f32; 3],
    /// Facing yaw about the Gamebryo up-axis (+Z) in radians, when the
    /// source provides one. `Some` on Skyrim+/FO4 — the
    /// `BSFurnitureMarker` `Heading` field, which nif.xml documents as
    /// "rotation around z-axis in radians". `None` on Oblivion/FO3/FNV,
    /// whose ushort `Orientation` field instead indexes a
    /// `furnituremarkerXX.nif` variant with no documented radian mapping.
    ///
    /// Kept in **source (Gamebryo)** space deliberately: the renderer-
    /// facing conversion (and the legacy orientation→facing mapping) is
    /// derived and *visually validated* when Phase C actually seats an
    /// actor — a wrong facing is immediately visible, so it is resolved
    /// against real data then, not guessed here.
    pub heading_z_radians: Option<f32>,
    /// Skyrim+ `AnimationType`: `1` = Sit, `2` = Sleep, `3` = Lean.
    /// `0` on the Oblivion/FO3/FNV path, which carries no AnimationType
    /// field (the position kind is implied by the referenced
    /// `furnituremarkerXX.nif`).
    pub animation_type: u16,
}

/// Sit / sleep / lean entry markers on a furniture entity.
///
/// Empty `markers` means the NIF authored a `BSFurnitureMarker` with zero
/// positions (unusual). Distinct from "no `Furniture` component", which
/// means the NIF carried no `BSFurnitureMarker` block at all.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Furniture {
    pub markers: Vec<FurnitureMarker>,
}

impl Component for Furniture {
    type Storage = SparseSetStorage<Self>;
}

impl Furniture {
    /// Number of entry markers.
    #[inline]
    pub fn len(&self) -> usize {
        self.markers.len()
    }

    /// True when the furniture authored a marker block with no positions.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furniture_default_is_empty() {
        let f = Furniture::default();
        assert!(f.is_empty());
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn furniture_holds_markers() {
        let f = Furniture {
            markers: vec![
                FurnitureMarker {
                    local_offset: [1.0, 0.0, -2.0],
                    heading_z_radians: Some(std::f32::consts::FRAC_PI_2),
                    animation_type: 1,
                },
                FurnitureMarker {
                    local_offset: [0.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 0,
                },
            ],
        };
        assert_eq!(f.len(), 2);
        assert_eq!(f.markers[0].animation_type, 1);
        assert!(f.markers[1].heading_z_radians.is_none());
    }
}
