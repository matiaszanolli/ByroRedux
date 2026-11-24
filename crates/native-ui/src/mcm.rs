//! The Mod Configuration page (LMC-UI-002, `docs/engine/mcm-host.md` §3.3): the registered mods,
//! the open mod's pages, the page's two-column option grid as the script built it, its info text,
//! and the slider, menu, colour, key-map and input dialogs and the blocking message.
//!
//! The page never changes a value itself. A click is a [`McmAction`]; the script answers through
//! its setters and the next snapshot shows the result (SkyUI's contract). Every text the page
//! draws arrives translated in the snapshot, or is a label or a listed engine word
//! (GUI-PAGE-005, `native-page-strings.tsv`, page `mcm`).

use byroredux_core::ui_labels::UiLabels;
use egui::{Color32, RichText};

use crate::{
    McmAction, McmCellKind, McmCellView, McmCustomContent, McmDialogView, McmSnapshot, PageOutputs,
    MCM_FLAG_DISABLED, MCM_FLAG_HIDDEN,
};

/// What the open dialog is editing before it is accepted. Seeded when a dialog opens; a dialog
/// the script closes clears it.
#[derive(Debug, Clone, Default)]
pub struct McmDraft {
    /// The dialog the working values belong to: its option id and kind.
    key: Option<(u32, &'static str)>,
    slider: f32,
    menu: i32,
    color: [u8; 3],
    text: String,
    /// The option the pointer was last over, so a highlight is reported once per option.
    highlighted: Option<u32>,
}

/// The dialog's identity: the option it belongs to and its kind. A message has no option.
fn dialog_key(dialog: &McmDialogView) -> (u32, &'static str) {
    match dialog {
        McmDialogView::Slider { id, .. } => (*id, "slider"),
        McmDialogView::Menu { id, .. } => (*id, "menu"),
        McmDialogView::Color { id, .. } => (*id, "color"),
        McmDialogView::Input { id, .. } => (*id, "input"),
        McmDialogView::KeyMap { id, .. } => (*id, "keymap"),
        McmDialogView::Message { .. } => (0, "message"),
    }
}

pub(super) fn draw(
    ui: &mut egui::Ui,
    labels: &UiLabels,
    mcm: Option<&McmSnapshot>,
    page: &mut crate::GameMenuPage,
    draft: &mut McmDraft,
    outputs: &mut PageOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button(format!("← {}", labels.text("Back"))).clicked() {
            *page = crate::GameMenuPage::Pause;
            outputs.mcm_actions.push(McmAction::Close);
        }
        ui.heading("Mod Configuration");
    });
    ui.separator();

    let Some(mcm) = mcm.filter(|mcm| !mcm.mods.is_empty()) else {
        ui.label("No mod has registered a configuration page.");
        return;
    };

    // A dialog the script closed leaves no stale working values behind.
    match &mcm.dialog {
        Some(dialog) => {
            let key = dialog_key(dialog);
            if draft.key != Some(key) {
                seed(draft, dialog, key);
            }
        }
        None => draft.key = None,
    }

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(190.0);
            egui::ScrollArea::vertical().id_salt("mcm_mods").auto_shrink([false, false]).show(ui, |ui| {
                draw_mod_list(ui, mcm, outputs);
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            draw_open_page(ui, mcm, draft, outputs);
        });
    });

    if let Some(dialog) = &mcm.dialog {
        draw_dialog(ui.ctx(), labels, dialog, draft, outputs);
    }
}

fn draw_mod_list(ui: &mut egui::Ui, mcm: &McmSnapshot, outputs: &mut PageOutputs) {
    for (index, entry) in mcm.mods.iter().enumerate() {
        let open = mcm.open_mod == Some(index);
        // Choosing the open mod again returns to its front page.
        if ui.selectable_label(open && mcm.open_page.is_none(), RichText::new(&entry.name).strong()).clicked() {
            outputs.mcm_actions.push(McmAction::SelectMod(index));
        }
        if open {
            for (page, name) in entry.pages.iter().enumerate() {
                let selected = mcm.open_page == Some(page);
                if ui.selectable_label(selected, format!("   {name}")).clicked() {
                    outputs.mcm_actions.push(McmAction::SelectPage(page));
                }
            }
        }
    }
}

fn draw_open_page(
    ui: &mut egui::Ui,
    mcm: &McmSnapshot,
    draft: &mut McmDraft,
    outputs: &mut PageOutputs,
) {
    if mcm.open_mod.is_none() {
        ui.label("Select a mod to configure it.");
        return;
    }
    if !mcm.title.is_empty() {
        ui.label(RichText::new(&mcm.title).strong().size(16.0));
    }
    // The info text sits under the grid; the grid takes what is left.
    let info_height = 54.0;
    let grid_height = (ui.available_height() - info_height).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("mcm_page")
        .max_height(grid_height)
        .auto_shrink([false, false])
        .show(ui, |ui| match &mcm.custom {
            Some(custom) => draw_custom(ui, custom, mcm.generation),
            None => draw_grid(ui, &mcm.cells, draft, outputs),
        });
    ui.separator();
    ui.label(RichText::new(&mcm.info).small().color(Color32::GRAY));
    if mcm.custom.is_none() && !mcm.cells.is_empty() {
        ui.label(RichText::new("Right-click an option to restore its default.").small().color(Color32::DARK_GRAY));
    }
}

fn draw_grid(ui: &mut egui::Ui, cells: &[McmCellView], draft: &mut McmDraft, outputs: &mut PageOutputs) {
    let mut hovered = None;
    egui::Grid::new("mcm_options").num_columns(2).spacing([28.0, 6.0]).min_col_width(260.0).show(ui, |ui| {
        for row in cells.chunks(2) {
            for cell in row {
                if let Some(response) = draw_cell(ui, cell, outputs) {
                    if response.hovered() {
                        hovered = Some(cell.id);
                    }
                    if response.secondary_clicked() || (response.hovered() && ui.input(|i| i.key_pressed(egui::Key::R))) {
                        outputs.mcm_actions.push(McmAction::Default(cell.id));
                    }
                }
            }
            ui.end_row();
        }
    });
    // The script shows its info text for the option the pointer is over.
    if let Some(id) = hovered
        && draft.highlighted != Some(id)
    {
        outputs.mcm_actions.push(McmAction::Highlight(id));
    }
    draft.highlighted = hovered;
}

/// Draw one option; its response when it can be interacted with.
fn draw_cell(ui: &mut egui::Ui, cell: &McmCellView, outputs: &mut PageOutputs) -> Option<egui::Response> {
    if cell.flags & MCM_FLAG_HIDDEN != 0 {
        ui.label("");
        return None;
    }
    let enabled = cell.flags & MCM_FLAG_DISABLED == 0;
    let activate = |outputs: &mut PageOutputs| outputs.mcm_actions.push(McmAction::Activate(cell.id));
    match cell.kind {
        McmCellKind::Empty => {
            ui.label("");
            None
        }
        McmCellKind::Header => {
            ui.label(RichText::new(&cell.label).strong().color(Color32::from_rgb(145, 175, 210)));
            None
        }
        McmCellKind::Toggle => {
            let mut on = cell.number != 0.0;
            let response = ui.add_enabled(enabled, egui::Checkbox::new(&mut on, &cell.label));
            if response.changed() {
                activate(outputs);
            }
            Some(response)
        }
        McmCellKind::Text | McmCellKind::Slider | McmCellKind::Menu | McmCellKind::Input | McmCellKind::KeyMap => {
            let response = ui
                .horizontal(|ui| {
                    ui.label(RichText::new(&cell.label).color(dim(enabled)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled(enabled, egui::Button::new(value_text(cell)))
                    })
                    .inner
                })
                .inner;
            if response.clicked() {
                activate(outputs);
            }
            Some(response)
        }
        McmCellKind::Color => {
            let [r, g, b] = rgb(cell.number as u32);
            let response = ui
                .horizontal(|ui| {
                    ui.label(RichText::new(&cell.label).color(dim(enabled)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled(
                            enabled,
                            egui::Button::new("      ").fill(Color32::from_rgb(r, g, b)).min_size(egui::vec2(48.0, 20.0)),
                        )
                    })
                    .inner
                })
                .inner;
            if response.clicked() {
                activate(outputs);
            }
            Some(response)
        }
    }
}

fn dim(enabled: bool) -> Color32 {
    if enabled { Color32::WHITE } else { Color32::DARK_GRAY }
}

/// The text on a value button: a key map shows the script's text, which names the key.
fn value_text(cell: &McmCellView) -> &str {
    if cell.value_text.is_empty() { " " } else { &cell.value_text }
}

fn rgb(color: u32) -> [u8; 3] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8]
}

fn draw_custom(ui: &mut egui::Ui, custom: &McmCustomContent, generation: u32) {
    let Some((width, height, pixels)) = &custom.image else {
        ui.label(format!("Unsupported custom content: {}", custom.source));
        return;
    };
    // One texture per image: the handle is cached with the context, not re-uploaded each frame.
    let id = egui::Id::new(("mcm_custom", &custom.source, generation));
    let texture = ui.ctx().data_mut(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let image = egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], pixels);
        let handle = ui.ctx().load_texture(format!("mcm_custom_{}", custom.source), image, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|data| data.insert_temp(id, handle.clone()));
        handle
    });
    ui.add_space(custom.y.max(0.0));
    ui.horizontal(|ui| {
        ui.add_space(custom.x.max(0.0));
        ui.image((texture.id(), egui::vec2(*width as f32, *height as f32)));
    });
}

fn seed(draft: &mut McmDraft, dialog: &McmDialogView, key: (u32, &'static str)) {
    draft.key = Some(key);
    match dialog {
        McmDialogView::Slider { start, .. } => draft.slider = *start,
        McmDialogView::Menu { start, .. } => draft.menu = *start,
        McmDialogView::Color { start, .. } => draft.color = rgb(*start),
        McmDialogView::Input { start_text, .. } => draft.text.clone_from(start_text),
        McmDialogView::KeyMap { .. } | McmDialogView::Message { .. } => {}
    }
}

/// A slider's value as its format string shows it (`{0}` whole, `{1}` one decimal, `{2}` two, …;
/// SkyUI's `formatString`). A format without a placeholder shows the number after it.
pub fn format_slider_value(format: &str, value: f32) -> String {
    let mut out = String::new();
    let mut rest = format;
    let mut used = false;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        let digits = &rest[open + 1..open + close];
        match digits.parse::<usize>() {
            Ok(decimals) => {
                out.push_str(&rest[..open]);
                out.push_str(&format!("{value:.decimals$}"));
                used = true;
            }
            Err(_) => out.push_str(&rest[..open + close + 1]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    if used || format.is_empty() { out } else { format!("{value} {out}") }
}

fn draw_dialog(
    ctx: &egui::Context,
    labels: &UiLabels,
    dialog: &McmDialogView,
    draft: &mut McmDraft,
    outputs: &mut PageOutputs,
) {
    egui::Window::new("mcm_dialog")
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(380.0, 0.0))
        .show(ctx, |ui| match dialog {
            McmDialogView::Slider { id, default, min, max, interval, format, .. } => {
                ui.label(format_slider_value(format, draft.slider));
                let mut slider = egui::Slider::new(&mut draft.slider, *min..=*max).show_value(false);
                if *interval > 0.0 {
                    slider = slider.step_by(f64::from(*interval));
                }
                ui.add_sized([ui.available_width(), 24.0], slider);
                let id = *id;
                let default = *default;
                let value = draft.slider;
                buttons(ui, labels, true, outputs, McmAction::SliderAccept(id, value), |draft| draft.slider = default, draft);
            }
            McmDialogView::Menu { id, options, default, .. } => {
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        if ui.selectable_label(draft.menu == index as i32, option).clicked() {
                            draft.menu = index as i32;
                        }
                    }
                });
                let (id, default, value) = (*id, *default, draft.menu);
                buttons(ui, labels, true, outputs, McmAction::MenuAccept(id, value), |draft| draft.menu = default, draft);
            }
            McmDialogView::Color { id, default, .. } => {
                ui.color_edit_button_srgb(&mut draft.color);
                let [r, g, b] = draft.color;
                let value = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                let (id, default) = (*id, *default);
                buttons(ui, labels, true, outputs, McmAction::ColorAccept(id, value), |draft| draft.color = rgb(default), draft);
            }
            McmDialogView::Input { id, .. } => {
                let edit = ui.add(egui::TextEdit::singleline(&mut draft.text).desired_width(340.0));
                edit.request_focus();
                let id = *id;
                let text = draft.text.clone();
                let entered = ui.input(|i| i.key_pressed(egui::Key::Enter));
                buttons(ui, labels, false, outputs, McmAction::InputAccept(id, text.clone()), |_| {}, draft);
                if entered {
                    outputs.mcm_actions.push(McmAction::InputAccept(id, text));
                }
            }
            McmDialogView::KeyMap { id, with_unmap, .. } => {
                ui.label("Press a key to bind it, Esc to cancel.");
                let id = *id;
                if let Some(code) = ui.input(|i| i.events.iter().find_map(pressed_scancode)) {
                    if code == 1 {
                        outputs.mcm_actions.push(McmAction::DialogCancel);
                    } else {
                        outputs.mcm_actions.push(McmAction::KeyMapChange(id, code));
                    }
                }
                ui.horizontal(|ui| {
                    if *with_unmap && ui.button("Unmap").clicked() {
                        outputs.mcm_actions.push(McmAction::KeyMapChange(id, 0));
                    }
                    if ui.button(labels.text("Cancel")).clicked() {
                        outputs.mcm_actions.push(McmAction::DialogCancel);
                    }
                });
            }
            McmDialogView::Message { title, text, buttons } => {
                if !title.is_empty() {
                    ui.label(RichText::new(title).strong());
                }
                ui.label(text);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if buttons.is_empty() && ui.button(labels.text("Ok")).clicked() {
                        outputs.mcm_actions.push(McmAction::MessageAnswer(0));
                    }
                    for (index, button) in buttons.iter().enumerate() {
                        if ui.button(button).clicked() {
                            outputs.mcm_actions.push(McmAction::MessageAnswer(index));
                        }
                    }
                });
            }
        });
}

/// The dialog's Ok, Reset (the option's default) and Cancel.
fn buttons(
    ui: &mut egui::Ui,
    labels: &UiLabels,
    with_default: bool,
    outputs: &mut PageOutputs,
    accept: McmAction,
    reset: impl FnOnce(&mut McmDraft),
    draft: &mut McmDraft,
) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(labels.text("Ok")).clicked() {
            outputs.mcm_actions.push(accept);
        }
        if with_default && ui.button(labels.text("Reset")).clicked() {
            reset(draft);
        }
        if ui.button(labels.text("Cancel")).clicked() {
            outputs.mcm_actions.push(McmAction::DialogCancel);
        }
    });
}

/// A key press as a DirectInput scancode (SKSE's key codes): `None` for events that are not a
/// press, and for keys with no scancode.
fn pressed_scancode(event: &egui::Event) -> Option<u32> {
    match event {
        egui::Event::Key { key, pressed: true, repeat: false, .. } => scancode(*key),
        _ => None,
    }
}

/// DirectInput's scancode of an egui key.
pub fn scancode(key: egui::Key) -> Option<u32> {
    use egui::Key::*;
    Some(match key {
        Escape => 0x01,
        Num1 => 0x02,
        Num2 => 0x03,
        Num3 => 0x04,
        Num4 => 0x05,
        Num5 => 0x06,
        Num6 => 0x07,
        Num7 => 0x08,
        Num8 => 0x09,
        Num9 => 0x0A,
        Num0 => 0x0B,
        Minus => 0x0C,
        Equals => 0x0D,
        Backspace => 0x0E,
        Tab => 0x0F,
        Q => 0x10,
        W => 0x11,
        E => 0x12,
        R => 0x13,
        T => 0x14,
        Y => 0x15,
        U => 0x16,
        I => 0x17,
        O => 0x18,
        P => 0x19,
        OpenBracket => 0x1A,
        CloseBracket => 0x1B,
        Enter => 0x1C,
        A => 0x1E,
        S => 0x1F,
        D => 0x20,
        F => 0x21,
        G => 0x22,
        H => 0x23,
        J => 0x24,
        K => 0x25,
        L => 0x26,
        Semicolon => 0x27,
        Quote => 0x28,
        Backtick => 0x29,
        Backslash => 0x2B,
        Z => 0x2C,
        X => 0x2D,
        C => 0x2E,
        V => 0x2F,
        B => 0x30,
        N => 0x31,
        M => 0x32,
        Comma => 0x33,
        Period => 0x34,
        Slash => 0x35,
        Space => 0x39,
        F1 => 0x3B,
        F2 => 0x3C,
        F3 => 0x3D,
        F4 => 0x3E,
        F5 => 0x3F,
        F6 => 0x40,
        F7 => 0x41,
        F8 => 0x42,
        F9 => 0x43,
        F10 => 0x44,
        F11 => 0x57,
        F12 => 0x58,
        Home => 0xC7,
        ArrowUp => 0xC8,
        PageUp => 0xC9,
        ArrowLeft => 0xCB,
        ArrowRight => 0xCD,
        End => 0xCF,
        ArrowDown => 0xD0,
        PageDown => 0xD1,
        Insert => 0xD2,
        Delete => 0xD3,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_formats_follow_skyuis_placeholders() {
        assert_eq!(format_slider_value("{0} units", 12.4), "12 units");
        assert_eq!(format_slider_value("{1}x", 1.25), "1.2x");
        assert_eq!(format_slider_value("{2}", 3.0), "3.00");
        assert_eq!(format_slider_value("{0} of {0}", 4.0), "4 of 4");
        assert_eq!(format_slider_value("", 7.0), "");
        assert_eq!(format_slider_value("seconds", 7.0), "7 seconds");
        assert_eq!(format_slider_value("{x}", 7.0), "7 {x}");
    }

    #[test]
    fn keys_map_to_directinput_scancodes() {
        assert_eq!(scancode(egui::Key::Escape), Some(1));
        assert_eq!(scancode(egui::Key::A), Some(0x1E));
        assert_eq!(scancode(egui::Key::Space), Some(0x39));
        assert_eq!(scancode(egui::Key::ArrowUp), Some(0xC8));
        assert_eq!(scancode(egui::Key::F11), Some(0x57));
    }

    #[test]
    fn a_colour_splits_into_its_channels() {
        assert_eq!(rgb(0x00FF8040), [0xFF, 0x80, 0x40]);
    }

    #[test]
    fn option_flag_values_match_the_skyui_api() {
        assert_eq!((MCM_FLAG_DISABLED, MCM_FLAG_HIDDEN, crate::MCM_FLAG_WITH_UNMAP), (1, 2, 4));
    }
}
