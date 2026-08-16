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

use egui::{
    Align2, Color32, Context, CornerRadius, Frame, Id, Margin, Order, RichText, Stroke, Window,
};

use byroredux_core::settings::{SettingChange, SettingControl, SettingEntry, SettingValue};

use crate::PanelState;

/// Read-only snapshot of the engine-side state the panels render.
/// Built each frame by the binary right before `DebugUiState::run`.
/// Cloning is cheap — `MetricsSnapshot` is small, the entity list
/// is name-only.
#[derive(Default, Clone)]
pub struct PanelSnapshot {
    /// Native in-world HUD prompt. Unlike the debug panels, this remains
    /// visible when the F3 operator overlay is closed.
    ///
    /// `&'static str`, not `String` (#2680 / PERF-D1-02): this is the one
    /// field rebuilt on every frame including the overlay-hidden path, and
    /// every prompt the producer can name is a compile-time constant. A
    /// future prompt that interpolates a reference name wants
    /// `Cow<'static, str>` here, not a per-frame `String`.
    pub interaction_prompt: Option<InteractionPrompt>,
    /// Whether the native reticle should be drawn while gameplay owns input.
    pub show_crosshair: bool,
    /// Whether contextual interaction prompts should be drawn.
    pub show_prompts: bool,
    pub metrics: Option<MetricsSnapshotView>,
    /// Deterministically ordered clone of the universal settings registry.
    /// Settings are small and only cloned while the overlay is visible.
    pub settings: Vec<SettingEntry>,
    /// Player inventory, populated only while the native inventory page is
    /// visible. `None` means the current scene has no character player.
    pub inventory: Option<InventorySnapshot>,
    /// `(entity_id, name)` pairs. `None` until the operator opens
    /// the Entities panel — populating this on every frame would
    /// be unnecessary work for an overlay that's hidden most of
    /// the time.
    pub entities: Option<Vec<(u32, String)>>,
}

/// Allocation-free native prompt assembled from the active binding and the
/// selected interaction's verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractionPrompt {
    pub binding: &'static str,
    pub verb: &'static str,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InventorySnapshot {
    pub items: Vec<InventoryItemView>,
    pub total_weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryItemView {
    pub index: u32,
    pub form_id: u32,
    pub name: String,
    pub category: &'static str,
    pub details: String,
    pub count: u32,
    pub value: u32,
    pub weight: f32,
    pub equipped: bool,
    pub equippable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryAction {
    ToggleEquip { index: u32 },
}

/// Draw the small gameplay HUD layer shared with the debug renderer.
pub fn draw_hud(ctx: &Context, snapshot: &PanelSnapshot) {
    if snapshot.show_crosshair {
        let center = ctx.content_rect().center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            Order::Middle,
            Id::new("gameplay_crosshair"),
        ));
        let stroke = Stroke::new(1.5, Color32::from_white_alpha(220));
        for (from, to) in [
            (egui::vec2(-7.0, 0.0), egui::vec2(-2.0, 0.0)),
            (egui::vec2(2.0, 0.0), egui::vec2(7.0, 0.0)),
            (egui::vec2(0.0, -7.0), egui::vec2(0.0, -2.0)),
            (egui::vec2(0.0, 2.0), egui::vec2(0.0, 7.0)),
        ] {
            painter.line_segment([center + from, center + to], stroke);
        }
    }

    let Some(prompt) = snapshot
        .show_prompts
        .then_some(snapshot.interaction_prompt)
        .flatten()
    else {
        return;
    };

    egui::Area::new(Id::new("interaction_prompt"))
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -64.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(190))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("[{}] {}", prompt.binding, prompt.verb))
                            .size(20.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
        });
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
    /// `None` per-entry means the bracket didn't run this snapshot
    /// cycle — distinct from `Some(0.0)`, a bracket that genuinely
    /// completed sub-microsecond. #2513 / REN-D20-NEW-03.
    pub gpu_pass_ms: Vec<(String, Option<f32>)>,
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
    /// Mutations applied to canonical player inventory state after egui drops
    /// its read-only frame snapshot.
    pub inventory_actions: Vec<InventoryAction>,
    /// True when the operator asked to refresh the entity list.
    /// The binary rebuilds the snapshot's `entities` next frame.
    pub refresh_entities: bool,
    /// Close the native pause menu and return input to the world.
    pub resume_game: bool,
    /// Perform the same orderly shutdown as the window close button.
    pub quit_game: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameMenuPage {
    #[default]
    Pause,
    Settings,
    Inventory,
}

/// Persistent navigation state for the player-facing native menu.
#[derive(Debug, Clone, Default)]
pub struct GameMenuState {
    pub visible: bool,
    pub page: GameMenuPage,
    pub selected_section: String,
    pub selected_inventory_category: String,
    pub selected_inventory_index: Option<u32>,
}

/// Player-facing pause/settings surface. This deliberately consumes the same
/// `SettingEntry` snapshot and emits the same `SettingChange` values as the F3
/// operator panel, so there is one validation/application path.
pub fn draw_game_menu(
    ctx: &Context,
    snapshot: &PanelSnapshot,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    let viewport = ctx.content_rect();
    egui::Area::new(Id::new("game_menu_backdrop"))
        .order(Order::Foreground)
        .fixed_pos(viewport.min)
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(205))
                .show(ui, |ui| {
                    ui.set_min_size(viewport.size());
                });
        });

    let title = match state.page {
        GameMenuPage::Pause => "Paused",
        GameMenuPage::Settings => "Settings",
        GameMenuPage::Inventory => "Inventory",
    };
    Window::new(title)
        .id(Id::new("game_menu_window"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(match state.page {
            GameMenuPage::Pause => egui::vec2(360.0, 390.0),
            GameMenuPage::Settings => egui::vec2(760.0, 620.0),
            GameMenuPage::Inventory => egui::vec2(900.0, 620.0),
        })
        .frame(
            Frame::window(&ctx.style())
                .fill(Color32::from_rgb(20, 23, 29))
                .stroke(Stroke::new(1.0, Color32::from_gray(75)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| match state.page {
            GameMenuPage::Pause => draw_pause_page(ui, state, outputs),
            GameMenuPage::Settings => draw_game_settings(ui, &snapshot.settings, state, outputs),
            GameMenuPage::Inventory => {
                draw_inventory_page(ui, snapshot.inventory.as_ref(), state, outputs)
            }
        });
}

fn draw_pause_page(ui: &mut egui::Ui, state: &mut GameMenuState, outputs: &mut PanelOutputs) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new("BYROREDUX")
                .size(13.0)
                .strong()
                .color(Color32::from_rgb(145, 175, 210)),
        );
        ui.heading(RichText::new("Paused").size(34.0));
        ui.add_space(24.0);
        if wide_button(ui, "Continue").clicked() {
            outputs.resume_game = true;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Settings").clicked() {
            state.page = GameMenuPage::Settings;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Inventory").clicked() {
            state.page = GameMenuPage::Inventory;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Quit to desktop").clicked() {
            outputs.quit_game = true;
        }
        ui.add_space(20.0);
        ui.label(RichText::new("Esc  Continue").small().color(Color32::GRAY));
    });
}

fn wide_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [280.0, 42.0],
        egui::Button::new(RichText::new(label).size(18.0)),
    )
}

fn draw_game_settings(
    ui: &mut egui::Ui,
    settings: &[SettingEntry],
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            state.page = GameMenuPage::Pause;
        }
        ui.heading("Settings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Changes are saved automatically")
                    .small()
                    .color(Color32::GRAY),
            );
        });
    });
    ui.separator();

    let mut sections: Vec<&str> = settings
        .iter()
        .map(|entry| entry.section.as_str())
        .collect();
    sections.sort_unstable_by_key(|section| section_rank(section));
    sections.dedup();
    if !sections
        .iter()
        .any(|section| *section == state.selected_section)
    {
        state.selected_section = sections.first().copied().unwrap_or_default().to_owned();
    }

    ui.horizontal_wrapped(|ui| {
        for section in &sections {
            ui.selectable_value(&mut state.selected_section, (*section).to_owned(), *section);
        }
    });
    ui.separator();

    if sections.is_empty() {
        ui.label("No settings are registered yet.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in settings
                .iter()
                .filter(|entry| entry.section == state.selected_section)
            {
                draw_player_setting(ui, entry, outputs);
                ui.add_space(7.0);
            }
        });
}

fn draw_inventory_page(
    ui: &mut egui::Ui,
    inventory: Option<&InventorySnapshot>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            state.page = GameMenuPage::Pause;
        }
        ui.heading("Inventory");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Tab / Esc  Close")
                    .small()
                    .color(Color32::GRAY),
            );
        });
    });
    ui.separator();

    let Some(inventory) = inventory else {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.heading("Inventory unavailable");
            ui.label("Enter character mode in a loaded game cell to use the player inventory.");
        });
        return;
    };
    if inventory.items.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.heading("Your inventory is empty");
        });
        return;
    }

    let mut categories: Vec<&str> = inventory.items.iter().map(|item| item.category).collect();
    categories.sort_unstable();
    categories.dedup();
    if state.selected_inventory_category.is_empty()
        || (state.selected_inventory_category != "All"
            && !categories.contains(&state.selected_inventory_category.as_str()))
    {
        state.selected_inventory_category = "All".to_owned();
    }
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(
            &mut state.selected_inventory_category,
            "All".to_owned(),
            "All",
        );
        for category in &categories {
            ui.selectable_value(
                &mut state.selected_inventory_category,
                (*category).to_owned(),
                *category,
            );
        }
    });
    ui.separator();

    let selected_category = state.selected_inventory_category.clone();
    let category_matches =
        |item: &InventoryItemView| selected_category == "All" || item.category == selected_category;

    if !inventory
        .items
        .iter()
        .filter(|item| category_matches(item))
        .any(|item| Some(item.index) == state.selected_inventory_index)
    {
        state.selected_inventory_index = inventory
            .items
            .iter()
            .find(|item| category_matches(item))
            .map(|item| item.index);
    }

    let visible_count = inventory
        .items
        .iter()
        .filter(|item| category_matches(item))
        .count();

    ui.columns(2, |columns| {
        columns[0].set_width(430.0);
        columns[0].label(
            RichText::new(format!(
                "{} of {} stacks  ·  {:.1} total weight",
                visible_count,
                inventory.items.len(),
                inventory.total_weight
            ))
            .small()
            .color(Color32::GRAY),
        );
        columns[0].add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("native_inventory_items")
            .auto_shrink([false, false])
            .show(&mut columns[0], |ui| {
                for item in inventory.items.iter().filter(|item| category_matches(item)) {
                    let suffix = if item.count > 1 {
                        format!(" ×{}", item.count)
                    } else {
                        String::new()
                    };
                    let equipped = if item.equipped { "  ◆" } else { "" };
                    let response = ui.selectable_label(
                        state.selected_inventory_index == Some(item.index),
                        format!("{}{}{}", item.name, suffix, equipped),
                    );
                    if response.clicked() {
                        state.selected_inventory_index = Some(item.index);
                    }
                    response.on_hover_text(format!("{} · {:08X}", item.category, item.form_id));
                }
            });

        let selected = state
            .selected_inventory_index
            .and_then(|index| inventory.items.iter().find(|item| item.index == index));
        let Some(item) = selected else {
            return;
        };
        columns[1].heading(&item.name);
        columns[1].label(
            RichText::new(item.category)
                .strong()
                .color(Color32::from_rgb(145, 175, 210)),
        );
        columns[1].add_space(12.0);
        if !item.details.is_empty() {
            columns[1].label(&item.details);
        }
        egui::Grid::new("native_inventory_details")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(&mut columns[1], |ui| {
                ui.label("Count");
                ui.monospace(item.count.to_string());
                ui.end_row();
                ui.label("Weight");
                ui.monospace(format!("{:.1}", item.weight));
                ui.end_row();
                ui.label("Value");
                ui.monospace(item.value.to_string());
                ui.end_row();
                ui.label("Form ID");
                ui.monospace(format!("{:08X}", item.form_id));
                ui.end_row();
            });
        columns[1].add_space(24.0);
        if item.equippable {
            let label = if item.equipped { "Unequip" } else { "Equip" };
            if columns[1]
                .add_sized(
                    [220.0, 42.0],
                    egui::Button::new(RichText::new(label).size(18.0)),
                )
                .clicked()
            {
                outputs
                    .inventory_actions
                    .push(InventoryAction::ToggleEquip { index: item.index });
            }
        } else {
            columns[1].add_enabled(false, egui::Button::new("Equip unavailable"));
            columns[1].label(
                RichText::new("This item type has no runtime equipment contract yet.")
                    .small()
                    .color(Color32::GRAY),
            );
        }
    });
}

fn section_rank(section: &str) -> (u8, &str) {
    let rank = match section {
        "Gameplay" => 0,
        "Controls" => 1,
        "Interface" => 2,
        "Rendering" => 3,
        "Audio" => 4,
        _ => 5,
    };
    (rank, section)
}

fn draw_player_setting(ui: &mut egui::Ui, entry: &SettingEntry, outputs: &mut PanelOutputs) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let mut title = RichText::new(&entry.label).strong().size(16.0);
                if entry.restart_required {
                    title = title.color(Color32::YELLOW);
                }
                ui.label(title);
                if !entry.description.is_empty() {
                    ui.label(
                        RichText::new(&entry.description)
                            .small()
                            .color(Color32::GRAY),
                    );
                }
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
    });
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
    //
    // #2476 / REN-D20-NEW-02 — `GpuTimerSnapshot`'s own doc forbids
    // summing its fields into an unqualified "total GPU ms": every
    // bracket's START is stamped at TOP_OF_PIPE, so queue-drain time
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
    let gpu_total: f32 = m.gpu_pass_ms.iter().filter_map(|(_, v)| *v).sum();
    ui.label(
        egui::RichText::new(format!("GPU passes — Σ upper bound {:.3} ms", gpu_total)).strong(),
    )
    .on_hover_text(
        "Each bracket's START is stamped at TOP_OF_PIPE, so queue-drain \
             time from prior in-flight work can be absorbed into it. This sum \
             is a ceiling, not a precise attribution — overlapping queue-wait \
             may be double-counted across adjacent brackets. Brackets that \
             didn't run this cycle (n/a) are excluded, not counted as zero.",
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
    fn interaction_prompt_produces_hud_shapes_without_debug_panels() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                interaction_prompt: Some(InteractionPrompt {
                    binding: "E",
                    verb: "Open",
                }),
                show_prompts: true,
                ..Default::default()
            },
        );
        let output = ctx.end_pass();
        assert!(
            !output.shapes.is_empty(),
            "a gameplay prompt must generate renderable HUD geometry"
        );
    }

    #[test]
    fn crosshair_produces_hud_shapes_without_an_interaction_target() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                show_crosshair: true,
                ..Default::default()
            },
        );
        let output = ctx.end_pass();
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn native_pause_menu_draws_without_the_debug_overlay() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let mut state = GameMenuState {
            visible: true,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        draw_game_menu(&ctx, &PanelSnapshot::default(), &mut state, &mut outputs);
        let output = ctx.end_pass();
        assert!(!output.shapes.is_empty());
        assert!(!outputs.resume_game);
        assert!(!outputs.quit_game);
    }

    #[test]
    fn native_inventory_page_draws_real_item_details() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let snapshot = PanelSnapshot {
            inventory: Some(InventorySnapshot {
                total_weight: 30.0,
                items: vec![InventoryItemView {
                    index: 0,
                    form_id: 0x1234,
                    name: "Iron Armor".to_owned(),
                    category: "Armor",
                    details: "Armor rating 25.0".to_owned(),
                    count: 1,
                    value: 125,
                    weight: 30.0,
                    equipped: true,
                    equippable: true,
                }],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Inventory,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let output = ctx.end_pass();
        assert!(!output.shapes.is_empty());
        assert_eq!(state.selected_inventory_category, "All");
        assert_eq!(state.selected_inventory_index, Some(0));
        assert!(outputs.inventory_actions.is_empty());
    }

    #[test]
    fn inventory_category_filter_selects_a_visible_item() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let item = |index: u32, name: &str, category: &'static str| InventoryItemView {
            index,
            form_id: 0x1000 + index,
            name: name.to_owned(),
            category,
            details: String::new(),
            count: 1,
            value: 1,
            weight: 1.0,
            equipped: false,
            equippable: false,
        };
        let snapshot = PanelSnapshot {
            inventory: Some(InventorySnapshot {
                total_weight: 2.0,
                items: vec![item(0, "Long Barrel", "Mods"), item(1, "Desk Fan", "Junk")],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Inventory,
            selected_inventory_category: "Junk".to_owned(),
            selected_inventory_index: Some(0),
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();

        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let output = ctx.end_pass();

        assert!(!output.shapes.is_empty());
        assert_eq!(state.selected_inventory_category, "Junk");
        assert_eq!(state.selected_inventory_index, Some(1));
    }

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
}
