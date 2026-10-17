use super::*;
use byroredux_plugin::esm::records::{GlobalRecord, MesgRecord, NpcRecord};

#[test]
fn printf_conversions_take_the_show_arguments() {
    assert_eq!(substitute_numbers("Current Bounty: %.0f", &[250.0]), "Current Bounty: 250");
    assert_eq!(
        substitute_numbers("learn 20%% faster (%.0f days, %.2f)", &[3.0, 1.5]),
        "learn 20% faster (3 days, 1.50)"
    );
    assert_eq!(substitute_numbers("%d and %.0f", &[7.9]), "7 and 0", "missing args read 0");
    assert_eq!(substitute_numbers("100% sure", &[]), "100% sure", "a bare % stays");
}

/// `<Global=…>` (floats to two places), `<Alias=Player>`, an unfilled alias
/// (`[...]`), and an unfillable tag left as written.
#[test]
fn text_replacement_fills_globals_aliases_and_leaves_the_rest() {
    const QUEST: u32 = 0x0001_0000;
    let mut world = World::new();
    crate::register(&mut world);
    let mut index = EsmIndex::default();
    index.globals.insert(
        0x10,
        GlobalRecord {
            form_id: 0x10,
            editor_id: "FFR04NirnCount".into(),
            value: SettingValue::Short(0),
        },
    );
    index.globals.insert(
        0x11,
        GlobalRecord {
            form_id: 0x11,
            editor_id: "DayRate".into(),
            value: SettingValue::Float(0.0),
        },
    );
    index.npcs.insert(
        crate::relationships::PLAYER_BASE,
        NpcRecord {
            form_id: crate::relationships::PLAYER_BASE,
            full_name: "Prisoner".into(),
            ..Default::default()
        },
    );
    index.messages.insert(
        0x20,
        MesgRecord {
            form_id: 0x20,
            description: "<Alias=Player> found <Global=FFR04NirnCount>/20 at <Global=DayRate>; <Alias=Hold> <Alias.Race=Player>".into(),
            owner_quest: QUEST,
            message_box: true,
            buttons: vec!["Ok".into()],
            ..Default::default()
        },
    );
    install_message_catalog(&mut world, &index);
    crate::scene::install_scene_quest_aliases(
        &mut world,
        [byroredux_plugin::esm::records::QustRecord {
            form_id: QUEST,
            aliases: vec![byroredux_plugin::esm::records::QuestAlias {
                alias_id: 3,
                name: "Hold".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    world.insert_resource(crate::Globals::default());
    {
        let mut globals = world.resource_mut::<crate::Globals>();
        globals.set(0x10, 7.0);
        globals.set(0x11, 1.5);
    }
    assert!(show_message(&world, 0x20, &[]));
    let pending = world.resource::<PendingScriptMessages>();
    assert_eq!(
        pending.0,
        vec![ScriptMessage {
            title: String::new(),
            text: "Prisoner found 7/20 at 1.50; [...] <Alias.Race=Player>".into(),
            message_box: true,
            buttons: vec!["Ok".into()],
            source: None,
        }]
    );
}

/// M47.3 — an ObScript `MessageBox` is formatted, gets the CS wiki's "Done"
/// when it names no button, and its answer reaches `GetButtonPressed` once.
#[test]
fn obscript_message_boxes_answer_get_button_pressed_once() {
    let mut world = World::new();
    crate::register(&mut world);
    let quest = QuestFormId(0x0001_C3D0);
    show_obscript_message(
        &world,
        quest,
        crate::obscript_vm::ObScriptMessage {
            text: "Erected by the Grateful Citizens of Bruma, 3E %.0f.".into(),
            values: vec![433.0],
            buttons: Vec::new(),
            message_box: true,
        },
    );
    let shown = world.resource::<PendingScriptMessages>().0.clone();
    assert_eq!(shown[0].text, "Erected by the Grateful Citizens of Bruma, 3E 433.");
    assert_eq!(shown[0].buttons, vec!["Done".to_owned()]);
    assert_eq!(shown[0].source, Some(quest));
    assert_eq!(take_button(&world), -1, "not answered yet");
    record_button(&world, 0);
    // The answer is global: the next script to read it may belong to a
    // different quest than the one that raised the box.
    assert_eq!(take_button(&world), 0);
    assert_eq!(take_button(&world), -1, "only the first call after the press");
}
