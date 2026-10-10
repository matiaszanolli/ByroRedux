//! ByroRedux launcher.
//!
//! Finds installed games, checks them, and starts the engine — the first thing
//! a person who has never opened a terminal sees.
//!
//! Runs on OpenGL (eframe's `glow` backend) rather than Vulkan, deliberately:
//! the launcher has to open on a machine where the engine's own Vulkan 1.3 +
//! ray-query requirement does not hold, because that is exactly the machine
//! whose owner needs to be told why. See `docs/engine/launcher.md` §0.
//!
//! It also stays resident behind the engine, so an engine that dies during
//! startup produces a readable window instead of a vanished process.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod engine;
mod preflight;
mod settings_screen;
mod state;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let profiles_path = match parse_args(std::env::args().skip(1).collect()) {
        Ok(None) => {
            print_usage();
            return Ok(());
        }
        Ok(Some(options)) => match options.profiles {
            Some(path) => path,
            None => byroredux_game_detect::profiles::user_profiles_path().unwrap_or_else(|| {
                eprintln!(
                    "byro-launcher: no home directory for the default profiles file; \
                     pass --profiles <path>"
                );
                std::process::exit(2);
            }),
        },
        Err(error) => {
            eprintln!("byro-launcher: {error}");
            eprintln!("Run `byro-launcher --help` for usage.");
            std::process::exit(2);
        }
    };
    #[cfg(target_os = "linux")]
    let force_x11 = std::env::var_os("BYROREDUX_LAUNCHER_X11").is_some();
    #[cfg(not(target_os = "linux"))]
    let force_x11 = false;

    run_launcher(native_options(force_x11), profiles_path)
}

/// Parsed command line — currently one flag.
#[derive(Debug, Default, PartialEq)]
struct Options {
    profiles: Option<PathBuf>,
}

/// Strict on purpose (#5294, the launcher twin of #5167): a Play click
/// writes `[roots]` into the profiles file, so a mistyped flag
/// (`--profile`) or a flag swallowed as a value (`--profiles --x`) must
/// be a usage error rather than silently retargeting real writes at the
/// default file. `Ok(None)` means `--help`.
fn parse_args(args: Vec<String>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--profiles" => {
                let value = args
                    .next()
                    .filter(|value| !value.starts_with('-'))
                    .ok_or("--profiles needs a path")?;
                // #5476 — absolutise against the launcher's cwd: the
                // engine is spawned with `current_dir(engine.parent())`,
                // so a relative path would otherwise resolve against the
                // ENGINE's directory (for both the boot request and
                // `$BYRO_PROFILES`) and every Play would fail with a
                // confusing boot-request error.
                let path = absolutise_against_cwd(&value);
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
/// `current_dir` failing leaves the value untouched; the downstream
/// error message then names what was actually attempted.
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
    println!("byro-launcher — find installed games and start the engine");
    println!();
    println!("USAGE:");
    println!("  byro-launcher [--profiles <path>]");
    println!();
    println!("OPTIONS:");
    println!("  --profiles <path>  Use this profiles file instead of");
    println!("                     ~/.byroredux/profiles.toml. The engine is");
    println!("                     started with $BYRO_PROFILES set to it, so");
    println!("                     detection, [roots] writes and the engine's");
    println!("                     own profile resolution all agree (#5294).");
    println!("  -h, --help         Print this help.");
}

fn native_options(force_x11: bool) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 620.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("ByroRedux"),
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    if force_x11 {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
    }

    options
}

fn run_launcher(options: eframe::NativeOptions, profiles_path: PathBuf) -> eframe::Result<()> {
    eframe::run_native(
        "ByroRedux",
        options,
        Box::new(move |_cc| Ok(Box::new(app::LauncherApp::new(profiles_path)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    /// #5294 — the pre-fix scanner (`position(== "--profiles")` +
    /// `nth(i+1)`) accepted every typo: `--profile x` silently selected the
    /// default file (retargeting real `[roots]` writes), a trailing bare
    /// `--profiles` did the same, and `--profiles --x` took `--x` as the
    /// path. Strict parsing rejects all three the way `byro-detect`
    /// already does (#5167).
    #[test]
    fn profiles_parsing_is_strict() {
        // The one accepted spelling.
        assert_eq!(
            parse_args(args(&["--profiles", "/tmp/alt.toml"])),
            Ok(Some(Options {
                profiles: Some(PathBuf::from("/tmp/alt.toml")),
            }))
        );
        assert_eq!(parse_args(args(&[])), Ok(Some(Options::default())));

        // #5476 — a relative path is absolutised against the launcher's
        // cwd: the engine spawns with current_dir(engine.parent()), so a
        // bare name would otherwise resolve (boot request AND
        // $BYRO_PROFILES) against the engine's directory.
        let relative = parse_args(args(&["--profiles", "alt.toml"])).unwrap().unwrap();
        let path = relative.profiles.expect("profiles set");
        assert!(
            path.is_absolute() && path.ends_with("alt.toml"),
            "a relative --profiles must come back absolute against the cwd, got {path:?}"
        );

        // Typos and missing values are errors, not silent defaults.
        assert!(parse_args(args(&["--profile", "/tmp/alt.toml"])).is_err());
        assert!(parse_args(args(&["--profiles"])).is_err());
        assert!(parse_args(args(&["--profiles", "--write"])).is_err());
        assert!(parse_args(args(&["--profiles", "a", "--profiles", "b"])).is_err());
        assert!(parse_args(args(&["--profiles", "a", "stray"])).is_err());

        // Help wins regardless of position.
        assert_eq!(parse_args(args(&["--profiles", "a", "--help"])), Ok(None));
    }
}
