//! LMC-UI-002 — the engine's Mod Configuration Menu host (`docs/engine/mcm-host.md`).
//!
//! A SkyUI config script `extends SKI_ConfigBase`. The engine supplies that script itself
//! ([`engine_script`], Papyrus written from the SkyUI wiki's MCM pages, shadowing SkyUI's own), and its
//! option calls reach [`McmHost`] through the `McmHost` static class the VM serves (`vm/natives/mcm.rs`).
//! The panel's actions — open a mod, pick a page, select / slide / pick / remap / type into an option —
//! run the config script's `_McmEvent`, which dispatches them as SkyUI does: a state option's event in
//! its state, then back. A page is never edited by the panel: every mod or page change clears it and
//! the script lays it out again in `OnPageReset`.
//!
//! What is saved is the config script's own variables (its settings, and the base script's
//! initialised flag and `CurrentVersion`), which the save carries already. [`McmHost`] is the session's
//! view: the registered mods (re-registered by `OnGameReload` after every load) and the open page.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::World;
use byroredux_papyrus::ast::Script;

use crate::quest_stages::QuestFormId;
use crate::translate::effects::VmStored;

/// The script a config script extends, lowercase.
pub const CONFIG_BASE: &str = "ski_configbase";

/// The cells of a page: SkyUI's 2-column by 64-row grid.
pub const CELLS: usize = 128;

/// The engine's own scripts, by lowercase name: they shadow a mod's script of the same name.
const ENGINE_SOURCES: [(&str, &str); 4] = [
    (CONFIG_BASE, include_str!("mcm/SKI_ConfigBase.psc")),
    ("ski_playerloadgamealias", include_str!("mcm/SKI_PlayerLoadGameAlias.psc")),
    // MCM Helper (slice 4, `helper`): its config base and its global API.
    (helper::HELPER_BASE, include_str!("mcm/MCM_ConfigBase.psc")),
    ("mcm", include_str!("mcm/MCM.psc")),
];

/// The engine's script named `name` (any case), parsed once.
pub fn engine_script(name: &str) -> Option<Arc<Script>> {
    static PARSED: OnceLock<Vec<(&'static str, Arc<Script>)>> = OnceLock::new();
    let parsed = PARSED.get_or_init(|| {
        ENGINE_SOURCES
            .iter()
            .filter_map(|(key, source)| match byroredux_papyrus::parse_script(source) {
                Ok((script, errors)) if errors.is_empty() => Some((*key, Arc::new(script))),
                Ok((_, errors)) | Err(errors) => {
                    log::error!("the engine's {key} does not parse: {:?}", errors.first());
                    None
                }
            })
            .collect()
    });
    let key = name.to_ascii_lowercase();
    parsed.iter().find(|(engine, _)| *engine == key).map(|(_, script)| Arc::clone(script))
}

/// What an option is (SkyUI's option types; `Input` is MCM 4's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McmKind {
    Empty,
    Header,
    Text,
    Toggle,
    Slider,
    Menu,
    Color,
    KeyMap,
    Input,
}

impl McmKind {
    /// The kind the engine `SKI_ConfigBase` passes as a number.
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Self::Empty,
            1 => Self::Header,
            2 => Self::Text,
            3 => Self::Toggle,
            4 => Self::Slider,
            5 => Self::Menu,
            6 => Self::Color,
            7 => Self::KeyMap,
            8 => Self::Input,
            _ => return None,
        })
    }
}

/// MCM Helper's config menus (slice 4).
pub mod helper;

/// Option flags, SkyUI's values.
pub mod flags {
    pub const DISABLED: u32 = 1;
    pub const HIDDEN: u32 = 2;
    pub const WITH_UNMAP: u32 = 4;
}

/// Cursor fill modes, SkyUI's values: `LEFT_TO_RIGHT` fills a row (n, n+1), `TOP_TO_BOTTOM` a column
/// (n, n+2).
pub const LEFT_TO_RIGHT: i32 = 1;
pub const TOP_TO_BOTTOM: i32 = 2;

/// One option on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct McmOption {
    pub kind: McmKind,
    pub label: String,
    /// The value text of a text, menu or input option; a slider's format string.
    pub text: String,
    /// A slider's value, a toggle's 0 / 1, a colour (0xRRGGBB), a key code.
    pub number: f32,
    pub flags: u32,
    /// The state its events run in; empty for an option by id.
    pub state: String,
}

impl McmOption {
    /// What the panel shows as its value: a slider's number through its format string (`{N}` is the
    /// number with N decimals), a toggle's mark, a colour, a key code, or the text.
    pub fn value_text(&self) -> String {
        match self.kind {
            McmKind::Slider => format_slider(&self.text, self.number),
            McmKind::Toggle => if self.number != 0.0 { "ON" } else { "OFF" }.to_owned(),
            McmKind::Color => format!("#{:06X}", self.number as u32 & 0xFF_FFFF),
            McmKind::KeyMap => format!("{}", self.number as i32),
            McmKind::Empty | McmKind::Header => String::new(),
            McmKind::Text | McmKind::Menu | McmKind::Input => self.text.clone(),
        }
    }
}

/// A slider value through SkyUI's format string: `{N}` is the value with N decimals.
pub fn format_slider(format: &str, value: f32) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        let digits = &rest[open + 1..open + close];
        match digits.parse::<usize>() {
            Ok(decimals) if decimals <= 6 => {
                out.push_str(&rest[..open]);
                out.push_str(&format!("{value:.decimals$}"));
            }
            _ => out.push_str(&rest[..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A dialog an option opened, with what the script's open event set.
#[derive(Debug, Clone, PartialEq)]
pub enum McmDialog {
    Slider { start: f32, default: f32, min: f32, max: f32, interval: f32 },
    Menu { options: Vec<String>, start: i32, default: i32 },
    Color { start: u32, default: u32 },
    Input { start_text: String },
    KeyMap,
}

/// A registered config menu.
#[derive(Debug, Clone, PartialEq)]
pub struct McmMod {
    pub quest: QuestFormId,
    /// The config script (the most derived), as the VM's store keys it.
    pub script: String,
    pub name: String,
    pub pages: Vec<String>,
    /// A config no script registers (Fallout 4 MCM, §5.2), by its mod name: the panel's actions reach
    /// [`helper`] directly instead of a config script's `_McmEvent`.
    pub config: Option<String>,
}

/// The page on screen.
#[derive(Debug, Clone)]
pub struct McmPage {
    pub mod_index: usize,
    /// The page's index in the mod's `Pages`; `None` for the front page.
    pub page: Option<usize>,
    pub cells: Vec<Option<McmOption>>,
    pub title: String,
    pub info: String,
    /// The option whose dialog is open, and the dialog.
    pub dialog: Option<(i32, McmDialog)>,
    /// `LoadCustomContent`: an `Interface\` path and its offset; the list is hidden while it is set.
    pub custom: Option<(String, f32, f32)>,
    cursor: Option<usize>,
    fill: i32,
    reset_requested: bool,
}

impl McmPage {
    /// An empty page for `mod_index`'s `page` (`None`: the front page): 128 unfilled cells, no dialog.
    pub fn new(mod_index: usize, page: Option<usize>) -> Self {
        Self {
            mod_index,
            page,
            cells: vec![None; CELLS],
            title: String::new(),
            info: String::new(),
            dialog: None,
            custom: None,
            cursor: Some(0),
            fill: LEFT_TO_RIGHT,
            reset_requested: false,
        }
    }

    /// SkyUI's page number in an option id: the front page 0, page `i` `i + 1`.
    fn number(&self) -> i32 {
        self.page.map_or(0, |page| page as i32 + 1)
    }

    /// The id of the option in cell `index`: SkyUI's `index + page number * 0x100`.
    pub fn option_id(&self, index: usize) -> i32 {
        index as i32 + self.number() * 0x100
    }

    /// The cell an option id names on this page; a stale page's id names none.
    pub fn cell_of(&self, id: i32) -> Option<usize> {
        let index = id - self.number() * 0x100;
        (0..CELLS as i32).contains(&index).then_some(index as usize).filter(|&i| self.cells[i].is_some())
    }

    /// The option an id names on this page.
    pub fn option(&self, id: i32) -> Option<&McmOption> {
        self.cell_of(id).and_then(|index| self.cells[index].as_ref())
    }
}

/// The session's Mod Configuration Menu: the registered config menus and the page on screen. Not
/// saved: every config script registers again in `OnGameReload` after a load.
#[derive(Debug, Default)]
pub struct McmHost {
    pub mods: Vec<McmMod>,
    pub open: Option<McmPage>,
    /// Bumped on every page change and option write, for a presentation's redraw.
    pub generation: u32,
    /// Config quests whose `OnGameReload` this session raised (or that registered).
    reloaded: HashSet<QuestFormId>,
    /// Whether a page is being laid out (`OnPageReset` running): only then do `Add…Option` add.
    building: bool,
}

impl Resource for McmHost {}

impl McmHost {
    /// `SKI_ConfigBase.OnGameReload`'s registration: the config menu of `quest`, by `script`.
    pub fn register(&mut self, quest: QuestFormId, script: &str, name: String, pages: Vec<String>) {
        self.reloaded.insert(quest);
        self.generation += 1;
        match self.mods.iter_mut().find(|entry| entry.quest == quest) {
            Some(entry) => {
                entry.name = name;
                entry.pages = pages;
            }
            None => self.mods.push(McmMod { quest, script: script.to_owned(), name, pages, config: None }),
        }
    }

    /// A config no script registers (Fallout 4 MCM): its menu, by the config's mod name.
    pub fn register_config(&mut self, mod_name: &str, name: String, pages: Vec<String>) {
        self.generation += 1;
        match self.mods.iter_mut().find(|entry| entry.config.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(mod_name))) {
            Some(entry) => {
                entry.name = name;
                entry.pages = pages;
            }
            None => self.mods.push(McmMod {
                quest: QuestFormId(0),
                script: String::new(),
                name,
                pages,
                config: Some(mod_name.to_owned()),
            }),
        }
    }

    /// `Add…Option`: the next cell, as the cursor and fill mode give it; `-1` outside a page reset or
    /// past the last cell (SkyUI's).
    pub fn add_option(&mut self, option: McmOption) -> i32 {
        if !self.building {
            return -1;
        }
        let Some(page) = self.open.as_mut() else { return -1 };
        let Some(cell) = page.cursor else { return -1 };
        page.cells[cell] = Some(option);
        let step = if page.fill == TOP_TO_BOTTOM { 2 } else { 1 };
        page.cursor = Some(cell + step).filter(|&next| next < CELLS);
        self.generation += 1;
        page.option_id(cell)
    }

    pub fn set_cursor(&mut self, position: i32) {
        if let Some(page) = self.open.as_mut().filter(|_| (0..CELLS as i32).contains(&position)) {
            page.cursor = Some(position as usize);
        }
    }

    pub fn set_fill_mode(&mut self, mode: i32) {
        if let Some(page) = self.open.as_mut().filter(|_| matches!(mode, LEFT_TO_RIGHT | TOP_TO_BOTTOM)) {
            page.fill = mode;
        }
    }

    fn option_mut(&mut self, id: i32) -> Option<&mut McmOption> {
        let page = self.open.as_mut()?;
        let cell = page.cell_of(id)?;
        self.generation += 1;
        page.cells[cell].as_mut()
    }

    pub fn set_option_text(&mut self, id: i32, text: String) {
        if let Some(option) = self.option_mut(id) {
            option.text = text;
        }
    }

    /// A number value; a non-empty `format` replaces a slider's format string.
    pub fn set_option_number(&mut self, id: i32, number: f32, format: String) {
        if let Some(option) = self.option_mut(id) {
            option.number = number;
            if !format.is_empty() && option.kind == McmKind::Slider {
                option.text = format;
            }
        }
    }

    pub fn set_option_flags(&mut self, id: i32, flags: i32) {
        if let Some(option) = self.option_mut(id) {
            option.flags = flags as u32;
        }
    }

    /// The id of the option laid out in `state`; `-1` when none is.
    pub fn state_option(&self, state: &str) -> i32 {
        self.open
            .as_ref()
            .and_then(|page| {
                page.cells.iter().position(|cell| {
                    cell.as_ref().is_some_and(|option| !state.is_empty() && option.state.eq_ignore_ascii_case(state))
                }).map(|index| page.option_id(index))
            })
            .unwrap_or(-1)
    }

    /// The state option `id`'s events run in; empty for an option by id.
    pub fn option_state(&self, id: i32) -> String {
        self.open.as_ref().and_then(|page| page.option(id)).map(|option| option.state.clone()).unwrap_or_default()
    }

    /// A dialog field (0 start, 1 default, 2 minimum, 3 maximum, 4 interval) of the dialog being opened.
    pub fn set_dialog_number(&mut self, field: i32, value: f32) {
        let Some((_, dialog)) = self.open.as_mut().and_then(|page| page.dialog.as_mut()) else { return };
        match dialog {
            McmDialog::Slider { start, default, min, max, interval } => match field {
                0 => *start = value,
                1 => *default = value,
                2 => *min = value,
                3 => *max = value,
                4 => *interval = value,
                _ => {}
            },
            McmDialog::Menu { start, default, .. } => match field {
                0 => *start = value as i32,
                1 => *default = value as i32,
                _ => {}
            },
            McmDialog::Color { start, default } => match field {
                0 => *start = value as u32,
                1 => *default = value as u32,
                _ => {}
            },
            McmDialog::Input { .. } | McmDialog::KeyMap => {}
        }
    }

    pub fn set_menu_dialog_options(&mut self, options: Vec<String>) {
        if let Some((_, McmDialog::Menu { options: menu, .. })) = self.open.as_mut().and_then(|page| page.dialog.as_mut()) {
            *menu = options;
        }
    }

    pub fn set_input_dialog_text(&mut self, text: String) {
        if let Some((_, McmDialog::Input { start_text })) = self.open.as_mut().and_then(|page| page.dialog.as_mut()) {
            *start_text = text;
        }
    }

    pub fn set_info_text(&mut self, text: String) {
        if let Some(page) = self.open.as_mut() {
            page.info = text;
            self.generation += 1;
        }
    }

    pub fn set_title_text(&mut self, text: String) {
        if let Some(page) = self.open.as_mut() {
            page.title = text;
            self.generation += 1;
        }
    }

    pub fn force_page_reset(&mut self) {
        if let Some(page) = self.open.as_mut() {
            page.reset_requested = true;
        }
    }

    pub fn load_custom_content(&mut self, source: String, x: f32, y: f32) {
        if let Some(page) = self.open.as_mut() {
            page.custom = Some((source, x, y));
            self.generation += 1;
        }
    }

    pub fn unload_custom_content(&mut self) {
        if let Some(page) = self.open.as_mut() {
            page.custom = None;
            self.generation += 1;
        }
    }

    /// A load replaced the session: every config script registers again.
    pub fn reset_session(&mut self) {
        *self = Self::default();
    }
}

/// The panel's actions, as the engine `SKI_ConfigBase._McmEvent` numbers them.
mod action {
    pub const PAGE_RESET: i32 = 0;
    pub const HIGHLIGHT: i32 = 1;
    pub const DEFAULT: i32 = 2;
    pub const SELECT: i32 = 3;
    pub const SLIDER_OPEN: i32 = 4;
    pub const SLIDER_ACCEPT: i32 = 5;
    pub const MENU_OPEN: i32 = 6;
    pub const MENU_ACCEPT: i32 = 7;
    pub const COLOR_OPEN: i32 = 8;
    pub const COLOR_ACCEPT: i32 = 9;
    pub const KEYMAP_CHANGE: i32 = 10;
    pub const INPUT_OPEN: i32 = 11;
    pub const INPUT_ACCEPT: i32 = 12;
    pub const CONFIG_OPEN: i32 = 13;
    pub const CONFIG_CLOSE: i32 = 14;
}

/// Run `_McmEvent(action, option, number, text)` on the open mod's config script, or, for a config no
/// script registers, answer the action from the config itself ([`raise_config`]).
fn raise(world: &World, action: i32, option: i32, number: f32, text: &str) -> Result<(), String> {
    let (quest, script, config) = {
        let host = world.try_resource::<McmHost>().ok_or("no MCM host")?;
        let page = host.open.as_ref().ok_or("no config menu is open")?;
        let entry = &host.mods[page.mod_index];
        (entry.quest, entry.script.clone(), entry.config.clone())
    };
    let result = match config {
        Some(mod_name) => raise_config(world, &mod_name, action, option, number, text),
        None => {
            let args = vec![VmStored::Int(action), VmStored::Int(option), VmStored::Float(number), VmStored::Str(text.to_owned())];
            crate::fragment::run_quest_own_script(world, quest, &script, "_McmEvent", args)
        }
    };
    // An MCM Helper control's action runs after the event that changed it, as MCM Helper dispatches it.
    helper::run_pending_actions(world);
    result
}

/// A panel action on a Fallout 4 MCM config (§5.2): the config answers it, and a change, an opening or a
/// closing is sent to the scripts registered for FO4 MCM's external events.
fn raise_config(world: &World, mod_name: &str, action: i32, option: i32, number: f32, text: &str) -> Result<(), String> {
    let owner = helper::McmOwner::Config(mod_name.to_owned());
    let changed = match action {
        action::PAGE_RESET => {
            helper::page_reset(world, &owner, text)?;
            String::new()
        }
        action::HIGHLIGHT => {
            helper::highlight(world, &owner, option);
            String::new()
        }
        action::DEFAULT => helper::reset(world, &owner, option),
        action::SELECT => helper::select(world, &owner, option),
        action::SLIDER_OPEN | action::MENU_OPEN | action::COLOR_OPEN | action::INPUT_OPEN => {
            helper::open_dialog(world, &owner, option);
            String::new()
        }
        action::SLIDER_ACCEPT | action::COLOR_ACCEPT | action::KEYMAP_CHANGE => {
            helper::accept(world, &owner, option, helper::Accepted::Number(number))
        }
        action::MENU_ACCEPT => helper::accept(world, &owner, option, helper::Accepted::Index(number as i32)),
        action::INPUT_ACCEPT => helper::accept(world, &owner, option, helper::Accepted::Text(text.to_owned())),
        action::CONFIG_OPEN => {
            crate::fragment::send_external_event(world, "OnMCMMenuOpen", mod_name, &[VmStored::Str(mod_name.to_owned())]);
            // The wiki's example registers a bare `OnMCMOpen`, with no arguments.
            crate::fragment::send_external_event(world, "OnMCMOpen", mod_name, &[]);
            String::new()
        }
        action::CONFIG_CLOSE => {
            crate::fragment::send_external_event(world, "OnMCMMenuClose", mod_name, &[VmStored::Str(mod_name.to_owned())]);
            String::new()
        }
        other => return Err(format!("unknown panel action {other}")),
    };
    if !changed.is_empty() {
        crate::fragment::send_external_event(world, "OnMCMSettingChange", mod_name, &[VmStored::Str(mod_name.to_owned()), VmStored::Str(changed)]);
    }
    Ok(())
}

/// Lay the open page out again: cleared, then the script's `OnPageReset`, again while it asks
/// (`ForcePageReset`, a few times at most).
fn reset_page(world: &World) -> Result<(), String> {
    for _ in 0..4 {
        let page_name = {
            let mut host = world.try_resource_mut::<McmHost>().ok_or("no MCM host")?;
            let host = &mut *host;
            let page = host.open.as_mut().ok_or("no config menu is open")?;
            let entry = &host.mods[page.mod_index];
            let name = page.page.and_then(|index| entry.pages.get(index).cloned()).unwrap_or_default();
            let title = if name.is_empty() { entry.name.clone() } else { name.clone() };
            *page = McmPage { title, ..McmPage::new(page.mod_index, page.page) };
            host.building = true;
            host.generation += 1;
            name
        };
        let result = raise(world, action::PAGE_RESET, 0, 0.0, &page_name);
        let again = {
            let mut host = world.resource_mut::<McmHost>();
            host.building = false;
            host.open.as_mut().is_some_and(|page| std::mem::take(&mut page.reset_requested))
        };
        result?;
        if !again {
            break;
        }
    }
    Ok(())
}

/// After an event: the page again if the script asked (`ForcePageReset`).
fn settle(world: &World) -> Result<(), String> {
    let again = world
        .try_resource_mut::<McmHost>()
        .and_then(|mut host| host.open.as_mut().map(|page| std::mem::take(&mut page.reset_requested)))
        .unwrap_or(false);
    if again { reset_page(world) } else { Ok(()) }
}

/// Open config menu `mod_index` on its front page: `OnConfigOpen`, then the page.
pub fn open(world: &World, mod_index: usize) -> Result<(), String> {
    {
        let mut host = world.try_resource_mut::<McmHost>().ok_or("no MCM host")?;
        if mod_index >= host.mods.len() {
            return Err(format!("no config menu {mod_index}"));
        }
        host.open = Some(McmPage::new(mod_index, None));
    }
    // The config script refreshes its name and pages as it opens (they are its properties).
    raise(world, action::CONFIG_OPEN, 0, 0.0, "")?;
    reset_page(world)
}

/// Show page `page` of the open config menu (`None`: its front page).
pub fn select_page(world: &World, page: Option<usize>) -> Result<(), String> {
    {
        let mut host = world.try_resource_mut::<McmHost>().ok_or("no MCM host")?;
        let host = &mut *host;
        let open = host.open.as_mut().ok_or("no config menu is open")?;
        if let Some(index) = page
            && index >= host.mods[open.mod_index].pages.len() {
                return Err(format!("no page {index}"));
            }
        open.page = page;
    }
    reset_page(world)
}

/// The panel's use of option `id`: a text or toggle option is selected; a slider, menu, colour or input
/// option opens its dialog (the script's open event fills it); a key-map option opens the key prompt.
/// A disabled or hidden option, a header and an empty cell do nothing.
pub fn activate(world: &World, id: i32) -> Result<(), String> {
    let kind = {
        let mut host = world.try_resource_mut::<McmHost>().ok_or("no MCM host")?;
        let page = host.open.as_mut().ok_or("no config menu is open")?;
        let option = page.option(id).ok_or_else(|| format!("no option {id} on this page"))?;
        if option.flags & (flags::DISABLED | flags::HIDDEN) != 0 {
            return Ok(());
        }
        let kind = option.kind;
        let dialog = match kind {
            McmKind::Slider => Some(McmDialog::Slider { start: option.number, default: option.number, min: 0.0, max: 100.0, interval: 1.0 }),
            McmKind::Menu => Some(McmDialog::Menu { options: Vec::new(), start: 0, default: 0 }),
            McmKind::Color => Some(McmDialog::Color { start: option.number as u32, default: option.number as u32 }),
            McmKind::Input => Some(McmDialog::Input { start_text: option.text.clone() }),
            McmKind::KeyMap => Some(McmDialog::KeyMap),
            _ => None,
        };
        page.dialog = dialog.map(|dialog| (id, dialog));
        kind
    };
    let action = match kind {
        McmKind::Text | McmKind::Toggle => action::SELECT,
        McmKind::Slider => action::SLIDER_OPEN,
        McmKind::Menu => action::MENU_OPEN,
        McmKind::Color => action::COLOR_OPEN,
        McmKind::Input => action::INPUT_OPEN,
        McmKind::KeyMap | McmKind::Header | McmKind::Empty => return Ok(()),
    };
    raise(world, action, id, 0.0, "")?;
    settle(world)
}

/// A value the open dialog accepted.
#[derive(Debug, Clone, PartialEq)]
pub enum McmAnswer {
    Slider(f32),
    Menu(i32),
    Color(u32),
    Input(String),
    /// A key code (DX scan code, SKSE's), and the control it conflicts with ("" when none).
    KeyMap(i32, String),
}

/// Accept the open dialog of option `id` with `answer`; the dialog closes.
pub fn accept(world: &World, id: i32, answer: McmAnswer) -> Result<(), String> {
    {
        let mut host = world.try_resource_mut::<McmHost>().ok_or("no MCM host")?;
        let page = host.open.as_mut().ok_or("no config menu is open")?;
        match page.dialog.take() {
            Some((open, _)) if open == id => {}
            _ => return Err(format!("option {id} has no open dialog")),
        }
    }
    let (action, number, text) = match answer {
        McmAnswer::Slider(value) => (action::SLIDER_ACCEPT, value, String::new()),
        McmAnswer::Menu(index) => (action::MENU_ACCEPT, index as f32, String::new()),
        McmAnswer::Color(color) => (action::COLOR_ACCEPT, (color & 0xFF_FFFF) as f32, String::new()),
        McmAnswer::Input(text) => (action::INPUT_ACCEPT, 0.0, text),
        McmAnswer::KeyMap(code, conflict) => (action::KEYMAP_CHANGE, code as f32, conflict),
    };
    raise(world, action, id, number, &text)?;
    settle(world)
}

/// Close the open dialog unanswered.
pub fn cancel_dialog(world: &World) {
    if let Some(mut host) = world.try_resource_mut::<McmHost>()
        && let Some(page) = host.open.as_mut() {
            page.dialog = None;
        }
}

/// The panel's "default" key on option `id` (`OnOptionDefault`).
pub fn reset_option(world: &World, id: i32) -> Result<(), String> {
    raise(world, action::DEFAULT, id, 0.0, "")?;
    settle(world)
}

/// The cursor rested on option `id` (`OnOptionHighlight`); the info text is the script's to set.
pub fn highlight(world: &World, id: i32) -> Result<(), String> {
    if let Some(mut host) = world.try_resource_mut::<McmHost>()
        && let Some(page) = host.open.as_mut() {
            page.info.clear();
        }
    raise(world, action::HIGHLIGHT, id, 0.0, "")?;
    settle(world)
}

/// Close the open config menu (`OnConfigClose`).
pub fn close(world: &World) -> Result<(), String> {
    let result = raise(world, action::CONFIG_CLOSE, 0, 0.0, "");
    if let Some(mut host) = world.try_resource_mut::<McmHost>() {
        host.open = None;
        host.generation += 1;
    }
    result
}

/// Raise `OnGameReload` on every config script whose `OnInit` already ran in this save and that has not
/// registered this session: after a load (and at boot for a save's quests). A script still waiting for
/// its `OnInit` registers from it. Also lays the open page out again when a handler that waited on a
/// `ShowMessage` box asked for it (`ForcePageReset`) after the panel's action returned.
pub fn mcm_registration_system(world: &World, _dt: f32) {
    // Fallout 4 MCM's configs register themselves: no script does it for them (§5.2).
    helper::register_config_menus(world);
    if let Err(error) = settle(world)
        && world.try_resource::<McmHost>().is_some_and(|host| host.open.is_some()) {
            log::warn!("MCM: page reset: {error}");
        }
    let pending: Vec<(QuestFormId, String)> = {
        let (Some(host), Some(scripts)) = (world.try_resource::<McmHost>(), world.try_resource::<crate::QuestOwnScripts>()) else {
            return;
        };
        let variables = world.try_resource::<crate::QuestScriptVariables>();
        scripts
            .running_scripts()
            .flat_map(|(quest, scripts)| scripts.iter().map(move |script| (quest, script)))
            .filter(|(quest, script)| {
                script.extends(CONFIG_BASE)
                    && !host.reloaded.contains(quest)
                    && variables.as_ref().is_some_and(|v| v.get(*quest, crate::fragment::initialized_key(&script.script)).is_some())
            })
            .map(|(quest, script)| (quest, script.script.clone()))
            .collect()
    };
    for (quest, script) in pending {
        if let Some(mut host) = world.try_resource_mut::<McmHost>() {
            host.reloaded.insert(quest);
        }
        if let Err(error) = crate::fragment::run_quest_own_script(world, quest, &script, "OnGameReload", Vec::new()) {
            log::warn!("MCM: {script}.OnGameReload on {:08X}: {error}", quest.0);
        }
    }
}
