//! Map markers (M47.2): `AddToMap` and the reads of a marker's state.
//!
//! The CK *ObjectReference Script*: `AddToMap(abFastTravel)` "adds this
//! reference (which must be a map marker) to the player's map, optionally
//! making it available for fast-travel"; `IsMapMarkerVisible` / 
//! `CanFastTravelToMarker` read it back. A marker's starting state is its
//! REFR `FNAM` flags (`0x01` Visible, `0x02` Can Travel To — xEdit's *Map
//! Marker* block): [`AuthoredMapMarkers`], rebuilt from the load order every
//! boot. Scripted additions are [`MapMarkers`], keyed by reference FormID and
//! saved. A marker is visible (or a travel target) when either says so.

use std::collections::HashSet;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// Markers scripts added to the map. Saved.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct MapMarkers {
    visible: HashSet<u32>,
    fast_travel: HashSet<u32>,
    /// Starfield's `SetMarkerVisibleOnStarMap(abVisible)`: markers a script made visible on
    /// the star map (the game's `ObjectReference.pex` declares the native with no body; the
    /// meaning is the name's, and nothing reads it yet - there is no star map).
    star_map_visible: HashSet<u32>,
}

impl Resource for MapMarkers {}

impl MapMarkers {
    /// Whether a script made `marker` visible on the map.
    pub fn is_visible(&self, marker: u32) -> bool {
        self.visible.contains(&marker)
    }

    /// `SetMarkerVisibleOnStarMap`.
    pub fn set_star_map_visible(&mut self, marker: u32, visible: bool) {
        if visible {
            self.star_map_visible.insert(marker);
        } else {
            self.star_map_visible.remove(&marker);
        }
    }

    pub fn is_star_map_visible(&self, marker: u32) -> bool {
        self.star_map_visible.contains(&marker)
    }

    /// Whether a script made `marker` a fast-travel target.
    pub fn is_fast_travel(&self, marker: u32) -> bool {
        self.fast_travel.contains(&marker)
    }

    /// `AddToMap`: visible, and a travel target when asked. A `false` never
    /// takes travel away.
    pub fn add(&mut self, marker: u32, fast_travel: bool) {
        self.visible.insert(marker);
        if fast_travel {
            self.fast_travel.insert(marker);
        }
    }
}

/// The markers the load order authors visible / travelable.
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the REFR flags every boot.
#[derive(Debug, Default, Clone)]
pub struct AuthoredMapMarkers {
    visible: HashSet<u32>,
    fast_travel: HashSet<u32>,
}

impl Resource for AuthoredMapMarkers {}

impl AuthoredMapMarkers {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut markers = Self::default();
        let cells = index
            .cells
            .cells
            .values()
            .chain(index.cells.exterior_cells.values().flat_map(|grid| grid.values()))
            .chain(index.cells.worldspace_persistent_cells.values());
        for placed in cells.flat_map(|cell| cell.references.iter()) {
            let Some(flags) = placed.map_marker else {
                continue;
            };
            if flags & 0x01 != 0 {
                markers.visible.insert(placed.form_id);
            }
            if flags & 0x02 != 0 {
                markers.fast_travel.insert(placed.form_id);
            }
        }
        markers
    }
}

/// Whether `marker` is on the map. Each guard is released before the next.
pub fn map_marker_visible(world: &World, marker: u32) -> bool {
    let scripted = world
        .try_resource::<MapMarkers>()
        .is_some_and(|markers| markers.visible.contains(&marker));
    scripted
        || world
            .try_resource::<AuthoredMapMarkers>()
            .is_some_and(|markers| markers.visible.contains(&marker))
}

/// Whether the player can fast-travel to `marker`.
pub fn map_marker_fast_travel(world: &World, marker: u32) -> bool {
    let scripted = world
        .try_resource::<MapMarkers>()
        .is_some_and(|markers| markers.fast_travel.contains(&marker));
    scripted
        || world
            .try_resource::<AuthoredMapMarkers>()
            .is_some_and(|markers| markers.fast_travel.contains(&marker))
}

/// Install the load order's [`AuthoredMapMarkers`]; returns how many start
/// visible.
pub fn install_map_markers(world: &mut World, index: &EsmIndex) -> usize {
    let markers = AuthoredMapMarkers::from_index(index);
    let count = markers.visible.len();
    world.insert_resource(markers);
    world.init_resource::<MapMarkers>();
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_flags_and_scripted_adds_combine() {
        use byroredux_plugin::esm::cell::{CellData, PlacedRef};
        let mut index = EsmIndex::default();
        let marker = |form_id, flags| PlacedRef {
            map_marker: Some(flags),
            encounter_zone_form: None,
            ..PlacedRef::at(form_id, 0x10, [0.0; 3], [0.0; 3])
        };
        index.cells.worldspace_persistent_cells.insert(
            "tamriel".into(),
            CellData {
                references: vec![marker(0xA, 0x03), marker(0xB, 0x00), marker(0xC, 0x01)],
                ..Default::default()
            },
        );
        let mut world = World::new();
        install_map_markers(&mut world, &index);
        assert!(map_marker_visible(&world, 0xA) && map_marker_fast_travel(&world, 0xA));
        assert!(!map_marker_visible(&world, 0xB));
        assert!(map_marker_visible(&world, 0xC) && !map_marker_fast_travel(&world, 0xC));

        world.resource_mut::<MapMarkers>().add(0xB, false);
        assert!(map_marker_visible(&world, 0xB) && !map_marker_fast_travel(&world, 0xB));
        world.resource_mut::<MapMarkers>().add(0xC, true);
        assert!(map_marker_fast_travel(&world, 0xC));
    }
}
