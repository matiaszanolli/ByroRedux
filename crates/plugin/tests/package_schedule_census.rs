//! M42 AIP-SCHED exit census: for each vanilla master, the weekday origin the loader resolves and how many of
//! its packages author a day-of-week, month or zero-duration constraint (PRD `ai-packages-completion` §5.6,
//! AIP-DX-003). A package with a day constraint on a game whose origin is `None` is left unevaluated; the exit
//! is zero of those on every game whose calendar is settled.
//!
//! Opt in (needs the game installed; a missing install fails under `BYROREDUX_REQUIRE_GAME_DATA=1` instead of
//! reporting `ok`):
//!
//! ```sh
//! cargo test --release -p byroredux-plugin --test package_schedule_census -- --ignored --nocapture
//! ```

use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::{PackDays, PackMonth};
use byroredux_plugin::esm::test_paths;

/// Parse `master` and print/return `(origin, packages, day_constrained, month_constrained, zero_duration)`.
fn census(label: &str, master: std::path::PathBuf) -> Option<(Option<u8>, usize, usize, usize, usize)> {
    if !master.is_file() {
        test_paths::require_or_skip(&format!("{label} master {}", master.display()));
        return None;
    }
    let index = parse_esm(&std::fs::read(&master).expect("read the master")).expect("parse the master");
    let (mut day, mut month, mut zero) = (0, 0, 0);
    for package in index.packages.values() {
        let Some(schedule) = package.schedule else { continue };
        day += usize::from(schedule.days != PackDays::Any);
        month += usize::from(schedule.month != PackMonth::Any);
        zero += usize::from(schedule.is_unsatisfiable());
    }
    println!(
        "[AIP-SCHED {label}] {:?} HEDR-resolved weekday origin {:?}: {} packages, {day} day-constrained, {month} month-constrained, {zero} zero-duration",
        index.game,
        index.weekday_origin,
        index.packages.len(),
    );
    Some((index.weekday_origin, index.packages.len(), day, month, zero))
}

fn assert_evaluated(label: &str, row: Option<(Option<u8>, usize, usize, usize, usize)>) {
    let Some((origin, packages, day, month, _)) = row else { return };
    assert!(packages > 0, "{label}: no packages parsed");
    assert!(
        origin.is_some() || day == 0,
        "{label}: {day} day-constrained packages and no weekday origin - they would run every day"
    );
    // No vanilla master authors a month constraint (PRD §5.6); a non-zero count means the offset is wrong.
    assert_eq!(month, 0, "{label}: a month constraint appeared - check the form-version-122 offset");
}

#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn oblivion_schedule_census() {
    assert_evaluated("Oblivion", census("Oblivion", test_paths::oblivion_esm()));
}

#[test]
#[ignore = "needs Fallout3.esm on disk"]
fn fallout3_schedule_census() {
    assert_evaluated("FO3", census("FO3", test_paths::fo3_esm()));
}

#[test]
#[ignore = "needs FalloutNV.esm on disk"]
fn fallout_new_vegas_schedule_census() {
    assert_evaluated("FNV", census("FNV", test_paths::fnv_esm()));
}

#[test]
#[ignore = "needs Skyrim.esm on disk"]
fn skyrim_schedule_census() {
    let row = census("Skyrim", test_paths::skyrim_se_esm());
    assert_evaluated("Skyrim", row);
    if let Some((origin, ..)) = row {
        // The Steam install is Special Edition (HEDR 1.70+): a Sundas (decision Q2).
        assert_eq!(origin, Some(0), "Skyrim SE starts on a Sundas");
    }
}

#[test]
#[ignore = "needs Fallout4.esm on disk"]
fn fallout4_schedule_census() {
    assert_evaluated("FO4", census("FO4", test_paths::fo4_esm()));
}

#[test]
#[ignore = "needs Starfield.esm on disk"]
fn starfield_schedule_census() {
    assert_evaluated("Starfield", census("Starfield", test_paths::starfield_esm()));
}
