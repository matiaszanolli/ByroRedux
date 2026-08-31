//! The Settings screen.
//!
//! Renders the *same* [`SettingsRegistry`] the engine builds and writes the
//! *same* file the engine reads before `VulkanContext` is created. That is the
//! "one model, two skins" rule from `docs/engine/launcher.md` §4: launcher and
//! in-game menu share the model and the persistence, and differ only in widgets.
//!
//! One thing this screen can do that the in-game menu cannot: nothing here is
//! restart-required, because the process has not started. `SettingEntry`'s
//! `restart_required` flag is therefore rendered as information rather than as a
//! reason to grey a control out.

use byroredux_core::settings::{SettingControl, SettingEntry, SettingValue, SettingsRegistry};
use byroredux_settings_io::presets::{apply, PresetFile, CUSTOM_LABEL};
use byroredux_settings_io::SettingsPersistence;

use crate::preflight::{Blocker, Capabilities};

/// Settings state, owned by the app.
pub struct SettingsState {
    pub registry: SettingsRegistry,
    pub presets: PresetFile,
    pub persistence: SettingsPersistence,
    /// `Ok` describes the adapter, `Err` says why the engine will not start.
    pub gpu: Result<Capabilities, Blocker>,
    /// Set when the user has been shown the recommendation, so it is offered
    /// once rather than nagging on every frame.
    pub recommendation_applied: bool,
    pub dirty: bool,
}

impl SettingsState {
    /// Build the registry, load the stored values over it, probe the GPU.
    pub fn load() -> Self {
        let mut registry = SettingsRegistry::default();
        if let Err(error) =
            byroredux_core::settings::builtin::register_builtin_settings(&mut registry)
        {
            log::error!("built-in settings are invalid: {error}");
        }
        let persistence = SettingsPersistence::discover();
        byroredux_settings_io::load(&mut registry, &persistence);
        Self {
            registry,
            presets: PresetFile::load_default(),
            persistence,
            gpu: crate::preflight::probe(),
            recommendation_applied: false,
            dirty: false,
        }
    }

    /// Write through the shared persistence, which preserves keys this
    /// registry does not know — the engine's key bindings among them.
    pub fn save(&mut self) {
        byroredux_settings_io::save(&self.registry, &self.persistence);
        self.dirty = false;
    }

    /// The preset the current values match, or `None` for a hand-tuned set.
    fn active_preset(&self) -> Option<String> {
        self.presets
            .active(&self.registry)
            .map(|(slug, _)| slug.to_owned())
    }
}

/// Draw the screen. Returns `true` when the user asked to go back.
pub fn draw(ui: &mut egui::Ui, state: &mut SettingsState) -> bool {
    let back = ui.button("Back").clicked();
    ui.add_space(4.0);
    ui.heading("Settings");

    draw_gpu(ui, state);
    ui.add_space(8.0);
    draw_presets(ui, state);
    ui.add_space(8.0);
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut sections: Vec<String> = state
            .registry
            .entries()
            .map(|entry| entry.section.clone())
            .collect();
        sections.sort();
        sections.dedup();

        for section in sections {
            ui.add_space(6.0);
            ui.strong(&section);
            let entries: Vec<SettingEntry> = state
                .registry
                .entries()
                .filter(|entry| entry.section == section)
                .cloned()
                .collect();
            for entry in entries {
                if let Some(value) = draw_control(ui, &entry) {
                    match state.registry.set(&entry.id, value) {
                        Ok(true) => state.dirty = true,
                        Ok(false) => {}
                        Err(error) => log::warn!("rejected {}: {error}", entry.id),
                    }
                }
            }
        }
    });

    if state.dirty {
        state.save();
    }
    back
}

fn draw_gpu(ui: &mut egui::Ui, state: &mut SettingsState) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match &state.gpu {
            Ok(caps) => {
                ui.small(caps.summary());
                match caps.verdict() {
                    Ok(()) => {
                        if !caps.meets_rt_floor() {
                            ui.colored_label(
                                egui::Color32::from_rgb(0xe0, 0xa0, 0x30),
                                format!(
                                    "Below the {} GB the ray-traced lighting wants. \
                                     Expect to run at a lower preset.",
                                    crate::preflight::RT_VRAM_FLOOR_BYTES / (1024 * 1024 * 1024)
                                ),
                            );
                        }
                        let recommended = caps.recommended_preset().to_owned();
                        let already = state.active_preset().as_deref() == Some(&*recommended);
                        if !already && !state.recommendation_applied {
                            if let Some(preset) = state.presets.presets.get(&recommended).cloned() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("Recommended for this card: {}", preset.name));
                                    if ui.button("Use it").clicked() {
                                        apply(&preset, &mut state.registry);
                                        state.recommendation_applied = true;
                                        state.dirty = true;
                                    }
                                });
                            }
                        }
                    }
                    Err(blocker) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(0xe0, 0x5c, 0x50),
                            blocker.explain(),
                        );
                    }
                }
            }
            Err(blocker) => {
                ui.colored_label(egui::Color32::from_rgb(0xe0, 0x5c, 0x50), blocker.explain());
            }
        }
    });
}

fn draw_presets(ui: &mut egui::Ui, state: &mut SettingsState) {
    let ordered: Vec<(String, String)> = state
        .presets
        .ordered()
        .into_iter()
        .map(|(slug, preset)| (slug.clone(), preset.name.clone()))
        .collect();
    if ordered.is_empty() {
        return;
    }
    let active = state.active_preset();
    ui.horizontal(|ui| {
        ui.label("Preset:");
        for (slug, name) in &ordered {
            let selected = active.as_deref() == Some(&**slug);
            if ui.selectable_label(selected, name).clicked() {
                if let Some(preset) = state.presets.presets.get(slug).cloned() {
                    apply(&preset, &mut state.registry);
                    state.dirty = true;
                }
            }
        }
        if active.is_none() {
            ui.label(CUSTOM_LABEL);
        }
    });
    if let Some(description) = active
        .as_deref()
        .and_then(|slug| state.presets.presets.get(slug))
        .map(|preset| preset.description.clone())
    {
        ui.small(description);
    }
}

/// One control. Returns the new value when the user changed it.
fn draw_control(ui: &mut egui::Ui, entry: &SettingEntry) -> Option<SettingValue> {
    let mut changed = None;
    ui.horizontal(|ui| {
        ui.label(&entry.label).on_hover_text(&entry.description);
        match (&entry.control, &entry.value) {
            (SettingControl::Toggle, SettingValue::Bool(current)) => {
                let mut value = *current;
                if ui.checkbox(&mut value, "").changed() {
                    changed = Some(SettingValue::Bool(value));
                }
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
                let slider = egui::Slider::new(&mut value, *min..=*max)
                    .step_by(f64::from(*step))
                    .suffix(unit.clone());
                if ui.add(slider).changed() {
                    changed = Some(SettingValue::Number(value));
                }
            }
            (SettingControl::Choice { options }, SettingValue::Choice(current)) => {
                let label = options
                    .iter()
                    .find(|option| &option.value == current)
                    .map(|option| option.label.clone())
                    .unwrap_or_else(|| current.clone());
                egui::ComboBox::from_id_salt(&entry.id)
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for option in options {
                            if ui
                                .selectable_label(&option.value == current, &option.label)
                                .clicked()
                            {
                                changed = Some(SettingValue::Choice(option.value.clone()));
                            }
                        }
                    });
            }
            // A control/value mismatch is a registry bug, not a user-facing
            // state; say so rather than drawing nothing.
            _ => {
                ui.weak("(unsupported control)");
            }
        }
        if entry.restart_required {
            // True in the engine's menu, not here: the process has not started.
            ui.weak("applies at next launch");
        }
    });
    changed
}
