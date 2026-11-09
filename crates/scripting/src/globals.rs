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
            start_date: byroredux_plugin::esm::records::calendar::start_date(records),
        }
    }
}

/// The game calendar: the whole game days the clock has run since the game began, and the weekday the
/// clock started on. `NOT_SAVED_BY_DESIGN`: `days_elapsed` is republished every frame from the saved game
/// clock, and `weekday_origin` is resolved from the master at load ([`install_calendar_origin`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameCalendar {
    pub days_elapsed: u64,
    /// The weekday (0 Sunday .. 6 Saturday) of day 0, resolved once at load from the master read
    /// (`byroredux_plugin::esm::records::calendar::weekday_origin`: Oblivion Morndas, Skyrim per edition,
    /// Fallout's authored start date, ...). `None` where no source establishes it: day constraints on package
    /// schedules then stay unevaluated and `GetDayOfWeek` counts from Sunday (AIP-SCHED-004). Past the loader
    /// nothing asks which game it is.
    pub weekday_origin: Option<u8>,
}

impl Resource for GameCalendar {}

pub use byroredux_plugin::esm::records::calendar::gregorian_weekday;

/// The weekday (0 Sunday .. 6 Saturday) `days_elapsed` whole days into the game, from the origin the loader
/// resolved. The engine keeps its own counter that "is not updated when the game date is changed" (CS),
/// advanced each midnight from the day a new game starts on. Shared by `GetDayOfWeek` and the package
/// schedules' Day-of-Week (#6079). Without an established origin it counts from Sunday.
pub fn day_of_week(world: &byroredux_core::ecs::World, days_elapsed: u64) -> u32 {
    let origin = world
        .try_resource::<GameCalendar>()
        .and_then(|calendar| calendar.weekday_origin)
        .unwrap_or(0);
    ((u64::from(origin) + days_elapsed) % 7) as u32
}

/// The weekday package schedules test their Day-of-Week against, `days_elapsed` days into the game — `None`
/// when the loader established no origin for the game, which leaves day constraints unevaluated. It reads the
/// calendar resource only, never the game kind (AIP-SCHED-004); the origin per game and edition is decided in
/// `byroredux_plugin::esm::records::calendar::weekday_origin` (Oblivion #6257, Fallout #6079, Skyrim and
/// Fallout 4 / Starfield per decision Q2).
pub fn package_weekday(world: &byroredux_core::ecs::World, days_elapsed: u64) -> Option<u32> {
    let origin = world.try_resource::<GameCalendar>()?.weekday_origin?;
    Some(((u64::from(origin) + days_elapsed) % 7) as u32)
}

/// Install the weekday origin the loader resolved from the master into the calendar (AIP-SCHED-004). Called
/// where the index's other per-game facts are installed.
pub fn install_calendar_origin(world: &mut byroredux_core::ecs::World, index: &byroredux_plugin::esm::EsmIndex) {
    world.init_resource::<GameCalendar>();
    if let Some(mut calendar) = world.try_resource_mut::<GameCalendar>() {
        calendar.weekday_origin = index.weekday_origin;
    }
}

/// Oblivion's month lengths, `GameMonth` 0 (Morning Star) .. 11 (Evening Star): "Same # of days as"
/// the Gregorian month, Sun's Dawn 28 (CS wiki *Special variables*). No leap year is documented.
const OBLIVION_MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// The day of the month (1-based, `GameDay`'s reading) `days_elapsed` days after the record-authored
/// start date `(GameMonth 0..=11, GameDay)`. `None` for a start date outside the calendar.
pub fn oblivion_day_of_month(start_month: u32, start_day: u32, days_elapsed: u64) -> Option<u32> {
    let start_len = *OBLIVION_MONTH_DAYS.get(start_month as usize)?;
    if !(1..=start_len).contains(&start_day) {
        return None;
    }
    let year: u64 = OBLIVION_MONTH_DAYS.iter().map(|&days| u64::from(days)).sum();
    let mut left = days_elapsed % year + u64::from(start_day - 1);
    let mut month = start_month as usize;
    loop {
        let len = u64::from(OBLIVION_MONTH_DAYS[month]);
        if left < len {
            return Some(left as u32 + 1);
        }
        left -= len;
        month = (month + 1) % 12;
    }
}

/// The day of the month package schedules test their PSDT date against (#6257) — Oblivion only, the
/// one game that authors a date (75 packages; FO3/FNV author 0 on all of theirs), advanced from the
/// `GameMonth`/`GameDay` a new game starts on (27 Last Seed in `Oblivion.esm`). `None` leaves the
/// constraint unevaluated.
pub fn package_day_of_month(world: &byroredux_core::ecs::World, days_elapsed: u64) -> Option<u32> {
    use byroredux_plugin::esm::reader::GameKind;
    if !matches!(world.try_resource::<crate::ScriptGame>()?.0, GameKind::Oblivion) {
        return None;
    }
    let (_, month, day) = world.try_resource::<GameClockGlobals>()?.start_date?;
    oblivion_day_of_month(month as u32, day as u32, days_elapsed)
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

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // MQ101's startup fragment writes GameHour before advancing to stage
    // 10. GLOB values are mutable game state, so a save/load must retain
    // the scripted value instead of silently restoring the ESM default.
    registry.register_resource::<Globals>("Globals");
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<GameCalendar>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #6257 — the Oblivion day of the month advances from the authored start (27 Last Seed) through
    /// the documented month lengths, wrapping the year.
    #[test]
    fn oblivion_day_of_month_walks_the_documented_months() {
        assert_eq!(oblivion_day_of_month(7, 27, 0), Some(27));
        assert_eq!(oblivion_day_of_month(7, 27, 4), Some(31), "Last Seed has 31 days");
        assert_eq!(oblivion_day_of_month(7, 27, 5), Some(1));
        assert_eq!(oblivion_day_of_month(7, 27, 5 + 30), Some(1), "Hearthfire has 30");
        assert_eq!(oblivion_day_of_month(0, 31, 28), Some(28), "Sun's Dawn has 28");
        assert_eq!(oblivion_day_of_month(0, 31, 29), Some(1));
        assert_eq!(oblivion_day_of_month(7, 27, 365), Some(27), "a year later");
        assert_eq!(oblivion_day_of_month(1, 29, 0), None);
        assert_eq!(oblivion_day_of_month(12, 1, 0), None);
    }

    /// AIP-SCHED-001 / -004 — the package schedules' weekday is the origin the loader put on the calendar plus the
    /// days run; it never asks which game it is. Decision Q2: Skyrim's first day is a Morndas on the launch master
    /// (HEDR 0.94) and a Sundas on SE / AE (1.71); Fallout 4 and Starfield take the Gregorian weekday of their
    /// authored start date.
    #[test]
    fn the_package_weekday_is_the_loaders_origin_plus_the_days_run() {
        use byroredux_plugin::esm::{reader::GameKind, EsmIndex};
        let weekdays = |game, hedr, date: Option<(f32, f32, f32)>| {
            let mut world = byroredux_core::ecs::World::new();
            let index = EsmIndex {
                weekday_origin: byroredux_plugin::esm::records::calendar::weekday_origin(game, hedr, date),
                ..Default::default()
            };
            install_calendar_origin(&mut world, &index);
            (0..3).map(|day| package_weekday(&world, day)).collect::<Vec<_>>()
        };
        assert_eq!(weekdays(GameKind::Skyrim, 0.94, None), [Some(1), Some(2), Some(3)], "launch edition: Morndas");
        assert_eq!(weekdays(GameKind::Skyrim, 1.71, None), [Some(0), Some(1), Some(2)], "SE: Sundas");
        // 2287-10-23 is a Sunday.
        assert_eq!(weekdays(GameKind::Fallout4, 0.95, Some((2287.0, 9.0, 23.0)))[0], Some(0));
        assert_eq!(weekdays(GameKind::Starfield, 0.0, Some((2330.0, 0.0, 1.0)))[0], Some(gregorian_weekday(2330, 1, 1)));
        assert_eq!(weekdays(GameKind::Oblivion, 0.8, None), [Some(1), Some(2), Some(3)]);
        // A week later is the same weekday.
        let mut world = byroredux_core::ecs::World::new();
        install_calendar_origin(&mut world, &EsmIndex { weekday_origin: Some(5), ..Default::default() });
        assert_eq!((package_weekday(&world, 0), package_weekday(&world, 7)), (Some(5), Some(5)));
        // No established origin: day constraints stay unevaluated, and a world with no calendar says nothing.
        assert_eq!(weekdays(GameKind::Fallout76, 0.96, None), [None, None, None]);
        assert_eq!(weekdays(GameKind::Skyrim, 0.0, None), [None, None, None], "Skyrim with no HEDR read");
        assert_eq!(package_weekday(&byroredux_core::ecs::World::new(), 3), None);
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
