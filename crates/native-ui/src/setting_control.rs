//! The control a universal setting is edited with (toggle, slider, choice), shared by the player's
//! settings page and the operator overlay's settings tab, so both emit the same `SettingChange`
//! values through one validation path.

use byroredux_core::settings::{SettingControl, SettingEntry, SettingValue};
use egui::Color32;

/// Draw `entry`'s control; the new value when the user changed it.
pub fn draw_setting_control(ui: &mut egui::Ui, entry: &SettingEntry) -> Option<SettingValue> {
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
