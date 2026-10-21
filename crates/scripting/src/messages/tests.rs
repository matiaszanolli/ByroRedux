use super::*;
use byroredux_plugin::esm::records::{GlobalRecord, MesgRecord, NpcRecord};

#[test]
fn printf_conversions_take_the_show_arguments() {
    assert_eq!(substitute_numbers("Current Bounty: %.0f", &[250.0]), "Current Bounty: 250");
    assert_eq!(
        substitute_numbers("learn 20%% faster (%.0f days, %.2f)", &[3.0, 1.5]),
        "learn 20% faster (3 days, 1.50)"
    );
    assert_eq!(substitute_numbers("%0.f", &[3.0]), "3");
    assert_eq!(substitute_numbers("%2.0f", &[3.0]), " 3", "width is a minimum, space-filled");
    assert_eq!(substitute_numbers("%d and %.0f", &[7.9]), "7 and 0", "missing args read 0");
    assert_eq!(substitute_numbers("100% sure", &[]), "100% sure", "a bare % stays");
}

/// #6014 — the CS wiki's switches `+`, space, `-`, `0` and the `%g` / `%e`
/// conversions parse, consume their argument and format; the arguments after
/// them stay aligned.
#[test]
fn printf_switches_consume_their_argument() {
    assert_eq!(substitute_numbers("%05.2f", &[123.45]), "123.45", "already wider than 5");
    assert_eq!(substitute_numbers("%08.2f", &[123.45]), "00123.45");
    assert_eq!(substitute_numbers("%08.2f", &[-123.45]), "-0123.45", "zeros go after the sign");
    assert_eq!(substitute_numbers("%+.0f", &[7.0]), "+7");
    assert_eq!(substitute_numbers("%+.0f", &[-7.0]), "-7");
    assert_eq!(substitute_numbers("% .0f", &[7.0]), " 7");
    assert_eq!(substitute_numbers("[%-5.0f]", &[12.0]), "[12   ]");
    assert_eq!(substitute_numbers("[%5.0f]", &[12.0]), "[   12]");
    assert_eq!(substitute_numbers("[%+-5.0f]", &[12.0]), "[+12  ]");
    assert_eq!(substitute_numbers("[%05d]", &[42.9]), "[00042]");
    assert_eq!(substitute_numbers("%+-5.0f|%.0f|% 5.2f|%e|%d", &[1.0, 2.0, 3.0, 4.0, 5.0]),
        "+1   |2| 3.00|4.000000E+000|5");
    assert_eq!(substitute_numbers("%.3e", &[123000.0]), "1.230E+005");
    assert_eq!(substitute_numbers("%.3e", &[0.00123]), "1.230E-003");
    assert_eq!(substitute_numbers("%g", &[999999.0]), "999999");
    assert_eq!(substitute_numbers("%g", &[1_000_000.0]), "1E+006");
    assert_eq!(substitute_numbers("%+5q|%.0f", &[9.0]), "%+5q|9", "an unknown conversion consumes nothing");
    assert_eq!(substitute_numbers("%-", &[]), "%-", "a dangling spec stays");
}

/// A mod-supplied width never allocates unbounded padding.
#[test]
fn printf_width_is_clamped() {
    let wide = substitute_numbers("%99999999999999999999.0f", &[1.0]);
    assert_eq!(wide.len(), MAX_PRINTF_WIDTH);
    assert_eq!(substitute_numbers("%64.0f", &[1.0]).len(), MAX_PRINTF_WIDTH);
    assert_eq!(substitute_numbers("%65.0f", &[1.0]).len(), MAX_PRINTF_WIDTH);
}

/// #6013 — a new box reads -1 until pressed; the previous unread answer does
/// not survive it.
#[test]
fn a_new_script_box_clears_the_unread_answer() {
    let mut world = World::new();
    crate::register(&mut world);
    record_button(&world, 1);
    show_obscript_message(
        &world,
        QuestFormId(0x0001_C3D0),
        crate::obscript_vm::ObScriptMessage {
            text: "Again?".into(),
            values: Vec::new(),
            buttons: Vec::new(),
            message_box: true,
        },
    );
    assert_eq!(take_button(&world), -1);
    // A plain `Message` is no box and leaves a pending answer alone.
    record_button(&world, 1);
    show_obscript_message(
        &world,
        QuestFormId(0x0001_C3D0),
        crate::obscript_vm::ObScriptMessage {
            text: "FYI".into(),
            values: Vec::new(),
            buttons: Vec::new(),
            message_box: false,
        },
    );
    assert_eq!(take_button(&world), 1);
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

/// A mod-supplied precision never reaches `format!` unbounded: 65 536 used to
/// panic and a smaller one allocated that many digits per conversion (#6011).
#[test]
fn printf_precision_is_clamped() {
    let huge = substitute_numbers("%.65536f", &[1.5]);
    assert_eq!(huge.len(), "1.".len() + MAX_PRINTF_PRECISION);
    let many = substitute_numbers(&"%.65535f".repeat(2000), &[1.5]);
    assert!(many.len() < 2000 * (2 + MAX_PRINTF_PRECISION) + 1);
    assert_eq!(substitute_numbers("%.2f", &[1.5]), "1.50", "ordinary precision is unchanged");
}
