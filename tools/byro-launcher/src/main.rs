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

    let profiles_path = profiles_path();
    #[cfg(target_os = "linux")]
    let force_x11 = std::env::var_os("BYROREDUX_LAUNCHER_X11").is_some();
    #[cfg(not(target_os = "linux"))]
    let force_x11 = false;

    run_launcher(native_options(force_x11), profiles_path)
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

/// The same per-user profiles file the engine's profile loader reads, so a path
/// the launcher records is a path the engine honours.
fn profiles_path() -> PathBuf {
    if let Some(path) = std::env::args()
        .position(|arg| arg == "--profiles")
        .and_then(|index| std::env::args().nth(index + 1))
    {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".byroredux").join("profiles.toml")
}
