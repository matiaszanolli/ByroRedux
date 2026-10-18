//! GLOB runtime values as a World resource (#1668).
//!
//! Bethesda CTDA conditions can compare a function result against a GLOB
//! record's current value ("Use Global" — `ConditionValue::Global`). The
//! parsed values live in `EsmIndex.globals`, but `EsmIndex` is not a
//! `Resource`. This module mirrors `EsmIndex.globals` into a lean,
//! evaluator-facing resource: a `FormID → f32` map keyed in the **global
//! load-order space** (the same space CTDA comparands are remapped into at
//! parse time, see `remap_condition_form_ids`), so the lookup is
//! space-consistent and free of the multi-plugin false-positive risk.
//!
//! Globals are script-mutable at runtime; the map is therefore owned (not
//! borrowed from the index) so `SetGlobalValue`-style mutations have a home.

use byroredux_core::ecs::resource::Resource;
use byroredux_plugin::esm::records::GlobalRecord;
use std::collections::HashMap;

/// Runtime GLOB values, keyed by global-load-order FormID.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct Globals(pub HashMap<u32, f32>);

impl Globals {
    /// An empty global table.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Snapshot a parsed `EsmIndex.globals` map into runtime values.
    /// Keys are preserved verbatim — `EsmIndex` already remaps record
    /// FormIDs into global load-order space at parse time.
    pub fn from_records(records: &HashMap<u32, GlobalRecord>) -> Self {
        Self(
            records
                .iter()
                .map(|(&form_id, rec)| (form_id, rec.value.as_f32()))
                .collect(),
        )
    }

    /// Current value of a GLOB, or `None` when the FormID is unknown.
    /// A missing GLOB resolves to `0.0` at the comparand site (Bethesda's
    /// "missing GLOB defaults to 0").
    pub fn get(&self, form_id: u32) -> Option<f32> {
        self.0.get(&form_id).copied()
    }

    /// Set (or insert) a GLOB's runtime value. Fragment effects such as
    /// Papyrus `GlobalVariable.SetValue` write through this surface.
    pub fn set(&mut self, form_id: u32, value: f32) {
        self.0.insert(form_id, value);
    }

    /// Number of globals tracked.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when no globals are tracked.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Resource for Globals {}

/// The game clock's globals: the CS documents `GetCurrentTime` ("the same value can be accessed by
/// referring to the special global variable Gamehour"), so a script that reads `GameHour` must see the
/// game clock, not the value the record authors — and `GameDaysPassed`, "the amount of days passed in
/// game" (GECK *List of Global Variables*), counts the days the clock has run.
/// `NOT_SAVED_BY_DESIGN`: found in the records when the globals are installed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GameClockGlobals {
    /// The FormID of the `GameHour` GLOB, if the load order has one.
    pub hour: Option<u32>,
    /// The FormID of the `GameDaysPassed` GLOB, and the value the record authors (the days that had
    /// passed when the game began).
    pub days_passed: Option<u32>,
    pub base_days: f32,
    /// The record-authored date `(GameYear, GameMonth, GameDay)`: the day a new game starts on.
    pub start_date: Option<(f32, f32, f32)>,
}

impl Resource for GameClockGlobals {}

impl GameClockGlobals {
    /// Find the clock globals among the parsed records, by editor ID.
    pub fn from_records(records: &HashMap<u32, GlobalRecord>) -> Self {
        let find = |name: &str| records.values().find(|record| record.editor_id.eq_ignore_ascii_case(name));
        let value = |name: &str| find(name).map(|record| record.value.as_f32());
        Self {
            hour: find("GameHour").map(|record| record.form_id),
            days_passed: find("GameDaysPassed").map(|record| record.form_id),
            base_days: value("GameDaysPassed").unwrap_or(0.0),
            start_date: value("GameYear").zip(value("GameMonth")).zip(value("GameDay")).map(|((year, month), day)| (year, month, day)),
        }
    }
}

/// The whole game days the clock has run since the game began, kept beside the clock globals for
/// `GetDayOfWeek`. `NOT_SAVED_BY_DESIGN`: republished every frame from the saved game clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameCalendar {
    pub days_elapsed: u64,
}

impl Resource for GameCalendar {}

/// The weekday (0 Sunday .. 6 Saturday) of the proleptic Gregorian date `year-month-day`
/// (`month` 1..=12), by days-from-civil.
pub fn gregorian_weekday(year: i64, month: i64, day: i64) -> u32 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468; // days since 1970-01-01, a Thursday (4)
    (days + 4).rem_euclid(7) as u32
}

/// Write the game clock into the globals: the hour of day into `GameHour`, and `GameDaysPassed` as the
/// authored starting count plus the days run (the fraction of the day in progress included — the GECK
/// notes the two "may disagree for a few frames" across midnight). A no-op without the globals.
pub fn sync_game_clock_globals(world: &byroredux_core::ecs::World, day: u64, hour_of_day: f32) {
    if let Some(mut calendar) = world.try_resource_mut::<GameCalendar>() {
        calendar.days_elapsed = day;
    }
    let Some(clock) = world.try_resource::<GameClockGlobals>().map(|clock| *clock) else {
        return;
    };
    if let Some(mut globals) = world.try_resource_mut::<Globals>() {
        if let Some(form) = clock.hour {
            globals.set(form, hour_of_day);
        }
        if let Some(form) = clock.days_passed {
            globals.set(form, clock.base_days + day as f32 + hour_of_day / 24.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gregorian_weekday_matches_known_dates() {
        assert_eq!(gregorian_weekday(1970, 1, 1), 4, "a Thursday");
        assert_eq!(gregorian_weekday(2000, 2, 29), 2, "a Tuesday");
        assert_eq!(gregorian_weekday(2024, 7, 4), 4, "a Thursday");
        assert_eq!(gregorian_weekday(2281, 10, 18), 2, "a Tuesday");
    }

    /// `GameHour` is the game clock: the sync writes it, and a world without the global (or
    /// without globals at all) is left alone.
    #[test]
    fn the_game_hour_global_follows_the_clock() {
        use byroredux_plugin::esm::records::SettingValue;
        let mut world = byroredux_core::ecs::World::new();
        sync_game_clock_globals(&world, 0, 5.0); // no resources: nothing to do
        let mut records = HashMap::new();
        records.insert(0x38, GlobalRecord { form_id: 0x38, editor_id: "GameHour".into(), value: SettingValue::Float(8.0) });
        records.insert(0x39, GlobalRecord { form_id: 0x39, editor_id: "GameDaysPassed".into(), value: SettingValue::Float(1.0) });
        world.insert_resource(Globals::from_records(&records));
        world.insert_resource(GameClockGlobals::from_records(&records));
        assert_eq!(world.resource::<GameClockGlobals>().hour, Some(0x38));
        sync_game_clock_globals(&world, 3, 21.0);
        let globals = world.resource::<Globals>();
        assert_eq!(globals.get(0x38), Some(21.0));
        assert_eq!(globals.get(0x39), Some(1.0 + 3.0 + 21.0 / 24.0), "the authored count plus the days run");
    }

    use byroredux_plugin::esm::records::SettingValue;

    fn rec(form_id: u32, value: SettingValue) -> GlobalRecord {
        GlobalRecord {
            form_id,
            editor_id: String::new(),
            value,
        }
    }

    #[test]
    fn from_records_widens_each_value_kind() {
        let mut src = HashMap::new();
        src.insert(0x0100_0001, rec(0x0100_0001, SettingValue::Int(7)));
        src.insert(0x0100_0002, rec(0x0100_0002, SettingValue::Float(2.5)));
        src.insert(0x0100_0003, rec(0x0100_0003, SettingValue::Short(-3)));
        src.insert(
            0x0100_0004,
            rec(0x0100_0004, SettingValue::String("x".into())),
        );

        let globals = Globals::from_records(&src);
        assert_eq!(globals.get(0x0100_0001), Some(7.0));
        assert_eq!(globals.get(0x0100_0002), Some(2.5));
        assert_eq!(globals.get(0x0100_0003), Some(-3.0));
        assert_eq!(globals.get(0x0100_0004), Some(0.0), "string → 0.0");
        assert_eq!(globals.get(0x0DEAD), None, "unknown FormID → None");
    }

    #[test]
    fn set_overwrites_runtime_value() {
        let mut globals = Globals::new();
        assert!(globals.is_empty());
        globals.set(0x42, 1.0);
        globals.set(0x42, 9.0);
        assert_eq!(globals.get(0x42), Some(9.0));
        assert_eq!(globals.len(), 1);
    }
}
