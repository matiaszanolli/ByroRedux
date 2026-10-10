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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("byro-detect: {error}");
            eprintln!("Run `byro-detect --help` for usage.");
            return ExitCode::from(2);
        }
    };
    let write = options.write;
    // #5294 — the shared per-user path, not a local copy: the engine skips
    // the per-user layer when no home exists, while this tool would have
    // written a cwd-relative `.byroredux/profiles.toml` the engine never
    // reads. Error instead.
    let profiles_path = match options
        .profiles
        .or_else(detect::profiles::user_profiles_path)
    {
        Some(path) => path,
        None => {
            eprintln!("byro-detect: no home directory for the default profiles file; pass --profiles <path>");
            return ExitCode::from(2);
        }
    };

    // #5476 — the registry from the SAME file this run was pointed at,
    // so a custom `[profiles.<key>]` block is known here.
    let registry = detect::profiles::load_with_user_path(Some(&profiles_path));
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

#[derive(Debug, Default, PartialEq)]
struct Options {
    write: bool,
    profiles: Option<PathBuf>,
}

/// Parse the arguments after the program name. `Ok(None)` means `--help`.
///
/// Strict on purpose (#5167): `--write` rewrites a file, so a mistyped flag
/// (`--profile`) or a flag swallowed as a value (`--profiles --write`) must be
/// a usage error rather than silently retargeting the write at the default
/// profiles file.
fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--write" => options.write = true,
            "--profiles" => {
                let value = args
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .ok_or("--profiles needs a path")?;
                // #5476 — same absolutisation as the launcher: this
                // tool's cwd is not where its outputs resolve later.
                let path = absolutise_against_cwd(value);
                if options.profiles.replace(path).is_some() {
                    return Err("--profiles given more than once".to_string());
                }
            }
            other => return Err(format!("unrecognised argument {other:?}")),
        }
    }
    Ok(Some(options))
}

/// #5476 — resolve `value` against the current directory when relative.
fn absolutise_against_cwd(value: &str) -> PathBuf {
    let value = PathBuf::from(value);
    if value.is_absolute() {
        return value;
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(&value))
        .unwrap_or(value)
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
    println!("                      The engine reads it too when started with");
    println!("                      BYRO_PROFILES=<path> ($BYRO_PROFILES overrides the home");
    println!("                      default in the engine's profile loader, #5294); without it,");
    println!("                      --write here edits a file the engine never reads.");
    println!("  -h, --help          Show this message.");
    println!();
    println!("Unknown arguments, and a --profiles value that starts with '-', are usage");
    println!("errors (exit status 2); nothing is written.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse_args(&args)
    }

    #[test]
    fn accepts_the_documented_flags() {
        assert_eq!(parse(&[]), Ok(Some(Options::default())));
        assert_eq!(
            parse(&["--write", "--profiles", "/tmp/p.toml"]),
            Ok(Some(Options {
                write: true,
                profiles: Some(PathBuf::from("/tmp/p.toml")),
            }))
        );
        assert_eq!(parse(&["--write", "--help"]), Ok(None));
    }

    /// #5476 — a relative `--profiles` is absolutised against the cwd,
    /// mirroring the launcher.
    #[test]
    fn a_relative_profiles_path_comes_back_absolute() {
        let parsed = parse(&["--profiles", "alt.toml"]).unwrap().unwrap();
        let path = parsed.profiles.expect("profiles set");
        assert!(
            path.is_absolute() && path.ends_with("alt.toml"),
            "relative --profiles must be absolutised against the cwd, got {path:?}"
        );
    }

    /// #5167 — a typo must not fall back to writing the default file.
    #[test]
    fn rejects_an_unknown_flag() {
        assert!(parse(&["--write", "--profile", "/tmp/p.toml"]).is_err());
        assert!(parse(&["stray"]).is_err());
    }

    /// #5167 — `--profiles --write` must not take `--write` as the path.
    #[test]
    fn rejects_a_flag_as_the_profiles_value() {
        assert!(parse(&["--profiles", "--write"]).is_err());
        assert!(parse(&["--profiles"]).is_err());
        assert!(parse(&["--profiles", "a", "--profiles", "b"]).is_err());
    }
}
