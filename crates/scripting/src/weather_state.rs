//! What scripts can ask of the weather (M47.2): `Weather.GetCurrentWeather()` and
//! `<weather>.GetClassification()`.
//!
//! CK *GetCurrentWeather - Weather*: "Gets the current weather ... used by the
//! sky system"; *GetClassification - Weather*: "Gets this weather's
//! classification, as specified in the editor": -1 no classification, 0
//! Pleasant, 1 Cloudy, 2 Rainy, 3 Snow. The host publishes [`CurrentWeather`]
//! when it installs a sky; the classifications come from the `WTHR` records.
//! Neither is saved: both are re-derived from the loaded world.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::weather::{WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW};
use byroredux_plugin::esm::records::EsmIndex;

/// The `WTHR` FormID the sky system runs now; `None` without a weather.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CurrentWeather(pub Option<u32>);

impl Resource for CurrentWeather {}

/// Every weather's Papyrus classification, by `WTHR` FormID.
#[derive(Debug, Default, Clone)]
pub struct WeatherClassifications(HashMap<u32, i32>);

impl Resource for WeatherClassifications {}

impl WeatherClassifications {
    /// Whether the load order has this weather.
    pub fn knows(&self, weather: u32) -> bool {
        self.0.contains_key(&weather)
    }

    /// The weather's classification; -1 for none or an unknown weather.
    pub fn of(&self, weather: u32) -> i32 {
        self.0.get(&weather).copied().unwrap_or(-1)
    }
}

/// A `WTHR` classification byte as Papyrus's int: the first of Pleasant,
/// Cloudy, Rainy, Snow it sets, else -1.
pub fn classification_of(flags: u8) -> i32 {
    [WTHR_PLEASANT, WTHR_CLOUDY, WTHR_RAINY, WTHR_SNOW]
        .iter()
        .position(|bit| flags & bit != 0)
        .map_or(-1, |index| index as i32)
}

/// Install the load order's weather classifications; returns how many weathers.
pub fn install_weather_classifications(world: &mut World, index: &EsmIndex) -> usize {
    let table: HashMap<u32, i32> = index
        .weathers
        .iter()
        .map(|(&form_id, weather)| (form_id, classification_of(weather.classification)))
        .collect();
    let count = table.len();
    world.insert_resource(WeatherClassifications(table));
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_follows_the_creation_kit_numbering() {
        assert_eq!(classification_of(0), -1);
        assert_eq!(classification_of(WTHR_PLEASANT), 0);
        assert_eq!(classification_of(WTHR_CLOUDY), 1);
        assert_eq!(classification_of(WTHR_RAINY), 2);
        assert_eq!(classification_of(WTHR_SNOW), 3);
        // Aurora flags alone are no classification.
        assert_eq!(classification_of(0x10 | 0x20), -1);
    }
}
