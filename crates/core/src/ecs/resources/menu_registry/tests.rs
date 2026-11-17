use super::*;

fn native(page: NativeMenu) -> MenuSurface {
    MenuSurface::Native { page: page.page().to_owned() }
}

fn def(game: MenuGame, name: &str) -> MenuDef {
    tables::table(game).by_name(name).unwrap_or_else(|| panic!("{} has no menu {name}", game.label())).clone()
}

const ALL_GAMES: [MenuGame; 4] = [MenuGame::Oblivion, MenuGame::Fallout3NV, MenuGame::Skyrim, MenuGame::Fallout4];

/// GUI-REG-004 / -006 — the compiled-in Phase 0 tables parse, and the rows the design names carry
/// the flags it states.
#[test]
fn phase0_tables_parse_with_the_flags_the_design_states() {
    // Row counts of the checked-in flags tables (plus the console row the coded games add).
    assert_eq!(tables::table(MenuGame::Skyrim).menus().iter().filter(|d| d.basis == FlagsBasis::Sourced).count(), 34);
    assert_eq!(tables::table(MenuGame::Fallout4).menus().iter().filter(|d| d.basis == FlagsBasis::Sourced).count(), 29);

    // Skyrim: the HUD (0x18902) carries neither kModal nor kPausesGame; InventoryMenu keeps
    // CommonLibSSE's 0xA481; the dialogue menu (0x4400) does not pause but shows a cursor.
    let hud = def(MenuGame::Skyrim, "HUD Menu");
    assert_eq!(hud.flags, MenuFlags::HUD | MenuFlags::ALLOWS_SAVING);
    assert_eq!(hud.target, TargetSurface::Movie);
    let inventory = def(MenuGame::Skyrim, "InventoryMenu");
    assert!(inventory.flags.contains(MenuFlags::PAUSES_GAME | MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT));
    assert_eq!(inventory.target, TargetSurface::Native("Inventory".into()));
    let dialogue = def(MenuGame::Skyrim, "Dialogue Menu");
    assert!(!dialogue.flags.contains(MenuFlags::PAUSES_GAME) && dialogue.flags.contains(MenuFlags::CLAIMS_INPUT));
    assert_eq!(def(MenuGame::Skyrim, "MessageBoxMenu").target, TargetSurface::Movie);
    assert_eq!(def(MenuGame::Skyrim, "MessageBoxMenu").document.as_deref(), Some("interface\\messagebox.swf"));
    assert_eq!(def(MenuGame::Skyrim, "Top Menu").basis, FlagsBasis::EngineDefault);
    assert_eq!(def(MenuGame::Skyrim, "Top Menu").flags, MenuFlags::NONE);
    assert_eq!(tables::table(MenuGame::Skyrim).by_document("Interface/HUDMenu.swf").map(|d| d.name.as_str()), Some("HUD Menu"));

    // Fallout 4: the VATS menu never pauses (VATS-UI-007); MessageBoxMenu 0x080184dd does.
    assert!(!def(MenuGame::Fallout4, "VATSMenu").flags.contains(MenuFlags::PAUSES_GAME));
    assert!(def(MenuGame::Fallout4, "MessageBoxMenu").flags.contains(MenuFlags::PAUSES_GAME));
    assert!(!def(MenuGame::Fallout4, "HUDMenu").flags.contains(MenuFlags::MENU_MODE));

    // Fallout 3 / New Vegas: codes joined by xNVSE name; everything modal except the overlays,
    // the VATS menu (no pause by policy) and the dialogue menu (no pause by engine default).
    let fallout = |name: &str| def(MenuGame::Fallout3NV, name);
    assert_eq!(fallout("MessageMenu").code, Some(1001));
    assert_eq!((fallout("InventoryMenu").code, fallout("InventoryMenu").class), (Some(1002), MenuClass::BigFour));
    assert_eq!(fallout("StartMenu").code, Some(1013));
    assert_eq!(fallout("BarterMenu").code, Some(1053));
    assert_eq!(fallout("CreditsMenu").code, Some(1047));
    assert_eq!(fallout("LockPickMenu").code, Some(1014));
    assert_eq!(fallout("RaceSexMenu").code, Some(1036));
    assert_eq!(fallout("HUDMainMenu").code, Some(1004));
    assert_eq!(fallout("HUDMainMenu").flags, MenuFlags::HUD);
    assert_eq!(fallout("InventoryMenu").document.as_deref(), Some("menus\\main\\inventory_menu.xml"));
    assert_eq!(fallout("InventoryMenu").target, TargetSurface::MenuXml);
    assert_eq!(fallout("InventoryMenu").fallback, TargetSurface::Native("Inventory".into()));
    assert_eq!(fallout("DialogMenu").flags, MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT);
    assert_eq!(fallout("VATSMenu").flags, MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT);
    assert_eq!(fallout("StartMenu").flags, MenuFlags::PAUSES_GAME | MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT);
    assert_eq!(fallout("QuickKeysMenu").flags, MenuFlags::NONE);

    // Oblivion: codes joined by the CS name; barter is the container menu 1008.
    let oblivion = |name: &str| def(MenuGame::Oblivion, name);
    assert_eq!(oblivion("ContainerMenu").code, Some(1008));
    assert_eq!(oblivion("PauseMenu").code, Some(1013));
    assert_eq!(oblivion("OptionsMenu").code, Some(1015));
    assert_eq!(oblivion("MessageMenu").code, Some(1001));
    assert_eq!(oblivion("MagicMenu").class, MenuClass::BigFour);
    assert_eq!(oblivion("HUDReticleMenu").code, Some(1006));
}

/// Every vanilla menu code of the code tables is some menu's code (no code is dropped by the join).
#[test]
fn every_vanilla_menu_code_names_a_menu() {
    for (game, tsv) in [(MenuGame::Fallout3NV, tables_tsv::FALLOUT), (MenuGame::Oblivion, tables_tsv::OBLIVION)] {
        for line in tsv.lines().skip(1) {
            let cells: Vec<&str> = line.split('\t').collect();
            let Ok(code) = cells[0].parse::<u32>() else { continue };
            if code < 1000 || cells[4] != "yes" {
                continue;
            }
            assert!(tables::table(game).by_code(code).is_some(), "{}: code {code} names no menu", game.label());
        }
    }
}

mod tables_tsv {
    pub const FALLOUT: &str = include_str!("../../../../../../docs/data/game-ui/menu-codes-fnv-fo3.tsv");
    pub const OBLIVION: &str = include_str!("../../../../../../docs/data/game-ui/menu-codes-oblivion.tsv");
}

/// GUI-PAGE-001 — every native page registers under a menu the game's table lists, except the
/// crafting page where the game has no crafting menu (an engine name with the default flags).
#[test]
fn native_pages_register_under_the_game_menu_names() {
    use NativeMenu::*;
    for game in ALL_GAMES {
        for page in [Pause, Settings, Inventory, Dialogue, MessageBox, Barter, Crafting, CharGen, Console, Loading, Hud] {
            let name = page.menu_name(game);
            let engine_coined = page == Crafting && matches!(game, MenuGame::Oblivion | MenuGame::Fallout4);
            assert_eq!(tables::table(game).by_name(name).is_some(), !engine_coined, "{} {page:?} -> {name}", game.label());
        }
    }
    assert_eq!(Dialogue.menu_name(MenuGame::Fallout3NV), "DialogMenu");
    assert_eq!(tables::table(MenuGame::Fallout3NV).by_name("DialogMenu").unwrap().code, Some(1009));
}

/// GUI-REG-001/002/003/009/011 — the stack, the derived answers, and the HUD under a modal.
#[test]
fn registry_answers_from_the_stack() {
    let mut registry = MenuRegistry::new(MenuGame::Skyrim);
    registry.publish([("HUD Menu".to_owned(), native(NativeMenu::Hud))]);
    assert!(registry.is_open("hud menu"));
    assert!(!registry.menu_mode() && !registry.paused() && !registry.menu_mode_query(0));
    assert!(registry.input_owner().is_none());

    // A modal over the HUD: both open, the modal owns input, the game pauses.
    registry.publish([
        ("HUD Menu".to_owned(), native(NativeMenu::Hud)),
        ("InventoryMenu".to_owned(), native(NativeMenu::Inventory)),
    ]);
    assert_eq!(registry.entries().iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["HUD Menu", "InventoryMenu"]);
    assert!(registry.is_open("HUD Menu") && registry.is_open("InventoryMenu"));
    assert_eq!(registry.input_owner().map(|e| e.name.as_str()), Some("InventoryMenu"));
    assert!(registry.paused() && registry.papyrus_in_menu_mode() && registry.menu_mode());

    // A message box opened on top takes input; closing it returns input to the inventory.
    registry.open("MessageBoxMenu", MenuSurface::Movie { path: "interface\\messagebox.swf".into(), profile: "Skyrim".into() });
    assert_eq!(registry.input_owner().map(|e| e.name.as_str()), Some("MessageBoxMenu"));
    assert!(registry.close("messageboxmenu").is_some());
    assert_eq!(registry.input_owner().map(|e| e.name.as_str()), Some("InventoryMenu"));

    // The dialogue menu: open and claiming input, but no pause, so IsInMenuMode is false.
    registry.publish([
        ("HUD Menu".to_owned(), native(NativeMenu::Hud)),
        ("Dialogue Menu".to_owned(), native(NativeMenu::Dialogue)),
    ]);
    assert!(!registry.is_open("InventoryMenu"));
    assert!(!registry.papyrus_in_menu_mode());
    assert_eq!(registry.input_owner().map(|e| e.name.as_str()), Some("Dialogue Menu"));

    // A HUD published after a modal still goes under it.
    registry.publish([("Dialogue Menu".to_owned(), native(NativeMenu::Dialogue))]);
    registry.publish([
        ("Dialogue Menu".to_owned(), native(NativeMenu::Dialogue)),
        ("HUD Menu".to_owned(), native(NativeMenu::Hud)),
    ]);
    assert_eq!(registry.entries()[0].name, "HUD Menu");
}

/// GUI-REG-002 — `MenuMode` types and codes on a coded game; the operator overlay answers only
/// `MenuMode 3`.
#[test]
fn menu_mode_types_and_codes() {
    let mut registry = MenuRegistry::new(MenuGame::Fallout3NV);
    registry.publish([
        ("HUDMainMenu".to_owned(), native(NativeMenu::Hud)),
        ("Console".to_owned(), native(NativeMenu::Console)),
    ]);
    assert!(registry.menu_mode_query(1004), "the HUD answers its own code");
    assert!(registry.menu_mode_query(3), "the overlay answers the console type");
    assert!(!registry.menu_mode_query(0) && !registry.menu_mode() && !registry.paused());
    assert_eq!(registry.input_owner().map(|e| e.name.as_str()), Some("Console"));

    registry.open("InventoryMenu", native(NativeMenu::Inventory));
    assert!(registry.menu_mode_query(0) && registry.menu_mode_query(1) && registry.menu_mode_query(1002));
    assert!(!registry.menu_mode_query(2) && !registry.menu_mode_query(1001));
    assert!(registry.menu_mode_block_runs(None) && registry.menu_mode_block_runs(Some(1002)));
    assert!(!registry.menu_mode_block_runs(Some(1004)), "the HUD is not a menu-mode menu");
    registry.close("InventoryMenu");

    let race = registry.name_for_code(1036).unwrap();
    registry.open(race, native(NativeMenu::CharGen));
    assert!(registry.menu_mode_query(2) && registry.menu_mode_query(1036) && registry.paused());

    let report = registry.report();
    assert!(report[0].starts_with("ui.menus: game=fallout3nv open=3 input_owner=RaceSexMenu paused=yes menu_mode=yes devices=0"), "{report:?}");
    assert!(report[1].contains("HUDMainMenu code=1004 surface=native Hud flags=hud basis=default"), "{report:?}");
}
