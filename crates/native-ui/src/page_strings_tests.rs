//! M48 GUI-PAGE-005 (#6666) — every word a native page draws is the game's, the snapshot's data,
//! or an engine string the Phase 0 inventory (`docs/data/game-ui/native-page-strings.tsv`) lists
//! for that page with its reason.
//!
//! Each page is drawn twice. First with every label resolved to a marker (`«sContinue»`), as on
//! a game that authors all of them: what is left once the markers and the snapshot's own data
//! are removed must be listed engine text. Then with the engine's defaults, as on New Vegas,
//! which authors none: each fallback must be listed too (a `game-string` row names the settings
//! it stands in for).

use super::*;
use byroredux_core::settings::{SettingEntry, SettingValue};
use byroredux_core::ui_labels::UiLabels;

const INVENTORY: &str = include_str!("../../../docs/data/game-ui/native-page-strings.tsv");

/// The inventory's rows: `(string, pages, disposition)`.
fn rows() -> Vec<(String, Vec<String>, String)> {
    INVENTORY
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.len() > 3).then(|| (cols[1].to_owned(), cols[2].split('/').map(str::to_owned).collect(), cols[3].to_owned()))
        })
        .collect()
}

/// The words of `text`: placeholders (`{n}`) and markers (`«…»`) dropped, digits and symbols
/// treated as separators.
fn words(text: &str) -> Vec<String> {
    let mut out = String::new();
    let mut skip = None;
    for c in text.chars() {
        match (skip, c) {
            (None, '{') => skip = Some('}'),
            (None, '«') => skip = Some('»'),
            (Some(end), c) if c == end => {
                skip = None;
                out.push(' ');
            }
            (Some(_), _) => {}
            (None, c) if c.is_alphabetic() || c == '\'' => out.push(c),
            (None, _) => out.push(' '),
        }
    }
    out.split_whitespace().map(|word| word.trim_matches('\'').to_owned()).filter(|word| !word.is_empty()).collect()
}

/// The listed strings of `pages` as word sequences, longest first: the engine text, and with
/// `fallbacks` the `game-string` rows too (the words drawn when the game authors none).
fn listed(pages: &[&str], fallbacks: bool) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = rows()
        .into_iter()
        .filter(|(_, row_pages, disposition)| {
            (disposition == "engine-text" || (fallbacks && disposition == "game-string"))
                && row_pages.iter().any(|page| pages.contains(&page.as_str()))
        })
        .map(|(string, _, _)| words(&string))
        .filter(|words| !words.is_empty())
        .collect();
    out.sort_by_key(|words| std::cmp::Reverse(words.len()));
    out
}

/// What `text` draws beyond the snapshot's `data` and the `listed` strings: empty when every
/// word is accounted for.
fn unlisted(text: &str, data: &[&str], listed: &[Vec<String>]) -> Vec<String> {
    let mut text = text.to_owned();
    let mut data = data.to_vec();
    data.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in data {
        text = text.replace(d, " ");
    }
    let mut remaining = words(&text);
    for sequence in listed {
        let mut i = 0;
        while i + sequence.len() <= remaining.len() {
            if remaining[i..i + sequence.len()] == sequence[..] {
                remaining.drain(i..i + sequence.len());
            } else {
                i += 1;
            }
        }
    }
    remaining
}

/// Every text shape `draw` paints into a fresh context (two passes: egui sizes a new window
/// before it paints the contents).
fn drawn(draw: impl Fn(&egui::Context)) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut output = None;
    for _ in 0..2 {
        ctx.begin_pass(egui::RawInput::default());
        draw(&ctx);
        output = Some(end_pass(&ctx));
    }
    output
        .unwrap()
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// Every label resolved to a marker naming its first setting.
fn game_labels() -> UiLabels {
    UiLabels::resolve(|name| Some(format!("«{name}»")))
}

/// Draw `snapshot` (with each label table) and assert nothing unlisted for `pages` is drawn.
fn assert_page(pages: &[&str], snapshot: &PageSnapshot, state: &GameMenuState, data: &[&str]) {
    for (mode, labels, fallbacks) in [("game settings", game_labels(), false), ("engine defaults", UiLabels::default(), true)] {
        // With the game's words, a fallback drawn anyway is a literal that skipped the labels.
        let listed = listed(pages, fallbacks);
        let snapshot = PageSnapshot { labels: labels.clone(), ..snapshot.clone() };
        let texts = drawn(|ctx| {
            let mut state = state.clone();
            let mut outputs = PageOutputs::default();
            if state.visible {
                draw_game_menu(ctx, &snapshot, &mut state, &mut outputs);
            } else {
                draw_hud(ctx, &snapshot);
            }
        });
        assert!(!texts.is_empty(), "{pages:?} ({mode}) drew no text");
        for text in &texts {
            let rest = unlisted(text, data, &listed);
            assert!(
                rest.is_empty(),
                "{pages:?} ({mode}) draws {text:?}: {rest:?} is neither the game's word, the snapshot's data, nor listed for the page in native-page-strings.tsv"
            );
        }
    }
}

fn menu(page: GameMenuPage) -> GameMenuState {
    GameMenuState { visible: true, page, ..Default::default() }
}

#[test]
fn oblivion_rest_fallback_draws_listed_labels_and_attribute_data() {
    for sleeping in [false, true] {
        let snapshot = PageSnapshot { rest: Some(RestView::Duration { sleeping, hours: 1 }), ..Default::default() };
        assert_page(&["oblivion rest"], &snapshot, &menu(GameMenuPage::SleepWait), &[]);
    }
    let names = ["Strength", "Intelligence", "Willpower", "Agility", "Speed", "Endurance", "Personality", "Luck"];
    let attributes = names.map(|name| RestAttribute { name, base: "40".into(), bonus: 1, selected: false, enabled: true });
    let snapshot = PageSnapshot {
        rest: Some(RestView::LevelUp { level: 2, attributes: Box::new(attributes), selected: 0, ready: false }),
        ..Default::default()
    };
    assert_page(&["oblivion rest"], &snapshot, &menu(GameMenuPage::LevelUp), &names);
}

#[test]
fn the_inventory_keeps_no_unresolved_candidates() {
    let left: Vec<String> = rows()
        .into_iter()
        .filter(|(_, _, disposition)| disposition == "candidate-for-game-string")
        .map(|(string, pages, _)| format!("{string} ({})", pages.join("/")))
        .collect();
    assert!(left.is_empty(), "rows still candidates: resolve them through UiLabels or list them with a reason: {left:?}");
}

#[test]
fn the_word_matcher_accounts_for_listed_strings_data_and_markers() {
    let listed = vec![words("Esc  Continue"), words("gold")];
    assert!(unlisted("«sBuy» — 12 gold", &[], &listed).is_empty());
    assert!(unlisted("Iron Dagger  ×2  —  10 «sGold»", &["Iron Dagger"], &listed).is_empty());
    assert_eq!(unlisted("Form ID", &[], &listed), ["Form", "ID"]);
}

#[test]
fn pause_page_draws_only_game_and_listed_words() {
    assert_page(&["pause"], &PageSnapshot::default(), &menu(GameMenuPage::Pause), &[]);
}

#[test]
fn settings_page_draws_only_game_and_listed_words() {
    let mut changed = SettingEntry::toggle("controls.invert", "Gameplay", "Activate", "Picks up and uses things.", true);
    changed.value = SettingValue::Bool(false);
    let snapshot = PageSnapshot {
        settings: vec![changed, SettingEntry::toggle("interface.crosshair", "Interface", "Show crosshair", "Draws the reticle.", true)],
        ..Default::default()
    };
    // A registry label is the registry's data, drawn through the labels when the game has the word.
    assert_page(&["settings"], &snapshot, &menu(GameMenuPage::Settings), &["Picks up and uses things."]);
}

#[test]
fn inventory_page_draws_only_game_and_listed_words() {
    let item = |index: u32, name: &str, category: &'static str, equippable: bool, consumable: bool| InventoryItemView {
        index,
        form_id: 0x100 + index,
        name: name.to_owned(),
        category,
        details: format!("{name} details"),
        count: 2,
        value: 15,
        weight: 1.5,
        equipped: equippable,
        equippable,
        consumable,
    };
    let snapshot = PageSnapshot {
        inventory: Some(InventorySnapshot {
            total_weight: 9.0,
            items: vec![item(0, "Stimpak", "Aid", false, true), item(1, "Varmint Rifle", "Weapon", true, false), item(2, "Bobby Pin", "Misc", false, false)],
        }),
        ..Default::default()
    };
    let data = ["Stimpak details", "Varmint Rifle", "Stimpak", "Bobby Pin"];
    assert_page(&["inventory"], &snapshot, &menu(GameMenuPage::Inventory), &data);
    // The other branches: a weapon selected (Unequip), an item with no contract, the empty states.
    let mut state = menu(GameMenuPage::Inventory);
    state.selected_inventory_index = Some(1);
    state.selected_inventory_category = "Weapon".into();
    assert_page(&["inventory"], &snapshot, &state, &["Varmint Rifle details", "Varmint Rifle"]);
    state.selected_inventory_index = Some(2);
    state.selected_inventory_category = "Misc".into();
    assert_page(&["inventory"], &snapshot, &state, &["Bobby Pin details", "Bobby Pin"]);
    assert_page(&["inventory"], &PageSnapshot::default(), &menu(GameMenuPage::Inventory), &[]);
    let empty = PageSnapshot { inventory: Some(InventorySnapshot::default()), ..Default::default() };
    assert_page(&["inventory"], &empty, &menu(GameMenuPage::Inventory), &[]);
}

#[test]
fn message_box_page_draws_only_game_and_listed_words() {
    let snapshot = PageSnapshot {
        message_box: Some(MessageBoxView { title: "Vault 21".into(), text: "The door is sealed.".into(), buttons: Vec::new() }),
        ..Default::default()
    };
    assert_page(&["message box"], &snapshot, &menu(GameMenuPage::MessageBox), &["Vault 21", "The door is sealed."]);
    assert_page(&["message box"], &PageSnapshot::default(), &menu(GameMenuPage::MessageBox), &[]);
}

#[test]
fn barter_page_draws_only_game_and_listed_words() {
    let goods = |name: &str| BarterItemView { form_id: 1, name: name.into(), count: 3, price: 12 };
    let snapshot = PageSnapshot {
        barter: Some(BarterSnapshot {
            vendor_name: "Belethor".into(),
            player_gold: 40,
            vendor_gold: 900,
            buy: vec![goods("Iron Dagger")],
            sell: Vec::new(),
        }),
        ..Default::default()
    };
    assert_page(&["barter"], &snapshot, &menu(GameMenuPage::Barter), &["Belethor", "Iron Dagger"]);
    assert_page(&["barter"], &PageSnapshot::default(), &menu(GameMenuPage::Barter), &[]);
}

#[test]
fn crafting_page_draws_only_game_and_listed_words() {
    let snapshot = PageSnapshot {
        crafting: Some(CraftingSnapshot {
            bench_name: "Forge".into(),
            recipes: vec![CraftingRecipeView {
                form_id: 1,
                name: "Iron Sword".into(),
                count: 2,
                ingredients: vec![("Iron Ingot".into(), 2, 1)],
                craftable: false,
            }],
        }),
        ..Default::default()
    };
    assert_page(&["crafting"], &snapshot, &menu(GameMenuPage::Crafting), &["Forge", "Iron Sword", "Iron Ingot"]);
    let empty = PageSnapshot { crafting: Some(CraftingSnapshot { bench_name: "Forge".into(), recipes: Vec::new() }), ..Default::default() };
    assert_page(&["crafting"], &empty, &menu(GameMenuPage::Crafting), &["Forge"]);
    assert_page(&["crafting"], &PageSnapshot::default(), &menu(GameMenuPage::Crafting), &[]);
}

#[test]
fn dialogue_page_draws_only_game_and_listed_words() {
    let snapshot = PageSnapshot {
        dialogue: Some(DialogueTopicSnapshot {
            npc: 7,
            speaker_name: "Eltrys".into(),
            topic_form_id: 0x2010,
            topic_name: "Greeting".into(),
            info_form_id: 0x2011,
            response_text: "Well met.".into(),
            topics: vec![
                DialogueTopicEntryView { topic_form_id: 0x2010, name: "Hello".into(), said: false },
                DialogueTopicEntryView { topic_form_id: 0x2012, name: "Leave".into(), said: true },
            ],
            speaking: true,
            voiced: false,
            choices: true,
        }),
        ..Default::default()
    };
    assert_page(&["dialogue"], &snapshot, &menu(GameMenuPage::Dialogue), &["Eltrys", "Greeting", "Well met.", "Hello", "Leave"]);
    assert_page(&["dialogue"], &PageSnapshot::default(), &menu(GameMenuPage::Dialogue), &[]);
}

#[test]
fn chargen_page_draws_only_game_and_listed_words() {
    let option = |id: u32, name: &str| ChargenOption { id, name: name.into(), description: String::new() };
    let screen = |screen: ChargenScreenView| PageSnapshot {
        chargen: Some(ChargenSnapshot {
            screen,
            serial: 1,
            title: "Vigor Tester".into(),
            name: "Courier".into(),
            female: false,
            race: 1,
            races: vec![option(1, "Caucasian")],
            attributes: vec![(option(2, "Strength"), 5)],
            skills: vec![(option(3, "Guns"), 15)],
            tagged: Vec::new(),
            traits: vec![option(4, "Wild Wasteland")],
            chosen_traits: Vec::new(),
        }),
        ..Default::default()
    };
    let data = ["Vigor Tester", "Courier", "Caucasian", "Strength", "Guns", "Wild Wasteland"];
    for view in [
        ChargenScreenView::Name,
        ChargenScreenView::RaceSex,
        ChargenScreenView::Special { points: 5, min: 1, max: 10 },
        ChargenScreenView::TagSkills { count: 3 },
        ChargenScreenView::Traits { max: 2 },
    ] {
        assert_page(&["chargen"], &screen(view), &menu(GameMenuPage::CharGen), &data);
    }
    assert_page(&["chargen"], &PageSnapshot::default(), &menu(GameMenuPage::CharGen), &[]);
}

#[test]
fn hud_prompt_and_vitals_draw_only_game_and_listed_words() {
    for (labels, fallbacks) in [(game_labels(), false), (UiLabels::default(), true)] {
        let snapshot = PageSnapshot {
            interaction_prompt: Some(InteractionPrompt { binding: "E", verb: labels.shared("Take all") }),
            show_prompts: true,
            show_vitals: true,
            vitals: Some(vec![
                VitalBarView { label: labels.shared("HP"), actor_value: "Health", current: 50.0, max: 100.0 },
                VitalBarView { label: labels.shared("RAD"), actor_value: "RadiationRads", current: 0.0, max: 1000.0 },
            ]),
            labels,
            ..Default::default()
        };
        let listed = listed(&["hud prompt", "hud vitals"], fallbacks);
        for text in drawn(|ctx| draw_hud(ctx, &snapshot)) {
            let rest = unlisted(&text, &[], &listed);
            assert!(rest.is_empty(), "the HUD draws {text:?}: {rest:?} is unlisted");
        }
    }
}

fn registered_mcm(dialog: Option<McmDialogView>, custom: Option<McmCustomContent>) -> McmSnapshot {
    let cell = |id: u32, kind, label: &str, value: &str| McmCellView {
        id,
        kind,
        label: label.into(),
        value_text: value.into(),
        number: 0.0,
        flags: 0,
    };
    McmSnapshot {
        mods: vec![McmModView { name: "Frostfall".into(), pages: vec!["Survival".into()] }],
        open_mod: Some(0),
        open_page: Some(0),
        title: "Survival settings".into(),
        info: "Sets how cold it gets.".into(),
        cells: vec![
            cell(0x101, McmCellKind::Header, "Core", ""),
            cell(0x102, McmCellKind::Toggle, "Enabled", ""),
            cell(0x103, McmCellKind::Slider, "Cold rate", "2.5 units"),
            cell(0x104, McmCellKind::KeyMap, "Wait key", "Z"),
        ],
        dialog,
        custom,
        generation: 1,
        ..Default::default()
    }
}

const MCM_DATA: &[&str] =
    &["Frostfall", "Survival", "Survival settings", "Sets how cold it gets.", "Core", "Enabled", "Cold rate", "units", "Wait key", "Z"];

#[test]
fn pause_page_offers_mod_configuration_in_listed_words() {
    let snapshot = PageSnapshot { mcm: Some(registered_mcm(None, None)), ..Default::default() };
    assert_page(&["pause"], &snapshot, &menu(GameMenuPage::Pause), &[]);
}

#[test]
fn mod_configuration_page_draws_only_game_snapshot_and_listed_words() {
    let dialogs = [
        None,
        Some(McmDialogView::Slider { id: 0x103, start: 2.5, default: 1.0, min: 0.0, max: 10.0, interval: 0.5, format: "{1} units".into() }),
        Some(McmDialogView::Menu { id: 0x105, options: vec!["Easy".into(), "Hard".into()], start: 0, default: 0 }),
        Some(McmDialogView::Color { id: 0x106, start: 0x00FF8040, default: 0 }),
        Some(McmDialogView::Input { id: 0x107, start_text: "Whiterun".into() }),
        Some(McmDialogView::KeyMap { id: 0x104, current: 0x2C, with_unmap: true }),
        Some(McmDialogView::Message { text: "Reset everything?".into(), with_cancel: true, accept_label: String::new(), cancel_label: String::new() }),
    ];
    let data: Vec<&str> = MCM_DATA.iter().copied().chain(["Easy", "Hard", "Whiterun", "Reset everything"]).collect();
    for dialog in dialogs {
        let snapshot = PageSnapshot { mcm: Some(registered_mcm(dialog, None)), ..Default::default() };
        assert_page(&["mod configuration"], &snapshot, &menu(GameMenuPage::ModConfiguration), &data);
    }
}

#[test]
fn mod_configuration_page_draws_its_empty_and_unsupported_states_in_listed_words() {
    for snapshot in [
        PageSnapshot::default(),
        PageSnapshot { mcm: Some(McmSnapshot { mods: vec![McmModView { name: "Frostfall".into(), pages: Vec::new() }], ..Default::default() }), ..Default::default() },
        PageSnapshot {
            mcm: Some(registered_mcm(None, Some(McmCustomContent { source: "frostfall/logo.swf".into(), x: 0.0, y: 0.0, image: None }))),
            ..Default::default()
        },
    ] {
        assert_page(&["mod configuration"], &snapshot, &menu(GameMenuPage::ModConfiguration), &[MCM_DATA, &["frostfall/logo.swf"]].concat());
    }
}
