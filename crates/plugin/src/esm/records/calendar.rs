//! The game calendar's weekday origin, resolved once at load (M42 AIP-SCHED-001 / -004).
//!
//! Package schedules test their `PSDT` Day-of-Week against the weekday the game clock is on. Where a
//! new game's first day falls is a per-game (and for Skyrim a per-edition) fact, so it is decided
//! here, at the plugin boundary, from the master the engine reads; everything past this point
//! (`GameCalendar.weekday_origin`, `package_weekday`, `GetDayOfWeek`) holds one number and never asks
//! which game it is.
//!
//! | Game | Origin | Source |
//! |---|---|---|
//! | Oblivion | Morndas (1) | UESP *Calendar*: "always starts on Morndas" |
//! | Fallout 3 / New Vegas | the Gregorian weekday of the authored `GameYear` / `GameMonth` / `GameDay` | no document gives the start weekday; the only vanilla use compares two days it stored, so no epoch is observable (#6079) |
//! | Skyrim, launch edition (HEDR 0.94) | Morndas (1) | owner decision Q2, 2026-11-03; 17 Last Seed 4E 201 is a Morndas on the 2011 master (UESP *Skyrim: Statistics*) |
//! | Skyrim SE / AE (HEDR 1.70+) | Sundas (0) | owner decision Q2; a Sundas on every later edition (UESP *Skyrim: Unbound*) |
//! | Fallout 4, Starfield | the Gregorian weekday of the authored start date | owner decision Q2 |
//! | Fallout 76 | none | not established; day constraints stay unevaluated |

use std::collections::HashMap;

use super::global::GlobalRecord;
use crate::esm::reader::GameKind;

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

/// The record-authored date a new game starts on, `(GameYear, GameMonth, GameDay)` with `GameMonth`
/// 0-based, found among the `GLOB` records by editor ID.
pub fn start_date(globals: &HashMap<u32, GlobalRecord>) -> Option<(f32, f32, f32)> {
    let value = |name: &str| {
        globals
            .values()
            .find(|record| record.editor_id.eq_ignore_ascii_case(name))
            .map(|record| record.value.as_f32())
    };
    value("GameYear")
        .zip(value("GameMonth"))
        .zip(value("GameDay"))
        .map(|((year, month), day)| (year, month, day))
}

/// The weekday (0 Sunday .. 6 Saturday) the game clock starts on for a master of `game` with `HEDR`
/// version `hedr_version` and the authored `start_date`; `None` when no source establishes it.
pub fn weekday_origin(game: GameKind, hedr_version: f32, start_date: Option<(f32, f32, f32)>) -> Option<u8> {
    let gregorian = || {
        start_date.map(|(year, month, day)| gregorian_weekday(year as i64, month as i64 + 1, day as i64) as u8)
    };
    match game {
        GameKind::Oblivion => Some(1),
        // Fallout's start weekday is in the executable; with no authored date the clock counts from
        // Sunday, as it did before the origin was resolved here.
        GameKind::Fallout3NV => Some(gregorian().unwrap_or(0)),
        GameKind::Skyrim if hedr_version >= 1.6 => Some(0),
        GameKind::Skyrim if hedr_version > 0.0 && hedr_version < 1.0 => Some(1),
        // A Skyrim master whose HEDR names neither edition: not established.
        GameKind::Skyrim => None,
        GameKind::Fallout4 | GameKind::Starfield => gregorian(),
        GameKind::Fallout76 => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::SettingValue;

    fn globals(year: f32, month: f32, day: f32) -> HashMap<u32, GlobalRecord> {
        [("GameYear", year), ("GameMonth", month), ("GameDay", day)]
            .into_iter()
            .enumerate()
            .map(|(i, (name, value))| {
                let form_id = 0x10 + i as u32;
                (form_id, GlobalRecord { form_id, editor_id: name.into(), value: SettingValue::Float(value) })
            })
            .collect()
    }

    #[test]
    fn the_gregorian_weekday_matches_known_dates() {
        assert_eq!(gregorian_weekday(1970, 1, 1), 4, "a Thursday");
        assert_eq!(gregorian_weekday(2000, 2, 29), 2, "a Tuesday");
        assert_eq!(gregorian_weekday(2024, 7, 4), 4, "a Thursday");
        assert_eq!(gregorian_weekday(2281, 10, 18), 2, "a Tuesday");
    }

    #[test]
    fn the_start_date_is_read_by_editor_id_and_needs_all_three() {
        assert_eq!(start_date(&globals(2287.0, 9.0, 23.0)), Some((2287.0, 9.0, 23.0)));
        let mut partial = globals(2287.0, 9.0, 23.0);
        partial.retain(|_, record| record.editor_id != "GameDay");
        assert_eq!(start_date(&partial), None);
    }

    /// Decision Q2: the launch edition's master (HEDR 0.94) starts on a Morndas, every later one (HEDR 1.70 / 1.71)
    /// on a Sundas; the edition is read here and nowhere past it.
    #[test]
    fn skyrim_starts_on_morndas_on_the_launch_master_and_sundas_after() {
        assert_eq!(weekday_origin(GameKind::Skyrim, 0.94, None), Some(1));
        assert_eq!(weekday_origin(GameKind::Skyrim, 1.70, None), Some(0));
        assert_eq!(weekday_origin(GameKind::Skyrim, 1.71, None), Some(0));
        assert_eq!(weekday_origin(GameKind::Skyrim, 0.0, None), None, "no HEDR read: not established");
        assert_eq!(weekday_origin(GameKind::Skyrim, 1.3, None), None, "an unrecognised HEDR: not established");
    }

    #[test]
    fn the_other_games_resolve_their_documented_origin() {
        let date = Some((2281.0, 9.0, 18.0)); // 2281-10-18, a Tuesday
        assert_eq!(weekday_origin(GameKind::Oblivion, 0.8, None), Some(1));
        assert_eq!(weekday_origin(GameKind::Fallout3NV, 1.34, date), Some(2));
        assert_eq!(weekday_origin(GameKind::Fallout3NV, 1.34, None), Some(0), "no authored date: from Sunday");
        assert_eq!(weekday_origin(GameKind::Fallout4, 0.95, date), Some(2));
        assert_eq!(weekday_origin(GameKind::Fallout4, 0.95, None), None, "no start date: not established");
        assert_eq!(weekday_origin(GameKind::Starfield, 0.0, date), Some(2));
        assert_eq!(weekday_origin(GameKind::Fallout76, 0.96, date), None);
    }
}
