//! PapyrusUtil's `JsonUtil` (LMC PRD Q5, LMC-SCR-004; `docs/engine/json-util.md`): mod-owned JSON files a
//! script reads and writes by name, served by the engine without the PapyrusUtil DLL.
//!
//! **The contract** (PapyrusUtil `Scripts/Source/JsonUtil.psc` and `StorageUtil.psc`,
//! `eeveelo/PapyrusUtil@ff854180db67`):
//!
//! - A file is named relative to `Data/SKSE/Plugins/StorageUtilData/`; `.json` is appended unless the name
//!   holds it, and a folder path may climb (`"../MyData/config"` is `Data/SKSE/Plugins/MyData/config.json`).
//! - "You do not need to call Load() or Save() manually": a file is read on first use; "when the player saves
//!   their game any modified file will be automatically saved"; loading another save without saving
//!   discards the changes ("revert back to whatever the contents of the current saved file are").
//! - Values are separate per type, and key names are not case sensitive (`StorageUtil.psc`).
//!
//! **The file format** is the one PapyrusUtil writes (`External.h` / `External.cpp`, `Forms.cpp`
//! `GetFormString`), so files a mod ships, or an earlier PapyrusUtil install wrote, read as they did: one
//! JSON object with the sections `int`, `float`, `string`, `form`, `intList`, `floatList`, `stringList`
//! and `formList`, each mapping a lowercased key to its value or list; a form is the string
//! `"0x<local id, hex>|<plugin file>"`, and `"0"` is None.
//!
//! **Confinement** (Q5, SEC-003): a name resolves inside `SKSE/Plugins/` and never above it; the engine
//! writes only to the profile's own directory ([`JsonUtilFiles`]), reading a mod root's shipped copy of a
//! file the profile has not written. A file is at most [`MAX_FILE_BYTES`], and a session keeps at most
//! [`MAX_OPEN_FILES`] files open.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::resource::Resource;
use serde_json::{Map, Value as Json};

/// The largest file read or written, in bytes (Q5's bound; an engine rule).
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
/// The most files a session keeps open at once (an engine rule).
pub const MAX_OPEN_FILES: usize = 256;
/// `ListResize`'s longest list (`StorageUtil.psc`: "Max length when using this function is 500").
pub const MAX_RESIZE: usize = 500;
/// The default folder a name is relative to, under `SKSE/Plugins/`.
const STORAGE_DIR: &str = "storageutildata";

/// The files under `SKSE/Plugins/`, by a resolved path ([`resolve`]): the profile's own first, then the
/// mod roots'.
pub trait JsonUtilFiles: Send + Sync {
    /// The file's bytes: the profile's copy, else the highest mod root's.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// Replace the profile's copy.
    fn write(&self, path: &str, text: &str) -> Result<(), String>;
    /// The `.json` files directly in `folder` (a resolved folder path), by file name, each once.
    fn list(&self, folder: &str) -> Vec<String>;
}

/// The value types, each its own section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Int,
    Float,
    String,
    Form,
}

impl Kind {
    pub fn section(self) -> &'static str {
        match self {
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::String => "string",
            Kind::Form => "form",
        }
    }

    pub fn list_section(self) -> &'static str {
        match self {
            Kind::Int => "intList",
            Kind::Float => "floatList",
            Kind::String => "stringList",
            Kind::Form => "formList",
        }
    }
}

/// Resolve a script's file name to a path under `SKSE/Plugins/` (lowercase, `/`-separated), or `None` when
/// it would leave that folder. An empty name is `noname.json`, as PapyrusUtil names it.
pub fn resolve(name: &str) -> Option<String> {
    let name = if name.trim().is_empty() { "noname.json".to_owned() } else { name.to_owned() };
    let name = if name.to_ascii_lowercase().contains(".json") { name } else { format!("{name}.json") };
    resolve_folder(&name)
}

/// Resolve a folder (or file) path relative to `StorageUtilData/`, without appending `.json`.
pub fn resolve_folder(path: &str) -> Option<String> {
    let mut parts: Vec<String> = vec![STORAGE_DIR.to_owned()];
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ if part.contains(':') || part.chars().any(char::is_control) => return None,
            _ => parts.push(part.to_ascii_lowercase()),
        }
    }
    Some(parts.join("/"))
}

/// One open file.
#[derive(Debug, Default, Clone)]
struct JsonFile {
    root: Map<String, Json>,
    modified: bool,
    /// The parser's complaint about the file read, empty when it read cleanly (`GetErrors`).
    errors: String,
}

/// The session's open `JsonUtil` files. `NOT_SAVED_BY_DESIGN`: the files are the state, written when the
/// game saves.
#[derive(Default)]
pub struct JsonUtilStore {
    files: Option<Arc<dyn JsonUtilFiles>>,
    open: HashMap<String, JsonFile>,
}

impl Resource for JsonUtilStore {}

impl JsonUtilStore {
    pub fn new(files: Arc<dyn JsonUtilFiles>) -> Self {
        Self { files: Some(files), open: HashMap::new() }
    }

    /// The open file at the resolved `path`, read on first use; `None` past [`MAX_OPEN_FILES`].
    fn file(&mut self, path: &str) -> Option<&mut JsonFile> {
        if !self.open.contains_key(path) {
            if self.open.len() >= MAX_OPEN_FILES {
                log::warn!("JsonUtil: {path} not opened: {MAX_OPEN_FILES} files are already open");
                return None;
            }
            let file = match self.files.as_ref().and_then(|files| files.read(path)) {
                None => JsonFile::default(),
                Some(bytes) if bytes.len() > MAX_FILE_BYTES => {
                    JsonFile { errors: format!("larger than {MAX_FILE_BYTES} bytes"), ..JsonFile::default() }
                }
                Some(bytes) => match serde_json::from_slice::<Json>(&bytes) {
                    Ok(Json::Object(root)) => JsonFile { root, ..JsonFile::default() },
                    Ok(_) => JsonFile { errors: "not a JSON object".to_owned(), ..JsonFile::default() },
                    Err(error) => JsonFile { errors: error.to_string(), ..JsonFile::default() },
                },
            };
            if !file.errors.is_empty() {
                log::warn!("JsonUtil: {path}: {}", file.errors);
            }
            self.open.insert(path.to_owned(), file);
        }
        self.open.get_mut(path)
    }

    /// `Load`: open the file; whether it read cleanly.
    pub fn load(&mut self, path: &str) -> bool {
        self.file(path).is_some_and(|file| file.errors.is_empty())
    }

    /// `IsGood`: the file is open and read cleanly.
    pub fn is_good(&self, path: &str) -> bool {
        self.open.get(path).is_some_and(|file| file.errors.is_empty())
    }

    /// `GetErrors`.
    pub fn errors(&self, path: &str) -> String {
        self.open.get(path).map(|file| file.errors.clone()).unwrap_or_default()
    }

    /// `IsPendingSave`.
    pub fn is_pending_save(&self, path: &str) -> bool {
        self.open.get(path).is_some_and(|file| file.modified)
    }

    /// `Save`: write the file now; whether it was written.
    pub fn save(&mut self, path: &str, minify: bool) -> bool {
        let Some(files) = self.files.clone() else {
            return false;
        };
        let Some(file) = self.open.get_mut(path) else {
            return false;
        };
        let root = Json::Object(file.root.clone());
        let text = if minify { serde_json::to_string(&root) } else { serde_json::to_string_pretty(&root) };
        let Ok(text) = text else {
            return false;
        };
        if text.len() > MAX_FILE_BYTES {
            log::warn!("JsonUtil: {path} not written: {} bytes is over the {MAX_FILE_BYTES}-byte bound", text.len());
            return false;
        }
        match files.write(path, &text) {
            Ok(()) => {
                file.modified = false;
                true
            }
            Err(error) => {
                log::warn!("JsonUtil: {path} not written: {error}");
                false
            }
        }
    }

    /// `Unload`: close the file, writing it first when asked and changed.
    pub fn unload(&mut self, path: &str, save_changes: bool, minify: bool) -> bool {
        if !self.open.contains_key(path) {
            return false;
        }
        if save_changes && self.is_pending_save(path) && !self.save(path, minify) {
            return false;
        }
        self.open.remove(path).is_some()
    }

    /// Write every changed file (the game saved). Returns how many were written.
    pub fn save_modified(&mut self) -> usize {
        let pending: Vec<String> = self.open.iter().filter(|(_, file)| file.modified).map(|(path, _)| path.clone()).collect();
        pending.into_iter().filter(|path| self.save(path, false)).count()
    }

    /// Drop every open file and its unsaved changes (a save was loaded).
    pub fn discard(&mut self) {
        self.open.clear();
    }

    /// `JsonInFolder`: the `.json` files in a folder.
    pub fn json_in_folder(&self, folder: &str) -> Vec<String> {
        self.files.as_ref().map(|files| files.list(folder)).unwrap_or_default()
    }

    /// `JsonExists`: the file exists, written by the profile or shipped by a mod.
    pub fn exists(&self, path: &str) -> bool {
        self.files.as_ref().is_some_and(|files| files.read(path).is_some())
    }

    /// The value of `key` in `kind`'s section.
    pub fn get(&mut self, path: &str, kind: Kind, key: &str) -> Option<Json> {
        self.file(path)?.root.get(kind.section())?.get(&key.to_ascii_lowercase()).cloned()
    }

    /// Set `key` in `kind`'s section.
    pub fn set(&mut self, path: &str, kind: Kind, key: &str, value: Json) {
        if let Some(file) = self.file(path) {
            section(&mut file.root, kind.section()).insert(key.to_ascii_lowercase(), value);
            file.modified = true;
        }
    }

    /// Remove `key` from `kind`'s section; whether it was there.
    pub fn unset(&mut self, path: &str, kind: Kind, key: &str) -> bool {
        let Some(file) = self.file(path) else {
            return false;
        };
        let removed = file
            .root
            .get_mut(kind.section())
            .and_then(Json::as_object_mut)
            .and_then(|section| section.remove(&key.to_ascii_lowercase()))
            .is_some();
        file.modified |= removed;
        removed
    }

    /// `key`'s list in `kind`'s list section, if any.
    pub fn list(&mut self, path: &str, kind: Kind, key: &str) -> Option<Vec<Json>> {
        self.file(path)?.root.get(kind.list_section())?.get(&key.to_ascii_lowercase())?.as_array().cloned()
    }

    /// Edit `key`'s list in `kind`'s list section, creating it; the edit returns whether it changed the list.
    pub fn edit_list<R>(&mut self, path: &str, kind: Kind, key: &str, edit: impl FnOnce(&mut Vec<Json>) -> (R, bool)) -> Option<R> {
        let file = self.file(path)?;
        let section = section(&mut file.root, kind.list_section());
        let key = key.to_ascii_lowercase();
        let mut list = section.get(&key).and_then(Json::as_array).cloned().unwrap_or_default();
        let (result, changed) = edit(&mut list);
        if changed {
            if list.is_empty() {
                section.remove(&key);
            } else {
                section.insert(key, Json::Array(list));
            }
            file.modified = true;
        }
        Some(result)
    }

    /// Remove `key`'s list; its length before (`ListClear`).
    pub fn clear_list(&mut self, path: &str, kind: Kind, key: &str) -> usize {
        self.edit_list(path, kind, key, |list| {
            let count = list.len();
            list.clear();
            (count, count > 0)
        })
        .unwrap_or(0)
    }

    /// How many keys of `kind` (a list section when `list`) start with `prefix` (`Count*Prefix`).
    pub fn count_prefix(&mut self, path: &str, kind: Kind, list: bool, prefix: &str) -> usize {
        if prefix.is_empty() {
            return 0;
        }
        let prefix = prefix.to_ascii_lowercase();
        let name = if list { kind.list_section() } else { kind.section() };
        self.file(path)
            .and_then(|file| file.root.get(name))
            .and_then(Json::as_object)
            .map_or(0, |section| section.keys().filter(|key| key.starts_with(&prefix)).count())
    }

    /// `ClearAll`: empty the file.
    pub fn clear_all(&mut self, path: &str) {
        if let Some(file) = self.file(path) {
            file.modified |= !file.root.is_empty();
            file.root.clear();
        }
    }
}

/// `name`'s section of `root` as an object, replacing a malformed one.
fn section<'a>(root: &'a mut Map<String, Json>, name: &str) -> &'a mut Map<String, Json> {
    let entry = root.entry(name.to_owned()).or_insert_with(|| Json::Object(Map::new()));
    if !entry.is_object() {
        *entry = Json::Object(Map::new());
    }
    entry.as_object_mut().expect("just made an object")
}

/// A form as PapyrusUtil writes it: `0x<local id>|<plugin file>`, `0` for None (`Forms.cpp`
/// `GetFormString`).
pub fn form_string(local: u32, plugin: &str) -> String {
    format!("0x{local:x}|{plugin}")
}

/// A form string's plugin file and local id (`Forms.cpp` `ParseFormString`: a hex id after `0x`, else
/// decimal; the plugin must end `.esp`, `.esm` or `.esl`). A temporary form (`…|<type>.FF`) has no stable
/// identity outside the session that wrote it and is `None`.
pub fn parse_form_string(text: &str) -> Option<(u32, &str)> {
    let (id, plugin) = text.split_once('|')?;
    let lower = plugin.to_ascii_lowercase();
    if !(lower.ends_with(".esp") || lower.ends_with(".esm") || lower.ends_with(".esl")) {
        return None;
    }
    let local = match id.strip_prefix("0x").or_else(|| id.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => id.parse().ok()?,
    };
    Some((local, plugin))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Files in memory: the profile's (written) over a mod's (shipped).
    #[derive(Default)]
    pub(crate) struct MemoryJsonFiles {
        pub(crate) profile: Mutex<HashMap<String, String>>,
        pub(crate) shipped: HashMap<String, String>,
    }

    impl JsonUtilFiles for MemoryJsonFiles {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.profile.lock().unwrap().get(path).or_else(|| self.shipped.get(path)).map(|text| text.clone().into_bytes())
        }
        fn write(&self, path: &str, text: &str) -> Result<(), String> {
            self.profile.lock().unwrap().insert(path.to_owned(), text.to_owned());
            Ok(())
        }
        fn list(&self, folder: &str) -> Vec<String> {
            let prefix = format!("{folder}/");
            let mut names: Vec<String> = self
                .profile
                .lock()
                .unwrap()
                .keys()
                .chain(self.shipped.keys())
                .filter_map(|path| path.strip_prefix(&prefix))
                .filter(|rest| !rest.contains('/') && rest.ends_with(".json"))
                .map(str::to_owned)
                .collect();
            names.sort();
            names.dedup();
            names
        }
    }

    /// PapyrusUtil's naming: `.json` appended, folders climbable inside `SKSE/Plugins/` only.
    #[test]
    fn names_resolve_inside_the_plugins_folder() {
        assert_eq!(resolve("MyMod/Config").as_deref(), Some("storageutildata/mymod/config.json"));
        assert_eq!(resolve("settings.json").as_deref(), Some("storageutildata/settings.json"));
        assert_eq!(resolve("../StorageUtil.json").as_deref(), Some("storageutil.json"));
        assert_eq!(resolve("../MyData/config").as_deref(), Some("mydata/config.json"));
        assert_eq!(resolve("").as_deref(), Some("storageutildata/noname.json"));
        assert_eq!(resolve("../../escape"), None, "never above SKSE/Plugins");
        assert_eq!(resolve("C:/Windows/x"), None);
    }

    /// A shipped file reads in PapyrusUtil's format; a change waits for the game's save and then goes to the
    /// profile, never the mod; a load discards an unsaved change.
    #[test]
    fn files_read_shipped_write_profile_and_follow_the_save() {
        let files = Arc::new(MemoryJsonFiles {
            shipped: HashMap::from([(
                "storageutildata/mymod.json".to_owned(),
                r#"{ "int": { "difficulty": 3 }, "form": { "item": "0x12eb7|Skyrim.esm" }, "intList": { "levels": [1, 2] } }"#.to_owned(),
            )]),
            ..Default::default()
        });
        let mut store = JsonUtilStore::new(files.clone());
        let path = resolve("MyMod").unwrap();
        assert_eq!(store.get(&path, Kind::Int, "Difficulty"), Some(Json::from(3)), "keys are not case sensitive");
        assert_eq!(store.get(&path, Kind::Form, "item").and_then(|v| v.as_str().map(str::to_owned)), Some("0x12eb7|Skyrim.esm".to_owned()));
        assert_eq!(store.list(&path, Kind::Int, "levels"), Some(vec![Json::from(1), Json::from(2)]));
        assert!(store.is_good(&path));

        store.set(&path, Kind::Int, "Difficulty", Json::from(5));
        assert!(store.is_pending_save(&path));
        assert!(files.profile.lock().unwrap().is_empty(), "nothing written before the game saves");
        assert_eq!(store.save_modified(), 1);
        let written: Json = serde_json::from_str(&files.profile.lock().unwrap()[&path]).unwrap();
        assert_eq!(written["int"]["difficulty"], Json::from(5));
        assert_eq!(written["intList"]["levels"], serde_json::json!([1, 2]), "the shipped content is kept");

        store.set(&path, Kind::Int, "difficulty", Json::from(9));
        store.discard();
        assert_eq!(store.get(&path, Kind::Int, "difficulty"), Some(Json::from(5)), "a load reverts to the written file");
    }

    #[test]
    fn a_broken_file_reports_its_errors_and_forms_parse_as_written() {
        let files = Arc::new(MemoryJsonFiles {
            shipped: HashMap::from([("storageutildata/broken.json".to_owned(), "{ not json".to_owned())]),
            ..Default::default()
        });
        let mut store = JsonUtilStore::new(files);
        let path = resolve("broken").unwrap();
        assert!(!store.load(&path));
        assert!(!store.errors(&path).is_empty());
        assert_eq!(parse_form_string("0x12eb7|Skyrim.esm"), Some((0x12eb7, "Skyrim.esm")));
        assert_eq!(parse_form_string("77495|Update.esm"), Some((77495, "Update.esm")));
        assert_eq!(parse_form_string("0x800|43.FF"), None, "a temporary form has no identity");
        assert_eq!(parse_form_string("0"), None);
        assert_eq!(form_string(0x12eb7, "Skyrim.esm"), "0x12eb7|Skyrim.esm");
    }
}
