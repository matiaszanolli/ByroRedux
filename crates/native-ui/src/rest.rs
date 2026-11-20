use byroredux_core::ui_labels::UiLabels;

use crate::{PageOutputs, RestAction, RestView};

/// Fallback UI for the same session and actions the MenuXml driver uses.
pub(super) fn draw(
    ui: &mut egui::Ui,
    labels: &UiLabels,
    view: Option<&RestView>,
    outputs: &mut PageOutputs,
) {
    match view {
        Some(RestView::Duration { sleeping, hours }) => {
            let verb = labels.text(if *sleeping { "Sleep" } else { "Wait" });
            ui.heading(verb);
            let mut hours = *hours;
            if ui
                .add(egui::Slider::new(&mut hours, 1..=24).text(labels.text("Hours")))
                .changed()
            {
                outputs.rest_actions.push(RestAction::SetHours(hours));
            }
            if ui.button(verb).clicked() {
                outputs.rest_actions.push(RestAction::Confirm);
            }
        }
        Some(RestView::LevelUp {
            level,
            attributes,
            selected,
            ready,
        }) => {
            ui.heading(format!("{} {level}", labels.text("Level")));
            ui.label(format!(
                "{selected}/3 {}",
                labels.text("Attributes selected")
            ));
            egui::Grid::new("level_up_attributes")
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    for (code, attribute) in attributes.iter().enumerate() {
                        let mut selected = attribute.selected;
                        if ui
                            .add_enabled(
                                attribute.enabled,
                                egui::Checkbox::new(&mut selected, labels.text(attribute.name)),
                            )
                            .changed()
                        {
                            outputs
                                .rest_actions
                                .push(RestAction::ToggleAttribute(code as u8));
                        }
                        ui.label(&attribute.base);
                        ui.label(format!("+{}", attribute.bonus));
                        ui.end_row();
                    }
                });
            if ui
                .add_enabled(*ready, egui::Button::new(labels.text("Done")))
                .clicked()
            {
                outputs.rest_actions.push(RestAction::Confirm);
            }
        }
        None => return,
    }
    if ui.button(labels.text("Cancel")).clicked() {
        outputs.rest_actions.push(RestAction::Cancel);
    }
}
