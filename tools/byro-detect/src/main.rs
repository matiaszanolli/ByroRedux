//! Find installed Bethesda titles, check them, and optionally remember where
//! they are.
//!
//! This is P1's demonstrable gate. It runs the launcher's entire
//! install-discovery path with no window, no GPU, and no engine — which makes
//! it both the way to develop that path and the fallback for a user whose
//! machine cannot start the launcher at all.
//!
//! ```text
//! byro-detect                     # report what is installed
//! byro-detect --write             # also remember the paths in [roots]
//! byro-detect --profiles <path>   # use a different per-user profiles file
//! ```
//!
//! `--write` is what makes `--game <key>` correct on a machine that is not the
//! developer's: it records each detected data directory as a `[roots]` entry,
//! which the engine's profile loader applies over the shipped registry.

use std::path::PathBuf;
use std::process::ExitCode;

use byroredux_game_detect as detect;
use byroredux_game_detect::validate::{Severity, ValidationReport};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return ExitCode::SUCCESS;
    }
    let write = args.iter().any(|arg| arg == "--write");
    let profiles_path = value_of(&args, "--profiles")
        .map(PathBuf::from)
        .unwrap_or_else(default_profiles_path);

    let registry = detect::profiles::load_default();
    let candidates = detect::detect_all(&profiles_path);

    if candidates.is_empty() {
        println!("No supported games found.");
        println!();
        println!("Searched these Steam roots:");
        let roots = detect::steam::steam_roots();
        if roots.is_empty() {
            println!("  (none — no Steam installation found)");
        }
        for root in roots {
            println!("  {}", root.display());
        }
        println!();
        println!(
            "Non-Steam installs are not detected yet. Record one by hand in {}:",
            profiles_path.display()
        );
        println!("  [roots]");
        println!("  skyrim_se = \"/path/to/Skyrim Special Edition/Data\"");
        return ExitCode::FAILURE;
    }

    let mut launchable = 0usize;
    for candidate in &candidates {
        let report = match registry.get(&candidate.profile) {
            Some(entry) => detect::validate::validate(entry, &candidate.data_dir),
            None => {
                println!(
                    "{} — no profile named {:?} in the registry; skipping",
                    candidate.display_name, candidate.profile
                );
                continue;
            }
        };
        if report.is_launchable() {
            launchable += 1;
        }
        print_report(candidate, &report);
    }

    println!(
        "{launchable} of {} detected game(s) ready to launch.",
        candidates.len()
    );

    if write {
        let overrides = detect::overrides_for(&candidates);
        match overrides.merge_into_file(&profiles_path) {
            Ok(()) => println!(
                "Recorded {} path(s) in {}.",
                overrides.roots.len(),
                profiles_path.display()
            ),
            Err(error) => {
                eprintln!("Could not write {}: {error}", profiles_path.display());
                return ExitCode::FAILURE;
            }
        }
    } else if candidates.iter().any(|c| c.source == detect::Source::Steam) {
        println!("Re-run with --write to remember these paths for `--game <key>`.");
    }

    if launchable == 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn print_report(candidate: &detect::Candidate, report: &ValidationReport) {
    let verdict = match report.verdict() {
        Severity::Ok => "ready",
        Severity::Warn => "ready, with warnings",
        Severity::Fail => "NOT ready",
    };
    let source = match candidate.source {
        detect::Source::Steam => "Steam",
        detect::Source::Configured => "configured",
        detect::Source::Manual => "manual",
    };
    println!();
    println!("{} — {verdict}  [{source}]", candidate.display_name);
    println!("  {}", candidate.data_dir.display());
    for check in &report.checks {
        let mark = match check.severity {
            Severity::Ok => "ok  ",
            Severity::Warn => "warn",
            Severity::Fail => "FAIL",
        };
        println!("  {mark}  {}: {}", check.label, check.detail);
    }
}

fn value_of(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

/// The same per-user file the engine's profile loader reads.
fn default_profiles_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".byroredux").join("profiles.toml")
}

fn print_usage() {
    println!("byro-detect — find installed games the engine can load");
    println!();
    println!("USAGE:");
    println!("  byro-detect [--write] [--profiles <path>]");
    println!();
    println!("OPTIONS:");
    println!("  --write             Record detected paths in the profiles file's [roots] table,");
    println!("                      which makes `--game <key>` resolve correctly on this machine.");
    println!("  --profiles <path>   Use this per-user profiles file instead of the default.");
    println!("  -h, --help          Show this message.");
}
