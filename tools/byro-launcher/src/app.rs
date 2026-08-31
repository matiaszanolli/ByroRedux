//! The eframe application — drawing and event handling only.
//!
//! Every decision this file makes is delegated to [`crate::state`] or
//! [`crate::engine`], which are testable without a window. What is left here is
//! layout.

use std::path::PathBuf;

use byroredux_boot_request::Action;
use byroredux_game_detect::validate::Severity;

use crate::engine::{EngineProcess, EngineStatus};
use crate::state::LauncherState;

/// Which screen is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Library,
    /// Validation detail for one entry, opened by Details or by Fix.
    Details(usize),
    /// The engine exited non-zero. This is the screen that justifies the
    /// launcher being a separate, resident process at all.
    Failure {
        code: Option<i32>,
        tail: Vec<String>,
    },
}

pub struct LauncherApp {
    state: LauncherState,
    engine: Option<PathBuf>,
    running: Option<EngineProcess>,
    screen: Screen,
    status: String,
    /// Which entry's play menu is expanded, if any.
    play_menu: Option<usize>,
}

impl LauncherApp {
    pub fn new(profiles_path: PathBuf) -> Self {
        let engine = crate::engine::locate_engine_beside_self();
        let state = LauncherState::load(profiles_path);
        let status = match &engine {
            Some(_) => format!("{} game(s) found.", state.entries.len()),
            None => format!(
                "The engine ({}) was not found next to the launcher.",
                crate::engine::ENGINE_EXE
            ),
        };
        Self {
            state,
            engine,
            running: None,
            screen: Screen::Library,
            status,
            play_menu: None,
        }
    }

    /// Remember the path, write the request, start the engine.
    ///
    /// The `[roots]` write happens first and on purpose: it is what lets the
    /// engine's own profile expander resolve the archives, so the request
    /// itself stays a bare profile key.
    fn play(&mut self, index: usize, action: Action) {
        let Some(engine) = self.engine.clone() else {
            self.status = "No engine binary to launch.".to_owned();
            return;
        };
        if let Err(error) = self.state.remember() {
            // Not fatal: the request may still resolve through the shipped
            // profile. Say so and continue rather than refusing to play.
            self.status = format!("Could not save game paths: {error}");
        }
        let Some(request) = self.state.boot_request(index, action) else {
            self.status = "That game has no engine profile to launch with.".to_owned();
            return;
        };
        let path = self.boot_request_path();
        if let Err(error) = request.save(&path) {
            self.status = format!("Could not write {}: {error}", path.display());
            return;
        }
        match EngineProcess::spawn(&engine, &path) {
            Ok(process) => {
                self.running = Some(process);
                self.play_menu = None;
                self.status = "Starting…".to_owned();
            }
            Err(error) => self.status = format!("Could not start the engine: {error}"),
        }
    }

    fn boot_request_path(&self) -> PathBuf {
        self.state
            .profiles_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(byroredux_boot_request::DEFAULT_FILE_NAME)
    }

    /// Drain the running engine's status once per frame.
    fn poll_engine(&mut self) {
        let Some(process) = self.running.as_mut() else {
            return;
        };
        match process.poll() {
            EngineStatus::Running => {}
            EngineStatus::Finished => {
                self.running = None;
                self.status = "The game exited.".to_owned();
            }
            EngineStatus::Failed { code, tail } => {
                self.running = None;
                self.status = "The game exited unexpectedly.".to_owned();
                self.screen = Screen::Failure { code, tail };
            }
        }
    }

    fn browse(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Select a game's Data folder")
            .pick_folder()
        else {
            return;
        };
        self.status = if self.state.add_manual(&folder) {
            format!("Added {}.", folder.display())
        } else {
            format!(
                "{} does not contain a game the engine recognises.",
                folder.display()
            )
        };
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_engine();
        if self.running.is_some() {
            // Keep polling while the engine owns the screen, but do not spin.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Rescan").clicked() {
                        self.state.refresh();
                        self.status = format!("{} game(s) found.", self.state.entries.len());
                    }
                    if ui.button("Add game folder…").clicked() {
                        self.browse();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.screen.clone() {
            Screen::Library => self.draw_library(ui),
            Screen::Details(index) => self.draw_details(ui, index),
            Screen::Failure { code, tail } => self.draw_failure(ui, code, &tail),
        });
    }
}

impl LauncherApp {
    fn draw_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("ByroRedux");
        ui.add_space(4.0);

        if self.running.is_some() {
            ui.label("The game is running.");
            return;
        }
        if self.state.entries.is_empty() {
            ui.label("No games found.");
            ui.label(
                "Only Steam installs are detected automatically. \
                 Use “Add game folder…” to point at a game's Data folder.",
            );
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for index in 0..self.state.entries.len() {
                self.draw_card(ui, index);
                ui.add_space(6.0);
            }
        });
    }

    fn draw_card(&mut self, ui: &mut egui::Ui, index: usize) {
        // Read what the card needs before any `&mut self` call below.
        let (title, verdict, severity, launchable, path, options) = {
            let entry = &self.state.entries[index];
            (
                entry.candidate.display_name.clone(),
                entry.verdict().to_owned(),
                entry.report.verdict(),
                entry.is_launchable(),
                entry.candidate.data_dir.display().to_string(),
                entry.play_options(),
            )
        };

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.strong(&title);
                ui.colored_label(severity_color(severity), verdict);
            });
            ui.small(path);
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if launchable {
                    // One click to launch when there is only one way to start.
                    if options.len() == 1 {
                        if ui.button("Play").clicked() {
                            self.play(index, options[0].1.clone());
                        }
                    } else if !options.is_empty() && ui.button("Play\u{2026}").clicked() {
                        self.play_menu = (self.play_menu != Some(index)).then_some(index);
                    }
                    if options.is_empty() {
                        ui.label("This profile has no start points configured.");
                    }
                } else if ui.button("Fix").clicked() {
                    // A failed install swaps Play for Fix on the same button,
                    // opening straight at the reason.
                    self.screen = Screen::Details(index);
                }
                if ui.button("Details").clicked() {
                    self.screen = Screen::Details(index);
                }
            });

            if self.play_menu == Some(index) {
                ui.add_space(4.0);
                for (label, action) in &options {
                    if ui.button(label).clicked() {
                        self.play(index, action.clone());
                    }
                }
            }
        });
    }

    fn draw_details(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(entry) = self.state.entries.get(index) else {
            self.screen = Screen::Library;
            return;
        };
        if ui.button("Back").clicked() {
            self.screen = Screen::Library;
        }
        ui.add_space(4.0);
        ui.heading(&entry.candidate.display_name);
        ui.small(entry.candidate.data_dir.display().to_string());
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for check in &entry.report.checks {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        severity_color(check.severity),
                        severity_mark(check.severity),
                    );
                    ui.strong(format!("{}:", check.label));
                    ui.label(&check.detail);
                });
            }
        });

        if !entry.is_launchable() {
            ui.add_space(8.0);
            ui.label(
                "Fix the items marked ✕ and press Rescan. \
                 If the folder above is wrong, use “Add game folder…”.",
            );
        }
    }

    fn draw_failure(&mut self, ui: &mut egui::Ui, code: Option<i32>, tail: &[String]) {
        if ui.button("Back").clicked() {
            self.screen = Screen::Library;
        }
        ui.add_space(4.0);
        ui.heading("The game did not start");
        match code {
            Some(code) => ui.label(format!("The engine exited with code {code}.")),
            None => ui.label("The engine was terminated."),
        };
        ui.add_space(8.0);
        ui.label("Last output from the engine:");
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                for line in tail {
                    ui.small(line);
                }
                if tail.is_empty() {
                    ui.small("(the engine produced no output)");
                }
            });
    }
}

fn severity_color(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Ok => egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
        Severity::Warn => egui::Color32::from_rgb(0xe0, 0xa0, 0x30),
        Severity::Fail => egui::Color32::from_rgb(0xe0, 0x5c, 0x50),
    }
}

fn severity_mark(severity: Severity) -> &'static str {
    match severity {
        Severity::Ok => "ok",
        Severity::Warn => "warn",
        Severity::Fail => "FAIL",
    }
}
