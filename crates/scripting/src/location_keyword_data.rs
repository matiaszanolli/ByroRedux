//! The float a script attaches to a keyword on a location (M47.2).
//!
//! CK *SetKeywordData - Location*: "Sets the data attached to the specified
//! keyword on this location"; *GetKeywordData - Location*: "Obtains the data
//! attached to the specified keyword on this location". The *GetKeywordDataForLocation*
//! condition function "returns this value, if any. It otherwise returns 0".
//! Keyed by the location and keyword FormIDs. Saved.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// Every keyword data a script stored, by (location, keyword).
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct LocationKeywordData {
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    data: HashMap<(u32, u32), f32>,
}

impl Resource for LocationKeywordData {}

impl LocationKeywordData {
    pub fn set(&mut self, location: u32, keyword: u32, value: f32) {
        self.data.insert((location, keyword), value);
    }

    /// The stored data; 0 when none was set.
    pub fn get(&self, location: u32, keyword: u32) -> f32 {
        self.data.get(&(location, keyword)).copied().unwrap_or(0.0)
    }
}

/// The data on `keyword` at `location`: 0 without a store or a stored value.
pub fn location_keyword_data(world: &World, location: u32, keyword: u32) -> f32 {
    world
        .try_resource::<LocationKeywordData>()
        .map_or(0.0, |data| data.get(location, keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_data_reads_zero_and_set_data_reads_back_per_keyword() {
        let mut data = LocationKeywordData::default();
        assert_eq!(data.get(1, 2), 0.0);
        data.set(1, 2, 10.0);
        data.set(1, 3, -4.5);
        assert_eq!((data.get(1, 2), data.get(1, 3), data.get(9, 2)), (10.0, -4.5, 0.0));
    }
}
