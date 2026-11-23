//! MCM Helper's hotkeys (LMC-UI-002 slice 4b; `docs/keybinds.schema.json` and the plugin's
//! `KeybindManager.cpp` / `KeybindEventHandler.cpp` / `Config/Action.cpp`, v1.6.2).
//!
//! A mod's `MCM/Config/<mod>/keybinds.json` names hotkeys by id, each with a description and an action.
//! The player binds a key to one through a key-map control of the mod's config that has an `id` and no
//! value source. The bindings of every mod are one list, saved as `{ "version", "keybinds": [ { "keycode",
//! "modName", "id" } ] }`: MCM Helper keeps it in `MCM/Settings/keybinds.json`; the engine reads that
//! file (never writing it) under the profile's own `keybinds.json`, the one it writes (owner's decisions
//! §6.4–6.5).
//!
//! A pressed key runs the actions bound to it: a function call on the key going down; `SendEvent`'s
//! `OnControlDown(id)` going down and `OnControlUp(id, heldSeconds)` coming up. MCM Helper ignores input
//! while the game is paused; the engine's input feed does the same.

use serde_json::Value;

use super::config::{self, Action, FormSpec};

/// What a hotkey does.
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyAction {
    /// `CallFunction` / `CallGlobalFunction`, on the key going down; a `{value}` parameter is None.
    Call(Action),
    /// `SendEvent`: `OnControlDown(id)` / `OnControlUp(id, heldSeconds)` on `script` attached to `form`.
    SendEvent { form: FormSpec, script: Option<String> },
    /// `RunConsoleCommand`: the engine has no game-console interpreter, so it is declined with a diagnostic.
    Console { command: String },
}

/// One hotkey of a mod's `keybinds.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Keybind {
    pub id: String,
    pub desc: String,
    pub action: HotkeyAction,
}

/// Parse a mod's `keybinds.json`.
pub fn parse_keybinds(bytes: &[u8]) -> Result<Vec<Keybind>, String> {
    let root = json(bytes)?;
    let list = root.get("keybinds").and_then(Value::as_array).ok_or("no 'keybinds' array")?;
    list.iter()
        .map(|entry| {
            let entry = entry.as_object().ok_or("a keybind is not an object")?;
            let id = entry.get("id").and_then(Value::as_str).ok_or("a keybind has no 'id'")?.to_owned();
            let desc = entry.get("desc").and_then(Value::as_str).unwrap_or_default().to_owned();
            let action = entry.get("action").ok_or_else(|| format!("keybind '{id}' has no 'action'"))?;
            Ok(Keybind { action: hotkey_action(action).map_err(|error| format!("keybind '{id}': {error}"))?, id, desc })
        })
        .collect()
}

fn hotkey_action(value: &Value) -> Result<HotkeyAction, String> {
    let object = value.as_object().ok_or("its 'action' is not an object")?;
    let string = |key: &str| -> Option<String> { object.get(key).and_then(Value::as_str).map(str::to_owned) };
    match string("type").as_deref() {
        Some("SendEvent") => Ok(HotkeyAction::SendEvent {
            form: FormSpec::parse(&string("form").ok_or("SendEvent without a 'form'")?)?,
            script: string("scriptName"),
        }),
        Some("RunConsoleCommand") => Ok(HotkeyAction::Console { command: string("command").ok_or("RunConsoleCommand without a 'command'")? }),
        _ => config::action(value).map(HotkeyAction::Call),
    }
}

fn json(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).map_err(|_| "not UTF-8".to_owned())?;
    serde_json::from_str(text).map_err(|error| format!("JSON: {error}"))
}

/// One binding: the key bound to a mod's hotkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub mod_name: String,
    pub id: String,
    pub keycode: i32,
}

/// Parse the bindings file. A malformed file or entry is skipped (it is the player's file, not a mod's).
pub fn parse_registrations(bytes: &[u8]) -> Vec<Registration> {
    let Ok(root) = json(bytes) else { return Vec::new() };
    root.get("keybinds")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|entry| {
                    Some(Registration {
                        keycode: entry.get("keycode")?.as_i64()? as i32,
                        mod_name: entry.get("modName")?.as_str()?.to_owned(),
                        id: entry.get("id")?.as_str()?.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The bindings file MCM Helper writes (`CommitKeybinds`), pretty-printed with two-space indents.
pub fn registrations_text(registrations: &[Registration]) -> String {
    let keybinds: Vec<Value> = registrations
        .iter()
        .map(|r| serde_json::json!({ "keycode": r.keycode, "modName": r.mod_name, "id": r.id }))
        .collect();
    let root = serde_json::json!({ "version": super::VERSION_CODE, "keybinds": keybinds });
    let mut text = serde_json::to_string_pretty(&root).unwrap_or_default();
    text.push('\n');
    text
}

/// Every mod's bindings: the earlier install's file under the profile's, a binding of the same mod and
/// id in the latter replacing the former's.
#[derive(Debug, Clone, Default)]
pub struct Registrations(Vec<Registration>);

impl Registrations {
    pub fn layered(legacy: Vec<Registration>, profile: Vec<Registration>) -> Self {
        let mut registrations = Self(Vec::new());
        for registration in legacy.into_iter().chain(profile) {
            registrations.set(registration);
        }
        registrations
    }

    fn position(&self, mod_name: &str, id: &str) -> Option<usize> {
        self.0.iter().position(|r| r.mod_name.eq_ignore_ascii_case(mod_name) && r.id == id)
    }

    fn set(&mut self, registration: Registration) {
        match self.position(&registration.mod_name, &registration.id) {
            Some(index) => self.0[index] = registration,
            None => self.0.push(registration),
        }
    }

    /// The key bound to `mod_name`'s hotkey `id`; `-1` (SkyUI's unmapped) when none is.
    pub fn key(&self, mod_name: &str, id: &str) -> i32 {
        self.position(mod_name, id).map_or(-1, |index| self.0[index].keycode)
    }

    /// Bind `keycode` to the hotkey, or unbind it for an unmapped key (`-1`, or any code below 1). Returns
    /// whether anything changed.
    pub fn bind(&mut self, mod_name: &str, id: &str, keycode: i32) -> bool {
        if self.key(mod_name, id) == keycode.max(-1) {
            return false;
        }
        match (self.position(mod_name, id), keycode > 0) {
            (Some(index), false) => {
                self.0.remove(index);
            }
            (None, false) => return false,
            (_, true) => self.set(Registration { mod_name: mod_name.to_owned(), id: id.to_owned(), keycode }),
        }
        true
    }

    /// The hotkeys bound to `keycode`, as `(mod, id)`.
    pub fn bound_to(&self, keycode: i32) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.0.iter().filter(move |r| r.keycode == keycode).map(|r| (r.mod_name.as_str(), r.id.as_str()))
    }

    pub fn all(&self) -> &[Registration] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcm::helper::config::{ActionTarget, Param};

    #[test]
    fn a_keybinds_file_parses_each_action_kind() {
        let parsed = parse_keybinds(
            br#"{ "modName": "Frost", "keybinds": [
                { "id": "warm", "desc": "$WarmUp", "action": { "type": "CallFunction", "function": "WarmUp", "params": [3] } },
                { "id": "map", "action": { "type": "CallGlobalFunction", "script": "FrostUtil", "function": "ShowMap" } },
                { "id": "sprint", "action": { "type": "SendEvent", "form": "Frost.esp|800", "scriptName": "FrostInput" } },
                { "id": "tgm", "action": { "type": "RunConsoleCommand", "command": "tgm" } }
            ] }"#,
        )
        .expect("parses");
        assert_eq!(parsed.len(), 4);
        assert_eq!((parsed[0].id.as_str(), parsed[0].desc.as_str()), ("warm", "$WarmUp"));
        assert!(matches!(&parsed[0].action, HotkeyAction::Call(Action { target: ActionTarget::Method { form: None, .. }, params, .. }) if params == &[Param::Int(3)]));
        assert!(matches!(&parsed[1].action, HotkeyAction::Call(Action { target: ActionTarget::Global { script }, .. }) if script == "FrostUtil"));
        assert_eq!(parsed[2].action, HotkeyAction::SendEvent { form: FormSpec { plugin: "Frost.esp".into(), local: 0x800 }, script: Some("FrostInput".into()) });
        assert_eq!(parsed[3].action, HotkeyAction::Console { command: "tgm".into() });
        for bad in [&b"{}"[..], br#"{ "keybinds": [ { "action": { "type": "SendEvent" } } ] }"#, br#"{ "keybinds": [ { "id": "x" } ] }"#] {
            assert!(parse_keybinds(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn bindings_layer_and_round_trip_through_the_file() {
        let legacy = parse_registrations(br#"{ "version": 15, "keybinds": [ { "keycode": 34, "modName": "Frost", "id": "warm" }, { "keycode": 35, "modName": "Frost", "id": "map" } ] }"#);
        let profile = parse_registrations(br#"{ "keybinds": [ { "keycode": 48, "modName": "frost", "id": "warm" }, { "bad": 1 } ] }"#);
        let mut registrations = Registrations::layered(legacy, profile);
        assert_eq!(registrations.key("Frost", "warm"), 48, "the profile's binding over the earlier install's");
        assert_eq!(registrations.key("Frost", "map"), 35);
        assert_eq!(registrations.key("Frost", "none"), -1);
        assert!(registrations.bind("Frost", "map", 36));
        assert!(!registrations.bind("Frost", "map", 36), "unchanged");
        assert!(registrations.bind("Frost", "warm", -1), "unmapped");
        assert!(!registrations.bind("Frost", "warm", -1));
        assert_eq!(registrations.bound_to(36).collect::<Vec<_>>(), [("Frost", "map")]);
        let text = registrations_text(registrations.all());
        assert!(text.contains("\"version\": 15"), "{text}");
        assert_eq!(parse_registrations(text.as_bytes()), registrations.all());
    }
}
