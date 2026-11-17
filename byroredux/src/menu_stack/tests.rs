use super::*;
use crate::extensions::ui_menu_snapshot;
use byroredux_sdk::compatibility::adapt_papyrus_ui_is_menu_open;
use byroredux_core::ecs::resources::menu_registry::MenuGame;

/// A world holding a registry for `game` with these native layers published.
fn published(game: MenuGame, state: &NativeMenuState) -> World {
    let mut world = World::new();
    let mut registry = MenuRegistry::new(game);
    let open = native_menus(&registry, state);
    registry.publish(open);
    world.insert_resource(registry);
    world
}

fn page(page: GameMenuPage) -> NativeMenuState {
    NativeMenuState { page: Some(page), ..Default::default() }
}

fn is_menu_open(world: &World, name: &str) -> bool {
    adapt_papyrus_ui_is_menu_open(&ui_menu_snapshot(world), name)
}

fn registry(world: &World) -> byroredux_core::ecs::ResourceRead<'_, MenuRegistry> {
    world.resource::<MenuRegistry>()
}

/// M48 A2 (GUI-REG-001/002/003/011, GUI-PAGE-001) — one registry answers the extender's
/// `UI.IsMenuOpen`, ObScript's `MenuMode` and Papyrus's `Utility.IsInMenuMode` alike: a native page
/// is visible to `UI.IsMenuOpen` under its game-facing name, a movie is visible to menu mode, the
/// HUD stays open under a modal, and the dialogue menu is open without being menu mode to Papyrus.
#[test]
fn menu_registry_answers_is_menu_open_and_menu_mode_alike() {
    // Skyrim: the native inventory page over the HUD.
    let world = published(MenuGame::Skyrim, &page(GameMenuPage::Inventory));
    assert!(is_menu_open(&world, "InventoryMenu"), "a native page is open to UI.IsMenuOpen");
    assert!(is_menu_open(&world, "HUD Menu"), "the HUD stays open under the modal");
    assert!(registry(&world).menu_mode() && registry(&world).papyrus_in_menu_mode());
    assert_eq!(registry(&world).input_owner().map(|open| open.name.as_str()), Some("InventoryMenu"));

    // The HUD alone is open but no menu mode.
    let world = published(MenuGame::Skyrim, &NativeMenuState::default());
    assert!(is_menu_open(&world, "HUD Menu"));
    assert!(!registry(&world).menu_mode() && !registry(&world).papyrus_in_menu_mode());
    assert!(registry(&world).input_owner().is_none(), "the HUD claims no input");

    // The dialogue menu: open, takes input, but IsInMenuMode is false (CK *IsInMenuMode*).
    let world = published(MenuGame::Skyrim, &page(GameMenuPage::Dialogue));
    assert!(is_menu_open(&world, "Dialogue Menu"));
    assert!(!registry(&world).papyrus_in_menu_mode() && !registry(&world).paused());

    // A Scaleform movie is a menu to every question, not only to UI.IsMenuOpen.
    let world = published(MenuGame::Skyrim, &NativeMenuState::default());
    world.resource_mut::<MenuRegistry>().open(
        "MessageBoxMenu",
        MenuSurface::Movie { path: "interface\\messagebox.swf".into(), profile: "SkyrimAvm1".into() },
    );
    assert!(is_menu_open(&world, "MessageBoxMenu") && registry(&world).papyrus_in_menu_mode());

    // New Vegas: the same page answers ObScript's codes and types through the code table.
    let world = published(MenuGame::Fallout3NV, &page(GameMenuPage::Inventory));
    assert!(is_menu_open(&world, "InventoryMenu"));
    let menus = registry(&world);
    assert!(menus.menu_mode_query(0) && menus.menu_mode_query(1) && menus.menu_mode_query(1002));
    assert!(!menus.menu_mode_query(2) && !menus.menu_mode_query(1013));
    drop(menus);
    let world = published(MenuGame::Fallout3NV, &NativeMenuState::default());
    assert!(registry(&world).menu_mode_query(1004), "HUDMainMenu (1004) is open");
    assert!(!registry(&world).menu_mode_query(0), "but the HUD alone is not menu mode");

    // The operator overlay answers MenuMode 3 and nothing that pauses or stops GameMode.
    let world = published(MenuGame::Fallout3NV, &NativeMenuState { overlay: true, ..Default::default() });
    assert!(registry(&world).menu_mode_query(3) && !registry(&world).menu_mode() && !registry(&world).paused());
}

/// #6620 — the barter page answers the load order's code and no other: 1008 on Oblivion (where
/// 1053 is not a code), 1053 on Fallout 3 / New Vegas (where 1008 is the container menu).
#[test]
fn the_barter_code_is_selected_by_the_load_order() {
    let mut world = World::new();
    install_menu_registry(&mut world, GameKind::Oblivion);
    let state = page(GameMenuPage::Barter);
    let open = native_menus(&registry(&world), &state);
    world.resource_mut::<MenuRegistry>().publish(open);
    assert!(registry(&world).menu_mode_query(1008) && !registry(&world).menu_mode_query(1053));
    assert!(registry(&world).menu_mode_query(0) && registry(&world).menu_mode_query(2), "still a menu");

    install_menu_registry(&mut world, GameKind::Fallout3NV);
    let open = native_menus(&registry(&world), &state);
    world.resource_mut::<MenuRegistry>().publish(open);
    assert!(registry(&world).menu_mode_query(1053) && !registry(&world).menu_mode_query(1008));

    // A barter session open behind no page still counts; with it shut the code is shut.
    let open = NativeMenuState { barter_open: true, ..Default::default() };
    let world = published(MenuGame::Fallout3NV, &open);
    assert!(registry(&world).menu_mode_query(1053));
    let world = published(MenuGame::Fallout3NV, &NativeMenuState::default());
    assert!(!registry(&world).menu_mode_query(1053));
}

/// #6576 — the crafting page is a menu but never the Pause menu, and each page registers exactly
/// its own menu; a waiting message box registers only when no other page is up.
#[test]
fn each_native_page_registers_exactly_its_own_menu() {
    let names = |game, state: &NativeMenuState| -> Vec<String> {
        registry(&published(game, state)).entries().iter().map(|open| open.name.clone()).collect()
    };
    assert_eq!(names(MenuGame::Skyrim, &page(GameMenuPage::Crafting)), ["HUD Menu", "Crafting Menu"]);
    assert_eq!(names(MenuGame::Fallout3NV, &page(GameMenuPage::Pause)), ["HUDMainMenu", "StartMenu"]);
    assert_eq!(names(MenuGame::Oblivion, &page(GameMenuPage::Settings)), ["HUDMainMenu", "OptionsMenu"]);
    assert_eq!(names(MenuGame::Fallout4, &page(GameMenuPage::Inventory)), ["HUDMenu", "PipboyMenu"]);
    let world = published(MenuGame::Fallout3NV, &page(GameMenuPage::Crafting));
    assert!(registry(&world).menu_mode_query(0) && registry(&world).menu_mode_query(2), "a bench is a menu");
    assert!(!registry(&world).menu_mode_query(1013) && !registry(&world).menu_mode_query(1));

    // The character-creation page is the screen's own menu, by its code.
    let chargen = NativeMenuState { page: Some(GameMenuPage::CharGen), chargen_code: Some(1036), ..Default::default() };
    assert_eq!(names(MenuGame::Fallout3NV, &chargen), ["HUDMainMenu", "RaceSexMenu"]);
    assert_eq!(names(MenuGame::Skyrim, &chargen), ["HUD Menu", "RaceSex Menu"]);

    // A message box waiting with no page up is open; behind the dialogue page it is not yet.
    let waiting = NativeMenuState { message_pending: true, ..Default::default() };
    assert_eq!(names(MenuGame::Fallout3NV, &waiting), ["HUDMainMenu", "MessageMenu"]);
    let behind = NativeMenuState { message_pending: true, ..page(GameMenuPage::Dialogue) };
    assert_eq!(names(MenuGame::Fallout3NV, &behind), ["HUDMainMenu", "DialogMenu"]);
}

/// `statement(op, payload)`: one compiled ObScript statement.
fn statement(op: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = op.to_le_bytes().to_vec();
    out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// `Begin <block> / set s<slot> to s<slot> + 1 / End`, compiled.
fn counting_block(block_type: u16, slot: u8) -> Vec<u8> {
    let expression = [b' ', b's', slot, 0, b' ', b'1', b' ', b'+'];
    let mut payload = vec![b's'];
    payload.extend_from_slice(&u16::from(slot).to_le_bytes());
    payload.extend_from_slice(&(expression.len() as u16).to_le_bytes());
    payload.extend_from_slice(&expression);
    let assignment = statement(0x15, &payload);
    let mut begin = block_type.to_le_bytes().to_vec();
    begin.extend_from_slice(&(assignment.len() as u32).to_le_bytes());
    [statement(0x10, &begin), assignment, statement(0x11, &[])].concat()
}

/// M48 A10 (GUI-REG-010, GUI-EVT-006) — the simulation pauses by the open registry menus' PauseGame
/// flags whatever their surface; `Begin MenuMode` blocks run on the frames a menu-mode menu is
/// open, the paused ones included, and `Begin GameMode` blocks never do.
#[test]
fn menu_mode_blocks_and_pause_follow_the_registry() {
    use byroredux_plugin::esm::records::{QustRecord, ScriptLocalVar, ScriptRecord, ScriptType};
    use byroredux_scripting::obscript_quests::{install_quest_scripts, obscript_quest_tick_system, ObScriptQuestLocals};
    use byroredux_scripting::obscript_vm::{BLOCK_GAME_MODE, BLOCK_MENU_MODE};
    use byroredux_scripting::quest_stages::{QuestFormId, QuestStageState};
    const QUEST: u32 = 0x0001_0F00;
    const SCRIPT: u32 = 0x0001_0F01;
    const GAME_MODE_RUNS: u16 = 1;
    const MENU_MODE_RUNS: u16 = 2;

    let compiled = [counting_block(BLOCK_GAME_MODE, 1), counting_block(BLOCK_MENU_MODE, 2)].concat();
    let local = |index: u32, name: &str| ScriptLocalVar { index, var_type: 1, name: name.into() };
    let script = ScriptRecord {
        form_id: SCRIPT,
        script_type: ScriptType::Quest,
        compiled_size: compiled.len() as u32,
        compiled,
        locals: vec![local(1, "gameMode"), local(2, "menuMode")],
        ..Default::default()
    };
    let quest = QustRecord { form_id: QUEST, script_ref: SCRIPT, ..Default::default() };
    let mut world = World::new();
    world.insert_resource(byroredux_scripting::ScriptGame(GameKind::Fallout3NV));
    install_quest_scripts(
        &mut world,
        &std::collections::HashMap::from([(QUEST, quest)]),
        &std::collections::HashMap::from([(SCRIPT, script)]),
    );
    let mut stages = QuestStageState::default();
    stages.start_quest(QuestFormId(QUEST), None);
    world.insert_resource(stages);
    install_menu_registry(&mut world, GameKind::Fallout3NV);

    let publish = |world: &World, state: &NativeMenuState| {
        let open = native_menus(&world.resource::<MenuRegistry>(), state);
        world.resource_mut::<MenuRegistry>().publish(open);
    };
    let runs = |world: &World| {
        let locals = world.resource::<ObScriptQuestLocals>();
        (locals.get(QUEST, GAME_MODE_RUNS), locals.get(QUEST, MENU_MODE_RUNS))
    };
    // One frame as `about_to_wait` runs it: the scheduler's quest systems unless a menu pauses
    // the game, the paused frame's menu-mode step if one does.
    let frame = |world: &World| {
        if simulation_paused(world, false) {
            run_menu_mode_frame(world, 10.0);
        } else {
            obscript_quest_tick_system(world, 10.0);
            byroredux_scripting::obscript_quests::obscript_quest_menu_mode_system(world, 10.0);
        }
    };

    // No menu but the HUD: the world runs and GameMode runs.
    publish(&world, &NativeMenuState::default());
    assert!(!simulation_paused(&world, false));
    frame(&world);
    assert_eq!(runs(&world), (1.0, 0.0), "GameMode, not MenuMode");

    // The pause page: the world pauses, and the paused frame runs MenuMode, never GameMode.
    publish(&world, &page(GameMenuPage::Pause));
    assert!(simulation_paused(&world, false));
    frame(&world);
    assert_eq!(runs(&world), (1.0, 1.0));

    // The dialogue menu: menu mode without a pause. The world advances, GameMode still waits.
    publish(&world, &page(GameMenuPage::Dialogue));
    assert!(!simulation_paused(&world, false));
    frame(&world);
    assert_eq!(runs(&world), (1.0, 2.0), "menu mode on an advancing frame runs MenuMode only");

    // The VATS menu pauses nothing on its own account (VATS-UI-007).
    let mut vats = MenuRegistry::new(MenuGame::Fallout3NV);
    vats.open("VATSMenu", MenuSurface::MenuXml { path: "menus\\vats_menu.xml".into() });
    assert!(!vats.paused() && vats.menu_mode());

    // A pausing menu on another surface pauses alike: a movie.
    world.resource_mut::<MenuRegistry>().open(
        "StartMenu",
        MenuSurface::Movie { path: "interface\\startmenu.swf".into(), profile: "test".into() },
    );
    assert!(simulation_paused(&world, false));
    // The loading cover holds the game its own way (the scheduler runs at a held clock), and its
    // own entry left over for the frame after it drops pauses nothing.
    assert!(!simulation_paused(&world, true));
    let cover = MenuRegistry::new(MenuGame::Skyrim);
    let mut cover_world = World::new();
    cover_world.insert_resource(cover);
    let open = native_menus(&cover_world.resource::<MenuRegistry>(), &NativeMenuState { loading: true, ..Default::default() });
    cover_world.resource_mut::<MenuRegistry>().publish(open);
    assert!(cover_world.resource::<MenuRegistry>().paused(), "ui.menus reports the load as paused");
    assert!(!simulation_paused(&cover_world, false), "but the cover, not the registry, holds the scheduler");

    // Back to the world: GameMode resumes.
    publish(&world, &NativeMenuState::default());
    frame(&world);
    assert_eq!(runs(&world), (2.0, 2.0));
}
