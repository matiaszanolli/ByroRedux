//! M48 menu registry (`docs/prd/game-ui-integration.md` GUI-REG-001…011, design in
//! `docs/engine/ui.md` § *Menu registry, stack and path access*).
//!
//! One list of every open menu by its game-facing name, bottom to top, with its surface and flags.
//! Every "is a menu open" question reads it: ObScript `MenuMode` and `Begin MenuMode`, Papyrus
//! `Utility.IsInMenuMode`, the extender `UI.IsMenuOpen`, the engine's pause and its input owner.
//! The surfaces (a Scaleform movie, a MenuXml document, a native page) own their presentation and
//! report what they have up; the app republishes the registry from them every frame
//! ([`MenuRegistry::publish`]), so it is presentation state and never saved
//! (`NOT_SAVED_BY_DESIGN`, GUI-EVT-005).
//!
//! The flags and names come from the Phase 0 tables in `docs/data/game-ui/` ([`tables`]).

mod tables;
#[cfg(test)]
mod tests;

use std::fmt;

use crate::ecs::resource::Resource;
pub use tables::{MenuDef, MenuTable};

/// The game whose menu names, codes and flags the registry uses. Fallout 3 and New Vegas share
/// one table: one `GameKind`, one code table, and New Vegas's menus are a superset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuGame {
    Oblivion,
    Fallout3NV,
    Skyrim,
    Fallout4,
}

impl MenuGame {
    pub fn label(self) -> &'static str {
        match self {
            Self::Oblivion => "oblivion",
            Self::Fallout3NV => "fallout3nv",
            Self::Skyrim => "skyrim",
            Self::Fallout4 => "fallout4",
        }
    }
}

/// What an open menu does to the game, in the engine's terms (GUI-REG-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MenuFlags(u8);

impl MenuFlags {
    pub const NONE: Self = Self(0);
    /// The simulation stops while it is open (GUI-REG-010).
    pub const PAUSES_GAME: Self = Self(1 << 0);
    /// It puts the game in menu mode: `MenuMode`, `Begin MenuMode` (GUI-EVT-006).
    pub const MENU_MODE: Self = Self(1 << 1);
    /// It takes the player's input while it is the topmost such menu (GUI-REG-009).
    pub const CLAIMS_INPUT: Self = Self(1 << 2);
    /// The HUD: stays open under every modal (GUI-REG-003).
    pub const HUD: Self = Self(1 << 3);
    /// The game may be saved while it is open.
    pub const ALLOWS_SAVING: Self = Self(1 << 4);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl std::ops::BitOr for MenuFlags {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl fmt::Display for MenuFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (Self::PAUSES_GAME, "pause"),
            (Self::MENU_MODE, "menu-mode"),
            (Self::CLAIMS_INPUT, "input"),
            (Self::HUD, "hud"),
            (Self::ALLOWS_SAVING, "save"),
        ];
        let set: Vec<&str> = names.iter().filter(|(flag, _)| self.contains(*flag)).map(|(_, name)| *name).collect();
        if set.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&set.join("|"))
        }
    }
}

/// Where a menu's flags came from (GUI-REG-004: every entry cited or recorded as a default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagsBasis {
    /// Read off the source the table row cites.
    Sourced,
    /// No source documents them; the engine default named in ui.md applies.
    EngineDefault,
}

/// The `MenuMode` type a menu answers to (CS / GECK *MenuTypes*): 1 the "main four", 2 any other
/// menu, 3 the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuClass {
    BigFour,
    Other,
    Console,
}

/// The surface Phase 0's inventory targets for a menu (PRD §3.0, decision Q1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetSurface {
    /// S1, the original movie.
    Movie,
    /// S2, the original MenuXml document.
    MenuXml,
    /// S3, the named native page.
    Native(String),
    /// S4, no surface.
    Absent,
}

/// What a script that opens an absent menu gets (PRD §3.0 S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsentFallback {
    Placeholder,
    Recorded,
    Declined,
}

/// The surface an open menu has right now (PRD §3.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuSurface {
    /// S1: a Scaleform movie under a host profile.
    Movie { path: String, profile: String },
    /// S2: a MenuXml document.
    MenuXml { path: String },
    /// S3: an engine-native page.
    Native { page: String },
    /// S4: none.
    Absent { fallback: AbsentFallback },
}

impl fmt::Display for MenuSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Movie { path, profile } => write!(f, "movie {path} profile={profile}"),
            Self::MenuXml { path } => write!(f, "menuxml {path}"),
            Self::Native { page } => write!(f, "native {page}"),
            Self::Absent { fallback } => {
                let fallback = match fallback {
                    AbsentFallback::Placeholder => "placeholder",
                    AbsentFallback::Recorded => "recorded",
                    AbsentFallback::Declined => "declined",
                };
                write!(f, "absent fallback={fallback}")
            }
        }
    }
}

/// The menus the engine's native pages and engine layers stand in for (GUI-PAGE-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMenu {
    Pause,
    Settings,
    Inventory,
    Dialogue,
    MessageBox,
    Barter,
    Crafting,
    /// The character-creation page; on games with menu codes the screen's own code names it.
    CharGen,
    /// The operator overlay, standing in for the console (`MenuMode 3`).
    Console,
    /// The loading cover.
    Loading,
    /// The HUD layer.
    Hud,
}

impl NativeMenu {
    /// The game-facing name of the menu this stands in for. Oblivion: CS *MenuTypes* names with
    /// xOBSE's spelling (1013 `PauseMenu`, 1015 `OptionsMenu`, barter is the container menu 1008,
    /// "Container, Barter"). Fallout 3 / New Vegas: xNVSE `s_menuNameToID` (1013 is `StartMenu`,
    /// 1053 `BarterMenu`, New Vegas's workbench `RecipeMenu` 1077). Skyrim: CK *UI Script* (the
    /// pause menu is the journal's system page). Fallout 4: FO4 CK *Menu* (the inventory is the
    /// Pip-Boy, character creation is `LooksMenu`). A page no game menu matches (crafting outside
    /// Skyrim and New Vegas) keeps the engine name `Crafting Menu`, with the default flags.
    pub fn menu_name(self, game: MenuGame) -> &'static str {
        use MenuGame::*;
        use NativeMenu::*;
        match (game, self) {
            (Oblivion, Pause) => "PauseMenu",
            (Oblivion, Settings) => "OptionsMenu",
            (Oblivion, Barter) => "ContainerMenu",
            (Oblivion, CharGen) => "RaceSexMenu",
            (Oblivion | Fallout3NV, Inventory) => "InventoryMenu",
            (Oblivion | Fallout3NV, Dialogue) => "DialogMenu",
            (Oblivion | Fallout3NV, MessageBox) => "MessageMenu",
            (Oblivion | Fallout3NV, Loading) => "LoadingMenu",
            (Oblivion | Fallout3NV, Hud) => "HUDMainMenu",
            (Fallout3NV, Pause | Settings) => "StartMenu",
            (Fallout3NV, Barter) => "BarterMenu",
            (Fallout3NV, Crafting) => "RecipeMenu",
            (Fallout3NV, CharGen) => "CharGenMenu",
            (Skyrim, Pause | Settings) => "Journal Menu",
            (Skyrim, Inventory) => "InventoryMenu",
            (Skyrim, Dialogue) => "Dialogue Menu",
            (Skyrim, Crafting) => "Crafting Menu",
            (Skyrim, CharGen) => "RaceSex Menu",
            (Skyrim, Loading) => "Loading Menu",
            (Skyrim, Hud) => "HUD Menu",
            (Fallout4, Pause | Settings) => "PauseMenu",
            (Fallout4, Inventory) => "PipboyMenu",
            (Fallout4, Dialogue) => "DialogueMenu",
            (Fallout4, CharGen) => "LooksMenu",
            (Fallout4, Loading) => "LoadingMenu",
            (Fallout4, Hud) => "HUDMenu",
            (Skyrim | Fallout4, MessageBox) => "MessageBoxMenu",
            (Skyrim | Fallout4, Barter) => "BarterMenu",
            (_, Console) => "Console",
            (Oblivion | Fallout4, Crafting) => "Crafting Menu",
        }
    }

    /// The native page name `ui.menus` reports (`surface=native <page>`).
    pub fn page(self) -> &'static str {
        match self {
            Self::Pause => "Pause",
            Self::Settings => "Settings",
            Self::Inventory => "Inventory",
            Self::Dialogue => "Dialogue",
            Self::MessageBox => "MessageBox",
            Self::Barter => "Barter",
            Self::Crafting => "Crafting",
            Self::CharGen => "CharGen",
            Self::Console => "OperatorOverlay",
            Self::Loading => "LoadingCover",
            Self::Hud => "Hud",
        }
    }

    /// The native page a Phase 0 `S3:<page>` target names.
    pub fn from_page(page: &str) -> Option<Self> {
        [
            Self::Pause,
            Self::Settings,
            Self::Inventory,
            Self::Dialogue,
            Self::MessageBox,
            Self::Barter,
            Self::Crafting,
            Self::CharGen,
        ]
        .into_iter()
        .find(|native| native.page().eq_ignore_ascii_case(page))
    }
}

/// One open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenMenu {
    pub name: String,
    pub code: Option<u32>,
    pub class: MenuClass,
    pub surface: MenuSurface,
    pub flags: MenuFlags,
    pub basis: FlagsBasis,
    /// The registry frame it was first published on.
    pub opened_frame: u64,
}

impl OpenMenu {
    /// Whether it answers `MenuMode <code>`: a type (0 any, 1 the main four, 2 other, 3 the
    /// console) or a specific menu's code.
    fn answers(&self, code: u32) -> bool {
        match code {
            0 => true,
            1 => self.class == MenuClass::BigFour,
            2 => self.class == MenuClass::Other,
            3 => self.class == MenuClass::Console,
            code => self.code == Some(code),
        }
    }
}

/// The open menus, bottom to top. See the module docs.
#[derive(Debug)]
pub struct MenuRegistry {
    game: MenuGame,
    stack: Vec<OpenMenu>,
    frame: u64,
    movie_devices: u32,
}

impl Default for MenuRegistry {
    /// The Fallout 3 / New Vegas table, as the menu codes defaulted before a load order named its game.
    fn default() -> Self {
        Self::new(MenuGame::Fallout3NV)
    }
}

impl Resource for MenuRegistry {}

impl MenuRegistry {
    pub fn new(game: MenuGame) -> Self {
        Self { game, stack: Vec::new(), frame: 0, movie_devices: 0 }
    }

    pub fn game(&self) -> MenuGame {
        self.game
    }

    /// Select the load order's game. The open menus keep their entries; their names are the old
    /// game's, so the next publish replaces them.
    pub fn set_game(&mut self, game: MenuGame) {
        self.game = game;
    }

    pub fn table(&self) -> &'static MenuTable {
        tables::table(self.game)
    }

    /// The definition of the menu `name` on this game: the table's, or the engine default for a
    /// name no table lists (a mod's menu, an engine-coined page name).
    pub fn def(&self, name: &str) -> MenuDef {
        self.table().by_name(name).cloned().unwrap_or_else(|| MenuDef {
            name: name.to_owned(),
            code: None,
            class: if name.eq_ignore_ascii_case("Console") { MenuClass::Console } else { MenuClass::Other },
            flags: tables::default_flags(self.game, false),
            basis: FlagsBasis::EngineDefault,
            document: None,
            target: TargetSurface::Absent,
            fallback: TargetSurface::Absent,
        })
    }

    /// The game-facing name of `native` on this game.
    pub fn native_name(&self, native: NativeMenu) -> &'static str {
        native.menu_name(self.game)
    }

    /// The name of the menu with `code` (`1036` -> `RaceSexMenu`) on a game with codes.
    pub fn name_for_code(&self, code: u32) -> Option<&'static str> {
        self.table().by_code(code).map(|def| def.name.as_str())
    }

    /// Open `name` with `surface`, on top; an open one keeps its place and takes the new surface.
    /// A HUD goes to the bottom: it stays under every modal (GUI-REG-003).
    pub fn open(&mut self, name: &str, surface: MenuSurface) {
        if let Some(open) = self.stack.iter_mut().find(|open| open.name.eq_ignore_ascii_case(name)) {
            open.surface = surface;
            return;
        }
        let def = self.def(name);
        // The operator overlay stands in for the console only so that `MenuMode 3` and
        // `UI.IsMenuOpen("Console")` see it. It is an engine tool, not the game's console: it
        // neither pauses the world it inspects (#5141) nor puts the game in menu mode, whatever
        // the console's own row says; it takes input.
        let overlay = matches!(&surface, MenuSurface::Native { page } if page == NativeMenu::Console.page());
        let (flags, basis) = if overlay { (MenuFlags::CLAIMS_INPUT, FlagsBasis::EngineDefault) } else { (def.flags, def.basis) };
        let entry = OpenMenu {
            name: def.name,
            code: def.code,
            class: def.class,
            surface,
            flags,
            basis,
            opened_frame: self.frame,
        };
        if entry.flags.contains(MenuFlags::HUD) {
            self.stack.insert(0, entry);
        } else {
            self.stack.push(entry);
        }
    }

    /// Close `name`; what it was, or `None` when it was not open.
    pub fn close(&mut self, name: &str) -> Option<OpenMenu> {
        let at = self.stack.iter().position(|open| open.name.eq_ignore_ascii_case(name))?;
        Some(self.stack.remove(at))
    }

    /// Republish the open menus from what the surfaces report this frame: a menu still up keeps
    /// its place in the stack, one that went away closes, a new one opens on top in the order
    /// given. One call per frame.
    pub fn publish(&mut self, open: impl IntoIterator<Item = (String, MenuSurface)>) {
        let open: Vec<(String, MenuSurface)> = open.into_iter().collect();
        self.stack.retain(|entry| open.iter().any(|(name, _)| name.eq_ignore_ascii_case(&entry.name)));
        for (name, surface) in open {
            self.open(&name, surface);
        }
        self.frame += 1;
    }

    /// The open menus, bottom to top.
    pub fn entries(&self) -> &[OpenMenu] {
        &self.stack
    }

    /// `UI.IsMenuOpen(name)`: the name is on the stack.
    pub fn is_open(&self, name: &str) -> bool {
        self.stack.iter().any(|open| open.name.eq_ignore_ascii_case(name))
    }

    /// Whether the game is in menu mode: an open menu counts toward it. The HUD alone does not.
    pub fn menu_mode(&self) -> bool {
        self.stack.iter().any(|open| open.flags.contains(MenuFlags::MENU_MODE))
    }

    /// ObScript `MenuMode <code>` (GUI-REG-002): the types 0–2 count only menu-mode menus; type 3
    /// asks for the console, and a specific code for that menu, whatever its flags.
    pub fn menu_mode_query(&self, code: u32) -> bool {
        self.stack.iter().any(|open| {
            let counts = matches!(code, 0..=2) && !open.flags.contains(MenuFlags::MENU_MODE);
            !counts && open.answers(code)
        })
    }

    /// Whether a `Begin MenuMode [code]` block runs this frame (GUI-EVT-006): a menu-mode menu is
    /// open that matches the block's code (any one when the block names none).
    pub fn menu_mode_block_runs(&self, filter: Option<u32>) -> bool {
        self.stack
            .iter()
            .any(|open| open.flags.contains(MenuFlags::MENU_MODE) && open.answers(filter.unwrap_or(0)))
    }

    /// Whether the simulation is paused: an open menu pauses the game (GUI-REG-010).
    pub fn paused(&self) -> bool {
        self.stack.iter().any(|open| open.flags.contains(MenuFlags::PAUSES_GAME))
    }

    /// Papyrus `Utility.IsInMenuMode` (GUI-REG-011): a menu that pauses the game is open, so an
    /// open dialogue menu answers `false` (CK / FO4 CK *IsInMenuMode*).
    pub fn papyrus_in_menu_mode(&self) -> bool {
        self.paused()
    }

    /// The menu that owns input: the topmost one whose flags claim it (GUI-REG-009).
    pub fn input_owner(&self) -> Option<&OpenMenu> {
        self.stack.iter().rev().find(|open| open.flags.contains(MenuFlags::CLAIMS_INPUT))
    }

    /// The wgpu devices the open movies render through (GUI-REG-005), as the UI layer reports it.
    pub fn movie_devices(&self) -> u32 {
        self.movie_devices
    }

    pub fn set_movie_devices(&mut self, devices: u32) {
        self.movie_devices = devices;
    }

    /// The `ui.menus` report (GUI-OPS-001): a summary line, then one line per open menu, bottom to top.
    pub fn report(&self) -> Vec<String> {
        let owner = self.input_owner().map_or("world", |open| open.name.as_str());
        let yes = |on: bool| if on { "yes" } else { "no" };
        let mut lines = vec![format!(
            "ui.menus: game={} open={} input_owner={owner} paused={} menu_mode={} devices={}",
            self.game.label(),
            self.stack.len(),
            yes(self.paused()),
            yes(self.menu_mode()),
            self.movie_devices,
        )];
        for (index, open) in self.stack.iter().enumerate() {
            let code = open.code.map_or_else(|| "-".to_owned(), |code| code.to_string());
            let basis = match open.basis {
                FlagsBasis::Sourced => "sourced",
                FlagsBasis::EngineDefault => "default",
            };
            lines.push(format!(
                "  [{index}] {} code={code} surface={} flags={} basis={basis}",
                open.name, open.surface, open.flags
            ));
        }
        lines
    }
}
