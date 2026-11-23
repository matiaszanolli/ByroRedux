//! The Mod Configuration page's binary-side adapter (LMC-UI-002, `docs/engine/mcm-host.md` §3.3):
//! [`McmHost`] to the [`McmSnapshot`] the native page draws, and the page's [`McmAction`]s to the
//! driver functions that run the config script's events.
//!
//! The page never changes a value: an action runs the script, the script's setters write the
//! host's page buffer, and the next snapshot shows it (SkyUI's contract). Every text the script
//! authored may be a `$`-key (SkyUI's `$SKI_…`, a mod's own translation file, catalog U4): the
//! snapshot is built through a translator, the Scaleform table in production.

use byroredux_core::ecs::world::World;
use byroredux_native_ui::{
    McmAction, McmCellKind, McmCellView, McmCustomContent, McmDialogView, McmModView, McmSnapshot, MessageBoxView,
};
use byroredux_scripting::mcm::{self, flags, McmAnswer, McmDialog, McmHost, McmKind, McmOption, McmPage};

/// Build the page's snapshot from the host: `None` while no config menu is registered (the pause
/// page then offers no entry). `translate` resolves a script text that is a `$`-key. `message` is the
/// script message box waiting for an answer, already translated: while a config page is open it
/// shows on the page (a box on another surface would draw under it or wait unseen), over any dialog.
pub(crate) fn snapshot(
    world: &World,
    translate: &dyn Fn(&str) -> String,
    message: Option<&MessageBoxView>,
) -> Option<McmSnapshot> {
    let host = world.try_resource::<McmHost>()?;
    if host.mods.is_empty() {
        return None;
    }
    let mods = host
        .mods
        .iter()
        .map(|entry| McmModView {
            name: translate(&entry.name),
            pages: entry.pages.iter().map(|page| translate(page)).collect(),
        })
        .collect();
    let mut view = McmSnapshot { mods, generation: host.generation, ..Default::default() };
    if let Some(page) = &host.open {
        view.open_mod = Some(page.mod_index);
        view.open_page = page.page;
        view.title = translate(&page.title);
        view.info = translate(&page.info);
        view.cells = cells(page, translate);
        view.dialog = match message {
            Some(message) => Some(McmDialogView::Message {
                title: message.title.clone(),
                text: message.text.clone(),
                buttons: message.buttons.clone(),
            }),
            None => page.dialog.as_ref().map(|(id, dialog)| dialog_view(page, *id, dialog, translate)),
        };
        view.custom = page.custom.as_ref().map(|(source, x, y)| McmCustomContent {
            source: source.clone(),
            x: *x,
            y: *y,
            // A `.dds` the game archive carries is decoded here once the interface archive's
            // textures are reachable from the adapter; until then the page reports it.
            image: None,
        });
    }
    Some(view)
}

/// The page's cells in grid order. A cell the script never filled is an empty one, so the two
/// columns keep their places; the trailing empties are dropped.
fn cells(page: &McmPage, translate: &dyn Fn(&str) -> String) -> Vec<McmCellView> {
    let used = page.cells.iter().rposition(Option::is_some).map_or(0, |last| last + 1);
    (0..used)
        .map(|index| {
            let id = page.option_id(index) as u32;
            match &page.cells[index] {
                Some(option) => cell_view(id, option, translate),
                None => McmCellView {
                    id,
                    kind: McmCellKind::Empty,
                    label: String::new(),
                    value_text: String::new(),
                    number: 0.0,
                    flags: 0,
                },
            }
        })
        .collect()
}

fn cell_view(id: u32, option: &McmOption, translate: &dyn Fn(&str) -> String) -> McmCellView {
    let kind = match option.kind {
        McmKind::Empty => McmCellKind::Empty,
        McmKind::Header => McmCellKind::Header,
        McmKind::Text => McmCellKind::Text,
        McmKind::Toggle => McmCellKind::Toggle,
        McmKind::Slider => McmCellKind::Slider,
        McmKind::Menu => McmCellKind::Menu,
        McmKind::Color => McmCellKind::Color,
        McmKind::KeyMap => McmCellKind::KeyMap,
        McmKind::Input => McmCellKind::Input,
    };
    // A text option's value, a menu's choice and an input's text are the script's words; a slider's
    // number, a colour and a key code are numbers the host formats.
    let value_text = match option.kind {
        McmKind::Text | McmKind::Menu | McmKind::Input => translate(&option.value_text()),
        _ => option.value_text(),
    };
    McmCellView {
        id,
        kind,
        label: translate(&option.label),
        value_text,
        number: option.number,
        flags: option.flags,
    }
}

fn dialog_view(page: &McmPage, id: i32, dialog: &McmDialog, translate: &dyn Fn(&str) -> String) -> McmDialogView {
    let id = id as u32;
    match dialog {
        McmDialog::Slider { start, default, min, max, interval } => McmDialogView::Slider {
            id,
            start: *start,
            default: *default,
            min: *min,
            max: *max,
            interval: *interval,
            // The slider option's text is its format string.
            format: page.option(id as i32).map(|option| translate(&option.text)).unwrap_or_default(),
        },
        McmDialog::Menu { options, start, default } => McmDialogView::Menu {
            id,
            options: options.iter().map(|option| translate(option)).collect(),
            start: *start,
            default: *default,
        },
        McmDialog::Color { start, default } => McmDialogView::Color { id, start: *start, default: *default },
        McmDialog::Input { start_text } => McmDialogView::Input { id, start_text: translate(start_text) },
        McmDialog::KeyMap => McmDialogView::KeyMap {
            id,
            current: page.option(id as i32).map_or(0, |option| option.number as u32),
            with_unmap: page.option(id as i32).is_some_and(|option| option.flags & flags::WITH_UNMAP != 0),
        },
    }
}

/// Run what the player did on the page, in order. Each action runs Papyrus, which writes the host, so
/// none is run while the host is borrowed. A refused action is logged and the rest still run.
pub(crate) fn apply(world: &World, actions: Vec<McmAction>) {
    for action in actions {
        if let Err(error) = apply_one(world, &action) {
            log::warn!("mod configuration: {action:?}: {error}");
        }
    }
}

fn apply_one(world: &World, action: &McmAction) -> Result<(), String> {
    match action {
        McmAction::SelectMod(index) => {
            let open = world.try_resource::<McmHost>().and_then(|host| host.open.as_ref().map(|page| page.mod_index));
            match open {
                // The open mod again: its front page.
                Some(open) if open == *index => mcm::select_page(world, None),
                // Another mod: the open one closes first, as it does when the player leaves it.
                Some(_) => {
                    mcm::close(world)?;
                    mcm::open(world, *index)
                }
                None => mcm::open(world, *index),
            }
        }
        McmAction::SelectPage(page) => mcm::select_page(world, Some(*page)),
        McmAction::Activate(id) => mcm::activate(world, *id as i32),
        McmAction::Default(id) => mcm::reset_option(world, *id as i32),
        McmAction::Highlight(id) => mcm::highlight(world, *id as i32),
        McmAction::SliderAccept(id, value) => mcm::accept(world, *id as i32, McmAnswer::Slider(*value)),
        McmAction::MenuAccept(id, index) => mcm::accept(world, *id as i32, McmAnswer::Menu(*index)),
        McmAction::ColorAccept(id, color) => mcm::accept(world, *id as i32, McmAnswer::Color(*color)),
        McmAction::InputAccept(id, text) => mcm::accept(world, *id as i32, McmAnswer::Input(text.clone())),
        // No conflict is reported: the engine's own control bindings are not asked yet, so a key
        // another control already uses binds without the script's conflict question.
        McmAction::KeyMapChange(id, code) => mcm::accept(world, *id as i32, McmAnswer::KeyMap(*code as i32, String::new())),
        McmAction::DialogCancel => {
            mcm::cancel_dialog(world);
            Ok(())
        }
        // The box is the shared queue's, whoever raised it: its answer reaches the script that waits
        // (`ShowMessage`'s ticket included) through the one path `ui.answer` drives.
        McmAction::MessageAnswer(button) => {
            crate::notifications::answer_message_box(world, Some(*button));
            Ok(())
        }
        McmAction::Close => mcm::close(world),
    }
}

/// The player left the menus: a config page still open is closed, so its script hears `OnConfigClose`.
pub(crate) fn leave(world: &World) {
    let open = world.try_resource::<McmHost>().is_some_and(|host| host.open.is_some());
    if open && let Err(error) = mcm::close(world) {
        log::warn!("mod configuration: closing on leaving the menus: {error}");
    }
}

/// The snapshot for the live page: the host's, translated through the game's table.
pub(crate) fn live_snapshot(world: &World) -> Option<McmSnapshot> {
    world.try_resource::<McmHost>().filter(|host| !host.mods.is_empty())?;
    snapshot(world, &crate::ui_translate::translate, crate::notifications::front_message_box(world).as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_scripting::mcm::McmMod;
    use byroredux_scripting::QuestFormId;

    /// A translator over a fixed table, as the Scaleform one is over the game's.
    fn table(key: &str) -> String {
        match key {
            "$Enabled" => "Enabled (translated)".to_owned(),
            "$Frost" => "Frostfall".to_owned(),
            other => other.to_owned(),
        }
    }

    fn option(kind: McmKind, label: &str, text: &str, number: f32, flags: u32) -> McmOption {
        McmOption { kind, label: label.into(), text: text.into(), number, flags, state: String::new() }
    }

    fn world_with(open: Option<McmPage>) -> World {
        let mut world = World::new();
        let mut host = McmHost::default();
        host.mods = vec![McmMod {
            quest: QuestFormId(0x0100_0800),
            script: "frostfallmcm".into(),
            name: "$Frost".into(),
            pages: vec!["General".into(), "$Enabled".into()],
        }];
        host.open = open;
        host.generation = 7;
        world.insert_resource(host);
        world
    }

    #[test]
    fn no_registered_menu_is_no_snapshot() {
        let world = World::new();
        assert!(snapshot(&world, &table, None).is_none());
        let mut world = World::new();
        world.insert_resource(McmHost::default());
        assert!(snapshot(&world, &table, None).is_none(), "an empty registry offers no pause entry");
    }

    #[test]
    fn registered_mods_and_pages_are_translated() {
        let world = world_with(None);
        let view = snapshot(&world, &table, None).expect("one mod registered");
        assert_eq!(view.mods[0].name, "Frostfall");
        assert_eq!(view.mods[0].pages, ["General", "Enabled (translated)"]);
        assert_eq!((view.open_mod, view.generation), (None, 7));
        assert!(view.cells.is_empty() && view.dialog.is_none());
    }

    #[test]
    fn the_open_page_carries_its_cells_ids_and_translated_texts() {
        let mut page = McmPage::new(0, Some(1));
        page.title = "$Enabled".into();
        page.info = "Turns it on.".into();
        page.cells[0] = Some(option(McmKind::Header, "$Enabled", "", 0.0, 0));
        // Cell 1 is never filled: the grid keeps its place.
        page.cells[2] = Some(option(McmKind::Toggle, "On", "", 1.0, 0));
        page.cells[3] = Some(option(McmKind::Slider, "Rate", "{1} units", 2.5, flags::DISABLED));
        page.cells[4] = Some(option(McmKind::Text, "State", "$Frost", 0.0, 0));
        let id = |cell: usize| page.option_id(cell) as u32;
        let ids = [id(0), id(1), id(2), id(3), id(4)];
        let world = world_with(Some(page));
        let view = snapshot(&world, &table, None).unwrap();
        assert_eq!((view.open_mod, view.open_page), (Some(0), Some(1)));
        assert_eq!((view.title.as_str(), view.info.as_str()), ("Enabled (translated)", "Turns it on."));
        let kinds: Vec<_> = view.cells.iter().map(|cell| cell.kind).collect();
        assert_eq!(
            kinds,
            [McmCellKind::Header, McmCellKind::Empty, McmCellKind::Toggle, McmCellKind::Slider, McmCellKind::Text],
            "the unfilled cell is an empty one, and the trailing empties are dropped"
        );
        assert_eq!(view.cells.iter().map(|cell| cell.id).collect::<Vec<_>>(), ids);
        assert_eq!(ids[0], 0x200, "page 1's ids carry page number 2 in the high byte (SkyUI's encoding)");
        assert_eq!(view.cells[0].label, "Enabled (translated)");
        assert_eq!(view.cells[3].value_text, "2.5 units", "the slider's number through its format");
        assert_eq!(view.cells[3].flags, flags::DISABLED);
        assert_eq!(view.cells[4].value_text, "Frostfall", "a text option's value is translated");
    }

    #[test]
    fn each_dialog_maps_with_its_options_own_data() {
        let key_map = |unmap: u32| {
            let mut page = McmPage::new(0, None);
            page.cells[5] = Some(option(McmKind::KeyMap, "Wait", "", 0x2C as f32, unmap));
            page.dialog = Some((page.option_id(5), McmDialog::KeyMap));
            let world = world_with(Some(page));
            snapshot(&world, &table, None).unwrap().dialog.unwrap()
        };
        assert_eq!(key_map(0), McmDialogView::KeyMap { id: 5, current: 0x2C, with_unmap: false });
        assert_eq!(key_map(flags::WITH_UNMAP), McmDialogView::KeyMap { id: 5, current: 0x2C, with_unmap: true });

        let mut page = McmPage::new(0, None);
        page.cells[1] = Some(option(McmKind::Slider, "Rate", "{2} x", 2.0, 0));
        let slider = page.option_id(1);
        page.dialog = Some((slider, McmDialog::Slider { start: 2.0, default: 1.0, min: 0.0, max: 5.0, interval: 0.5 }));
        let world = world_with(Some(page));
        assert_eq!(
            snapshot(&world, &table, None).unwrap().dialog.unwrap(),
            McmDialogView::Slider { id: 1, start: 2.0, default: 1.0, min: 0.0, max: 5.0, interval: 0.5, format: "{2} x".into() },
            "the format is the slider option's own text"
        );

        let mut page = McmPage::new(0, None);
        page.dialog = Some((9, McmDialog::Menu { options: vec!["$Enabled".into(), "Off".into()], start: 1, default: 0 }));
        let world = world_with(Some(page));
        assert_eq!(
            snapshot(&world, &table, None).unwrap().dialog.unwrap(),
            McmDialogView::Menu { id: 9, options: vec!["Enabled (translated)".into(), "Off".into()], start: 1, default: 0 }
        );
    }

    #[test]
    fn custom_content_is_carried_with_its_offset() {
        let mut page = McmPage::new(0, None);
        page.custom = Some(("Interface/frostfall/logo.dds".into(), 10.0, 20.0));
        let world = world_with(Some(page));
        let custom = snapshot(&world, &table, None).unwrap().custom.unwrap();
        assert_eq!((custom.source.as_str(), custom.x, custom.y), ("Interface/frostfall/logo.dds", 10.0, 20.0));
    }

    #[test]
    fn dismissing_a_dialog_clears_it() {
        let mut page = McmPage::new(0, None);
        page.dialog = Some((3, McmDialog::Input { start_text: "x".into() }));
        let world = world_with(Some(page));
        apply(&world, vec![McmAction::DialogCancel]);
        assert!(world.resource::<McmHost>().open.as_ref().unwrap().dialog.is_none());
    }

    #[test]
    fn a_refused_action_is_logged_and_the_rest_still_run() {
        let mut page = McmPage::new(0, None);
        page.dialog = Some((3, McmDialog::Input { start_text: "x".into() }));
        let world = world_with(Some(page));
        // A page the mod does not have: refused, with the open menu left as it was.
        assert!(apply_one(&world, &McmAction::SelectPage(9)).is_err());
        apply(&world, vec![McmAction::SelectPage(9), McmAction::DialogCancel]);
        assert!(world.resource::<McmHost>().open.as_ref().unwrap().dialog.is_none(), "the cancel after the refusal ran");
        // No menu open: nothing to activate.
        let world = world_with(None);
        assert!(apply_one(&world, &McmAction::Activate(0x100)).is_err());
    }

    #[test]
    fn leaving_with_nothing_open_does_nothing() {
        let world = world_with(None);
        leave(&world);
        assert!(world.resource::<McmHost>().open.is_none());
        assert_eq!(world.resource::<McmHost>().generation, 7, "no close was run");
    }

    fn box_of(buttons: &[&str]) -> byroredux_scripting::ScriptMessage {
        byroredux_scripting::ScriptMessage {
            title: "Frostfall".into(),
            text: "Reset everything?".into(),
            message_box: true,
            buttons: buttons.iter().map(|b| (*b).to_owned()).collect(),
            answer: byroredux_scripting::MessageAnswer::Unread,
        }
    }

    #[test]
    fn a_waiting_message_box_shows_on_the_open_page_over_a_dialog() {
        let mut page = McmPage::new(0, None);
        page.dialog = Some((3, McmDialog::Input { start_text: "x".into() }));
        let world = world_with(Some(page));
        let message = MessageBoxView { title: "Frostfall".into(), text: "Reset everything?".into(), buttons: vec!["Yes".into(), "No".into()] };
        let dialog = snapshot(&world, &table, Some(&message)).unwrap().dialog.unwrap();
        assert_eq!(
            dialog,
            McmDialogView::Message { title: "Frostfall".into(), text: "Reset everything?".into(), buttons: vec!["Yes".into(), "No".into()] }
        );
        // No page open: the box is not this page's to show.
        let world = world_with(None);
        assert!(snapshot(&world, &table, Some(&message)).unwrap().dialog.is_none());
    }

    #[test]
    fn a_message_answer_takes_the_front_box_off_the_shared_queue() {
        let mut world = world_with(None);
        world.insert_resource(crate::notifications::MessageBoxQueue(
            [box_of(&["Yes", "No"]), box_of(&["Later"])].into_iter().collect(),
        ));
        apply(&world, vec![McmAction::MessageAnswer(1)]);
        let queue = world.resource::<crate::notifications::MessageBoxQueue>();
        assert_eq!(queue.0.len(), 1, "the front box was answered");
        assert_eq!(queue.0[0].buttons, ["Later"]);
    }
}
