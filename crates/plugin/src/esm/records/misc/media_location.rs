//! `ALOC` — a Fallout: New Vegas media location controller: the media sets an audio marker's area
//! plays (Game Music PRD MUS-SRC-005, #6882).
//!
//! Layout per xEdit's `wbDefinitionsFNV.pas` `ALOC` (no other game defines the record): `EDID`,
//! `FULL`, `NAM1` flags, `NAM4` location delay, `NAM5` / `NAM6` day and night start, `NAM7`
//! retrigger delay, then one repeated FormID sub-record per media-set role — `HNAM` Neutral, `ZNAM`
//! Ally, `XNAM` Friend, `YNAM` Enemy, `LNAM` Location, `GNAM` Battle — and `RNAM` the conditional
//! faction.
//!
//! Census of `FalloutNV.esm` (2026-11-13): 89 controllers, every one with day start 64 and night start
//! 255; 8 to 10 carry the faction-relation lists. `NAM7` reads as an uninitialised float on most
//! records (values from 1e-44 to 3e18), so it is kept raw and has no reader.
//!
//! `NAM1`'s first byte packs the GECK dialog's options (xEdit: "Flags and Enums, messily combined";
//! decoded by JohnnyGuitar NVSE's `MediaLocationController`, `_Flags`, and agreeing with OpenMW's ESM4
//! `MLC_Flags`, #6906): bits 0-3 "Faction Not Found" (JG `DeadRep`), bits 4-5 the Looping Options, bit
//! 6 "Use Defaults" for the day / night times. Vanilla: 0x44 ×48, 0x54 ×37, 0x40 ×2, 0x45, 0x55.

use super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaLocationRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `NAM4` — location delay.
    pub location_delay: f32,
    /// `NAM5` — day start, in 1/256 of a day (xEdit `wbAlocTime`: `value / 256` rendered as a time).
    pub day_start: u32,
    /// `NAM6` — night start, same unit.
    pub night_start: u32,
    /// `NAM7` — retrigger delay, as authored (see the module doc).
    pub retrigger_delay: f32,
    pub neutral_sets: Vec<u32>,
    pub ally_sets: Vec<u32>,
    pub friend_sets: Vec<u32>,
    pub enemy_sets: Vec<u32>,
    pub location_sets: Vec<u32>,
    pub battle_sets: Vec<u32>,
    /// `RNAM` — the faction whose relation to the player picks among the relation sets.
    pub conditional_faction: Option<u32>,
    /// `NAM1`'s first byte (see the module doc and [`Self::faction_not_found`], [`Self::looping`]).
    pub flags: u8,
}

/// The list a controller plays while no actor of its conditional faction is found (`NAM1` bits 0-3, the
/// GECK's "Faction Not Found", JG's `DeadRep`): "Don't use any Media Set" for `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionNotFound {
    Neutral,
    Enemy,
    Ally,
    Friend,
    Location,
    None,
}

/// How a controller picks among a list's sets (`NAM1` bits 4-5, the GECK's Looping Options): Loop
/// "through the Media Location Set in the active list", Random "a random Media Location Set";
/// Retrigger and None are "Unknown" in the GECK wiki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopingOption {
    Loop,
    Random,
    Retrigger,
    None,
}

impl MediaLocationRecord {
    /// `NAM1` bits 0-3; an undefined value reads as Location.
    pub fn faction_not_found(&self) -> FactionNotFound {
        match self.flags & 0x0F {
            0 => FactionNotFound::Neutral,
            1 => FactionNotFound::Enemy,
            2 => FactionNotFound::Ally,
            3 => FactionNotFound::Friend,
            5 => FactionNotFound::None,
            _ => FactionNotFound::Location,
        }
    }

    /// `NAM1` bits 4-5.
    pub fn looping(&self) -> LoopingOption {
        match (self.flags >> 4) & 0x3 {
            0 => LoopingOption::Loop,
            1 => LoopingOption::Random,
            2 => LoopingOption::Retrigger,
            _ => LoopingOption::None,
        }
    }

    /// `NAM1` bit 6 — "Use Defaults" for the day and night start times.
    pub fn uses_default_times(&self) -> bool {
        self.flags & 0x40 != 0
    }

    /// A `NAM5` / `NAM6` value as an hour of the day (`64` → 6.0).
    pub fn start_hour(value: u32) -> f32 {
        value as f32 / 256.0 * 24.0
    }

    /// Whether `hour` (0..24) falls in the controller's day: from the day start up to the night
    /// start, wrapping past midnight when the night starts earlier in the day than the day does.
    pub fn is_day(&self, hour: f32) -> bool {
        let day = Self::start_hour(self.day_start);
        let night = Self::start_hour(self.night_start);
        if day <= night {
            (day..night).contains(&hour)
        } else {
            hour >= day || hour < night
        }
    }

    /// Whether any faction-relation list (Neutral, Ally, Friend, Enemy) is authored.
    pub fn has_relation_sets(&self) -> bool {
        !(self.neutral_sets.is_empty()
            && self.ally_sets.is_empty()
            && self.friend_sets.is_empty()
            && self.enemy_sets.is_empty())
    }
}

/// Decode an `ALOC`, applying the plugin's FormID remap to every media-set and faction reference.
pub fn parse_aloc(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> MediaLocationRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut out = MediaLocationRecord { form_id, editor_id: common.editor_id, ..Default::default() };
    for sub in subs {
        let mut reader = SubReader::new(&sub.data);
        let form = |raw: u32| Some(remap_fid(raw, remap)).filter(|&v| v != 0);
        match &sub.sub_type {
            b"NAM1" => out.flags = sub.data.first().copied().unwrap_or_default(),
            b"NAM4" => out.location_delay = reader.f32().unwrap_or_default(),
            b"NAM5" => out.day_start = reader.u32().unwrap_or_default(),
            b"NAM6" => out.night_start = reader.u32().unwrap_or_default(),
            b"NAM7" => out.retrigger_delay = reader.f32().unwrap_or_default(),
            b"RNAM" => out.conditional_faction = reader.u32().ok().and_then(form),
            role => {
                let list = match role {
                    b"HNAM" => &mut out.neutral_sets,
                    b"ZNAM" => &mut out.ally_sets,
                    b"XNAM" => &mut out.friend_sets,
                    b"YNAM" => &mut out.enemy_sets,
                    b"LNAM" => &mut out.location_sets,
                    b"GNAM" => &mut out.battle_sets,
                    _ => continue,
                };
                if let Some(set) = reader.u32().ok().and_then(form) {
                    list.push(set);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    #[test]
    fn a_controller_reads_its_times_and_sets_per_role_in_authored_order() {
        let subs = vec![
            sub(b"EDID", b"musCtrlGoodsprings\0"),
            sub(b"NAM4", 0.0f32.to_le_bytes()),
            sub(b"NAM5", 64u32.to_le_bytes()),
            sub(b"NAM6", 255u32.to_le_bytes()),
            sub(b"LNAM", 0x10u32.to_le_bytes()),
            sub(b"LNAM", 0x11u32.to_le_bytes()),
            sub(b"GNAM", 0x20u32.to_le_bytes()),
            sub(b"YNAM", 0x30u32.to_le_bytes()),
            sub(b"RNAM", 0x40u32.to_le_bytes()),
            sub(b"GNAM", 0u32.to_le_bytes()),
        ];
        let aloc = parse_aloc(7, &subs, &None);
        assert_eq!(aloc.editor_id, "musCtrlGoodsprings");
        assert_eq!((aloc.day_start, aloc.night_start), (64, 255));
        assert_eq!(aloc.location_sets, [0x10, 0x11]);
        assert_eq!(aloc.battle_sets, [0x20], "a null set is no set");
        assert_eq!(aloc.enemy_sets, [0x30]);
        assert_eq!(aloc.conditional_faction, Some(0x40));
        assert!(aloc.has_relation_sets());
    }

    /// #6906 — the five vanilla `NAM1` values.
    #[test]
    fn nam1_packs_faction_not_found_looping_and_default_times() {
        let read = |byte: u8| {
            let aloc = parse_aloc(1, &[sub(b"NAM1", [byte, 0, 0, 0])], &None);
            (aloc.faction_not_found(), aloc.looping(), aloc.uses_default_times())
        };
        assert_eq!(read(0x44), (FactionNotFound::Location, LoopingOption::Loop, true));
        assert_eq!(read(0x54), (FactionNotFound::Location, LoopingOption::Random, true));
        assert_eq!(read(0x40), (FactionNotFound::Neutral, LoopingOption::Loop, true), "musCtrlGoodsprings");
        assert_eq!(read(0x55), (FactionNotFound::None, LoopingOption::Random, true), "musCtrlViperHwy95");
        assert_eq!(read(0x45), (FactionNotFound::None, LoopingOption::Loop, true), "musCtrlSmallRV");
    }

    #[test]
    fn day_runs_from_the_day_start_to_the_night_start() {
        let aloc = MediaLocationRecord { day_start: 64, night_start: 255, ..Default::default() };
        assert_eq!(MediaLocationRecord::start_hour(64), 6.0);
        assert!(!aloc.is_day(5.9));
        assert!(aloc.is_day(6.0));
        assert!(aloc.is_day(23.8));
        assert!(!aloc.is_day(23.95), "255/256 of a day is 23:54");
        let wrapped = MediaLocationRecord { day_start: 200, night_start: 64, ..Default::default() };
        assert!(wrapped.is_day(23.0) && wrapped.is_day(2.0) && !wrapped.is_day(12.0));
    }
}
