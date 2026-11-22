//! M48 Phase 4 (#6666, Q1) — Fallout 3 / New Vegas script message boxes on the game's own
//! document (S2), `menus\message_menu.xml` (`MessageMenu`, `MenuMode` 1001).
//!
//! The document marks what native code sets ("set in code"); this driver stands in for it:
//!
//! - `MM_Title` `string` (its header art copies the title's `visible`, so a box with no title
//!   hides it) and `MM_MessageText` `string`;
//! - the buttons: `MM_ButtonTemplate` rows under `MM_ButtonList` (a `list_box.xml`), each with
//!   `listindex`, `_y`, `height` and `string`, from a pool of [`BUTTON_POOL`] instantiated once;
//!   the list's `height` is the rows'. A box with no authored buttons gets the game's "Ok"
//!   (`sOk`, `UiLabels`), as on the native page;
//! - `MM_MainRect` `width`: the wider of the measured message and title texts plus the
//!   document's `_horbuf` on each side, within its `_MinMenuWidth` / `_MaxMenuWidth`; the button
//!   list (whose authored width is only a cap) gets the inner width.
//!
//! A click on button row *i* answers the box with button *i* (`answer_message_box`, the native
//! page's path).

use byroredux_menuxml::TileHit;
use byroredux_native_ui::MessageBoxView;

use crate::menuxml_document::MenuDocument;

/// `MessageMenu`'s `MenuMode` code (xEdit `wbMenuModeEnum`).
pub(crate) const MESSAGE_MENU_CODE: u32 = 1001;
/// Button rows instantiated once; a `MESG` authors at most ten (`ITXT`, the CS / GECK limit).
pub(crate) const BUTTON_POOL: usize = 10;
/// A button row's height: one line of the row's font 2 (`Monofonto_Large`, 31 pt) plus
/// `list_box_template.xml`'s `_VerticalSpacing` (20), the height native code gives a row whose
/// `height` is 0.
pub(crate) const BUTTON_HEIGHT: f32 = 51.0;

fn button_name(index: usize) -> String {
    format!("MM_Button{index}")
}

/// The button an activated tile answers with.
pub(crate) fn choice_of(hit: &TileHit) -> Option<usize> {
    hit.name.as_deref()?.strip_prefix("MM_Button")?.parse().ok()
}

/// Make `document` ready for message boxes: the button rows instantiated once (hidden).
pub(crate) fn prepare(document: &mut MenuDocument) -> Result<(), String> {
    for index in 0..BUTTON_POOL {
        let name = button_name(index);
        document
            .renderer
            .instantiate_template("MM_ButtonTemplate", "MM_ButtonList", &name)
            .map_err(|error| format!("{}: button row: {error}", document.document))?;
        document.renderer.set_override(&name, "listindex", -1.0);
    }
    Ok(())
}

/// Present `message` on `document` when it changed; `ok` is the label of the implicit button of
/// a box that authors none.
pub(crate) fn feed(document: &mut MenuDocument, message: &MessageBoxView, ok: &str) {
    let signature = signature(message);
    if document.signature == Some(signature) {
        return;
    }
    document.signature = Some(signature);
    document.touch();
    let buttons: Vec<&str> = if message.buttons.is_empty() { vec![ok] } else { message.buttons.iter().map(String::as_str).collect() };
    let r = &mut document.renderer;
    r.set_override_str("MM_Title", "string", &message.title);
    r.set_override("MM_Title", "visible", if message.title.is_empty() { 0.0 } else { 2.0 });
    r.set_override_str("MM_MessageText", "string", &message.text);
    for index in 0..BUTTON_POOL {
        let row = button_name(index);
        match buttons.get(index) {
            Some(label) => {
                r.set_override(&row, "listindex", index as f32);
                r.set_override(&row, "_y", index as f32 * BUTTON_HEIGHT);
                r.set_override(&row, "height", BUTTON_HEIGHT);
                r.set_override_str(&row, "string", label);
            }
            None => r.set_override(&row, "listindex", -1.0),
        }
    }
    r.set_override("MM_ButtonList", "height", buttons.len().min(BUTTON_POOL) as f32 * BUTTON_HEIGHT);
    // The box fits its widest text: the measured message and title (the title sits 60 in from
    // the left), within the document's bounds.
    let value = |tile: &str, name: &str| r.value(tile, name).map_or(0.0, |v| v.as_num());
    let (horbuf, min, max) = (value("MessageMenu", "_horbuf"), value("MessageMenu", "_MinMenuWidth"), value("MessageMenu", "_MaxMenuWidth"));
    let text = value("MM_MessageText", "width").max(value("MM_Title", "width") + 60.0);
    let width = (text + 2.0 * horbuf).clamp(min, max.max(min));
    r.set_override("MM_MainRect", "width", width);
    // The list's authored width is only a cap (`list_box.xml`: "REQUIRED OVERRIDE"); native code
    // gives it the box's inner width.
    r.set_override("MM_ButtonList", "width", width - 2.0 * horbuf);
}

fn signature(message: &MessageBoxView) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message.title.hash(&mut hasher);
    message.text.hash(&mut hasher);
    message.buttons.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_button_row_answers_with_its_index() {
        let hit = |name: &str| TileHit { tile: 0, name: Some(name.to_owned()), id: None };
        assert_eq!(choice_of(&hit("MM_Button0")), Some(0));
        assert_eq!(choice_of(&hit("MM_Button7")), Some(7));
        assert_eq!(choice_of(&hit("MM_ButtonList")), None);
        assert_eq!(choice_of(&hit("scrollbar_vert_up")), None);
    }

    /// The corpus check: a script message box lands on the tiles native code feeds; its buttons
    /// are hit targets in order, a click on one is its answer, the rows past the box's buttons
    /// are hidden, the box is sized to its text, and a box with no buttons offers "Ok".
    #[test]
    #[ignore = "needs FNV game data on disk"]
    fn fnv_message_document_presents_a_message_box() {
        use byroredux_plugin::esm::test_paths::{data_dir, FNV_DEFAULT, FNV_ENV};
        let assets = crate::hud::HudAssets::open_in(crate::hud::HudGameProfile::fallout_nv(), &data_dir(FNV_ENV, FNV_DEFAULT)).expect("FNV menu archives");
        let mut doc = MenuDocument::headless(MESSAGE_MENU_CODE, "MessageMenu", "menus\\message_menu.xml", assets, (1280, 720));
        prepare(&mut doc).expect("the button rows instantiate");
        let message = MessageBoxView { title: "Vault 21".into(), text: "Rest here until morning?".into(), buttons: vec!["Yes".into(), "No".into()] };
        feed(&mut doc, &message, "Ok");
        let targets = doc.renderer.hit_targets();
        let centre = |index: usize| {
            let tile = doc.renderer.tile(&button_name(index)).unwrap();
            let target = targets.iter().find(|t| t.tile == tile).unwrap_or_else(|| panic!("button {index} is a target: {targets:?}"));
            (target.rect.x + target.rect.w / 2.0, target.rect.y + target.rect.h / 2.0)
        };
        let (no_x, no_y) = centre(1);
        assert!(centre(0).1 < no_y, "the buttons stack in order");
        assert!(!targets.iter().any(|t| Some(t.tile) == doc.renderer.tile(&button_name(2))), "unused rows are hidden");
        doc.renderer.pointer_down(no_x, no_y);
        doc.renderer.pointer_up(no_x, no_y);
        assert_eq!(doc.renderer.take_activations().iter().filter_map(choice_of).collect::<Vec<_>>(), [1]);
        let width = doc.renderer.value("MM_MainRect", "width").unwrap().as_num();
        assert!((200.0..=700.0).contains(&width), "sized within the document's bounds: {width}");
        assert!(doc.render().as_chunks::<4>().0.iter().any(|p| p[3] > 0), "the document draws");

        let bare = MessageBoxView { title: String::new(), text: "The door is sealed.".into(), buttons: Vec::new() };
        feed(&mut doc, &bare, "Ok");
        assert_eq!(doc.renderer.value("MM_Button0", "string"), Some(byroredux_menuxml::Scalar::Str("Ok".into())));
        assert_eq!(doc.renderer.value("MM_Title", "visible").map(|v| v.truthy()), Some(false));
    }
}
