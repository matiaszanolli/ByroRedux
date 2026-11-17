//! The per-game menu tables the registry reads (GUI-REG-004 / -006): the Phase 0 TSVs under
//! `docs/data/game-ui/`, compiled in and parsed once per game. Every flag a row carries is either
//! read off a cited source (the row's `source` column) or the engine default the design names
//! (`docs/engine/ui.md` § *Registry and stack*); [`FlagsBasis`] records which.

use std::sync::OnceLock;

use super::{FlagsBasis, MenuClass, MenuFlags, MenuGame, TargetSurface};

const SKYRIM_FLAGS: &str = include_str!("../../../../../../docs/data/game-ui/menu-flags-skyrim.tsv");
const FALLOUT4_FLAGS: &str = include_str!("../../../../../../docs/data/game-ui/menu-flags-fallout4.tsv");
const FALLOUT_FLAGS: &str = include_str!("../../../../../../docs/data/game-ui/menu-flags-fnv-fo3.tsv");
const OBLIVION_FLAGS: &str = include_str!("../../../../../../docs/data/game-ui/menu-flags-oblivion.tsv");
const FALLOUT_CODES: &str = include_str!("../../../../../../docs/data/game-ui/menu-codes-fnv-fo3.tsv");
const OBLIVION_CODES: &str = include_str!("../../../../../../docs/data/game-ui/menu-codes-oblivion.tsv");
const SKYRIM_INVENTORY: &str = include_str!("../../../../../../docs/data/game-ui/menu-inventory-skyrim-se.tsv");
const FALLOUT4_INVENTORY: &str = include_str!("../../../../../../docs/data/game-ui/menu-inventory-fallout4.tsv");
/// New Vegas's inventory is Fallout 3's plus New Vegas's own menus; the two games share one
/// `GameKind` and one code table, so the registry reads the superset.
const FALLOUT_INVENTORY: &str = include_str!("../../../../../../docs/data/game-ui/menu-inventory-fnv.tsv");
const OBLIVION_INVENTORY: &str = include_str!("../../../../../../docs/data/game-ui/menu-inventory-oblivion.tsv");

/// The `IMenu` flag bits Skyrim and Fallout 4 share (SKSE64 / F4SE `GameMenus.h`, CommonLibSSE
/// `IMenu.h`, commonlibf4 `GameMenuBase.h`; the same 28 names on both games, PRD §9.1).
mod imenu {
    pub const PAUSES_GAME: u32 = 1 << 0;
    pub const USES_CURSOR: u32 = 1 << 2;
    pub const USES_MENU_CONTEXT: u32 = 1 << 3;
    pub const MODAL: u32 = 1 << 4;
    pub const UPDATE_USES_CURSOR: u32 = 1 << 10;
    pub const ALLOW_SAVING: u32 = 1 << 11;
    /// A menu takes the player's input when it shows a cursor, pushes a menu input context or is
    /// modal; the HUD, fader, cursor and loading menus carry none of these.
    pub const CLAIMS_INPUT: u32 = USES_CURSOR | USES_MENU_CONTEXT | MODAL | UPDATE_USES_CURSOR;
}

/// One menu a game knows: its game-facing name, its `MenuMode` code (Oblivion, Fallout 3 and
/// New Vegas), its flags and where they came from, and the surface Phase 0 targets for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuDef {
    pub name: String,
    pub code: Option<u32>,
    pub class: MenuClass,
    pub flags: MenuFlags,
    pub basis: FlagsBasis,
    /// The original movie or MenuXml document the inventory joins the menu to, if any.
    pub document: Option<String>,
    pub target: TargetSurface,
    /// The surface it falls back to while the target is not served (`S3:<page>` or S4).
    pub fallback: TargetSurface,
}

/// One game's menus, looked up by name (case-insensitively, as the games compare them) or code.
#[derive(Debug, Default)]
pub struct MenuTable {
    menus: Vec<MenuDef>,
}

impl MenuTable {
    pub fn menus(&self) -> &[MenuDef] {
        &self.menus
    }

    pub fn by_name(&self, name: &str) -> Option<&MenuDef> {
        self.menus.iter().find(|def| def.name.eq_ignore_ascii_case(name))
    }

    pub fn by_code(&self, code: u32) -> Option<&MenuDef> {
        self.menus.iter().find(|def| def.code == Some(code))
    }

    /// The menu whose original movie or document is `path` (`interface\hudmenu.swf`), compared
    /// without case and with either slash.
    pub fn by_document(&self, path: &str) -> Option<&MenuDef> {
        let wanted = normalize_path(path);
        self.menus.iter().find(|def| {
            def.document.as_deref().is_some_and(|document| document.split(';').any(|one| normalize_path(one) == wanted))
        })
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

/// The table of `game`, parsed on first use.
pub fn table(game: MenuGame) -> &'static MenuTable {
    static OBLIVION: OnceLock<MenuTable> = OnceLock::new();
    static FALLOUT: OnceLock<MenuTable> = OnceLock::new();
    static SKYRIM: OnceLock<MenuTable> = OnceLock::new();
    static FALLOUT4: OnceLock<MenuTable> = OnceLock::new();
    match game {
        MenuGame::Oblivion => OBLIVION.get_or_init(|| coded_table(game, OBLIVION_FLAGS, OBLIVION_CODES, OBLIVION_INVENTORY)),
        MenuGame::Fallout3NV => FALLOUT.get_or_init(|| coded_table(game, FALLOUT_FLAGS, FALLOUT_CODES, FALLOUT_INVENTORY)),
        MenuGame::Skyrim => SKYRIM.get_or_init(|| imenu_table(game, SKYRIM_FLAGS, SKYRIM_INVENTORY)),
        MenuGame::Fallout4 => FALLOUT4.get_or_init(|| imenu_table(game, FALLOUT4_FLAGS, FALLOUT4_INVENTORY)),
    }
}

/// The data rows of a TSV (header dropped), split on tabs.
fn rows(tsv: &str) -> impl Iterator<Item = Vec<&str>> {
    tsv.lines().skip(1).filter(|line| !line.trim().is_empty()).map(|line| line.split('\t').collect())
}

fn column<'a>(row: &[&'a str], index: usize) -> &'a str {
    row.get(index).map_or("", |cell| cell.trim())
}

/// Whether a menu named `name` is the game's HUD (it stays open under every modal, GUI-REG-003).
fn is_hud(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "hud menu" | "hudmenu" | "hudmainmenu" | "powerarmorhudmenu")
}

/// The VATS menu never pauses on its own account, whatever bit a source gives it (`VATS-UI-007`).
fn is_vats(name: &str) -> bool {
    name.eq_ignore_ascii_case("VATSMenu")
}

/// The engine default for a menu no source documents. Skyrim and Fallout 4: `kNone` (SKSE64
/// initialises `IMenu` flags to 0). Oblivion, Fallout 3 and New Vegas: a menu document whose
/// `<stackingtype>` is `&does_not_stack;` is an overlay, everything else is modal (pauses, counts
/// toward menu mode, takes input) — ui.md § *Registry and stack*.
pub(super) fn default_flags(game: MenuGame, overlay: bool) -> MenuFlags {
    match game {
        MenuGame::Skyrim | MenuGame::Fallout4 => MenuFlags::NONE,
        MenuGame::Oblivion | MenuGame::Fallout3NV if overlay => MenuFlags::NONE,
        MenuGame::Oblivion | MenuGame::Fallout3NV => MenuFlags::PAUSES_GAME | MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT,
    }
}

/// The parsed fallback surface of an inventory row (`S3:<page>` or `S4:…`).
fn fallback(row: &[&str]) -> TargetSurface {
    match column(row, 9).strip_prefix("S3:") {
        Some(page) => TargetSurface::Native(page.to_owned()),
        None => TargetSurface::Absent,
    }
}

/// The parsed target surface of an inventory row (`S1` … `S4`, with the fallback's native page).
fn target(row: &[&str]) -> TargetSurface {
    let page = |cell: &str| cell.strip_prefix("S3:").map(str::to_owned);
    match column(row, 8) {
        "S1" => TargetSurface::Movie,
        "S2" => TargetSurface::MenuXml,
        "S3" => TargetSurface::Native(page(column(row, 9)).unwrap_or_default()),
        _ => match page(column(row, 9)) {
            Some(page) => TargetSurface::Native(page),
            None => TargetSurface::Absent,
        },
    }
}

/// A document path the inventory names, or `None` for the "no movie / document found" rows.
fn document(row: &[&str]) -> Option<String> {
    let cell = column(row, 2);
    (cell.starts_with("interface\\") || cell.starts_with("menus\\")).then(|| cell.to_owned())
}

/// The menu rows (top-level menus and the HUD) of an inventory.
fn inventory_rows(tsv: &str) -> impl Iterator<Item = Vec<&str>> {
    rows(tsv).filter(|row| matches!(column(row, 3), "top-level menu" | "hud"))
}

/// Skyrim / Fallout 4: names and `IMenu` words from the flags table, joined by name to the inventory.
fn imenu_table(game: MenuGame, flags_tsv: &str, inventory_tsv: &str) -> MenuTable {
    let inventory: Vec<Vec<&str>> = inventory_rows(inventory_tsv).collect();
    let row_of = |name: &str| inventory.iter().find(|row| column(row, 0).eq_ignore_ascii_case(name));
    let mut menus = Vec::new();
    for row in rows(flags_tsv) {
        let name = column(&row, 0);
        let raw = u32::from_str_radix(column(&row, 2).trim_start_matches("0x"), 16).ok();
        let (mut flags, basis) = match raw {
            Some(word) => (imenu_flags(word), FlagsBasis::Sourced),
            None => (default_flags(game, false), FlagsBasis::EngineDefault),
        };
        if is_vats(name) {
            flags = flags.without(MenuFlags::PAUSES_GAME | MenuFlags::MENU_MODE);
        }
        if is_hud(name) {
            flags = flags | MenuFlags::HUD;
        }
        let inventory = row_of(name);
        menus.push(MenuDef {
            name: name.to_owned(),
            code: None,
            class: if name.eq_ignore_ascii_case("Console") { MenuClass::Console } else { MenuClass::Other },
            flags,
            basis,
            document: inventory.and_then(|row| document(row)),
            target: inventory.map_or(TargetSurface::Absent, |row| target(row)),
            fallback: inventory.map_or(TargetSurface::Absent, |row| fallback(row)),
        });
    }
    // Inventory menus no flags source names (none on Skyrim SE or Fallout 4 at Phase 0, but the
    // join must not drop one) get the engine default.
    for row in &inventory {
        let name = column(row, 0);
        if name.starts_with('(') || menus.iter().any(|def: &MenuDef| def.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        menus.push(MenuDef {
            name: name.to_owned(),
            code: None,
            class: MenuClass::Other,
            flags: default_flags(game, false),
            basis: FlagsBasis::EngineDefault,
            document: document(row),
            target: target(row),
            fallback: fallback(row),
        });
    }
    MenuTable { menus }
}

/// The canonical flags an `IMenu` word carries. "Counts toward menu mode" is `kPausesGame` on both
/// games (SKSE64 `UI::numPausesGame`, F4SE `numPauseGame`, CK *IsInMenuMode*).
fn imenu_flags(word: u32) -> MenuFlags {
    let mut flags = MenuFlags::NONE;
    if word & imenu::PAUSES_GAME != 0 {
        flags = flags | MenuFlags::PAUSES_GAME | MenuFlags::MENU_MODE;
    }
    if word & imenu::CLAIMS_INPUT != 0 {
        flags = flags | MenuFlags::CLAIMS_INPUT;
    }
    if word & imenu::ALLOW_SAVING != 0 {
        flags = flags | MenuFlags::ALLOWS_SAVING;
    }
    flags
}

/// The menu codes of the "main four" (`MenuMode 1`): the character interface. CS / GECK
/// *MenuTypes*: "stats, magic, inventory, quest log". Oblivion's quest log is in the map menu;
/// Fallout has no magic menu and its quest log is the Pip-Boy Data page.
fn big_four(game: MenuGame, code: u32) -> bool {
    match game {
        MenuGame::Oblivion => matches!(code, 1002 | 1003 | 1022 | 1023),
        _ => matches!(code, 1002 | 1003 | 1023),
    }
}

/// The game-facing name a code row gives: xNVSE's `s_menuNameToID` string when the row quotes one
/// (`kMenuType_Message ("MessageMenu")`), else the CS / GECK name's first word plus `Menu`
/// (`Container, Barter` -> `ContainerMenu`, `Credits` -> `CreditsMenu`).
fn code_row_name(row: &[&str]) -> Option<String> {
    let extender = column(row, 3);
    if let Some(quoted) = extender.split('"').nth(1).filter(|name| !name.is_empty()) {
        return Some(quoted.to_owned());
    }
    let word: String = column(row, 1).chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!word.is_empty()).then(|| format!("{word}Menu"))
}

/// Oblivion / Fallout 3 / New Vegas: the flags table's names, each joined to its code by name and
/// to its document by code (the inventory is keyed by the GECK / CS code names).
fn coded_table(game: MenuGame, flags_tsv: &str, codes_tsv: &str, inventory_tsv: &str) -> MenuTable {
    let codes: Vec<(u32, String)> = rows(codes_tsv)
        .filter(|row| column(row, 4) == "yes")
        .filter_map(|row| Some((column(&row, 0).parse::<u32>().ok().filter(|code| *code >= 1000)?, code_row_name(&row)?)))
        .collect();
    let inventory: Vec<Vec<&str>> = inventory_rows(inventory_tsv).collect();
    let code_of = |name: &str| codes.iter().find(|(_, coded)| coded.eq_ignore_ascii_case(name)).map(|(code, _)| *code);
    let inventory_of = |code: Option<u32>, name: &str| {
        inventory.iter().find(|row| match code {
            Some(code) => column(row, 1) == code.to_string(),
            None => column(row, 0).eq_ignore_ascii_case(name),
        })
    };
    let class_of = |code: Option<u32>| match code {
        Some(code) if big_four(game, code) => MenuClass::BigFour,
        _ => MenuClass::Other,
    };
    let mut menus = Vec::new();
    for row in rows(flags_tsv) {
        let name = column(&row, 0);
        let overlay = column(&row, 2) == "stackingtype=does_not_stack";
        let mut flags = default_flags(game, overlay);
        if is_vats(name) {
            flags = flags.without(MenuFlags::PAUSES_GAME);
        }
        // Deliberate engine default (ui.md § *Registry and stack*): the dialogue menu keeps the
        // world running. No source documents a pause bit for it on these games, the documented
        // Skyrim and Fallout 4 dialogue menus carry none, and the engine's dialogue page plays its
        // timed responses through the scheduler (766e1746e). It still counts toward menu mode
        // (GECK / CS *MenuMode* 1009).
        if name.eq_ignore_ascii_case("DialogMenu") {
            flags = flags.without(MenuFlags::PAUSES_GAME);
        }
        if is_hud(name) {
            flags = flags | MenuFlags::HUD;
        }
        let code = code_of(name);
        let inventory = inventory_of(code, name);
        menus.push(MenuDef {
            name: name.to_owned(),
            code,
            class: class_of(code),
            flags,
            basis: FlagsBasis::EngineDefault,
            document: inventory.and_then(|row| document(row)),
            target: inventory.map_or(TargetSurface::Absent, |row| target(row)),
            fallback: inventory.map_or(TargetSurface::Absent, |row| fallback(row)),
        });
    }
    // Codes no flags row names (Oblivion's `HUDReticle`, `Generic`, `BirthSign`): the menu exists
    // and answers its code; its flags are the default.
    for (code, name) in &codes {
        if menus.iter().any(|def: &MenuDef| def.code == Some(*code)) {
            continue;
        }
        let inventory = inventory_of(Some(*code), name);
        menus.push(MenuDef {
            name: name.clone(),
            code: Some(*code),
            class: class_of(Some(*code)),
            flags: default_flags(game, false),
            basis: FlagsBasis::EngineDefault,
            document: inventory.and_then(|row| document(row)),
            target: inventory.map_or(TargetSurface::Absent, |row| target(row)),
            fallback: inventory.map_or(TargetSurface::Absent, |row| fallback(row)),
        });
    }
    // The console has no menu code or document on these games: `MenuMode 3` asks for it by type.
    menus.push(MenuDef {
        name: "Console".to_owned(),
        code: None,
        class: MenuClass::Console,
        flags: default_flags(game, false),
        basis: FlagsBasis::EngineDefault,
        document: None,
        target: TargetSurface::Absent,
        fallback: TargetSurface::Absent,
    });
    MenuTable { menus }
}
