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
pub mod settings;

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

use self::config::{ActionTarget, Control, ControlKind, DefaultValue, FormSpec, GroupBehavior, HelperConfig, NumberKind, PageContent, Param, Source};
use self::settings::{HelperFiles, SettingStore};
use super::{flags, McmHost, McmKind, McmOption};
use crate::quest_stages::QuestFormId;
use crate::translate::effects::VmStored;

/// MCM Helper v1.6.2's `MCM.GetVersionCode()`, the release the engine follows (CMakeLists `PLUGIN_VERSION`).
pub const VERSION_CODE: u32 = 15;

/// The script a config script extends for MCM Helper, lowercase.
pub const HELPER_BASE: &str = "mcm_configbase";

/// The text option MCM Helper shows for a config it cannot load (`ErrorControl`).
const ERROR_LABEL: &str = "Error loading config";
const ERROR_VALUE: &str = "Click to view";

/// One control the open page laid out, with what it needs to read and write its value.
#[derive(Debug, Clone)]
struct PlacedControl {
    control: Control,
}

/// The open MCM Helper page (`ConfigPageCache`): its controls by option id, its group switches, and the
/// menu options a script replaced (`SetMenuOptions`).
#[derive(Debug, Clone)]
struct HelperPage {
    quest: QuestFormId,
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
            ..Default::default()
        }
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
                Some(bytes) => config::parse(&bytes),
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
        if let Some(page) = self.page.as_mut().filter(|page| page.quest == quest) {
            page.menu_overrides.insert(id.to_ascii_lowercase(), (options, short_names));
        }
    }
}

fn stem(file: &str) -> &str {
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

/// Who is asking: the config quest, its mod and its config script.
#[derive(Debug, Clone)]
struct Context {
    quest: QuestFormId,
    mod_name: String,
    script: String,
}

fn context(world: &World, quest: QuestFormId) -> Result<Context, String> {
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
    Ok(Context { quest, mod_name, script })
}

/// A form a config names (`"Plugin.esp|D62"`), as a load-order FormID.
fn resolve_form(world: &World, form: &FormSpec) -> Option<u32> {
    let plugin = byroredux_core::form_id::PluginId::from_filename(&form.plugin);
    world.try_resource::<crate::load_order::LoadOrderIdentity>()?.form_from_file(form.local, plugin)
}

/// The quest a property source or a method action targets: the config quest by default. A form that is
/// not a quest with scripts is refused: the VM keeps script variables for quests only.
fn target_quest(world: &World, ctx: &Context, form: Option<&FormSpec>) -> Result<QuestFormId, String> {
    let Some(form) = form else { return Ok(ctx.quest) };
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
        ControlKind::Empty | ControlKind::Header => VmStored::None,
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
pub fn page_reset(world: &World, quest: QuestFormId, page_name: &str) -> Result<(), String> {
    let ctx = context(world, quest)?;
    let config = world.resource_mut::<McmHelperStore>().config(&ctx.mod_name);
    let mut page = HelperPage {
        quest,
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
fn placed(world: &World, quest: QuestFormId, option: i32) -> Option<(Context, Control)> {
    let store = world.try_resource::<McmHelperStore>()?;
    let page = store.page.as_ref().filter(|page| page.quest == quest)?;
    let control = page.controls.get(&option)?.control.clone();
    Some((Context { quest, mod_name: page.mod_name.clone(), script: page.script.clone() }, control))
}

/// Redraw every control's value and flags (`RefreshPage`), after a value changed.
pub fn refresh(world: &World, quest: QuestFormId) {
    let Some((ctx, controls, groups)) = world.try_resource::<McmHelperStore>().and_then(|store| {
        let page = store.page.as_ref().filter(|page| page.quest == quest && page.error.is_none())?;
        let ctx = Context { quest, mod_name: page.mod_name.clone(), script: page.script.clone() };
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
            ControlKind::Empty | ControlKind::Header | ControlKind::HiddenToggle => {}
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
fn update_info(world: &World, quest: QuestFormId) {
    let highlighted = world.try_resource::<McmHelperStore>().and_then(|store| store.page.as_ref().and_then(|page| page.highlighted));
    if let Some(option) = highlighted {
        highlight(world, quest, option);
    }
}

/// The id a change reports to `OnSettingChange`, after the page shows the new value.
fn changed(world: &World, ctx: &Context, control: &Control) -> String {
    let regroup = control.group_control > 0;
    refresh(world, ctx.quest);
    if regroup {
        log::trace!("MCM Helper: group {} switched; the page refreshed", control.group_control);
    }
    update_info(world, ctx.quest);
    control.id.clone()
}

/// `OnOptionHighlight`: the control's help, `{value}` filled; an error page's error.
pub fn highlight(world: &World, quest: QuestFormId, option: i32) {
    let error = world.try_resource::<McmHelperStore>().and_then(|store| store.page.as_ref().and_then(|page| page.error.clone()));
    if let Some(mut store) = world.try_resource_mut::<McmHelperStore>()
        && let Some(page) = store.page.as_mut().filter(|page| page.quest == quest)
    {
        page.highlighted = Some(option);
    }
    let text = match (error, placed(world, quest, option)) {
        (Some(error), _) => error,
        (None, Some((ctx, control))) => info_text(world, &ctx, &control),
        (None, None) => return,
    };
    world.resource_mut::<McmHost>().set_info_text(text);
}

/// `OnOptionSelect`: a toggle flips, a stepper steps; the id to report, `""` for none. Every control's
/// action is queued (MCM Helper invokes it on any select).
pub fn select(world: &World, quest: QuestFormId, option: i32) -> String {
    let Some((ctx, control)) = placed(world, quest, option) else { return String::new() };
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
pub fn reset(world: &World, quest: QuestFormId, option: i32) -> String {
    let Some((ctx, control)) = placed(world, quest, option) else { return String::new() };
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
pub fn open_dialog(world: &World, quest: QuestFormId, option: i32) {
    let Some((ctx, control)) = placed(world, quest, option) else { return };
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
pub fn accept(world: &World, quest: QuestFormId, option: i32, value: Accepted) -> String {
    let Some((ctx, control)) = placed(world, quest, option) else { return String::new() };
    let result = match (control.kind, value) {
        (ControlKind::Menu, Accepted::Index(index)) => {
            let (options, _) = menu_options(world, &control);
            match usize::try_from(index).ok().and_then(|i| options.get(i)) {
                Some(choice) => write_text(world, &ctx, &control, choice.clone()),
                None => Ok(()),
            }
        }
        (ControlKind::Enum, Accepted::Index(index)) => write_number(world, &ctx, &control, index as f32),
        (ControlKind::KeyMap, Accepted::Number(_)) if control.values.source.is_none() => {
            // A key map bound to a hotkey of `keybinds.json`: slice 4b.
            Err("a key map bound to an MCM Helper hotkey is not served yet".to_owned())
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
            Ok(PendingAction { quest: ctx.quest, script: script.clone(), function: action.function.clone(), args, global: true })
        }
        ActionTarget::Method { form, script } => target_quest(world, ctx, form.as_ref()).map(|quest| PendingAction {
            quest,
            script: script.clone().unwrap_or_else(|| ctx.script.clone()),
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

