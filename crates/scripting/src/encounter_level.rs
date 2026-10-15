//! `CalculateEncounterLevel` (M47.2) — the level an encounter zone resolves
//! for a reference.
//!
//! CK *CalculateEncounterLevel - ObjectReference*: "Calculates the reference's
//! encounter level, based on the player's level, the area's level, and the
//! passed-in difficulty" (0 Easy, 1 Medium, 2 Hard, 3 Very Hard, 4 None).
//!
//! The pieces the CK states:
//!
//! - **The area's level** (CK *Encounter Zone*): the zone's `Min Level` lifts
//!   a lower player level to it — unless `Match PC Below Minimum Level` is set,
//!   when the zone "will match the player's level" instead — and its `Max
//!   Level` (0 = none) caps a higher one. A reference's zone is its own `XEZN`,
//!   else its cell's; one in neither stands at the player's level.
//! - **The difficulty** (CK *LeveledCharacter*, "Leveled Character
//!   References"): Easy / Medium / Hard / Very Hard are the
//!   `fLeveledActorMultEasy` / `Medium` / `Hard` / `VeryHard` game
//!   settings (0.33, 0.67, 1.0, 1.25 in Skyrim.esm and Fallout4.esm) of
//!   the resolved level, "without exceeding" it; `None` is the resolved
//!   level itself.
//!
//! Engine policy where the CK is silent: the scaled level rounds down
//! (the CK's "without exceeding" wording) and never drops below 1 —
//! levels start at 1. A game whose load order lacks a multiplier
//! setting cannot scale by that difficulty, and the call stays
//! unsupported rather than guess one.

use std::collections::HashMap;

use byroredux_core::character::CharacterLevel;
use byroredux_core::ecs::components::{CellFormId, CellRoot};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::EsmIndex;

/// The `XEZN` a placed reference carries itself (it overrides its cell's zone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitEncounterZone(pub u32);

impl Component for ExplicitEncounterZone {
    type Storage = SparseSetStorage<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZoneBounds {
    min: u8,
    max: u8,
    match_pc_below_minimum: bool,
}

/// The load order's encounter zones and difficulty multipliers.
/// `NOT_SAVED_BY_DESIGN`: re-derived from the load order every boot.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EncounterLevelRules {
    zones: HashMap<u32, ZoneBounds>,
    cell_zones: HashMap<u32, u32>,
    /// Easy, Medium, Hard, Very Hard.
    multipliers: [Option<f32>; 4],
}

impl Resource for EncounterLevelRules {}

/// The game setting behind each of difficulty 0..=3.
const MULTIPLIER_SETTINGS: [&str; 4] = [
    "fLeveledActorMultEasy",
    "fLeveledActorMultMedium",
    "fLeveledActorMultHard",
    "fLeveledActorMultVeryHard",
];

/// The `aiDifficulty` default and "None": no scaling.
const DIFFICULTY_NONE: i32 = 4;

impl EncounterLevelRules {
    pub fn from_index(index: &EsmIndex) -> Self {
        let zones = index
            .encounter_zones
            .values()
            .map(|zone| {
                (
                    zone.form_id,
                    ZoneBounds {
                        min: zone.min_level,
                        max: zone.max_level,
                        match_pc_below_minimum: zone.match_pc_below_minimum(),
                    },
                )
            })
            .collect();
        let exterior = index.cells.exterior_cells.values().flat_map(|grid| grid.values());
        let cell_zones = index
            .cells
            .cells
            .values()
            .chain(exterior)
            .filter_map(|cell| Some((cell.form_id, cell.encounter_zone_form?)))
            .collect();
        Self {
            zones,
            cell_zones,
            multipliers: MULTIPLIER_SETTINGS.map(|name| index.game_setting_float(name)),
        }
    }

    /// The level `zone` resolves for a player of `player_level`.
    pub fn resolved_level(&self, zone: Option<u32>, player_level: u32) -> u32 {
        let Some(bounds) = zone.and_then(|zone| self.zones.get(&zone)) else {
            return player_level;
        };
        let mut level = player_level;
        if level < u32::from(bounds.min) && !bounds.match_pc_below_minimum {
            level = u32::from(bounds.min);
        }
        if bounds.max > 0 && level > u32::from(bounds.max) {
            level = u32::from(bounds.max);
        }
        level
    }

    /// `resolved` scaled for `difficulty`; `None` for a difficulty outside
    /// 0..=4 or one whose multiplier the load order does not define.
    pub fn at_difficulty(&self, resolved: u32, difficulty: i32) -> Option<u32> {
        if difficulty == DIFFICULTY_NONE {
            return Some(resolved);
        }
        let multiplier = (*self.multipliers.get(usize::try_from(difficulty).ok()?)?)?;
        Some(((resolved as f32 * multiplier).floor() as u32).max(1))
    }

    fn zone_of(&self, world: &World, reference: EntityId) -> Option<u32> {
        if let Some(own) = world.get::<ExplicitEncounterZone>(reference) {
            return Some(own.0);
        }
        let root = world.get::<CellRoot>(reference).map_or(reference, |root| root.0);
        let cell = world.get::<CellFormId>(root)?.0;
        self.cell_zones.get(&cell).copied()
    }
}

/// `reference.CalculateEncounterLevel(difficulty)`: `None` when the player has
/// no level or the difficulty cannot be scaled.
pub fn calculate_encounter_level(world: &World, reference: EntityId, difficulty: i32) -> Option<u32> {
    let rules = world.try_resource::<EncounterLevelRules>()?;
    let player = world.try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()?.0;
    let player_level = world.get::<CharacterLevel>(player)?.level;
    let zone = rules.zone_of(world, reference);
    rules.at_difficulty(rules.resolved_level(zone, player_level.into()), difficulty)
}

/// Install the rules from the load order.
pub fn install_encounter_level_rules(world: &mut World, index: &EsmIndex) {
    world.insert_resource(EncounterLevelRules::from_index(index));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(zones: &[(u32, u8, u8, bool)]) -> EncounterLevelRules {
        EncounterLevelRules {
            zones: zones
                .iter()
                .map(|&(id, min, max, match_pc_below_minimum)| {
                    (id, ZoneBounds { min, max, match_pc_below_minimum })
                })
                .collect(),
            cell_zones: HashMap::new(),
            multipliers: [Some(0.33), Some(0.67), Some(1.0), Some(1.25)],
        }
    }

    /// CK *Encounter Zone* examples: BoulderfallCaveZone (min 6) lifts a
    /// level-2 player to 6; BleakFallsBarrowZone (6..20) caps a level-30 one at
    /// 20; ValthumeZone (min 24) lifts to 24.
    #[test]
    fn the_zone_clamps_the_player_level_like_the_ck_examples() {
        let rules = rules(&[(1, 6, 0, false), (2, 6, 20, false), (3, 24, 0, false)]);
        assert_eq!(rules.resolved_level(Some(1), 2), 6);
        assert_eq!(rules.resolved_level(Some(1), 40), 40, "no max: never maxes out");
        assert_eq!(rules.resolved_level(Some(2), 30), 20);
        assert_eq!(rules.resolved_level(Some(2), 12), 12);
        assert_eq!(rules.resolved_level(Some(3), 2), 24);
        assert_eq!(rules.resolved_level(None, 9), 9, "no zone stands at the player's level");
        assert_eq!(rules.resolved_level(Some(99), 9), 9, "an unknown zone too");
    }

    #[test]
    fn match_pc_below_minimum_keeps_the_players_level() {
        let rules = rules(&[(1, 10, 0, true)]);
        assert_eq!(rules.resolved_level(Some(1), 3), 3);
        assert_eq!(rules.resolved_level(Some(1), 12), 12);
    }

    #[test]
    fn difficulty_scales_by_the_game_settings_rounding_down_to_at_least_one() {
        let rules = rules(&[]);
        assert_eq!(rules.at_difficulty(30, 4), Some(30), "None: the resolved level");
        assert_eq!(rules.at_difficulty(30, 0), Some(9), "Easy 0.33");
        assert_eq!(rules.at_difficulty(30, 1), Some(20), "Medium 0.67");
        assert_eq!(rules.at_difficulty(30, 2), Some(30), "Hard 1.0");
        assert_eq!(rules.at_difficulty(30, 3), Some(37), "Very Hard 1.25");
        assert_eq!(rules.at_difficulty(1, 0), Some(1), "levels start at 1");
        assert_eq!(rules.at_difficulty(30, 5), None);
        assert_eq!(rules.at_difficulty(30, -1), None);
        let mut missing = rules.clone();
        missing.multipliers[0] = None;
        assert_eq!(missing.at_difficulty(30, 0), None, "no multiplier, no guess");
        assert_eq!(missing.at_difficulty(30, 4), Some(30));
    }

    #[test]
    fn a_reference_uses_its_own_zone_before_its_cells() {
        let mut rules = rules(&[(1, 6, 0, false), (2, 20, 0, false)]);
        rules.cell_zones.insert(0xCE11, 1);
        let mut world = World::new();
        let root = world.spawn();
        world.insert(root, CellFormId(0xCE11));
        let inside = world.spawn();
        world.insert(inside, CellRoot(root));
        assert_eq!(rules.zone_of(&world, inside), Some(1));
        world.insert(inside, ExplicitEncounterZone(2));
        assert_eq!(rules.zone_of(&world, inside), Some(2));
        let outside = world.spawn();
        assert_eq!(rules.zone_of(&world, outside), None);
    }
}
