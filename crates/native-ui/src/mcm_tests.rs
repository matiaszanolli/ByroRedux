//! The Mod Configuration page by its clicks: each interaction hands back the action the SkyUI
//! contract needs, and nothing else. The page is driven the way a player does: find where a text
//! was drawn, press and release the pointer on it.

use super::*;
use crate::end_pass;

const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 800.0));

/// One page under test: its context, its snapshot and its menu state, with the texts of the last frame.
struct Page {
    ctx: egui::Context,
    snapshot: PageSnapshot,
    state: GameMenuState,
    texts: Vec<(String, egui::Rect)>,
    outputs: PageOutputs,
}

impl Page {
    fn new(page: GameMenuPage, mcm: McmSnapshot) -> Self {
        let mut page = Self {
            ctx: egui::Context::default(),
            snapshot: PageSnapshot { mcm: Some(mcm), ..Default::default() },
            state: GameMenuState { visible: true, page, ..Default::default() },
            texts: Vec::new(),
            outputs: PageOutputs::default(),
        };
        // egui sizes a new window before it paints its contents, and a window that grows moves its
        // anchored centre: the page is settled when a frame draws what the one before did.
        for _ in 0..8 {
            page.frame(Vec::new());
        }
        page
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.ctx.begin_pass(egui::RawInput { screen_rect: Some(SCREEN), events, ..Default::default() });
        self.outputs = PageOutputs::default();
        draw_game_menu(&self.ctx, &self.snapshot, &mut self.state, &mut self.outputs);
        let output = end_pass(&self.ctx);
        self.texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => {
                    Some((text.galley.text().to_owned(), text.galley.rect.translate(text.pos.to_vec2())))
                }
                _ => None,
            })
            .collect();
    }

    fn center_of(&self, text: &str) -> egui::Pos2 {
        self.texts
            .iter()
            .find(|(drawn, _)| drawn.trim() == text)
            .unwrap_or_else(|| panic!("{text:?} is not drawn: {:?}", self.texts.iter().map(|t| &t.0).collect::<Vec<_>>()))
            .1
            .center()
    }

    fn drawn(&self, text: &str) -> bool {
        self.texts.iter().any(|(drawn, _)| drawn.trim() == text)
    }

    /// Press and release on `text`; the actions that click raised.
    fn click(&mut self, text: &str) -> Vec<McmAction> {
        let pos = self.center_of(text);
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        self.frame(vec![button(true), button(false)]);
        std::mem::take(&mut self.outputs.mcm_actions)
    }

    /// Move the pointer onto `text` and rest it there a frame; every action either frame raised.
    fn hover(&mut self, text: &str) -> Vec<McmAction> {
        let pos = self.center_of(text);
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        let mut actions = std::mem::take(&mut self.outputs.mcm_actions);
        self.frame(Vec::new());
        actions.append(&mut self.outputs.mcm_actions);
        actions
    }
}

fn cell(id: u32, kind: McmCellKind, label: &str, value: &str, number: f32, flags: u32) -> McmCellView {
    McmCellView { id, kind, label: label.into(), value_text: value.into(), number, flags }
}

fn registered() -> McmSnapshot {
    McmSnapshot {
        mods: vec![
            McmModView { name: "Frostfall".into(), pages: vec!["General".into(), "Survival".into()] },
            McmModView { name: "Immersive Sounds".into(), pages: Vec::new() },
        ],
        open_mod: Some(0),
        open_page: Some(0),
        title: "General settings".into(),
        info: "Turns the mod on.".into(),
        cells: vec![
            cell(0x101, McmCellKind::Header, "Core", "", 0.0, 0),
            cell(0x102, McmCellKind::Empty, "", "", 0.0, 0),
            cell(0x103, McmCellKind::Toggle, "Enabled", "", 1.0, 0),
            cell(0x104, McmCellKind::Slider, "Cold rate", "2.5 units", 2.5, 0),
            cell(0x105, McmCellKind::Menu, "Difficulty", "Normal", 0.0, 0),
            cell(0x106, McmCellKind::Text, "Locked option", "Off", 0.0, MCM_FLAG_DISABLED),
            cell(0x107, McmCellKind::Toggle, "Secret option", "", 0.0, MCM_FLAG_HIDDEN),
        ],
        generation: 1,
        ..Default::default()
    }
}

#[test]
fn choosing_a_mod_or_a_page_is_an_action() {
    let mut page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert_eq!(page.click("Immersive Sounds"), [McmAction::SelectMod(1)]);
    assert_eq!(page.click("Survival"), [McmAction::SelectPage(1)]);
    assert_eq!(page.click("Frostfall"), [McmAction::SelectMod(0)], "the open mod again: its front page");
}

#[test]
fn an_option_click_activates_it_by_id() {
    let mut page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert_eq!(page.click("Enabled"), [McmAction::Activate(0x103)]);
    assert_eq!(page.click("2.5 units"), [McmAction::Activate(0x104)]);
    assert_eq!(page.click("Normal"), [McmAction::Activate(0x105)]);
}

#[test]
fn a_disabled_option_does_nothing_and_a_hidden_one_is_not_drawn() {
    let mut page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert!(page.drawn("Locked option"));
    assert!(page.click("Off").is_empty());
    assert!(!page.drawn("Secret option"));
}

#[test]
fn hovering_an_option_reports_its_highlight_once() {
    let mut page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert_eq!(page.hover("Normal"), [McmAction::Highlight(0x105)]);
    assert!(page.hover("Normal").is_empty(), "the same option is not reported again");
    assert_eq!(page.hover("2.5 units"), [McmAction::Highlight(0x104)]);
}

#[test]
fn the_page_draws_the_scripts_title_and_info() {
    let page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert!(page.drawn("General settings"));
    assert!(page.drawn("Turns the mod on."));
}

#[test]
fn a_slider_dialog_accepts_its_value() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Slider {
        id: 0x104,
        start: 2.5,
        default: 1.0,
        min: 0.0,
        max: 10.0,
        interval: 0.5,
        format: "{1} units".into(),
    });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(page.drawn("2.5 units"), "the working value, formatted");
    assert_eq!(page.click("Ok"), [McmAction::SliderAccept(0x104, 2.5)]);
    assert_eq!(page.click("Cancel"), [McmAction::DialogCancel]);
}

#[test]
fn a_slider_dialog_resets_to_the_options_default() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Slider {
        id: 0x104,
        start: 2.5,
        default: 1.0,
        min: 0.0,
        max: 10.0,
        interval: 0.5,
        format: "{1}".into(),
    });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(page.click("Reset").is_empty(), "reset edits the dialog, the script hears nothing");
    assert_eq!(page.click("Ok"), [McmAction::SliderAccept(0x104, 1.0)]);
}

#[test]
fn a_menu_dialog_accepts_the_chosen_index() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Menu {
        id: 0x105,
        options: vec!["Easy".into(), "Normal".into(), "Hard".into()],
        start: 1,
        default: 0,
    });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(page.click("Hard").is_empty());
    assert_eq!(page.click("Ok"), [McmAction::MenuAccept(0x105, 2)]);
}

#[test]
fn a_colour_dialog_accepts_the_start_colour_unchanged() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Color { id: 0x108, start: 0x00FF8040, default: 0 });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert_eq!(page.click("Ok"), [McmAction::ColorAccept(0x108, 0x00FF8040)]);
}

#[test]
fn an_input_dialog_accepts_its_text() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Input { id: 0x109, start_text: "Whiterun".into() });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert_eq!(page.click("Ok"), [McmAction::InputAccept(0x109, "Whiterun".into())]);
}

#[test]
fn a_key_map_dialog_binds_the_next_key_and_esc_cancels() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::KeyMap { id: 0x10A, current: 0x2D, with_unmap: true });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    let key = |key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
    page.frame(vec![key(egui::Key::G)]);
    assert_eq!(page.outputs.mcm_actions, [McmAction::KeyMapChange(0x10A, 0x22)]);
    page.frame(vec![key(egui::Key::Escape)]);
    assert_eq!(page.outputs.mcm_actions, [McmAction::DialogCancel]);
    assert_eq!(page.click("Unmap"), [McmAction::KeyMapChange(0x10A, 0)]);
}

#[test]
fn a_message_waits_for_its_answer() {
    let mut mcm = registered();
    mcm.dialog = Some(McmDialogView::Message {
        text: "Reset every setting?".into(),
        with_cancel: true,
        accept_label: "Reset all".into(),
        cancel_label: String::new(),
    });
    let mut page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(page.drawn("Reset every setting?"));
    assert_eq!(page.click("Cancel"), [McmAction::MessageAnswer(false)], "an empty label falls back to the game's word");
    assert_eq!(page.click("Reset all"), [McmAction::MessageAnswer(true)]);
}

#[test]
fn custom_content_replaces_the_option_list_and_an_swf_is_reported() {
    let mut mcm = registered();
    mcm.custom = Some(McmCustomContent { source: "frostfall/logo.swf".into(), x: 0.0, y: 0.0, image: None });
    let page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(!page.drawn("Enabled"), "the option list is hidden while custom content is set");
    assert!(page.drawn("Unsupported custom content: frostfall/logo.swf"));
}

#[test]
fn custom_content_with_a_decoded_image_draws_it() {
    let mut mcm = registered();
    mcm.custom = Some(McmCustomContent {
        source: "frostfall/logo.dds".into(),
        x: 4.0,
        y: 4.0,
        image: Some((2, 2, std::sync::Arc::from(vec![255u8; 16]))),
    });
    let page = Page::new(GameMenuPage::ModConfiguration, mcm);
    assert!(!page.drawn("Unsupported custom content: frostfall/logo.dds"));
}

#[test]
fn the_pause_page_offers_the_page_only_once_a_mod_registered() {
    let mut none = Page::new(GameMenuPage::Pause, McmSnapshot::default());
    assert!(!none.drawn("Mod Configuration"));
    let mut some = Page::new(GameMenuPage::Pause, registered());
    assert!(some.drawn("Mod Configuration"));
    some.click("Mod Configuration");
    assert_eq!(some.state.page, GameMenuPage::ModConfiguration);
    none.snapshot.mcm = None;
    none.frame(Vec::new());
    assert!(!none.drawn("Mod Configuration"));
}

#[test]
fn back_returns_to_the_pause_page_and_leaves_the_config() {
    let mut page = Page::new(GameMenuPage::ModConfiguration, registered());
    assert_eq!(page.click("← Back"), [McmAction::Close]);
    assert_eq!(page.state.page, GameMenuPage::Pause);
}
