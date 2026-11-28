//! The locomotion clip census of PRD `character-movement` (MOT-DX-005): what each installed game
//! ships to move its actors with, and what the engine can read of it.
//!
//! Only vanilla archives are read. The report holds file names, text-key and annotation labels,
//! counts and one rounded speed per base-set gait, and no keys, curves or timings (MOT-SUP-007).
//!
//! ```text
//! cargo run --release -p byroredux-nif --example locomotion_census -- --write   # update the report
//! cargo run --release -p byroredux-nif --example locomotion_census -- --check   # fail if it is stale
//! cargo run --release -p byroredux-nif --example locomotion_census              # print it
//! cargo test -p byroredux-nif --example locomotion_census                       # the name rules
//! ```
//!
//! Game directories are found as the NIF real-data tests find them (`BYROREDUX_*_DATA`, else the
//! reference machine's paths).

#[path = "../../tests/common/mod.rs"]
mod common;
mod report;
mod roles;
mod scan;

use common::Game;
use std::process::ExitCode;

const REPORT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/engine/locomotion-census.md"
);

fn main() -> ExitCode {
    let mode = std::env::args().nth(1);
    if !matches!(mode.as_deref(), None | Some("--write" | "--check")) {
        eprintln!("usage: locomotion_census [--write | --check]");
        return ExitCode::FAILURE;
    }
    let mut games = Vec::new();
    for game in Game::ALL {
        match scan::scan(game) {
            Some(census) => games.push(census),
            None => {
                // A report missing a game would read as "that game ships nothing".
                eprintln!(
                    "{}: data or an archive is missing; no report produced",
                    game.label()
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let text = report::render(&games);
    match mode.as_deref() {
        Some("--write") => {
            if let Err(error) = std::fs::write(REPORT, &text) {
                eprintln!("{REPORT}: {error}");
                return ExitCode::FAILURE;
            }
            eprintln!("wrote {REPORT}");
        }
        Some("--check") => {
            if std::fs::read_to_string(REPORT).map_or(true, |on_disk| on_disk != text) {
                eprintln!("{REPORT} is stale: rerun with --write");
                return ExitCode::FAILURE;
            }
            eprintln!("{REPORT} is current");
        }
        _ => print!("{text}"),
    }
    ExitCode::SUCCESS
}
