//! Concrete debug-UI panels (Phase 4b of the debug-UI plan).
//!
//! Drawn inside the egui closure passed to `DebugUiState::run` by the
//! binary's main loop. Each panel reads from a pre-built
//! [`PanelSnapshot`] (a frozen view of the World resources the
//! panels need) and writes any actions back through
//! [`PanelOutputs`]; the binary applies those to the World after
//! `run` returns, sidestepping the borrow-checker conflict between
//! `&mut DebugUiState` and the world references the panels would
//! otherwise need.

use egui::{Color32, Context, Window};

use byroredux_core::settings::{SettingChange, SettingControl, SettingEntry, SettingValue};

use crate::PanelState;

/// Read-only snapshot of the engine-side state the panels render.
/// Built each frame by the binary right before `DebugUiState::run`.
/// Cloning is cheap — `MetricsSnapshot` is small, the entity list
/// is name-only.
#[derive(Default, Clone)]
pub struct PanelSnapshot {
    pub metrics: Option<MetricsSnapshotView>,
    /// Deterministically ordered clone of the universal settings registry.
    /// Settings are small and only cloned while the overlay is visible.
    pub settings: Vec<SettingEntry>,
    /// `(entity_id, name)` pairs. `None` until the operator opens
    /// the Entities panel — populating this on every frame would
    /// be unnecessary work for an overlay that's hidden most of
    /// the time.
    pub entities: Option<Vec<(u32, String)>>,
}

/// Local twin of `byroredux_core::ecs::MetricsSnapshot` — the
/// debug-ui crate doesn't depend on core's resource types directly
/// (the binary owns the conversion). Same field semantics.
#[derive(Default, Clone)]
pub struct MetricsSnapshotView {
    pub sampled_at_secs: u64,
    pub cpu_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub process_ram_mb: u64,
    pub vram_used_mb: u64,
    pub vram_reserved_mb: u64,
    pub vram_budget_mb: u64,
    pub gpu_pass_ms: Vec<(String, f32)>,
    /// CPU-side per-frame wall-clock breakdown
    /// (`fence_wait` / `submit_present` / `cmd_record` / etc.).
    /// Surfaces operations the GPU TIMESTAMP brackets can't see —
    /// fence-blocked waits, present stalls, host-side recording.
    pub cpu_pass_ms: Vec<(String, f32)>,
    /// Per-system wall-time of the most recent `Scheduler::run`,
    /// sorted desc by ms. Surfaces the ECS system that dominates
    /// `atw_scheduler_ms` when that bracket reads ~500 ms. Phase 11.
    pub top_systems_ms: Vec<(String, f32)>,
}

/// Actions the panels asked the App to perform. Drained by the
/// binary after [`DebugUiState::run`] returns.
#[derive(Default, Clone)]
pub struct PanelOutputs {
    /// NIF / cell loads to queue against `PendingDebugLoadSlot`.
    pub queued_loads: Vec<QueuedLoad>,
    /// Console expressions to evaluate via the existing
    /// `CommandRegistry`. The binary translates each into the same
    /// path the debug-server's `Eval` request takes.
    pub console_evals: Vec<String>,
    /// Validated by the universal registry after the egui frame completes.
    pub setting_changes: Vec<SettingChange>,
    /// True when the operator asked to refresh the entity list.
    /// The binary rebuilds the snapshot's `entities` next frame.
    pub refresh_entities: bool,
}

/// One queued load request. The binary maps this 1:1 onto a
/// `PendingDebugLoad` enum variant — kept as a separate type here
/// so the debug-ui crate doesn't need to depend on core's
/// `PendingDebugLoad` directly.
#[derive(Debug, Clone)]
pub enum QueuedLoad {
    Nif { path: String, label: Option<String> },
}

/// Top-level draw — orchestrates the five panel tabs. Called by
/// the binary inside `DebugUiState::run`'s closure.
pub fn draw(
    ctx: &Context,
    snapshot: &PanelSnapshot,
    state: &mut PanelState,
    outputs: &mut PanelOutputs,
) {
    Window::new("ByroRedux Debug")
        .default_width(420.0)
        .default_height(520.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.active_tab, PanelTab::Metrics, "Metrics");
                ui.selectable_value(&mut state.active_tab, PanelTab::Loader, "Loader");
                ui.selectable_value(&mut state.active_tab, PanelTab::Entities, "Entities");
                ui.selectable_value(&mut state.active_tab, PanelTab::Console, "Console");
                ui.selectable_value(&mut state.active_tab, PanelTab::Settings, "Settings");
            });
            ui.separator();

            match state.active_tab {
                PanelTab::Metrics => draw_metrics(ui, snapshot.metrics.as_ref()),
                PanelTab::Loader => draw_loader(ui, state, outputs),
                PanelTab::Entities => draw_entities(ui, snapshot.entities.as_deref(), outputs),
                PanelTab::Console => draw_console(ui, state, outputs),
                PanelTab::Settings => draw_settings(ui, &snapshot.settings, state, outputs),
            }
        });
}

/// Tab selector enum — `PartialEq` because `selectable_value` needs
/// it to highlight the active choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelTab {
    #[default]
    Metrics,
    Loader,
    Entities,
    Console,
    Settings,
}

fn draw_metrics(ui: &mut egui::Ui, snap: Option<&MetricsSnapshotView>) {
    let Some(m) = snap else {
        ui.label("Waiting for first metrics sample…");
        return;
    };
    ui.label(format!("sampled_at_secs: {}", m.sampled_at_secs));
    ui.separator();

    // CPU
    let cpu_ratio = (m.cpu_pct.clamp(0.0, 100.0)) / 100.0;
    ui.label(format!("CPU: {:.1}%", m.cpu_pct));
    ui.add(egui::ProgressBar::new(cpu_ratio).show_percentage());

    // RAM
    ui.add_space(6.0);
    ui.label(format!(
        "RAM (system): {} / {} MB",
        m.ram_used_mb, m.ram_total_mb
    ));
    let ram_ratio = ratio(m.ram_used_mb, m.ram_total_mb);
    ui.add(
        egui::ProgressBar::new(ram_ratio as f32)
            .text(format!("process RSS: {} MB", m.process_ram_mb)),
    );

    // VRAM
    ui.add_space(6.0);
    let vram_label = if m.vram_budget_mb > 0 {
        format!(
            "VRAM: {} used / {} reserved / {} budget MB",
            m.vram_used_mb, m.vram_reserved_mb, m.vram_budget_mb
        )
    } else {
        format!(
            "VRAM: {} used / {} reserved MB (budget unknown)",
            m.vram_used_mb, m.vram_reserved_mb
        )
    };
    ui.label(vram_label);
    let vram_ratio = ratio(m.vram_used_mb, m.vram_budget_mb);
    ui.add(egui::ProgressBar::new(vram_ratio as f32));

    // GPU passes
    ui.add_space(6.0);
    ui.separator();
    let gpu_total: f32 = m.gpu_pass_ms.iter().map(|(_, v)| *v).sum();
    ui.label(egui::RichText::new(format!("GPU passes — Σ {:.3} ms", gpu_total)).strong());
    if m.gpu_pass_ms.is_empty() {
        ui.label("(none reported)");
    } else {
        egui::Grid::new("gpu_passes_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, ms) in &m.gpu_pass_ms {
                    ui.label(name);
                    ui.monospace(format!("{:.3} ms", ms));
                    ui.end_row();
                }
            });
    }

    // CPU pass times — Phase 8 of the debug-UI plan. Surfaces
    // fence_wait / submit_present / cmd_record so a "GPU
    // timestamps sum < wall frame time" gap localises to the
    // CPU-side culprit.
    ui.add_space(6.0);
    ui.separator();
    let cpu_total: f32 = m.cpu_pass_ms.iter().map(|(_, v)| *v).sum();
    ui.label(egui::RichText::new(format!("CPU draw_frame — Σ {:.3} ms", cpu_total)).strong());
    if m.cpu_pass_ms.is_empty() {
        ui.label("(none reported)");
    } else {
        egui::Grid::new("cpu_passes_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, ms) in &m.cpu_pass_ms {
                    ui.label(name);
                    ui.monospace(format!("{:.3} ms", ms));
                    ui.end_row();
                }
            });
    }

    // Top systems — Phase 11. The scheduler hands out a desc-
    // sorted list of every system's wall-time. Show the top
    // (≤ 10) so the operator can see which ECS system dominates
    // `atw_scheduler_ms`.
    ui.add_space(6.0);
    ui.separator();
    let sys_count = m.top_systems_ms.len();
    ui.label(egui::RichText::new(format!("Top systems (of {})", sys_count)).strong());
    if m.top_systems_ms.is_empty() {
        ui.label("(none reported — scheduler hasn't run yet)");
    } else {
        egui::Grid::new("top_systems_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, ms) in m.top_systems_ms.iter().take(10) {
                    ui.label(name);
                    ui.monospace(format!("{:.3} ms", ms));
                    ui.end_row();
                }
            });
    }
}

fn draw_loader(ui: &mut egui::Ui, state: &mut PanelState, outputs: &mut PanelOutputs) {
    ui.label("Load a NIF mesh into the running scene.");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Path:");
        ui.add(
            egui::TextEdit::singleline(&mut state.loader_path)
                .hint_text("meshes\\…\\foo.nif or /abs/path.nif")
                .desired_width(280.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Label:");
        ui.add(
            egui::TextEdit::singleline(&mut state.loader_label)
                .hint_text("(optional)")
                .desired_width(280.0),
        );
    });
    ui.add_space(8.0);
    let path_valid = !state.loader_path.trim().is_empty();
    ui.add_enabled_ui(path_valid, |ui| {
        if ui.button("Queue load").clicked() {
            outputs.queued_loads.push(QueuedLoad::Nif {
                path: state.loader_path.trim().to_string(),
                label: if state.loader_label.trim().is_empty() {
                    None
                } else {
                    Some(state.loader_label.trim().to_string())
                },
            });
        }
    });
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Loose absolute paths are tried first, then every --bsa archive \
             the engine was launched with. Cell-load form lands with the \
             game profile registry (Phase 5).",
        )
        .small()
        .color(Color32::DARK_GRAY),
    );
}

fn draw_entities(
    ui: &mut egui::Ui,
    entities: Option<&[(u32, String)]>,
    outputs: &mut PanelOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button("Refresh").clicked() {
            outputs.refresh_entities = true;
        }
        if let Some(list) = entities {
            ui.label(format!("({} entities)", list.len()));
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| match entities {
        None => {
            ui.label("Click 'Refresh' to load the entity list.");
        }
        Some([]) => {
            ui.label("(no named entities)");
        }
        Some(list) => {
            egui::Grid::new("entities_grid")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for (id, name) in list {
                        ui.monospace(format!("{}", id));
                        ui.label(name);
                        ui.end_row();
                    }
                });
        }
    });
}

fn draw_console(ui: &mut egui::Ui, state: &mut PanelState, outputs: &mut PanelOutputs) {
    ui.horizontal(|ui| {
        ui.label("Run console commands against the engine.");
        if ui.button("Copy all").clicked() {
            let joined = state.console_history.join("\n");
            ui.ctx().copy_text(joined);
        }
        if ui.button("Clear").clicked() {
            state.console_history.clear();
        }
    });
    ui.separator();
    // Selectable monospace block. Rendering the full history as one
    // multiline label (instead of per-line `ui.monospace`) lets the
    // operator drag-select across lines and hit Ctrl+C natively;
    // the "Copy all" button above handles the no-mouse case.
    let avail = ui.available_height() - 60.0;
    egui::ScrollArea::vertical()
        .max_height(avail.max(80.0))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let joined = state.console_history.join("\n");
            let text = egui::RichText::new(joined).monospace();
            ui.add(
                egui::Label::new(text)
                    .selectable(true)
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
    ui.separator();
    let input_resp = ui.add(
        egui::TextEdit::singleline(&mut state.console_input)
            .hint_text("type a command, Enter to send")
            .desired_width(f32::INFINITY),
    );
    if input_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let text = state.console_input.trim().to_string();
        if !text.is_empty() {
            state.console_history.push(format!("byro> {}", text));
            outputs.console_evals.push(text);
            state.console_input.clear();
            input_resp.request_focus();
        }
    }
}

fn draw_settings(
    ui: &mut egui::Ui,
    settings: &[SettingEntry],
    state: &mut PanelState,
    outputs: &mut PanelOutputs,
) {
    ui.label("Universal engine settings — shared across every supported game.");
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.settings_filter)
            .hint_text("Filter settings…")
            .desired_width(f32::INFINITY),
    );
    ui.separator();

    if settings.is_empty() {
        ui.label("No settings are registered yet.");
        return;
    }

    let filter = state.settings_filter.trim().to_lowercase();
    let mut visible_count = 0usize;
    let mut current_section = String::new();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in settings {
                if !setting_matches(entry, &filter) {
                    continue;
                }
                visible_count += 1;
                if current_section != entry.section {
                    if !current_section.is_empty() {
                        ui.add_space(8.0);
                    }
                    current_section.clone_from(&entry.section);
                    ui.heading(&entry.section);
                }

                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            let mut title = egui::RichText::new(&entry.label).strong();
                            if entry.restart_required {
                                title = title.color(Color32::YELLOW);
                            }
                            ui.label(title);
                            if !entry.description.is_empty() {
                                ui.label(
                                    egui::RichText::new(&entry.description)
                                        .small()
                                        .color(Color32::GRAY),
                                );
                            }
                            ui.label(
                                egui::RichText::new(&entry.id)
                                    .small()
                                    .monospace()
                                    .color(Color32::DARK_GRAY),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if entry.value != entry.default && ui.small_button("Reset").clicked() {
                                outputs
                                    .setting_changes
                                    .push(SettingChange::new(&entry.id, entry.default.clone()));
                            }
                            if let Some(value) = draw_setting_control(ui, entry) {
                                outputs
                                    .setting_changes
                                    .push(SettingChange::new(&entry.id, value));
                            }
                        });
                    });
                    if entry.restart_required {
                        ui.label(
                            egui::RichText::new("Applied on next launch")
                                .small()
                                .color(Color32::YELLOW),
                        );
                    }
                });
                ui.add_space(4.0);
            }
        });

    if visible_count == 0 {
        ui.label("No settings match this filter.");
    }
}

fn draw_setting_control(ui: &mut egui::Ui, entry: &SettingEntry) -> Option<SettingValue> {
    match (&entry.control, &entry.value) {
        (SettingControl::Toggle, SettingValue::Bool(current)) => {
            let mut value = *current;
            ui.checkbox(&mut value, "")
                .changed()
                .then_some(SettingValue::Bool(value))
        }
        (
            SettingControl::Slider {
                min,
                max,
                step,
                unit,
            },
            SettingValue::Number(current),
        ) => {
            let mut value = *current;
            let response = ui.add(
                egui::Slider::new(&mut value, *min..=*max)
                    .step_by(*step as f64)
                    .suffix(unit.as_str()),
            );
            response.changed().then_some(SettingValue::Number(value))
        }
        (SettingControl::Choice { options }, SettingValue::Choice(current)) => {
            let mut value = current.clone();
            let selected_label = options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| value.clone());
            egui::ComboBox::from_id_salt(("universal_setting", &entry.id))
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for option in options {
                        ui.selectable_value(&mut value, option.value.clone(), &option.label);
                    }
                });
            (value != *current).then_some(SettingValue::Choice(value))
        }
        _ => {
            ui.colored_label(Color32::RED, "Invalid setting");
            None
        }
    }
}

fn setting_matches(entry: &SettingEntry, filter: &str) -> bool {
    filter.is_empty()
        || entry.id.to_lowercase().contains(filter)
        || entry.section.to_lowercase().contains(filter)
        || entry.label.to_lowercase().contains(filter)
        || entry.description.to_lowercase().contains(filter)
}

/// Safe `used / total` clamped to [0, 1]. Zero `total` collapses to
/// zero so the progress bar doesn't NaN.
fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 / total as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_filter_matches_metadata_case_insensitively() {
        let entry = SettingEntry::toggle(
            "interface.show_fps",
            "Interface",
            "Show FPS",
            "Display the current frame rate.",
            false,
        );
        assert!(setting_matches(&entry, "fps"));
        assert!(setting_matches(&entry, "interface"));
        assert!(setting_matches(&entry, "frame rate"));
        assert!(!setting_matches(&entry, "audio"));
    }
}
