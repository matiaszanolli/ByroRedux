//! MCM Helper's `config.json` (`docs/config.schema.json` of MCM Helper v1.6.2, and the plugin's
//! `Json/*Handler.cpp` for what each key means): a mod's pages, each a list of controls bound to where
//! their values live.
//!
//! The file is mod input: a malformed config is an error string (MCM Helper shows it on an error page),
//! never a panic. Unknown keys are ignored rather than refused, so a config written for a later release
//! still shows what this one understands.

use serde_json::{Map, Value};

/// A parsed config.
#[derive(Debug, Clone, PartialEq)]
pub struct HelperConfig {
    pub mod_name: String,
    pub display_name: String,
    /// The lowest MCM Helper version code the config needs (`minMcmVersion`).
    pub min_mcm_version: u32,
    /// Plugins that must be loaded for the config to load (`pluginRequirements`).
    pub plugin_requirements: Vec<String>,
    /// `cursorFillMode`: SkyUI's `LEFT_TO_RIGHT` (1, the default) or `TOP_TO_BOTTOM` (2).
    pub fill_mode: i32,
    /// The front page (the config's own `content` / `customContent`).
    pub main: Option<PageContent>,
    /// The sub-pages in file order, by display name; a repeated name replaces the earlier page in its place
    /// (MCM Helper keys them by name in an insertion-ordered map).
    pub pages: Vec<(String, PageContent)>,
}

/// What a page shows.
#[derive(Debug, Clone, PartialEq)]
pub enum PageContent {
    Layout(Vec<Control>),
    /// `customContent`: a `.swf` or `.dds` drawn in place of the list.
    Custom { source: String, x: f32, y: f32 },
}

/// A control's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Empty,
    Header,
    Text,
    Toggle,
    /// A toggle that holds a value (a group's switch) but is never drawn.
    HiddenToggle,
    Slider,
    /// A text option that steps through its options on select.
    Stepper,
    /// A menu whose value is the chosen option's text.
    Menu,
    /// A menu whose value is the chosen option's index.
    Enum,
    Color,
    KeyMap,
    Input,
}

impl ControlKind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "empty" => Self::Empty,
            "header" => Self::Header,
            "text" => Self::Text,
            "toggle" => Self::Toggle,
            "hiddenToggle" => Self::HiddenToggle,
            "slider" => Self::Slider,
            "stepper" => Self::Stepper,
            "menu" => Self::Menu,
            "enum" => Self::Enum,
            "color" => Self::Color,
            "keymap" => Self::KeyMap,
            "input" => Self::Input,
            _ => return None,
        })
    }

    /// Whether its value is text (a text, menu or input control) rather than a number.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Text | Self::Menu | Self::Input)
    }
}

/// What a control does when its group condition is false (`groupBehavior`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBehavior {
    #[default]
    Disable,
    Hide,
    /// Not added to the page at all.
    Skip,
}

/// How a condition's operands combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Conjunction {
    /// A bare number or an array: MCM Helper's default.
    #[default]
    Or,
    And,
    /// The listed groups are active and no other group is.
    Only,
    Not,
}

/// A `groupCondition` tree over group numbers (a group is active while its `groupControl` toggle is
/// checked).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupCondition {
    pub conjunction: Conjunction,
    pub groups: Vec<u32>,
    pub subtrees: Vec<GroupCondition>,
}

impl GroupCondition {
    /// Whether the condition holds, given which groups are active and every group the page defines
    /// (`GroupConditionTree::GetIsActive`).
    pub fn holds(&self, active: &dyn Fn(u32) -> bool, all_groups: &[u32]) -> bool {
        match self.conjunction {
            Conjunction::Or => self.groups.iter().any(|g| active(*g)) || self.subtrees.iter().any(|t| t.holds(active, all_groups)),
            Conjunction::And => self.groups.iter().all(|g| active(*g)) && self.subtrees.iter().all(|t| t.holds(active, all_groups)),
            Conjunction::Only => {
                self.subtrees.is_empty()
                    && self.groups.iter().all(|g| active(*g))
                    && all_groups.iter().filter(|g| !self.groups.contains(g)).all(|g| !active(*g))
            }
            Conjunction::Not => !self.groups.iter().any(|g| active(*g)) && !self.subtrees.iter().any(|t| t.holds(active, all_groups)),
        }
    }
}

/// A form named in a config: `"Plugin.esp|D62"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSpec {
    pub plugin: String,
    pub local: u32,
}

impl FormSpec {
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        let (plugin, id) = text.split_once('|').ok_or_else(|| format!("form '{text}' is not 'Plugin.esp|ID'"))?;
        let digits = id.trim_start_matches("0x").trim_start_matches("0X");
        let local = u32::from_str_radix(digits, 16).map_err(|_| format!("form '{text}': '{id}' is not a hex form id"))?;
        Ok(Self { plugin: plugin.to_owned(), local })
    }
}

/// The kind of value a numeric source holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    Bool,
    Int,
    Float,
}

/// Where a control's value lives (`valueOptions.sourceType`, or for a text control its property or its id).
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// An Auto property of a script on a form; the form and script default to the config quest and its
    /// script. `number` is `None` for a string property.
    Property { number: Option<NumberKind>, form: Option<FormSpec>, script: Option<String>, property: String },
    /// A mod setting named by the control's `id` (`key:Section`). `number` is `None` for a string setting.
    ModSetting { number: Option<NumberKind> },
    /// A `GLOB`'s value.
    Global { form: FormSpec },
}

/// A control's `defaultValue`.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
    Number(f32),
    Text(String),
}

/// A control's `valueOptions`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueOptions {
    pub source: Option<Source>,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub format: String,
    pub options: Vec<String>,
    pub short_names: Vec<String>,
    /// A text control's fixed value.
    pub value: String,
    pub default: Option<DefaultValue>,
}

impl Default for ValueOptions {
    fn default() -> Self {
        Self {
            source: None,
            min: 0.0,
            max: 0.0,
            step: 0.0,
            format: "{0}".to_owned(),
            options: Vec::new(),
            short_names: Vec::new(),
            value: String::new(),
            default: None,
        }
    }
}

/// One parameter of an action (`params`): `{value}` is the control's value; `{i}`, `{u}`, `{b}`, `{f}`,
/// `{s}` and `{r}` type a literal; any other text is a string.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    ControlValue,
    Int(i32),
    Bool(bool),
    Float(f32),
    Text(String),
}

impl Param {
    pub(super) fn parse(value: &Value) -> Self {
        match value {
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) if n.is_i64() => Self::Int(n.as_i64().unwrap_or(0) as i32),
            Value::Number(n) => Self::Float(n.as_f64().unwrap_or(0.0) as f32),
            Value::String(text) => Self::parse_text(text),
            _ => Self::Text(String::new()),
        }
    }

    fn parse_text(text: &str) -> Self {
        if text == "{value}" {
            return Self::ControlValue;
        }
        let typed = |prefix: &str| text.strip_prefix(prefix).map(str::trim);
        if let Some(rest) = typed("{i}") {
            return Self::Int(rest.parse().unwrap_or(0));
        }
        if let Some(rest) = typed("{u}") {
            return Self::Int(rest.parse::<u32>().unwrap_or(0) as i32);
        }
        if let Some(rest) = typed("{r}") {
            return Self::Int(rest.parse::<u32>().unwrap_or(0) as i32);
        }
        if let Some(rest) = typed("{b}") {
            return Self::Bool(matches!(rest, "1" | "true" | "True" | "TRUE"));
        }
        if let Some(rest) = typed("{f}") {
            return Self::Float(rest.parse().unwrap_or(0.0));
        }
        if let Some(rest) = text.strip_prefix("{s}") {
            return Self::Text(rest.to_owned());
        }
        Self::Text(text.to_owned())
    }
}

/// What an action calls.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionTarget {
    /// `CallFunction`: a function of `script` attached to `form`; both default to the config quest and
    /// its script.
    Method { form: Option<FormSpec>, script: Option<String> },
    /// `CallGlobalFunction`: `script.function`.
    Global { script: String },
}

/// A control's `action`, run after its value changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub target: ActionTarget,
    pub function: String,
    pub params: Vec<Param>,
}

/// One control.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub kind: ControlKind,
    pub id: String,
    pub text: String,
    /// `position`: the cell to place it in, or `None` to follow the cursor.
    pub position: Option<i32>,
    pub help: String,
    pub group_condition: Option<GroupCondition>,
    pub group_behavior: GroupBehavior,
    /// The group a toggle switches (`groupControl`), 0 for none.
    pub group_control: u32,
    pub ignore_conflicts: bool,
    pub action: Option<Action>,
    pub values: ValueOptions,
}

/// Parse `config.json`.
pub fn parse(bytes: &[u8]) -> Result<HelperConfig, String> {
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
        .map_err(|_| "the config is not UTF-8".to_owned())?;
    let root: Value = serde_json::from_str(text).map_err(|error| format!("JSON: {error}"))?;
    let root = root.as_object().ok_or("the config is not a JSON object")?;
    let string = |key: &str| root.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let mut pages: Vec<(String, PageContent)> = Vec::new();
    if let Some(list) = root.get("pages") {
        for page in list.as_array().ok_or("'pages' is not an array")? {
            let page = page.as_object().ok_or("a page is not an object")?;
            let name = page.get("pageDisplayName").and_then(Value::as_str).ok_or("a page has no 'pageDisplayName'")?;
            let content = content(page)?.ok_or_else(|| format!("page '{name}' has no 'content' or 'customContent'"))?;
            match pages.iter_mut().find(|(existing, _)| existing == name) {
                Some(slot) => slot.1 = content,
                None => pages.push((name.to_owned(), content)),
            }
        }
    }
    Ok(HelperConfig {
        mod_name: string("modName"),
        display_name: string("displayName"),
        min_mcm_version: root.get("minMcmVersion").and_then(Value::as_u64).unwrap_or(0) as u32,
        plugin_requirements: root
            .get("pluginRequirements")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default(),
        fill_mode: match root.get("cursorFillMode").and_then(Value::as_str) {
            Some("topToBottom") => crate::mcm::TOP_TO_BOTTOM,
            _ => crate::mcm::LEFT_TO_RIGHT,
        },
        main: content(root)?,
        pages,
    })
}

/// A page's `content` or `customContent`, if it has either.
fn content(object: &Map<String, Value>) -> Result<Option<PageContent>, String> {
    if let Some(custom) = object.get("customContent") {
        let custom = custom.as_object().ok_or("'customContent' is not an object")?;
        let source = custom.get("source").and_then(Value::as_str).ok_or("'customContent' has no 'source'")?;
        let number = |key: &str| custom.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        return Ok(Some(PageContent::Custom { source: source.to_owned(), x: number("x"), y: number("y") }));
    }
    let Some(list) = object.get("content") else { return Ok(None) };
    let list = list.as_array().ok_or("'content' is not an array")?;
    list.iter().map(control).collect::<Result<Vec<_>, _>>().map(|controls| Some(PageContent::Layout(controls)))
}

fn control(value: &Value) -> Result<Control, String> {
    let object = value.as_object().ok_or("a control is not an object")?;
    let string = |key: &str| object.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let type_name = object.get("type").and_then(Value::as_str).ok_or("a control has no 'type'")?;
    let kind = ControlKind::parse(type_name).ok_or_else(|| format!("unknown control type '{type_name}'"))?;
    let id = string("id");
    let action = object.get("action").map(action).transpose()?;
    let values = match object.get("valueOptions") {
        Some(options) => value_options(options.as_object().ok_or("'valueOptions' is not an object")?, kind, &id)?,
        // A string control with an id and no options still stores its value as that mod setting.
        None if kind.is_text() && !id.is_empty() => {
            ValueOptions { source: Some(Source::ModSetting { number: None }), ..Default::default() }
        }
        None => ValueOptions::default(),
    };
    Ok(Control {
        kind,
        text: string("text"),
        position: object.get("position").and_then(Value::as_i64).map(|p| p as i32),
        help: string("help"),
        group_condition: object.get("groupCondition").map(group_condition).transpose()?,
        group_behavior: match object.get("groupBehavior").and_then(Value::as_str) {
            Some("hide") => GroupBehavior::Hide,
            Some("skip") => GroupBehavior::Skip,
            _ => GroupBehavior::Disable,
        },
        group_control: object.get("groupControl").and_then(Value::as_u64).unwrap_or(0) as u32,
        ignore_conflicts: object.get("ignoreConflicts").and_then(Value::as_bool).unwrap_or(false),
        action,
        values,
        id,
    })
}

fn value_options(object: &Map<String, Value>, kind: ControlKind, id: &str) -> Result<ValueOptions, String> {
    let string = |key: &str| -> Option<String> { object.get(key).and_then(Value::as_str).map(str::to_owned) };
    let number = |key: &str| object.get(key).and_then(Value::as_f64).map(|n| n as f32);
    let strings = |key: &str| -> Vec<String> {
        object.get(key).and_then(Value::as_array).map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default()
    };
    let form = string("sourceForm").map(|text| FormSpec::parse(&text)).transpose()?;
    let source = if kind.is_text() {
        // A string control: a string property when it names one, else the mod setting its id names
        // (`ContentHandler`; the schema's text `sourceType` is unused).
        match string("propertyName") {
            Some(property) => Some(Source::Property { number: None, form: form.clone(), script: string("scriptName"), property }),
            None if !id.is_empty() => Some(Source::ModSetting { number: None }),
            None => None,
        }
    } else {
        match string("sourceType").as_deref() {
            None | Some("") => None,
            Some(source) => Some(numeric_source(source, form.clone(), string("scriptName"), string("propertyName"))?),
        }
    };
    let default = match object.get("defaultValue") {
        Some(Value::Bool(b)) => Some(DefaultValue::Number(f32::from(u8::from(*b)))),
        Some(Value::Number(n)) => Some(DefaultValue::Number(n.as_f64().unwrap_or(0.0) as f32)),
        Some(Value::String(text)) => Some(match Param::parse_text(text) {
            Param::Int(i) => DefaultValue::Number(i as f32),
            Param::Bool(b) => DefaultValue::Number(f32::from(u8::from(b))),
            Param::Float(f) => DefaultValue::Number(f),
            _ => DefaultValue::Text(text.clone()),
        }),
        _ => None,
    };
    Ok(ValueOptions {
        source,
        min: number("min").unwrap_or(0.0),
        max: number("max").unwrap_or(0.0),
        step: number("step").unwrap_or(0.0),
        format: string("formatString").unwrap_or_else(|| "{0}".to_owned()),
        options: strings("options"),
        short_names: strings("shortNames"),
        value: string("value").unwrap_or_default(),
        default,
    })
}

fn numeric_source(source: &str, form: Option<FormSpec>, script: Option<String>, property: Option<String>) -> Result<Source, String> {
    let kind = |suffix: &str| match suffix {
        "Bool" => Some(NumberKind::Bool),
        "Int" => Some(NumberKind::Int),
        "Float" => Some(NumberKind::Float),
        _ => None,
    };
    if let Some(suffix) = source.strip_prefix("PropertyValue") {
        let number = kind(suffix).ok_or_else(|| format!("unknown sourceType '{source}'"))?;
        let property = property.ok_or_else(|| format!("{source} without a 'propertyName'"))?;
        return Ok(Source::Property { number: Some(number), form, script, property });
    }
    if let Some(suffix) = source.strip_prefix("ModSetting") {
        let number = kind(suffix).ok_or_else(|| format!("unknown sourceType '{source}'"))?;
        return Ok(Source::ModSetting { number: Some(number) });
    }
    if source == "GlobalValue" {
        return Ok(Source::Global { form: form.ok_or("GlobalValue without a 'sourceForm'")? });
    }
    Err(format!("unknown sourceType '{source}'"))
}

pub(super) fn action(value: &Value) -> Result<Action, String> {
    let object = value.as_object().ok_or("an 'action' is not an object")?;
    let string = |key: &str| -> Option<String> { object.get(key).and_then(Value::as_str).map(str::to_owned) };
    let function = string("function").ok_or("an action has no 'function'")?;
    let target = match string("type").as_deref() {
        // Without a form, MCM Helper takes the script from `script` as well (`ActionHandler`).
        Some("CallFunction") => ActionTarget::Method {
            form: string("form").map(|text| FormSpec::parse(&text)).transpose()?,
            script: string("scriptName").or_else(|| string("form").is_none().then(|| string("script")).flatten()),
        },
        Some("CallGlobalFunction") => ActionTarget::Global { script: string("script").ok_or("CallGlobalFunction without a 'script'")? },
        Some(other) => return Err(format!("Unsupported action type '{other}'")),
        None => return Err("an action has no 'type'".to_owned()),
    };
    let params = object.get("params").and_then(Value::as_array).map(|list| list.iter().map(Param::parse).collect()).unwrap_or_default();
    Ok(Action { target, function, params })
}

fn group_condition(value: &Value) -> Result<GroupCondition, String> {
    match value {
        Value::Number(n) => Ok(GroupCondition { groups: vec![n.as_u64().unwrap_or(0) as u32], ..Default::default() }),
        Value::Array(items) => operands(items, Conjunction::Or),
        Value::Object(object) => {
            let (key, inner) = object.iter().next().ok_or("an empty 'groupCondition'")?;
            let conjunction = match key.as_str() {
                "OR" => Conjunction::Or,
                "AND" => Conjunction::And,
                "ONLY" => Conjunction::Only,
                "NOT" => Conjunction::Not,
                other => return Err(format!("unknown group conjunction '{other}'")),
            };
            match inner {
                Value::Array(items) => operands(items, conjunction),
                single => {
                    let mut tree = operands(std::slice::from_ref(single), conjunction)?;
                    tree.conjunction = conjunction;
                    Ok(tree)
                }
            }
        }
        _ => Err("a 'groupCondition' is not a number, array or object".to_owned()),
    }
}

fn operands(items: &[Value], conjunction: Conjunction) -> Result<GroupCondition, String> {
    let mut tree = GroupCondition { conjunction, ..Default::default() };
    for item in items {
        match item {
            Value::Number(n) => tree.groups.push(n.as_u64().unwrap_or(0) as u32),
            other => tree.subtrees.push(group_condition(other)?),
        }
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
        "modName": "Frostfall",
        "displayName": "$FrostfallName",
        "minMcmVersion": 13,
        "pluginRequirements": ["Frostfall.esp"],
        "cursorFillMode": "topToBottom",
        "content": [ { "type": "header", "text": "Front" } ],
        "pages": [
            {
                "pageDisplayName": "General",
                "content": [
                    { "id": "bEnabled:Main", "text": "Enabled", "type": "toggle", "groupControl": 1,
                      "help": "Now {value}", "valueOptions": { "sourceType": "ModSettingBool", "defaultValue": true } },
                    { "id": "fRate:Main", "text": "Rate", "type": "slider", "position": 3,
                      "groupCondition": { "AND": [1, { "NOT": 2 }] }, "groupBehavior": "skip",
                      "valueOptions": { "min": 0, "max": 5, "step": 0.5, "formatString": "{1}x", "sourceType": "ModSettingFloat" },
                      "action": { "type": "CallFunction", "function": "Apply", "params": ["{value}", "{i}7", "{f}1.5", "{b}true", 3, "plain"] } },
                    { "text": "Speed", "type": "enum", "valueOptions": { "options": ["Slow", "Fast"], "sourceType": "PropertyValueInt",
                      "sourceForm": "Frostfall.esp|0x800", "scriptName": "FrostMain", "propertyName": "Speed" } },
                    { "id": "sName:Main", "text": "Name", "type": "input" },
                    { "text": "Version", "type": "text", "valueOptions": { "value": "3.4" } },
                    { "text": "Cold", "type": "slider", "valueOptions": { "sourceType": "GlobalValue", "sourceForm": "Frostfall.esp|D62" },
                      "action": { "type": "CallGlobalFunction", "script": "FrostUtil", "function": "Notify" } }
                ]
            },
            { "pageDisplayName": "Logo", "customContent": { "source": "frostfall/logo.dds", "x": 10, "y": 20 } },
            { "pageDisplayName": "General", "content": [ { "type": "empty" } ] }
        ]
    }"#;

    #[test]
    fn a_config_parses_its_top_level_and_keeps_page_order() {
        let config = parse(CONFIG.as_bytes()).expect("parses");
        assert_eq!((config.mod_name.as_str(), config.display_name.as_str()), ("Frostfall", "$FrostfallName"));
        assert_eq!((config.min_mcm_version, config.fill_mode), (13, crate::mcm::TOP_TO_BOTTOM));
        assert_eq!(config.plugin_requirements, ["Frostfall.esp"]);
        assert!(matches!(&config.main, Some(PageContent::Layout(controls)) if controls[0].kind == ControlKind::Header));
        let names: Vec<_> = config.pages.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["General", "Logo"], "a repeated name replaces the page in its place");
        assert!(matches!(&config.pages[0].1, PageContent::Layout(controls) if controls.len() == 1 && controls[0].kind == ControlKind::Empty));
        assert_eq!(config.pages[1].1, PageContent::Custom { source: "frostfall/logo.dds".into(), x: 10.0, y: 20.0 });
    }

    #[test]
    fn controls_parse_their_sources_conditions_and_actions() {
        let text = CONFIG.replace(r#"{ "pageDisplayName": "General", "content": [ { "type": "empty" } ] }"#, r#"{ "pageDisplayName": "Other", "content": [] }"#);
        let config = parse(text.as_bytes()).unwrap();
        let PageContent::Layout(controls) = &config.pages[0].1 else { panic!("a layout") };
        let toggle = &controls[0];
        assert_eq!((toggle.kind, toggle.group_control, toggle.id.as_str()), (ControlKind::Toggle, 1, "bEnabled:Main"));
        assert_eq!(toggle.values.source, Some(Source::ModSetting { number: Some(NumberKind::Bool) }));
        assert_eq!(toggle.values.default, Some(DefaultValue::Number(1.0)));
        let slider = &controls[1];
        assert_eq!((slider.position, slider.group_behavior), (Some(3), GroupBehavior::Skip));
        assert_eq!((slider.values.min, slider.values.max, slider.values.step, slider.values.format.as_str()), (0.0, 5.0, 0.5, "{1}x"));
        let condition = slider.group_condition.as_ref().unwrap();
        assert_eq!((condition.conjunction, condition.groups.as_slice()), (Conjunction::And, &[1][..]));
        assert_eq!(condition.subtrees[0].conjunction, Conjunction::Not);
        let action = slider.action.as_ref().unwrap();
        assert_eq!(action.target, ActionTarget::Method { form: None, script: None });
        assert_eq!(
            action.params,
            [Param::ControlValue, Param::Int(7), Param::Float(1.5), Param::Bool(true), Param::Int(3), Param::Text("plain".into())]
        );
        let enumeration = &controls[2];
        assert_eq!(
            enumeration.values.source,
            Some(Source::Property {
                number: Some(NumberKind::Int),
                form: Some(FormSpec { plugin: "Frostfall.esp".into(), local: 0x800 }),
                script: Some("FrostMain".into()),
                property: "Speed".into()
            })
        );
        assert_eq!(controls[3].values.source, Some(Source::ModSetting { number: None }), "an input with an id is its setting");
        assert_eq!((controls[4].values.source.as_ref(), controls[4].values.value.as_str()), (None, "3.4"));
        assert_eq!(controls[5].values.source, Some(Source::Global { form: FormSpec { plugin: "Frostfall.esp".into(), local: 0xD62 } }));
        assert_eq!(controls[5].action.as_ref().unwrap().target, ActionTarget::Global { script: "FrostUtil".into() });
    }

    #[test]
    fn a_bad_config_is_an_error_not_a_panic() {
        for bad in [
            "",
            "[]",
            "{ \"pages\": 3 }",
            "{ \"pages\": [ { \"content\": [] } ] }",
            "{ \"content\": [ { \"type\": \"dial\" } ] }",
            "{ \"content\": [ { \"type\": \"slider\", \"valueOptions\": { \"sourceType\": \"ModSettingDate\" } } ] }",
            "{ \"content\": [ { \"type\": \"slider\", \"valueOptions\": { \"sourceType\": \"GlobalValue\" } } ] }",
            "{ \"content\": [ { \"type\": \"toggle\", \"action\": { \"type\": \"RunConsoleCommand\", \"function\": \"x\" } } ] }",
            "{ \"content\": [ { \"type\": \"toggle\", \"valueOptions\": { \"sourceType\": \"GlobalValue\", \"sourceForm\": \"x.esp|zz\" } } ] }",
        ] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad:?} is refused");
        }
    }

    #[test]
    fn group_conditions_follow_mcm_helpers_conjunctions() {
        let parsed = |json: &str| group_condition(&serde_json::from_str(json).unwrap()).unwrap();
        let active = |on: &'static [u32]| move |group: u32| on.contains(&group);
        let all = [1, 2, 3];
        assert!(parsed("1").holds(&active(&[1]), &all));
        assert!(!parsed("1").holds(&active(&[2]), &all));
        assert!(parsed("[1, 2]").holds(&active(&[2]), &all), "an array is OR");
        assert!(!parsed(r#"{"AND": [1, 2]}"#).holds(&active(&[2]), &all));
        assert!(parsed(r#"{"AND": [1, 2]}"#).holds(&active(&[1, 2]), &all));
        assert!(parsed(r#"{"ONLY": [1]}"#).holds(&active(&[1]), &all));
        assert!(!parsed(r#"{"ONLY": [1]}"#).holds(&active(&[1, 3]), &all), "another group is on");
        assert!(parsed(r#"{"NOT": 2}"#).holds(&active(&[1]), &all));
        assert!(!parsed(r#"{"NOT": [1, {"AND": [2]}]}"#).holds(&active(&[2]), &all));
    }
}
