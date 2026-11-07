//! The developer studio overlay: metrics, studio, gallery, loader, entities, console and settings tabs, and the GPU-pass helpers (#6578, split from `panels.rs`).

use super::*;
use egui::{Color32, Context, Frame, RichText, Window};

use byroredux_core::settings::{SettingChange, SettingControl, SettingEntry, SettingValue};
use byroredux_sdk::studio::{MaterialValue, StudioCommand, StudioSnapshot, TransformValue};

use crate::PanelState;

/// Top-level draw — orchestrates the five panel tabs. Called by
/// the binary inside `DebugUiState::run`'s closure.
pub fn draw(
    ctx: &Context,
    snapshot: &PanelSnapshot,
    state: &mut PanelState,
    outputs: &mut PanelOutputs,
) {
    let studio = snapshot.studio.as_ref();
    let window = Window::new(if studio.is_some() {
        "ByroRedux Studio"
    } else {
        "ByroRedux Debug"
    })
    .default_width(if studio.is_some() { 560.0 } else { 420.0 })
    .default_height(520.0)
    .resizable(true);
    let window = if studio.is_some() {
        window.frame(Frame::window(&ctx.global_style()).fill(Color32::from_black_alpha(240)))
    } else {
        window
    };
    window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            if studio.is_some() {
                ui.selectable_value(&mut state.active_tab, PanelTab::Studio, "Studio");
            }
            ui.selectable_value(&mut state.active_tab, PanelTab::Metrics, "Metrics");
            ui.selectable_value(&mut state.active_tab, PanelTab::Loader, "Loader");
            ui.selectable_value(&mut state.active_tab, PanelTab::Entities, "Entities");
            ui.selectable_value(&mut state.active_tab, PanelTab::Console, "Console");
            ui.selectable_value(&mut state.active_tab, PanelTab::Settings, "Settings");
        });
        ui.separator();

        match state.active_tab {
            PanelTab::Studio => draw_studio(ui, studio, state, outputs),
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
    Studio,
    Loader,
    Entities,
    Console,
    Settings,
}

fn draw_studio(
    ui: &mut egui::Ui,
    snapshot: Option<&StudioSnapshot>,
    state: &mut PanelState,
    outputs: &mut PanelOutputs,
) {
    let Some(snapshot) = snapshot else {
        ui.label("No Studio document is open.");
        return;
    };

    ui.heading(&snapshot.source_label);
    draw_gallery(ui, snapshot, state, outputs);
    ui.label(
        RichText::new(format!(
            "{} editable objects · revision {}",
            snapshot.objects.len(),
            snapshot.revision
        ))
        .small()
        .color(Color32::GRAY),
    );
    ui.horizontal(|ui| {
        if ui.button("Pick at crosshair").clicked() {
            outputs.studio_commands.push(StudioCommand::PickFromView);
        }
        ui.label(RichText::new("F3 toggles editor / fly controls").small());
    });
    ui.separator();

    ui.columns(2, |columns| {
        columns[0].set_width(220.0);
        columns[0].label(RichText::new("Scene objects").strong());
        egui::ScrollArea::vertical()
            .id_salt("studio_object_list")
            .max_height(260.0)
            .show(&mut columns[0], |ui| {
                for object in &snapshot.objects {
                    let label = if object.name.is_empty() {
                        format!("Object {}", object.id)
                    } else {
                        format!("{}  #{}", object.name, object.id)
                    };
                    if ui
                        .selectable_label(snapshot.selected == Some(object.id), label)
                        .clicked()
                    {
                        outputs
                            .studio_commands
                            .push(StudioCommand::Select(Some(object.id)));
                    }
                }
            });

        let Some(selected) = snapshot
            .selected
            .and_then(|id| snapshot.objects.iter().find(|object| object.id == id))
        else {
            columns[1].heading("Inspector");
            columns[1].label("Select an object from the list or use the crosshair picker.");
            return;
        };

        columns[1].heading(if selected.name.is_empty() {
            format!("Object {}", selected.id)
        } else {
            selected.name.clone()
        });
        let mut transform = selected.transform;
        columns[1].label(RichText::new("Transform").strong());
        let transform_changed =
            edit_vec3(&mut columns[1], "Position", &mut transform.translation, 0.1)
                | edit_vec3(
                    &mut columns[1],
                    "Rotation °",
                    &mut transform.rotation_degrees,
                    0.25,
                )
                | columns[1]
                    .horizontal(|ui| {
                        ui.label("Scale");
                        ui.add(
                            egui::DragValue::new(&mut transform.scale)
                                .speed(0.01)
                                .range(0.001..=10_000.0),
                        )
                        .changed()
                    })
                    .inner;
        if transform_changed {
            outputs.studio_commands.push(StudioCommand::SetTransform {
                object: selected.id,
                value: sanitize_transform(transform),
            });
        }
        columns[1].horizontal(|ui| {
            if ui.button("Reset transform").clicked() {
                outputs
                    .studio_commands
                    .push(StudioCommand::ResetTransform(selected.id));
            }
            if ui.button("Frame selection").clicked() {
                outputs
                    .studio_commands
                    .push(StudioCommand::FrameSelection(selected.id));
            }
        });

        if let Some(mut material) = selected.material {
            columns[1].separator();
            columns[1].label(RichText::new("Material").strong());
            let mut changed = columns[1]
                .horizontal(|ui| {
                    ui.label("Base color");
                    ui.color_edit_button_rgb(&mut material.diffuse_color)
                        .changed()
                })
                .inner;
            changed |= edit_scalar(
                &mut columns[1],
                "Metalness",
                &mut material.metalness,
                0.0..=1.0,
            );
            changed |= edit_scalar(
                &mut columns[1],
                "Roughness",
                &mut material.roughness,
                0.0..=1.0,
            );
            changed |= edit_scalar(&mut columns[1], "Alpha", &mut material.alpha, 0.0..=1.0);
            changed |= edit_scalar(&mut columns[1], "IOR", &mut material.ior, 1.0..=3.0);
            if changed {
                outputs.studio_commands.push(StudioCommand::SetMaterial {
                    object: selected.id,
                    value: sanitize_material(material),
                });
            }
        }
    });
}

/// Material gallery: pick a game, filter its meshes, add one at a time.
fn draw_gallery(
    ui: &mut egui::Ui,
    snapshot: &StudioSnapshot,
    state: &mut PanelState,
    outputs: &mut PanelOutputs,
) {
    let catalog = &snapshot.catalog;
    egui::CollapsingHeader::new(RichText::new("Add object").strong())
        .id_salt("studio_gallery")
        .default_open(true)
        .show(ui, |ui| {
            if catalog.games.is_empty() {
                ui.label(
                    RichText::new("No installed game data found (games root / profiles).")
                        .small()
                        .color(Color32::GRAY),
                );
                return;
            }
            if state.studio_game.is_empty() {
                if let Some(game) = &catalog.game {
                    state.studio_game = game.clone();
                }
            }
            let mut browse = false;
            ui.horizontal(|ui| {
                let current = catalog
                    .games
                    .iter()
                    .find(|game| game.key == state.studio_game)
                    .map_or("Choose a game…", |game| game.name.as_str());
                egui::ComboBox::from_id_salt("studio_gallery_game")
                    .selected_text(current)
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for game in &catalog.games {
                            if ui
                                .selectable_label(game.key == state.studio_game, &game.name)
                                .clicked()
                                && game.key != state.studio_game
                            {
                                state.studio_game = game.key.clone();
                                browse = true;
                            }
                        }
                    });
                browse |= ui
                    .add(
                        egui::TextEdit::singleline(&mut state.studio_filter)
                            .hint_text("filter, e.g. clutter bottle")
                            .desired_width(220.0),
                    )
                    .changed();
            });
            if browse && !state.studio_game.is_empty() {
                state.studio_pick = None;
                outputs.studio_commands.push(StudioCommand::BrowseCatalog {
                    game: state.studio_game.clone(),
                    filter: state.studio_filter.clone(),
                });
            }

            if let Some(game) = &catalog.game {
                ui.label(
                    RichText::new(format!(
                        "{} shown · {} matching · {} meshes",
                        catalog.page.matches.len(),
                        catalog.page.total_matches,
                        catalog.total_assets
                    ))
                    .small()
                    .color(Color32::GRAY),
                );
                let mut add: Option<String> = None;
                egui::ScrollArea::vertical()
                    .id_salt("studio_gallery_matches")
                    .max_height(150.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for path in &catalog.page.matches {
                            let picked = state.studio_pick.as_deref() == Some(path.as_str());
                            let response = ui.selectable_label(picked, path);
                            if response.clicked() {
                                state.studio_pick = Some(path.clone());
                            }
                            if response.double_clicked() {
                                add = Some(path.clone());
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.studio_pick.is_some(),
                            egui::Button::new("Add to room"),
                        )
                        .clicked()
                    {
                        add = state.studio_pick.clone();
                    }
                    ui.label(RichText::new("double-click a row to add it").small());
                });
                if let Some(path) = add {
                    outputs.studio_commands.push(StudioCommand::AddAsset {
                        game: game.clone(),
                        path,
                    });
                }
            }

            if !snapshot.assets.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("In the room").strong());
                for asset in &snapshot.assets {
                    ui.horizontal(|ui| {
                        if ui.small_button("Remove").clicked() {
                            outputs
                                .studio_commands
                                .push(StudioCommand::RemoveAsset(asset.id));
                        }
                        ui.label(
                            RichText::new(format!(
                                "{} · {} ({} objects)",
                                asset.game, asset.path, asset.object_count
                            ))
                            .small(),
                        );
                    });
                }
                if ui.button("Clear room").clicked() {
                    outputs.studio_commands.push(StudioCommand::ClearAssets);
                }
            }
            if let Some(status) = &snapshot.status {
                ui.label(RichText::new(status).small().color(Color32::LIGHT_YELLOW));
            }
        });
    ui.separator();
}

fn edit_vec3(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], speed: f64) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut changed = false;
        for axis in value {
            changed |= ui.add(egui::DragValue::new(axis).speed(speed)).changed();
        }
        changed
    })
    .inner
}

fn edit_scalar(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::Slider::new(value, range)).changed()
    })
    .inner
}

fn sanitize_transform(mut value: TransformValue) -> TransformValue {
    if !value.scale.is_finite() {
        value.scale = 1.0;
    }
    value.scale = value.scale.clamp(0.001, 10_000.0);
    value
}

fn sanitize_material(mut value: MaterialValue) -> MaterialValue {
    value.diffuse_color = value.diffuse_color.map(|v| v.clamp(0.0, 1.0));
    value.metalness = value.metalness.clamp(0.0, 1.0);
    value.roughness = value.roughness.clamp(0.0, 1.0);
    value.alpha = value.alpha.clamp(0.0, 1.0);
    value.ior = value.ior.clamp(1.0, 3.0);
    value
}

fn draw_metrics(ui: &mut egui::Ui, snap: Option<&byroredux_core::ecs::MetricsSnapshot>) {
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

    if let (Some(used), Some(budget)) = (m.vulkan_heap_used_mb, m.vulkan_heap_budget_mb) {
        ui.label(format!(
            "Vulkan heap: {used} / {budget} MB (live device-wide)"
        ));
    }
    if let (Some(used), Some(total)) = (m.driver_vram_used_mb, m.driver_vram_total_mb) {
        ui.label(format!("NVIDIA VRAM: {used} / {total} MB (driver-wide)"));
    }
    if let Some(busy) = m.gpu_busy_pct {
        ui.label(format!("GPU busy (NVIDIA): {busy:.0}%"));
        ui.add(egui::ProgressBar::new((busy / 100.0).clamp(0.0, 1.0)));
    }
    if let Some(busy) = m.gpu_memory_busy_pct {
        ui.label(format!("GPU memory-controller busy (NVIDIA): {busy:.0}%"));
        ui.add(egui::ProgressBar::new((busy / 100.0).clamp(0.0, 1.0)));
    }

    // GPU passes
    //
    // #2476 / REN-D20-NEW-02 — `GpuTimerSnapshot`'s own doc forbids
    // summing its fields into an unqualified "total GPU ms": every
    // many brackets' START is stamped at TOP_OF_PIPE, so queue-drain time
    // from prior in-flight work is absorbed into whichever bracket
    // happens to be starting when it drains, and that overlapping
    // wait can double-count across adjacent brackets. The label below
    // carries that caveat instead of presenting the sum as a precise
    // wall-GPU-time figure the adjacent CPU Σ could be compared
    // against 1:1.
    ui.add_space(6.0);
    ui.separator();
    // #2513 / REN-D20-NEW-03 — sum only the brackets that actually ran
    // this cycle. A `None` entry (skipped bracket) contributing a clean
    // `0.0` made the sum look more complete/trustworthy than it was —
    // the sibling gap REN-D20-NEW-02 flagged in the same report.
    let gpu_total = gpu_pass_upper_bound_ms(&m.gpu_pass_ms);
    ui.label(
        egui::RichText::new(format!("GPU passes — Σ upper bound {:.3} ms", gpu_total)).strong(),
    )
    .on_hover_text(
        "Many brackets start at TOP_OF_PIPE, so queue-drain \
             time from prior in-flight work can be absorbed into it. This sum \
             is a ceiling, not a precise attribution — overlapping queue-wait \
             may be double-counted across adjacent brackets. Brackets that \
             didn't run this cycle (n/a) are excluded, not counted as zero, \
             and child brackets nested inside a listed parent (volumetrics_inject \
             / volumetrics_integrate / volumetrics_local_sim inside volumetrics) \
             are shown but not summed.",
    );
    if m.gpu_pass_ms.is_empty() {
        ui.label("(none reported)");
    } else {
        egui::Grid::new("gpu_passes_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, ms) in &m.gpu_pass_ms {
                    ui.label(name);
                    let cell = ui.monospace(format_gpu_pass_ms(*ms));
                    if ms.is_none() {
                        cell.on_hover_text(
                            "bracket did not run this snapshot cycle \
                             (or GPU timestamps are unavailable)",
                        );
                    }
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
    ui.label(egui::RichText::new("CPU phases").strong()).on_hover_text(
        "These intervals overlap: atw_post contains render_one_frame, and \
         rof_draw_call contains the draw sub-phases. Adding all rows does \
         not measure total frame time. fog_cluster is excluded from cmd_record.",
    );
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

pub(super) fn draw_setting_control(ui: &mut egui::Ui, entry: &SettingEntry) -> Option<SettingValue> {
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

/// `(child, parent)` `gpu_pass_ms` rows whose GPU bracket is recorded
/// *inside* the parent's, so the parent's time already includes theirs
/// (#4977): `record_volumetrics_pass` wraps `VolumetricsPipeline::dispatch`,
/// which stamps the inject, integrate and local-grid sim brackets (#6754), the direct-shadow
/// filter is dispatched inside the SVGF bracket (#5959), and the
/// cloud-shadow bake is recorded inside the sky-cube one (#6316). Shown as rows,
/// summed only on a cycle the parent did not run (the filter still runs
/// without SVGF).
const NESTED_GPU_BRACKETS: &[(&str, &str)] = &[
    ("volumetrics_inject", "volumetrics"),
    ("volumetrics_integrate", "volumetrics"),
    ("volumetrics_local_sim", "volumetrics"),
    ("shadow_filter", "svgf"),
    ("cloud_shadow", "sky_cube"),
];

/// The overlay's "GPU passes Σ": every bracket that ran this cycle (#2513),
/// excluding a [`NESTED_GPU_BRACKETS`] child whose parent ran, so no pass is
/// counted twice (#4977).
fn gpu_pass_upper_bound_ms(gpu_pass_ms: &std::collections::BTreeMap<String, Option<f32>>) -> f32 {
    let nested_in_a_running_parent = |name: &str| {
        NESTED_GPU_BRACKETS.iter().any(|&(child, parent)| {
            child == name && gpu_pass_ms.get(parent).is_some_and(Option::is_some)
        })
    };
    gpu_pass_ms
        .iter()
        .filter(|(name, _)| !nested_in_a_running_parent(name))
        .filter_map(|(_, v)| *v)
        .sum()
}

/// Render one `gpu_pass_ms` grid cell's value text — the millisecond
/// figure for a bracket that ran this cycle, or `"n/a"` for one that
/// didn't (distinct from a bracket that genuinely completed at `0.000
/// ms`). #2513 / REN-D20-NEW-03.
fn format_gpu_pass_ms(ms: Option<f32>) -> String {
    match ms {
        Some(ms) => format!("{ms:.3} ms"),
        None => "n/a".to_string(),
    }
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

    /// Regression for #2513 / REN-D20-NEW-03: an inactive bracket
    /// (`None`) must render as "n/a", not an indistinguishable
    /// `"0.000 ms"` — the exact ambiguity #2278 fixed at the producer
    /// but that had no consumer until this fix.
    #[test]
    fn inactive_bracket_renders_as_na_not_zero() {
        assert_eq!(format_gpu_pass_ms(None), "n/a");
        assert_eq!(format_gpu_pass_ms(Some(0.0)), "0.000 ms");
        assert_eq!(format_gpu_pass_ms(Some(1.184)), "1.184 ms");
    }

    /// #4977 — the volumetrics inject/integrate brackets are nested inside
    /// the `volumetrics` bracket, so the Σ over a snapshot carrying all three
    /// equals the parent-only sum.
    #[test]
    fn gpu_pass_sum_excludes_nested_child_brackets() {
        let rows: std::collections::BTreeMap<String, Option<f32>> = [
            ("svgf".to_string(), Some(0.5)),
            ("volumetrics".to_string(), Some(1.0)),
            ("volumetrics_inject".to_string(), Some(0.6)),
            ("volumetrics_integrate".to_string(), Some(0.3)),
            ("ssao".to_string(), None),
        ]
        .into_iter()
        .collect();
        assert_eq!(gpu_pass_upper_bound_ms(&rows), 1.5);
        let parents_only: std::collections::BTreeMap<_, _> = rows
            .iter()
            .filter(|(n, _)| !n.starts_with("volumetrics_"))
            .map(|(n, v)| (n.clone(), *v))
            .collect();
        assert_eq!(
            gpu_pass_upper_bound_ms(&rows),
            gpu_pass_upper_bound_ms(&parents_only)
        );
    }

    /// #6754 — the local-grid sim is recorded inside the `volumetrics` bracket
    /// (`VolumetricsPipeline::dispatch`, stage B2), so a fire scene's Σ must not
    /// add its time a second time.
    #[test]
    fn the_local_grid_sim_is_not_summed_with_the_volumetrics_bracket_that_contains_it() {
        let rows: std::collections::BTreeMap<String, Option<f32>> = [
            ("volumetrics".to_string(), Some(1.0)),
            ("volumetrics_local_sim".to_string(), Some(0.4)),
            ("composite".to_string(), Some(0.25)),
        ]
        .into_iter()
        .collect();
        assert_eq!(gpu_pass_upper_bound_ms(&rows), 1.0 + 0.25);
        // With the parent absent (a skipped volumetrics cycle) the child
        // would count on its own, like the shadow filter without SVGF.
        let orphan: std::collections::BTreeMap<String, Option<f32>> =
            [("volumetrics_local_sim".to_string(), Some(0.4))].into_iter().collect();
        assert_eq!(gpu_pass_upper_bound_ms(&orphan), 0.4);
    }

    /// #6316 — the cloud-shadow bake is recorded inside the sky-cube bracket,
    /// so the overlay's Σ counts the sky cube alone.
    #[test]
    fn cloud_shadow_is_not_summed_with_the_sky_cube_that_contains_it() {
        let rows: std::collections::BTreeMap<String, Option<f32>> = [
            ("sky_cube".to_string(), Some(0.4)),
            ("cloud_shadow".to_string(), Some(0.3)),
            ("composite".to_string(), Some(0.25)),
        ]
        .into_iter()
        .collect();
        assert_eq!(gpu_pass_upper_bound_ms(&rows), 0.4 + 0.25);
    }

    /// #5959 — the shadow filter is nested in SVGF when SVGF runs; with SVGF
    /// absent the filter runs on its own and counts.
    #[test]
    fn shadow_filter_counts_only_when_svgf_did_not_run() {
        let mut rows: std::collections::BTreeMap<String, Option<f32>> = [
            ("svgf".to_string(), Some(0.9)),
            ("shadow_filter".to_string(), Some(0.35)),
            ("composite".to_string(), Some(0.25)),
        ]
        .into_iter()
        .collect();
        assert_eq!(gpu_pass_upper_bound_ms(&rows), 0.9 + 0.25);
        rows.insert("svgf".to_string(), None);
        assert_eq!(gpu_pass_upper_bound_ms(&rows), 0.35 + 0.25);
    }
}
