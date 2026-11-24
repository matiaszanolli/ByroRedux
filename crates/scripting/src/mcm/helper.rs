//! MCM Helper (LMC-UI-002 slice 4, `docs/engine/mcm-host.md` §5.1): config menus described by a
//! mod's `MCM/Config/<mod>/config.json` instead of by script.
//!
//! A config script `extends MCM_ConfigBase`, which the engine supplies (`mcm/MCM_ConfigBase.psc`,
//! shadowing MCM Helper's). Its overrides of SkyUI's page and option events call the static class
//! `McmHelper` (`fragment/vm/natives/mcm_helper.rs`), which lands here: [`page_reset`] lays the config's
//! page out into [`McmHost`] through the same page buffer a SkyUI script writes, and each option event
//! reads or writes the control's value where the config says it lives (a mod setting, a script property,
//! a global) and hands back the id the script's `OnSettingChange` receives. A control's `action` is
//! queued and run after the event ([`run_pending_actions`]), as MCM Helper dispatches it on the VM.
//!
//! What a control shows and does follows MCM Helper's `Config/Control.cpp` and
//! `Papyrus/MCM_ConfigBase.cpp` (v1.6.2).

pub mod config;
pub mod keybinds;
pub mod settings;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

use self::config::{ActionTarget, Dialect, Control, ControlKind, DefaultValue, FormSpec, GroupBehavior, HelperConfig, NumberKind, PageContent, Param, Source};
use self::keybinds::{HotkeyAction, Keybind, Registrations};
use self::settings::{HelperFiles, SettingStore};
use super::{flags, McmHost, McmKind, McmOption};
use crate::quest_stages::QuestFormId;
use crate::translate::effects::VmStored;

/// MCM Helper v1.6.2's `MCM.GetVersionCode()`, the release the engine follows (CMakeLists `PLUGIN_VERSION`).
pub const VERSION_CODE: u32 = 15;

/// FO4 MCM 1.40's `MCM.GetVersionCode()` (`changelog.txt` of reg2k/f4mcm).
pub const FALLOUT4_VERSION_CODE: u32 = 9;

/// The script a config script extends for MCM Helper, lowercase.
pub const HELPER_BASE: &str = "mcm_configbase";

/// The text option MCM Helper shows for a config it cannot load (`ErrorControl`).
const ERROR_LABEL: &str = "Error loading config";
const ERROR_VALUE: &str = "Click to view";

/// Whose config menu a page belongs to: a config script's quest (MCM Helper, §5.1), or a config that no
/// script registers (Fallout 4 MCM, §5.2), by its mod name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McmOwner {
    Script(QuestFormId),
    Config(String),
}

/// One control the open page laid out, with what it needs to read and write its value.
#[derive(Debug, Clone)]
struct PlacedControl {
    control: Control,
}

/// The open MCM Helper page (`ConfigPageCache`): its controls by option id, its group switches, and the
/// menu options a script replaced (`SetMenuOptions`).
#[derive(Debug, Clone)]
struct HelperPage {
    owner: McmOwner,
    quest: Option<QuestFormId>,
    mod_name: String,
    script: String,
    controls: HashMap<i32, PlacedControl>,
    /// The page's `groupControl` toggles (drawn or hidden), by group.
    groups: Vec<(u32, Control)>,
    menu_overrides: HashMap<String, (Vec<String>, Vec<String>)>,
    /// The option the cursor rests on, for the info text.
    highlighted: Option<i32>,
    /// The load error the page shows, if any.
    error: Option<String>,
}

/// A control's action waiting to run once the event that raised it returns.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingAction {
    /// The quest whose script is called (`CallFunction`), or the config quest the global call runs under.
    pub quest: QuestFormId,
    pub script: String,
    pub function: String,
    pub args: Vec<VmStored>,
    /// `CallGlobalFunction`: no `Self`.
    pub global: bool,
}

/// A mod's config quest and config script, which its hotkeys' actions default to (no quest for a config no
/// script registers).
type ConfigQuest = (Option<QuestFormId>, String);

/// The MCM Helper state: the files, each mod's parsed config and settings, the open page.
#[derive(Default)]
pub struct McmHelperStore {
    files: Option<Arc<dyn HelperFiles>>,
    configs: HashMap<String, Arc<Result<HelperConfig, String>>>,
    pub settings: SettingStore,
    /// Each plugin's file stem by its identity: a config quest's mod is its plugin's stem.
    plugin_stems: HashMap<u128, String>,
    /// The session's plugin file names, lowercase (`pluginRequirements`).
    plugins: Vec<String>,
    page: Option<HelperPage>,
    pending: Vec<PendingAction>,
    /// The hotkeys of each mod whose config loaded, by mod (lowercase); MCM Helper reads a mod's
    /// `keybinds.json` when its config registers.
    keybinds: HashMap<String, Vec<Keybind>>,
    /// Each such mod's config quest and config script, which a hotkey's action defaults to (none for a
    /// config no script registers).
    config_quests: HashMap<String, ConfigQuest>,
    /// The player's key bindings, read on first use.
    registrations: Option<Registrations>,
    /// Hotkeys whose console command was declined, so the diagnostic is given once.
    console_declined: HashSet<String>,
    /// Which front end's configs the game's are (MCM Helper on Skyrim, FO4 MCM on Fallout 4).
    dialect: Dialect,
    /// What `MCM.GetVersionCode()` answers: the release of the front end the engine follows.
    version_code: u32,
    /// Fallout 4 MCM's configs found under the mod roots, by mod name: they register themselves.
    config_mods: Vec<String>,
}

impl Resource for McmHelperStore {}

impl McmHelperStore {
    /// A store reading through `files`, for a session whose plugins (file names, load order) are
    /// `plugins`.
    pub fn new(files: Arc<dyn HelperFiles>, plugins: &[String]) -> Self {
        let plugin_stems = plugins
            .iter()
            .map(|name| (byroredux_core::form_id::PluginId::from_filename(name).0, stem(name).to_owned()))
            .collect();
        Self {
            files: Some(files),
            plugin_stems,
            plugins: plugins.iter().map(|name| name.to_ascii_lowercase()).collect(),
            version_code: VERSION_CODE,
            ..Default::default()
        }
    }

    /// The same store serving Fallout 4 MCM (§5.2): its config dialect and version code, and the configs
    /// found under the mod roots, which register themselves.
    pub fn for_fallout4(mut self, config_mods: Vec<String>) -> Self {
        self.dialect = Dialect::Fallout4;
        self.version_code = FALLOUT4_VERSION_CODE;
        self.config_mods = config_mods;
        self
    }

    /// `MCM.GetVersionCode()`.
    pub fn version_code(&self) -> u32 {
        if self.version_code == 0 { VERSION_CODE } else { self.version_code }
    }

    fn files(&self) -> Result<Arc<dyn HelperFiles>, String> {
        self.files.clone().ok_or_else(|| "MCM Helper has no files to read".to_owned())
    }

    /// `mod_name`'s config, parsed once.
    fn config(&mut self, mod_name: &str) -> Arc<Result<HelperConfig, String>> {
        let key = mod_name.to_ascii_lowercase();
        if let Some(config) = self.configs.get(&key) {
            return Arc::clone(config);
        }
        let parsed = match self.files() {
            Ok(files) => match files.config(mod_name) {
                Some(bytes) => config::parse_in(&bytes, self.dialect),
                None => Err(format!("no MCM/Config/{mod_name}/config.json")),
            },
            Err(error) => Err(error),
        };
        if let Err(error) = &parsed {
            log::warn!("MCM Helper: the config of {mod_name} does not load: {error}");
        }
        let parsed = Arc::new(parsed);
        self.configs.insert(key, Arc::clone(&parsed));
        parsed
    }

    /// Why `config` cannot be shown in this session, if it cannot: a later MCM Helper than the engine
    /// follows, or a required plugin missing.
    fn refusal(&self, config: &HelperConfig) -> Option<String> {
        if config.min_mcm_version > VERSION_CODE {
            return Some(format!("the config needs MCM Helper version code {}, the engine serves {VERSION_CODE}", config.min_mcm_version));
        }
        config
            .plugin_requirements
            .iter()
            .find(|plugin| !self.plugins.contains(&plugin.to_ascii_lowercase()))
            .map(|plugin| format!("the config needs {plugin}, which is not loaded"))
    }

    /// Typed mod-setting reads and writes, by mod and `key:Section`.
    pub fn setting_int(&mut self, mod_name: &str, name: &str) -> i32 {
        match self.files() {
            Ok(files) => self.settings.int(files.as_ref(), mod_name, name),
            Err(_) => -1,
        }
    }
    pub fn setting_float(&mut self, mod_name: &str, name: &str) -> f32 {
        match self.files() {
            Ok(files) => self.settings.float(files.as_ref(), mod_name, name),
            Err(_) => -1.0,
        }
    }
    pub fn setting_bool(&mut self, mod_name: &str, name: &str) -> bool {
        self.files().is_ok_and(|files| self.settings.bool(files.as_ref(), mod_name, name))
    }
    pub fn setting_text(&mut self, mod_name: &str, name: &str) -> String {
        match self.files() {
            Ok(files) => self.settings.text(files.as_ref(), mod_name, name).unwrap_or_default(),
            Err(_) => String::new(),
        }
    }
    pub fn set_setting_number(&mut self, mod_name: &str, name: &str, value: f32) -> bool {
        self.files().is_ok_and(|files| self.settings.set_number(files.as_ref(), mod_name, name, value))
    }
    pub fn set_setting_text(&mut self, mod_name: &str, name: &str, value: String) -> bool {
        self.files().is_ok_and(|files| self.settings.set_text(files.as_ref(), mod_name, name, value))
    }

    /// The mod a config quest belongs to: its plugin's file stem.
    pub fn mod_of(&self, world: &World, quest: QuestFormId) -> Option<String> {
        let pair = world.try_resource::<crate::load_order::LoadOrderIdentity>()?.pair(quest.0)?;
        self.plugin_stems.get(&pair.plugin.0).cloned()
    }

    /// The display name and pages the config gives a config script, for its registration: `None` when
    /// the quest's mod has no config that loads (MCM Helper then names the menu after the mod).
    pub fn registration(&mut self, world: &World, quest: QuestFormId) -> Option<(String, Vec<String>)> {
        let mod_name = self.mod_of(world, quest)?;
        let config = self.config(&mod_name);
        match &*config {
            Ok(config) => Some((config.display_name.clone(), config.pages.iter().map(|(name, _)| name.clone()).collect())),
            Err(_) => Some((mod_name, Vec::new())),
        }
    }

    /// Take the actions the last events queued.
    pub fn take_pending(&mut self) -> Vec<PendingAction> {
        std::mem::take(&mut self.pending)
    }

    /// `SetMenuOptions(id, options, shortNames)` for the open page of `quest`.
    pub fn set_menu_options(&mut self, quest: QuestFormId, id: &str, options: Vec<String>, short_names: Vec<String>) {
        if let Some(page) = self.page.as_mut().filter(|page| page.owner == McmOwner::Script(quest)) {
            page.menu_overrides.insert(id.to_ascii_lowercase(), (options, short_names));
        }
    }
}

impl McmHelperStore {
    /// The player's key bindings, read the first time they are needed.
    fn registrations(&mut self) -> &mut Registrations {
        if self.registrations.is_none() {
            let parse = |bytes: Option<Vec<u8>>| bytes.as_deref().map(keybinds::parse_registrations).unwrap_or_default();
            // An earlier FO4 MCM install's `Keybinds.json` names keys in an undocumented code space, so it
            // is not read (§5.2).
            let read_legacy = self.dialect != Dialect::Fallout4;
            let (legacy, profile) = match self.files() {
                Ok(files) => (
                    if read_legacy { parse(files.legacy_registrations()) } else { Vec::new() },
                    parse(files.profile_registrations()),
                ),
                Err(_) => (Vec::new(), Vec::new()),
            };
            self.registrations = Some(Registrations::layered(legacy, profile));
        }
        self.registrations.get_or_insert_with(Registrations::default)
    }

    /// The key bound to `mod_name`'s hotkey `id`; `-1` when none is.
    pub fn bound_key(&mut self, mod_name: &str, id: &str) -> i32 {
        self.registrations().key(mod_name, id)
    }

    /// Bind (or, with an unmapped key, unbind) `mod_name`'s hotkey `id` and commit the profile's bindings.
    pub fn bind_key(&mut self, mod_name: &str, id: &str, keycode: i32) {
        if !self.registrations().bind(mod_name, id, keycode) {
            return;
        }
        let text = keybinds::registrations_text(self.registrations().all());
        if let Err(error) = self.files().and_then(|files| files.write_profile_registrations(&text)) {
            log::warn!("MCM Helper: the key bindings could not be written: {error}");
        }
    }

    /// The description of `mod_name`'s hotkey `id`, if its `keybinds.json` declares it.
    fn keybind_desc(&self, mod_name: &str, id: &str) -> Option<String> {
        self.keybinds.get(&mod_name.to_ascii_lowercase())?.iter().find(|k| k.id == id).map(|k| k.desc.clone())
    }
}

fn stem(file: &str) -> &str {
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

/// Who is asking: the config quest, its mod and its config script.
#[derive(Debug, Clone)]
struct Context {
    owner: McmOwner,
    /// The config quest a property or method defaults to; none for a config no script registers.
    quest: Option<QuestFormId>,
    mod_name: String,
    script: String,
}

fn context(world: &World, owner: &McmOwner) -> Result<Context, String> {
    let quest = match owner {
        McmOwner::Script(quest) => *quest,
        McmOwner::Config(mod_name) => {
            return Ok(Context { owner: owner.clone(), quest: None, mod_name: mod_name.clone(), script: String::new() });
        }
    };
    let mod_name = world
        .try_resource::<McmHelperStore>()
        .ok_or("no MCM Helper store")?
        .mod_of(world, quest)
        .ok_or_else(|| format!("quest {:08X} belongs to no loaded plugin", quest.0))?;
    let script = world
        .try_resource::<crate::QuestOwnScripts>()
        .and_then(|scripts| {
            scripts
                .running_scripts()
                .find(|(owner, _)| *owner == quest)
                .and_then(|(_, scripts)| scripts.iter().find(|script| script.extends(HELPER_BASE)).map(|script| script.script.clone()))
        })
        .unwrap_or_default();
    Ok(Context { owner: owner.clone(), quest: Some(quest), mod_name, script })
}

/// A config script registered: its mod's hotkeys become live (`KeybindManager::ReadKeybinds`, which MCM
/// Helper runs for each config it reads), with the config quest their actions default to.
pub fn note_config_quest(world: &World, quest: QuestFormId) {
    let Ok(ctx) = context(world, &McmOwner::Script(quest)) else { return };
    let key = ctx.mod_name.to_ascii_lowercase();
    let mut store = world.resource_mut::<McmHelperStore>();
    if store.config(&ctx.mod_name).is_err() {
        return;
    }
    store.config_quests.insert(key, (Some(quest), ctx.script.clone()));
    store.load_keybinds(&ctx.mod_name);
}

impl McmHelperStore {
    /// Read `mod_name`'s `keybinds.json` once.
    fn load_keybinds(&mut self, mod_name: &str) {
        let key = mod_name.to_ascii_lowercase();
        if self.keybinds.contains_key(&key) {
            return;
        }
        let parsed = match self.files().map(|files| files.keybinds(mod_name)) {
            Ok(Some(bytes)) => keybinds::parse_keybinds(&bytes).unwrap_or_else(|error| {
                log::warn!("MCM: the hotkeys of {mod_name} do not load: {error}");
                Vec::new()
            }),
            _ => Vec::new(),
        };
        self.keybinds.insert(key, parsed);
    }
}

/// Register Fallout 4 MCM's configs (§5.2) that the host does not list yet: each whose
/// `pluginRequirements` are loaded and, when it names none, whose own plugin is. Its hotkeys become live
/// with it. Runs every frame, so a load (which clears the host) registers them again.
pub fn register_config_menus(world: &World) {
    let pending: Vec<String> = {
        let (Some(store), Some(host)) = (world.try_resource::<McmHelperStore>(), world.try_resource::<McmHost>()) else { return };
        if store.config_mods.is_empty() {
            return;
        }
        store
            .config_mods
            .iter()
            .filter(|name| !host.mods.iter().any(|entry| entry.config.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name))))
            .cloned()
            .collect()
    };
    for mod_name in pending {
        let registration = {
            let mut store = world.resource_mut::<McmHelperStore>();
            let config = store.config(&mod_name);
            let Ok(config) = &*config else { continue };
            if let Some(refusal) = store.refusal(config) {
                log::info!("FO4 MCM: {mod_name} is not listed: {refusal}");
                store.config_mods.retain(|name| name != &mod_name);
                continue;
            }
            let own = ["esp", "esm", "esl"].iter().any(|ext| store.plugins.contains(&format!("{mod_name}.{ext}").to_ascii_lowercase()));
            if config.plugin_requirements.is_empty() && !own {
                log::info!("FO4 MCM: {mod_name} is not listed: no plugin of that name is loaded");
                store.config_mods.retain(|name| name != &mod_name);
                continue;
            }
            let pages = config.pages.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
            let name = if config.display_name.is_empty() { mod_name.clone() } else { config.display_name.clone() };
            // The config's own spelling of its mod (FO4: `modName` "must match your directory name").
            let canonical = if config.mod_name.eq_ignore_ascii_case(&mod_name) { config.mod_name.clone() } else { mod_name.clone() };
            store.config_quests.insert(mod_name.to_ascii_lowercase(), (None, String::new()));
            store.load_keybinds(&mod_name);
            (name, pages, canonical)
        };
        world.resource_mut::<McmHost>().register_config(&registration.2, registration.0, registration.1);
    }
}

/// `MCM.RefreshMenu()` (FO4's global): the open page's values again, whichever menu it is.
pub fn refresh_open(world: &World) {
    let owner = world.try_resource::<McmHelperStore>().and_then(|store| store.page.as_ref().map(|page| page.owner.clone()));
    if let Some(owner) = owner {
        refresh(world, &owner);
    }
}

/// A form a config names (`"Plugin.esp|D62"`), as a load-order FormID.
fn resolve_form(world: &World, form: &FormSpec) -> Option<u32> {
    let plugin = byroredux_core::form_id::PluginId::from_filename(&form.plugin);
    world.try_resource::<crate::load_order::LoadOrderIdentity>()?.form_from_file(form.local, plugin)
}

/// The quest a property source or a method action targets: the config quest by default. A form that is
/// not a quest with scripts is refused: the VM keeps script variables for quests only.
fn target_quest(world: &World, ctx: &Context, form: Option<&FormSpec>) -> Result<QuestFormId, String> {
    let Some(form) = form else {
        return ctx.quest.ok_or_else(|| "no form, and the config has no quest to default to".to_owned());
    };
    let id = resolve_form(world, form).ok_or_else(|| format!("{}|{:X} is not in the load order", form.plugin, form.local))?;
    let is_quest = world.try_resource::<crate::QuestScriptInstances>().is_some_and(|instances| instances.0.contains_key(&QuestFormId(id)));
    if is_quest {
        Ok(QuestFormId(id))
    } else {
        Err(format!("{}|{:X} is not a quest; only a quest's script properties and functions are served", form.plugin, form.local))
    }
}

// ---- values ----

/// A control's value as a number (toggle 0 / 1, slider, stepper and enum index, colour, key code).
fn read_number(world: &World, ctx: &Context, control: &Control) -> f32 {
    match &control.values.source {
        Some(Source::ModSetting { number }) => {
            let mut store = world.resource_mut::<McmHelperStore>();
            match number {
                Some(NumberKind::Bool) => f32::from(u8::from(store.setting_bool(&ctx.mod_name, &control.id))),
                Some(NumberKind::Float) => store.setting_float(&ctx.mod_name, &control.id),
                _ => store.setting_int(&ctx.mod_name, &control.id) as f32,
            }
        }
        Some(Source::Property { number: Some(kind), form, property, .. }) => match target_quest(world, ctx, form.as_ref()) {
            Ok(quest) => {
                let value = crate::quest_variable(world, quest, crate::QuestScriptVariables::property_key(property)).unwrap_or(0.0);
                match kind {
                    NumberKind::Bool => f32::from(u8::from(value != 0.0)),
                    NumberKind::Int => value.trunc(),
                    NumberKind::Float => value,
                }
            }
            Err(error) => {
                log::debug!("MCM Helper: {}: {error}", control.id);
                0.0
            }
        },
        Some(Source::Global { form }) => resolve_form(world, form)
            .and_then(|id| world.try_resource::<crate::Globals>().and_then(|globals| globals.get(id)))
            .unwrap_or(0.0),
        // A key map with no source shows the key bound to the hotkey its id names (slice 4b).
        None if control.kind == ControlKind::KeyMap && !control.id.is_empty() => {
            world.resource_mut::<McmHelperStore>().bound_key(&ctx.mod_name, &control.id) as f32
        }
        _ => 0.0,
    }
}

fn write_number(world: &World, ctx: &Context, control: &Control, value: f32) -> Result<(), String> {
    match &control.values.source {
        Some(Source::ModSetting { .. }) => {
            world.resource_mut::<McmHelperStore>().set_setting_number(&ctx.mod_name, &control.id, value);
            Ok(())
        }
        Some(Source::Property { number: Some(kind), form, property, .. }) => {
            let quest = target_quest(world, ctx, form.as_ref())?;
            let value = match kind {
                NumberKind::Bool => f32::from(u8::from(value != 0.0)),
                NumberKind::Int => value.trunc(),
                NumberKind::Float => value,
            };
            world.resource_mut::<crate::QuestScriptVariables>().set(quest, crate::QuestScriptVariables::property_key(property), value);
            Ok(())
        }
        Some(Source::Global { form }) => {
            let id = resolve_form(world, form).ok_or_else(|| format!("{}|{:X} is not in the load order", form.plugin, form.local))?;
            world.resource_mut::<crate::Globals>().set(id, value);
            Ok(())
        }
        _ => Err(format!("control '{}' has no value source", control.id)),
    }
}

/// The value a numeric control's reset restores (`GetDefaultValue`): the mod setting's `settings.ini`
/// default, else the `defaultValue`, else the value it has.
fn default_number(world: &World, ctx: &Context, control: &Control) -> f32 {
    let declared = match &control.values.default {
        Some(DefaultValue::Number(n)) => Some(*n),
        _ => None,
    };
    if let Some(Source::ModSetting { .. }) = &control.values.source {
        let default = {
            let mut store = world.resource_mut::<McmHelperStore>();
            store.files().ok().and_then(|files| store.settings.default_text(files.as_ref(), &ctx.mod_name, &control.id))
        };
        if let Some(default) = default {
            return default.trim().parse::<f32>().unwrap_or(0.0);
        }
    }
    declared.unwrap_or_else(|| read_number(world, ctx, control))
}

/// A string control's value: its source's, else its fixed `value`.
fn read_text(world: &World, ctx: &Context, control: &Control) -> String {
    match &control.values.source {
        Some(Source::ModSetting { .. }) => world.resource_mut::<McmHelperStore>().setting_text(&ctx.mod_name, &control.id),
        Some(Source::Property { number: None, form, property, .. }) => match target_quest(world, ctx, form.as_ref()) {
            Ok(quest) => match world
                .try_resource::<crate::QuestScriptObjects>()
                .and_then(|objects| objects.get(quest, crate::QuestScriptVariables::property_key(property)))
            {
                Some(crate::ScriptObject::Str(text)) => text,
                _ => String::new(),
            },
            Err(_) => String::new(),
        },
        _ => control.values.value.clone(),
    }
}

fn write_text(world: &World, ctx: &Context, control: &Control, value: String) -> Result<(), String> {
    match &control.values.source {
        Some(Source::ModSetting { .. }) => {
            world.resource_mut::<McmHelperStore>().set_setting_text(&ctx.mod_name, &control.id, value);
            Ok(())
        }
        Some(Source::Property { number: None, form, property, .. }) => {
            let quest = target_quest(world, ctx, form.as_ref())?;
            world.resource_mut::<crate::QuestScriptObjects>().set(
                quest,
                crate::QuestScriptVariables::property_key(property),
                crate::ScriptObject::Str(value),
            );
            Ok(())
        }
        _ => Err(format!("control '{}' has no value source", control.id)),
    }
}

fn default_text(world: &World, ctx: &Context, control: &Control) -> String {
    if let Some(Source::ModSetting { .. }) = &control.values.source {
        let default = {
            let mut store = world.resource_mut::<McmHelperStore>();
            store.files().ok().and_then(|files| store.settings.default_text(files.as_ref(), &ctx.mod_name, &control.id))
        };
        if let Some(default) = default {
            return default;
        }
    }
    match &control.values.default {
        Some(DefaultValue::Text(text)) => text.clone(),
        _ => read_text(world, ctx, control),
    }
}

/// The menu options of a menu or enum control: the script's `SetMenuOptions` override, else the config's.
fn menu_options(world: &World, control: &Control) -> (Vec<String>, Vec<String>) {
    let overridden = world.try_resource::<McmHelperStore>().and_then(|store| {
        store.page.as_ref().and_then(|page| page.menu_overrides.get(&control.id.to_ascii_lowercase()).cloned())
    });
    overridden.unwrap_or_else(|| (control.values.options.clone(), control.values.short_names.clone()))
}

/// What a menu or enum control shows: its value's short name when the short names match the options,
/// else the value (`GetShortText`).
fn menu_text(world: &World, ctx: &Context, control: &Control) -> String {
    let (options, short_names) = menu_options(world, control);
    let index = if control.kind == ControlKind::Enum {
        let index = read_number(world, ctx, control) as i32;
        if index < 0 || index as usize >= options.len() {
            return String::new();
        }
        index as usize
    } else {
        let value = read_text(world, ctx, control);
        match options.iter().position(|option| *option == value) {
            Some(index) => index,
            None => return if options.len() == short_names.len() { String::new() } else { value },
        }
    };
    if options.len() == short_names.len() { short_names[index].clone() } else { options[index].clone() }
}

/// What a control shows, and the number SkyUI's option holds.
fn shown(world: &World, ctx: &Context, control: &Control) -> (McmKind, String, f32) {
    match control.kind {
        ControlKind::Empty => (McmKind::Empty, String::new(), 0.0),
        ControlKind::Header => (McmKind::Header, String::new(), 0.0),
        ControlKind::Text | ControlKind::Input => {
            let kind = if control.kind == ControlKind::Text { McmKind::Text } else { McmKind::Input };
            (kind, read_text(world, ctx, control), 0.0)
        }
        ControlKind::Toggle | ControlKind::HiddenToggle => {
            (McmKind::Toggle, String::new(), f32::from(u8::from(read_number(world, ctx, control) != 0.0)))
        }
        ControlKind::Slider => (McmKind::Slider, control.values.format.clone(), read_number(world, ctx, control)),
        ControlKind::Stepper => {
            let index = read_number(world, ctx, control) as i32;
            let text = usize::try_from(index).ok().and_then(|i| control.values.options.get(i)).cloned().unwrap_or_default();
            (McmKind::Text, text, 0.0)
        }
        ControlKind::Menu | ControlKind::Enum => (McmKind::Menu, menu_text(world, ctx, control), 0.0),
        ControlKind::Color => (McmKind::Color, String::new(), read_number(world, ctx, control)),
        ControlKind::KeyMap => (McmKind::KeyMap, String::new(), read_number(world, ctx, control)),
        // FO4's button: a text option with no value; its select runs its action.
        ControlKind::Button => (McmKind::Text, String::new(), 0.0),
    }
}

/// The value as MCM Helper's `{value}` token and action parameter see it (`GetValueString`).
fn value_param(world: &World, ctx: &Context, control: &Control) -> VmStored {
    match control.kind {
        ControlKind::Toggle | ControlKind::HiddenToggle => VmStored::Bool(read_number(world, ctx, control) != 0.0),
        ControlKind::Slider => VmStored::Float(read_number(world, ctx, control)),
        ControlKind::Stepper | ControlKind::Enum | ControlKind::Color | ControlKind::KeyMap => {
            VmStored::Int(read_number(world, ctx, control) as i32)
        }
        ControlKind::Text | ControlKind::Menu | ControlKind::Input => VmStored::Str(read_text(world, ctx, control)),
        ControlKind::Empty | ControlKind::Header | ControlKind::Button => VmStored::None,
    }
}

fn value_string(world: &World, ctx: &Context, control: &Control) -> String {
    match value_param(world, ctx, control) {
        VmStored::Bool(b) => u8::from(b).to_string(),
        VmStored::Float(f) => {
            // The slider's value to the step's precision (`SliderControl::GetValueString`).
            let step = format!("{}", control.values.step);
            let decimals = step.split_once('.').map_or(0, |(_, fraction)| fraction.len());
            format!("{f:.decimals$}")
        }
        VmStored::Int(i) => i.to_string(),
        VmStored::Str(text) => text,
        _ => String::new(),
    }
}

/// The info text for a control: its `help`, with `{value}` replaced (`Control::GetInfoText`).
fn info_text(world: &World, ctx: &Context, control: &Control) -> String {
    // A hotkey's key map without help shows the hotkey's description (`KeyMapControl::GetInfoText`).
    if control.help.is_empty() && control.kind == ControlKind::KeyMap && control.values.source.is_none() && !control.id.is_empty() {
        return world.resource::<McmHelperStore>().keybind_desc(&ctx.mod_name, &control.id).unwrap_or_default();
    }
    if !control.help.contains("{value}") {
        return control.help.clone();
    }
    control.help.replace("{value}", &value_string(world, ctx, control))
}

// ---- the page ----

/// Whether each of the page's groups is on, and every group it defines.
fn group_state(world: &World, ctx: &Context, groups: &[(u32, Control)]) -> (HashMap<u32, bool>, Vec<u32>) {
    let state = groups.iter().map(|(group, toggle)| (*group, read_number(world, ctx, toggle) != 0.0)).collect();
    (state, groups.iter().map(|(group, _)| *group).collect())
}

/// What the control does under its group condition: drawn normally, or with these flags, or skipped.
fn behaviour(control: &Control, active: &HashMap<u32, bool>, all: &[u32]) -> Option<u32> {
    let holds = control
        .group_condition
        .as_ref()
        .is_none_or(|condition| condition.holds(&|group| active.get(&group).copied().unwrap_or(false), all));
    if holds {
        return Some(0);
    }
    match control.group_behavior {
        GroupBehavior::Disable => Some(flags::DISABLED),
        GroupBehavior::Hide => Some(flags::DISABLED | flags::HIDDEN),
        GroupBehavior::Skip => None,
    }
}

/// `OnPageReset(page)` of an MCM Helper config script: the config's page laid out into the host.
pub fn page_reset(world: &World, owner: &McmOwner, page_name: &str) -> Result<(), String> {
    let ctx = context(world, owner)?;
    let config = world.resource_mut::<McmHelperStore>().config(&ctx.mod_name);
    let mut page = HelperPage {
        owner: owner.clone(),
        quest: ctx.quest,
        mod_name: ctx.mod_name.clone(),
        script: ctx.script.clone(),
        controls: HashMap::new(),
        groups: Vec::new(),
        menu_overrides: HashMap::new(),
        highlighted: None,
        error: None,
    };
    let content = match &*config {
        Err(error) => Err(error.clone()),
        Ok(config) => match world.resource::<McmHelperStore>().refusal(config) {
            Some(refusal) => Err(refusal),
            None => {
                world.resource_mut::<McmHost>().set_fill_mode(config.fill_mode);
                if page_name.is_empty() {
                    Ok(config.main.clone())
                } else {
                    Ok(config.pages.iter().find(|(name, _)| name == page_name).map(|(_, content)| content.clone()))
                }
            }
        },
    };
    match content {
        Err(error) => {
            let id = world.resource_mut::<McmHost>().add_option(McmOption {
                kind: McmKind::Text,
                label: ERROR_LABEL.to_owned(),
                text: ERROR_VALUE.to_owned(),
                number: 0.0,
                flags: 0,
                state: String::new(),
            });
            page.error = Some(error);
            page.controls.insert(id, PlacedControl { control: error_control() });
        }
        Ok(None) => {}
        Ok(Some(PageContent::Custom { source, x, y })) => world.resource_mut::<McmHost>().load_custom_content(source, x, y),
        Ok(Some(PageContent::Layout(controls))) => {
            page.groups = controls.iter().filter(|c| c.group_control > 0).map(|c| (c.group_control, c.clone())).collect();
            let (active, all) = group_state(world, &ctx, &page.groups);
            for control in controls {
                if control.kind == ControlKind::HiddenToggle {
                    continue;
                }
                let Some(group_flags) = behaviour(&control, &active, &all) else { continue };
                let (kind, text, number) = shown(world, &ctx, &control);
                // A key map offers its unmap button unless its group disables it (`KeyMapControl::GetFlags`).
                let option_flags = if control.kind == ControlKind::KeyMap && group_flags == 0 { flags::WITH_UNMAP } else { group_flags };
                let mut host = world.resource_mut::<McmHost>();
                if let Some(position) = control.position {
                    host.set_cursor(position);
                }
                let label = control.text.clone();
                let id = host.add_option(McmOption { kind, label, text, number, flags: option_flags, state: String::new() });
                drop(host);
                if id >= 0 {
                    page.controls.insert(id, PlacedControl { control });
                }
            }
        }
    }
    world.resource_mut::<McmHelperStore>().page = Some(page);
    Ok(())
}

fn error_control() -> Control {
    Control {
        kind: ControlKind::Text,
        id: String::new(),
        text: ERROR_LABEL.to_owned(),
        position: None,
        help: String::new(),
        group_condition: None,
        group_behavior: GroupBehavior::Disable,
        group_control: 0,
        ignore_conflicts: false,
        action: None,
        values: Default::default(),
    }
}

/// The open page's control for `option`, with the context to read it, if `quest`'s page is open.
fn placed(world: &World, owner: &McmOwner, option: i32) -> Option<(Context, Control)> {
    let store = world.try_resource::<McmHelperStore>()?;
    let page = store.page.as_ref().filter(|page| page.owner == *owner)?;
    let control = page.controls.get(&option)?.control.clone();
    Some((Context { owner: owner.clone(), quest: page.quest, mod_name: page.mod_name.clone(), script: page.script.clone() }, control))
}

/// Redraw every control's value and flags (`RefreshPage`), after a value changed.
pub fn refresh(world: &World, owner: &McmOwner) {
    let Some((ctx, controls, groups)) = world.try_resource::<McmHelperStore>().and_then(|store| {
        let page = store.page.as_ref().filter(|page| page.owner == *owner && page.error.is_none())?;
        let ctx = Context { owner: owner.clone(), quest: page.quest, mod_name: page.mod_name.clone(), script: page.script.clone() };
        let controls: Vec<(i32, Control)> = page.controls.iter().map(|(id, placed)| (*id, placed.control.clone())).collect();
        Some((ctx, controls, page.groups.clone()))
    }) else {
        return;
    };
    let (active, all) = group_state(world, &ctx, &groups);
    for (id, control) in controls {
        let (_, text, number) = shown(world, &ctx, &control);
        let mut host = world.resource_mut::<McmHost>();
        match control.kind {
            ControlKind::Text | ControlKind::Input | ControlKind::Stepper | ControlKind::Menu | ControlKind::Enum => host.set_option_text(id, text),
            ControlKind::Slider => host.set_option_number(id, number, text),
            ControlKind::Toggle | ControlKind::Color | ControlKind::KeyMap => host.set_option_number(id, number, String::new()),
            ControlKind::Empty | ControlKind::Header | ControlKind::HiddenToggle | ControlKind::Button => {}
        }
        // A control without a group condition keeps the flags it was laid out with (`RefreshFlags`).
        if control.group_condition.is_some()
            && let Some(group_flags) = behaviour(&control, &active, &all)
        {
            let option_flags = if control.kind == ControlKind::KeyMap && group_flags == 0 { flags::WITH_UNMAP } else { group_flags };
            host.set_option_flags(id, option_flags as i32);
        }
    }
}

/// After a change: the info text again if the cursor is on an option (`UpdateInfoText(…, true)`).
fn update_info(world: &World, owner: &McmOwner) {
    let highlighted = world.try_resource::<McmHelperStore>().and_then(|store| store.page.as_ref().and_then(|page| page.highlighted));
    if let Some(option) = highlighted {
        highlight(world, owner, option);
    }
}

/// The id a change reports to `OnSettingChange`, after the page shows the new value.
fn changed(world: &World, ctx: &Context, control: &Control) -> String {
    let regroup = control.group_control > 0;
    refresh(world, &ctx.owner);
    if regroup {
        log::trace!("MCM Helper: group {} switched; the page refreshed", control.group_control);
    }
    update_info(world, &ctx.owner);
    control.id.clone()
}

/// `OnOptionHighlight`: the control's help, `{value}` filled; an error page's error.
pub fn highlight(world: &World, owner: &McmOwner, option: i32) {
    let error = world.try_resource::<McmHelperStore>().and_then(|store| store.page.as_ref().and_then(|page| page.error.clone()));
    if let Some(mut store) = world.try_resource_mut::<McmHelperStore>()
        && let Some(page) = store.page.as_mut().filter(|page| page.owner == *owner)
    {
        page.highlighted = Some(option);
    }
    let text = match (error, placed(world, owner, option)) {
        (Some(error), _) => error,
        (None, Some((ctx, control))) => info_text(world, &ctx, &control),
        (None, None) => return,
    };
    world.resource_mut::<McmHost>().set_info_text(text);
}

/// `OnOptionSelect`: a toggle flips, a stepper steps; the id to report, `""` for none. Every control's
/// action is queued (MCM Helper invokes it on any select).
pub fn select(world: &World, owner: &McmOwner, option: i32) -> String {
    let Some((ctx, control)) = placed(world, owner, option) else { return String::new() };
    let id = match control.kind {
        ControlKind::Toggle => {
            let flipped = if read_number(world, &ctx, &control) == 0.0 { 1.0 } else { 0.0 };
            report(write_number(world, &ctx, &control, flipped), &control);
            changed(world, &ctx, &control)
        }
        ControlKind::Stepper if !control.values.options.is_empty() => {
            let next = (read_number(world, &ctx, &control) as i64 + 1).rem_euclid(control.values.options.len() as i64);
            report(write_number(world, &ctx, &control, next as f32), &control);
            changed(world, &ctx, &control)
        }
        _ => String::new(),
    };
    queue_action(world, &ctx, &control);
    id
}

fn report(result: Result<(), String>, control: &Control) {
    if let Err(error) = result {
        log::warn!("MCM Helper: control '{}' cannot store its value: {error}", control.id);
    }
}

/// `OnOptionDefault`: the value back to its default; the id to report.
pub fn reset(world: &World, owner: &McmOwner, option: i32) -> String {
    let Some((ctx, control)) = placed(world, owner, option) else { return String::new() };
    if control.values.source.is_none() {
        return String::new();
    }
    let result = if control.kind.is_text() {
        write_text(world, &ctx, &control, default_text(world, &ctx, &control))
    } else {
        write_number(world, &ctx, &control, default_number(world, &ctx, &control))
    };
    report(result, &control);
    changed(world, &ctx, &control)
}

/// `OnOptionSliderOpen` / `MenuOpen` / `ColorOpen` / `InputOpen`: the dialog's values.
pub fn open_dialog(world: &World, owner: &McmOwner, option: i32) {
    let Some((ctx, control)) = placed(world, owner, option) else { return };
    match control.kind {
        ControlKind::Slider => {
            let (start, default) = (read_number(world, &ctx, &control), default_number(world, &ctx, &control));
            let mut host = world.resource_mut::<McmHost>();
            host.set_dialog_number(0, start);
            host.set_dialog_number(1, default);
            host.set_dialog_number(2, control.values.min);
            host.set_dialog_number(3, control.values.max);
            host.set_dialog_number(4, control.values.step);
        }
        ControlKind::Menu | ControlKind::Enum => {
            let (options, _) = menu_options(world, &control);
            let (start, default) = if control.kind == ControlKind::Enum {
                (read_number(world, &ctx, &control) as i32, Some(default_number(world, &ctx, &control) as i32))
            } else {
                let position = |value: &str| options.iter().position(|option| option == value).map(|i| i as i32);
                (position(&read_text(world, &ctx, &control)).unwrap_or(-1), position(&default_text(world, &ctx, &control)))
            };
            let mut host = world.resource_mut::<McmHost>();
            host.set_menu_dialog_options(options);
            if start >= 0 {
                host.set_dialog_number(0, start as f32);
            }
            if let Some(default) = default {
                host.set_dialog_number(1, default as f32);
            }
        }
        ControlKind::Color => {
            let (start, default) = (read_number(world, &ctx, &control), default_number(world, &ctx, &control));
            let mut host = world.resource_mut::<McmHost>();
            host.set_dialog_number(0, start);
            host.set_dialog_number(1, default);
        }
        ControlKind::Input => {
            let text = read_text(world, &ctx, &control);
            world.resource_mut::<McmHost>().set_input_dialog_text(text);
        }
        _ => {}
    }
}

/// What a dialog accepted.
#[derive(Debug, Clone, PartialEq)]
pub enum Accepted {
    Number(f32),
    /// A menu's index.
    Index(i32),
    Text(String),
}

/// `OnOptionSliderAccept` / `MenuAccept` / `ColorAccept` / `KeyMapChange` / `InputAccept`: the value
/// stored and the action queued; the id to report.
pub fn accept(world: &World, owner: &McmOwner, option: i32, value: Accepted) -> String {
    let Some((ctx, control)) = placed(world, owner, option) else { return String::new() };
    let result = match (control.kind, value) {
        (ControlKind::Menu, Accepted::Index(index)) => {
            let (options, _) = menu_options(world, &control);
            match usize::try_from(index).ok().and_then(|i| options.get(i)) {
                Some(choice) => write_text(world, &ctx, &control, choice.clone()),
                None => Ok(()),
            }
        }
        (ControlKind::Enum, Accepted::Index(index)) => write_number(world, &ctx, &control, index as f32),
        (ControlKind::KeyMap, Accepted::Number(code)) if control.values.source.is_none() => {
            // A key map bound to a hotkey of `keybinds.json` (`KeybindManager::Register`).
            if control.id.is_empty() {
                Err("a key map with neither a value source nor an id".to_owned())
            } else {
                world.resource_mut::<McmHelperStore>().bind_key(&ctx.mod_name, &control.id, code as i32);
                Ok(())
            }
        }
        (ControlKind::Slider | ControlKind::Color | ControlKind::KeyMap, Accepted::Number(number)) => {
            write_number(world, &ctx, &control, number)
        }
        (ControlKind::Input, Accepted::Text(text)) => write_text(world, &ctx, &control, text),
        (kind, value) => Err(format!("{value:?} does not fit a {kind:?} control")),
    };
    report(result, &control);
    let id = changed(world, &ctx, &control);
    queue_action(world, &ctx, &control);
    id
}

// ---- actions ----

fn queue_action(world: &World, ctx: &Context, control: &Control) {
    let Some(action) = &control.action else { return };
    let value = value_param(world, ctx, control);
    let args = action
        .params
        .iter()
        .map(|param| match param {
            Param::ControlValue => value.clone(),
            Param::Int(i) => VmStored::Int(*i),
            Param::Bool(b) => VmStored::Bool(*b),
            Param::Float(f) => VmStored::Float(*f),
            Param::Text(text) => VmStored::Str(text.clone()),
        })
        .collect();
    let pending = match &action.target {
        ActionTarget::Global { script } => {
            Ok(PendingAction { quest: ctx.quest.unwrap_or(QuestFormId(0)), script: script.clone(), function: action.function.clone(), args, global: true })
        }
        ActionTarget::Method { form, script } => target_quest(world, ctx, form.as_ref()).map(|quest| PendingAction {
            quest,
            script: method_script(world, ctx, quest, script.as_deref()),
            function: action.function.clone(),
            args,
            global: false,
        }),
    };
    match pending {
        Ok(pending) => world.resource_mut::<McmHelperStore>().pending.push(pending),
        Err(error) => log::warn!("MCM Helper: control '{}': its action is not run: {error}", control.id),
    }
}

/// Run the actions the last events queued, each checked first: a function the VM cannot run is declined
/// whole with a diagnostic, never half-run.
pub fn run_pending_actions(world: &World) {
    let pending = match world.try_resource_mut::<McmHelperStore>() {
        Some(mut store) => store.take_pending(),
        None => return,
    };
    for action in pending {
        if let Err(error) = run_action(world, &action) {
            log::warn!("MCM Helper: {}.{} is not run: {error}", action.script, action.function);
        }
    }
}

fn run_action(world: &World, action: &PendingAction) -> Result<(), String> {
    crate::fragment::check_installed_function(world, &action.script, &action.function)?;
    if action.global {
        crate::fragment::run_global_function(world, action.quest, &action.script, &action.function, action.args.clone());
        Ok(())
    } else {
        crate::fragment::run_quest_own_script(world, action.quest, &action.script, &action.function, action.args.clone())
    }
}


// ---- hotkeys (slice 4b) ----

/// This frame's key presses (`downs`) and releases with how long each was held (`ups`), as DirectInput
/// codes (a mouse button is 256 + its index): every hotkey bound to the key runs its action. The input
/// feed sends nothing while the game is paused, as MCM Helper ignores input then.
pub fn hotkey_events(world: &World, downs: &[i32], ups: &[(i32, f32)]) {
    let presses = downs.iter().map(|&code| (code, None)).chain(ups.iter().map(|&(code, held)| (code, Some(held))));
    let mut queued = false;
    for (code, released) in presses {
        let hotkeys: Vec<(String, Keybind, Option<ConfigQuest>)> = {
            let mut store = world.resource_mut::<McmHelperStore>();
            let bound: Vec<(String, String)> =
                store.registrations().bound_to(code).map(|(mod_name, id)| (mod_name.to_owned(), id.to_owned())).collect();
            bound
                .into_iter()
                .filter_map(|(mod_name, id)| {
                    let key = mod_name.to_ascii_lowercase();
                    let keybind = store.keybinds.get(&key)?.iter().find(|k| k.id == id)?.clone();
                    Some((mod_name, keybind, store.config_quests.get(&key).cloned()))
                })
                .collect()
        };
        for (mod_name, keybind, config) in hotkeys {
            queued |= queue_hotkey(world, &mod_name, &keybind, config, released);
        }
    }
    if queued {
        run_pending_actions(world);
    }
}

/// Queue what a hotkey does on this press; whether anything was queued.
fn queue_hotkey(world: &World, mod_name: &str, keybind: &Keybind, config: Option<ConfigQuest>, released: Option<f32>) -> bool {
    let Some((quest, script)) = config else { return false };
    let owner = quest.map_or_else(|| McmOwner::Config(mod_name.to_owned()), McmOwner::Script);
    let ctx = Context { owner, quest, mod_name: mod_name.to_owned(), script };
    let pending = match (&keybind.action, released) {
        (HotkeyAction::Call(action), None) => {
            let args = action
                .params
                .iter()
                .map(|param| match param {
                    Param::ControlValue => VmStored::None,
                    Param::Int(i) => VmStored::Int(*i),
                    Param::Bool(b) => VmStored::Bool(*b),
                    Param::Float(f) => VmStored::Float(*f),
                    Param::Text(text) => VmStored::Str(text.clone()),
                })
                .collect();
            match &action.target {
                ActionTarget::Global { script } => {
                    Ok(PendingAction { quest: quest.unwrap_or(QuestFormId(0)), script: script.clone(), function: action.function.clone(), args, global: true })
                }
                ActionTarget::Method { form, script } => target_quest(world, &ctx, form.as_ref()).map(|target| PendingAction {
                    quest: target,
                    script: method_script(world, &ctx, target, script.as_deref()),
                    function: action.function.clone(),
                    args,
                    global: false,
                }),
            }
        }
        (HotkeyAction::Call(_), Some(_)) => return false,
        (HotkeyAction::SendEvent { form, script }, released) => target_quest(world, &ctx, Some(form)).and_then(|target| {
            let script = script.clone().or_else(|| first_script(world, target)).ok_or("the form runs no script")?;
            let (function, args) = match released {
                None => ("OnControlDown", vec![VmStored::Str(keybind.id.clone())]),
                Some(held) => ("OnControlUp", vec![VmStored::Str(keybind.id.clone()), VmStored::Float(held)]),
            };
            Ok(PendingAction { quest: target, script, function: function.to_owned(), args, global: false })
        }),
        (HotkeyAction::Console { command }, None) => {
            let mut store = world.resource_mut::<McmHelperStore>();
            if store.console_declined.insert(format!("{mod_name}:{}", keybind.id)) {
                log::warn!("MCM Helper: hotkey '{}' of {mod_name} runs the console command '{command}', which the engine does not interpret", keybind.id);
            }
            return false;
        }
        (HotkeyAction::Console { .. }, Some(_)) => return false,
    };
    match pending {
        Ok(pending) => {
            world.resource_mut::<McmHelperStore>().pending.push(pending);
            true
        }
        Err(error) => {
            log::warn!("MCM Helper: hotkey '{}' of {mod_name} is not run: {error}", keybind.id);
            false
        }
    }
}

/// The script a method action calls on `quest`: its `scriptName`; else, on the config quest, the config
/// script; else the quest's own script (FO4 MCM: `scriptName` "is only needed if the form has several
/// scripts").
fn method_script(world: &World, ctx: &Context, quest: QuestFormId, named: Option<&str>) -> String {
    match named {
        Some(script) => script.to_owned(),
        None if ctx.quest == Some(quest) && !ctx.script.is_empty() => ctx.script.clone(),
        None => first_script(world, quest).unwrap_or_default(),
    }
}

/// The first script the VM runs on `quest`, for an event whose `scriptName` is omitted.
fn first_script(world: &World, quest: QuestFormId) -> Option<String> {
    world.try_resource::<crate::QuestOwnScripts>().and_then(|scripts| {
        scripts.running_scripts().find(|(owner, _)| *owner == quest).and_then(|(_, scripts)| scripts.first().map(|s| s.script.clone()))
    })
}
