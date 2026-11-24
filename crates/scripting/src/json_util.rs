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

/// The files under `Data/`, by a resolved path ([`resolve`], [`resolve_data_path`]; lowercase, `/`-separated): the
/// profile's own first, then the mod roots'.
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

/// Resolve a folder (or file) path relative to `StorageUtilData/`, without appending `.json`: a path under `Data/`
/// that never leaves `SKSE/Plugins/`.
pub fn resolve_folder(path: &str) -> Option<String> {
    let mut parts: Vec<String> = ["skse", "plugins", STORAGE_DIR].map(str::to_owned).to_vec();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." if parts.len() > 2 => {
                parts.pop();
            }
            ".." => return None,
            _ if part.contains(':') || part.chars().any(char::is_control) => return None,
            _ => parts.push(part.to_ascii_lowercase()),
        }
    }
    Some(parts.join("/"))
}

/// Resolve a path a script names relative to the game folder (JContainers' `readFromFile`, `writeToFile`) to a path
/// under `Data/`: a leading `Data/` is dropped, and a path that leaves `Data/` (or names a drive) is `None`.
pub fn resolve_data_path(path: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (i, part) in path.split(['/', '\\']).enumerate() {
        match part {
            "" | "." => {}
            _ if i == 0 && part.eq_ignore_ascii_case("data") => {}
            ".." => {
                parts.pop()?;
            }
            _ if part.contains(':') || part.chars().any(char::is_control) => return None,
            _ => parts.push(part.to_ascii_lowercase()),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// One segment of a JSON path (jsoncpp `Json::Path`, which `JsonUtil`'s path functions use: `.member` and
/// `[index]`, a leading `.` implied).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Member(String),
    Index(usize),
}

/// A path's segments; `None` for an empty or malformed path, `Some(vec![])` for the root (`.`).
fn parse_path(path: &str) -> Option<Vec<Segment>> {
    if path.is_empty() {
        return None;
    }
    let path = if path.starts_with('.') || path.starts_with('[') { path.to_owned() } else { format!(".{path}") };
    let mut segments = Vec::new();
    let mut rest = path.as_str();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('[') {
            let (index, tail) = after.split_once(']')?;
            segments.push(Segment::Index(index.trim().parse().ok()?));
            rest = tail;
        } else if let Some(after) = rest.strip_prefix('.') {
            let end = after.find(['.', '[']).unwrap_or(after.len());
            if end > 0 {
                segments.push(Segment::Member(after[..end].to_owned()));
            }
            rest = &after[end..];
        } else {
            return None;
        }
    }
    Some(segments)
}

/// One document of typed values: a `JsonUtil` file, or one `StorageUtil` object's values. Its sections are
/// PapyrusUtil's (`int`, `float`, `string`, `form` and their `…List`s), keys lowercased; the path functions
/// see the whole document.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueDoc {
    root: Json,
    /// Changed since it was read or last written.
    #[cfg_attr(feature = "save", serde(skip))]
    dirty: bool,
}

impl Default for ValueDoc {
    fn default() -> Self {
        Self { root: Json::Object(Map::new()), dirty: false }
    }
}

impl ValueDoc {
    fn from_root(root: Map<String, Json>) -> Self {
        Self { root: Json::Object(root), dirty: false }
    }

    fn root(&self) -> &Map<String, Json> {
        self.root.as_object().expect("a document's root is an object")
    }

    fn root_mut(&mut self) -> &mut Map<String, Json> {
        if !self.root.is_object() {
            self.root = Json::Object(Map::new());
        }
        self.root.as_object_mut().expect("just made an object")
    }

    pub fn is_empty(&self) -> bool {
        self.root().values().all(|section| section.as_object().is_none_or(Map::is_empty))
    }

    /// The value of `key` in `kind`'s section.
    pub fn get(&self, kind: Kind, key: &str) -> Option<Json> {
        self.root().get(kind.section())?.get(&key.to_ascii_lowercase()).cloned()
    }

    /// Set `key` in `kind`'s section.
    pub fn set(&mut self, kind: Kind, key: &str, value: Json) {
        section_of(self.root_mut(), kind.section()).insert(key.to_ascii_lowercase(), value);
        self.dirty = true;
    }

    /// Remove `key` from `kind`'s section, returning what it held.
    pub fn take(&mut self, kind: Kind, key: &str) -> Option<Json> {
        let taken = self
            .root_mut()
            .get_mut(kind.section())
            .and_then(Json::as_object_mut)
            .and_then(|section| section.remove(&key.to_ascii_lowercase()));
        self.dirty |= taken.is_some();
        taken
    }

    /// `key`'s list in `kind`'s list section, if any.
    pub fn list(&self, kind: Kind, key: &str) -> Option<Vec<Json>> {
        self.root().get(kind.list_section())?.get(&key.to_ascii_lowercase())?.as_array().cloned()
    }

    /// Edit `key`'s list, creating it; the edit returns whether it changed the list. An emptied list is
    /// removed.
    pub fn edit_list<R>(&mut self, kind: Kind, key: &str, edit: impl FnOnce(&mut Vec<Json>) -> (R, bool)) -> R {
        let key = key.to_ascii_lowercase();
        let section = section_of(self.root_mut(), kind.list_section());
        let mut list = section.get(&key).and_then(Json::as_array).cloned().unwrap_or_default();
        let (result, changed) = edit(&mut list);
        if changed {
            if list.is_empty() {
                section.remove(&key);
            } else {
                section.insert(key, Json::Array(list));
            }
            self.dirty = true;
        }
        result
    }

    /// The keys of `kind`'s section (its list section when `list`).
    pub fn keys(&self, kind: Kind, list: bool) -> Vec<String> {
        let name = if list { kind.list_section() } else { kind.section() };
        self.root().get(name).and_then(Json::as_object).map(|section| section.keys().cloned().collect()).unwrap_or_default()
    }

    /// How many keys of `kind` (its list section when `list`) start with `prefix`; none for an empty prefix.
    pub fn count_prefix(&self, kind: Kind, list: bool, prefix: &str) -> usize {
        if prefix.is_empty() {
            return 0;
        }
        let prefix = prefix.to_ascii_lowercase();
        self.keys(kind, list).iter().filter(|key| key.starts_with(&prefix)).count()
    }

    /// Remove the keys of `kind` (its list section when `list`) that start with `prefix`; how many.
    pub fn clear_prefix(&mut self, kind: Kind, list: bool, prefix: &str) -> usize {
        if prefix.is_empty() {
            return 0;
        }
        let prefix = prefix.to_ascii_lowercase();
        let name = if list { kind.list_section() } else { kind.section() };
        let Some(section) = self.root_mut().get_mut(name).and_then(Json::as_object_mut) else {
            return 0;
        };
        let before = section.len();
        section.retain(|key, _| !key.starts_with(&prefix));
        let removed = before - section.len();
        self.dirty |= removed > 0;
        removed
    }

    /// Empty the document.
    pub fn clear_all(&mut self) {
        self.dirty |= !self.root().is_empty();
        self.root_mut().clear();
    }

    /// The value at a JSON path, if it resolves (`Null` reads as unresolved, as jsoncpp's does).
    pub fn path(&self, path: &str) -> Option<&Json> {
        let mut node = &self.root;
        for segment in parse_path(path)? {
            node = match segment {
                Segment::Member(name) => node.as_object()?.get(&name)?,
                Segment::Index(index) => node.as_array()?.get(index)?,
            };
        }
        (!node.is_null()).then_some(node)
    }

    /// Set the value at a JSON path, creating the path as needed ("The path will be created as necessary").
    pub fn set_path(&mut self, path: &str, value: Json) -> bool {
        let Some(segments) = parse_path(path) else {
            return false;
        };
        let mut node = &mut self.root;
        for segment in segments {
            node = match segment {
                Segment::Member(name) => {
                    if !node.is_object() {
                        *node = Json::Object(Map::new());
                    }
                    node.as_object_mut().expect("just made an object").entry(name).or_insert(Json::Null)
                }
                Segment::Index(index) => {
                    if !node.is_array() {
                        *node = Json::Array(Vec::new());
                    }
                    let array = node.as_array_mut().expect("just made an array");
                    if array.len() <= index {
                        array.resize(index + 1, Json::Null);
                    }
                    &mut array[index]
                }
            };
        }
        *node = value;
        if !self.root.is_object() {
            self.root = Json::Object(Map::new());
        }
        self.dirty = true;
        true
    }

    /// Remove the value at a JSON path (`ClearPath`; `.` empties the document); whether one was removed.
    pub fn clear_path(&mut self, path: &str) -> bool {
        let Some(mut segments) = parse_path(path) else {
            return false;
        };
        let Some(last) = segments.pop() else {
            let had = !self.root().is_empty();
            self.clear_all();
            return had;
        };
        let mut node = &mut self.root;
        for segment in segments {
            node = match (segment, node) {
                (Segment::Member(name), Json::Object(map)) => match map.get_mut(&name) {
                    Some(next) => next,
                    None => return false,
                },
                (Segment::Index(index), Json::Array(array)) => match array.get_mut(index) {
                    Some(next) => next,
                    None => return false,
                },
                _ => return false,
            };
        }
        let removed = match (last, node) {
            (Segment::Member(name), Json::Object(map)) => map.remove(&name).is_some(),
            (Segment::Index(index), Json::Array(array)) if index < array.len() => {
                array.remove(index);
                true
            }
            _ => false,
        };
        self.dirty |= removed;
        removed
    }

    /// Remove element `index` of the array at a path (`ClearPathIndex`); whether one was removed.
    pub fn clear_path_index(&mut self, path: &str, index: usize) -> bool {
        let Some(Json::Array(mut array)) = self.path(path).cloned() else {
            return false;
        };
        if index >= array.len() {
            return false;
        }
        array.remove(index);
        self.set_path(path, Json::Array(array))
    }
}

/// One open file.
#[derive(Debug, Default, Clone)]
struct JsonFile {
    doc: ValueDoc,
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
            let failed = |errors: String| JsonFile { errors, ..JsonFile::default() };
            let file = match self.files.as_ref().and_then(|files| files.read(path)) {
                None => JsonFile::default(),
                Some(bytes) if bytes.len() > MAX_FILE_BYTES => failed(format!("larger than {MAX_FILE_BYTES} bytes")),
                Some(bytes) => match serde_json::from_slice::<Json>(&bytes) {
                    Ok(Json::Object(root)) => JsonFile { doc: ValueDoc::from_root(root), errors: String::new() },
                    Ok(_) => failed("not a JSON object".to_owned()),
                    Err(error) => failed(error.to_string()),
                },
            };
            if !file.errors.is_empty() {
                log::warn!("JsonUtil: {path}: {}", file.errors);
            }
            self.open.insert(path.to_owned(), file);
        }
        self.open.get_mut(path)
    }

    /// The document of the file at the resolved `path`, opened on first use.
    pub fn doc(&mut self, path: &str) -> Option<&mut ValueDoc> {
        self.file(path).map(|file| &mut file.doc)
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
        self.open.get(path).is_some_and(|file| file.doc.dirty)
    }

    /// `Save`: write the file now; whether it was written.
    pub fn save(&mut self, path: &str, minify: bool) -> bool {
        let Some(files) = self.files.clone() else {
            return false;
        };
        let Some(file) = self.open.get_mut(path) else {
            return false;
        };
        let text = if minify { serde_json::to_string(&file.doc.root) } else { serde_json::to_string_pretty(&file.doc.root) };
        let Ok(text) = text else {
            return false;
        };
        if text.len() > MAX_FILE_BYTES {
            log::warn!("JsonUtil: {path} not written: {} bytes is over the {MAX_FILE_BYTES}-byte bound", text.len());
            return false;
        }
        match files.write(path, &text) {
            Ok(()) => {
                file.doc.dirty = false;
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
        let pending: Vec<String> = self.open.iter().filter(|(_, file)| file.doc.dirty).map(|(path, _)| path.clone()).collect();
        pending.into_iter().filter(|path| self.save(path, false)).count()
    }

    /// Drop every open file and its unsaved changes (a save was loaded).
    pub fn discard(&mut self) {
        self.open.clear();
    }

    /// The mod data files, shared with JContainers' file functions.
    pub fn files(&self) -> Option<Arc<dyn JsonUtilFiles>> {
        self.files.clone()
    }

    /// `JsonInFolder`: the `.json` files in a folder.
    pub fn json_in_folder(&self, folder: &str) -> Vec<String> {
        self.files.as_ref().map(|files| files.list(folder)).unwrap_or_default()
    }

    /// `JsonExists`: the file exists, written by the profile or shipped by a mod.
    pub fn exists(&self, path: &str) -> bool {
        self.files.as_ref().is_some_and(|files| files.read(path).is_some())
    }
}

/// PapyrusUtil `StorageUtil`'s values set on a form or globally (`StorageUtil.psc`: "Values will stay on forms
/// or globally until they are Unset or Cleared"), one [`ValueDoc`] per object: keyed by the object's
/// [`form_string`] (`""` globally), so a save names its forms by plugin and id, not by load-order slot. Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageUtilValues {
    objects: std::collections::BTreeMap<String, ValueDoc>,
}

impl Resource for StorageUtilValues {}

impl StorageUtilValues {
    /// The values of the object `key`, created on first use.
    pub fn doc(&mut self, key: &str) -> &mut ValueDoc {
        self.objects.entry(key.to_owned()).or_default()
    }

    /// Drop the objects left with no value.
    pub fn prune(&mut self) {
        self.objects.retain(|_, doc| !doc.is_empty());
    }

    /// Drop the object `key` when it holds no value.
    pub fn prune_key(&mut self, key: &str) {
        if self.objects.get(key).is_some_and(ValueDoc::is_empty) {
            self.objects.remove(key);
        }
    }

    /// Every object's key and values.
    pub fn objects_mut(&mut self) -> impl Iterator<Item = (&String, &mut ValueDoc)> {
        self.objects.iter_mut()
    }

    /// The keys of the objects holding a value of `kind` (a list of it when `list`), in order.
    pub fn objects_with(&self, kind: Kind, list: bool) -> Vec<String> {
        self.objects.iter().filter(|(_, doc)| !doc.keys(kind, list).is_empty()).map(|(key, _)| key.clone()).collect()
    }

    /// Remove an object's values (`debug_DeleteValues`).
    pub fn delete(&mut self, key: &str) {
        self.objects.remove(key);
    }

    /// Remove every object's values (`debug_DeleteAllValues`).
    pub fn delete_all(&mut self) {
        self.objects.clear();
    }
}

/// `name`'s section of `root` as an object, replacing a malformed one.
fn section_of<'a>(root: &'a mut Map<String, Json>, name: &str) -> &'a mut Map<String, Json> {
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

/// A form outside the load order (a reference the session created): its session FormID, `0x<id>|FF`. Like
/// PapyrusUtil's temporary forms (`…|<type>.FF`) it names the form only in the session that wrote it.
pub fn session_form_string(form: u32) -> String {
    format!("0x{form:x}|FF")
}

/// The session FormID of a [`session_form_string`].
pub fn parse_session_form_string(text: &str) -> Option<u32> {
    let id = text.strip_suffix("|FF")?;
    u32::from_str_radix(id.strip_prefix("0x")?, 16).ok()
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

/// This subsystem's world setup: `StorageUtil`'s values (saved). `JsonUtilStore` is the engine's to install,
/// with its files.
pub fn install(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<StorageUtilValues>();
}

/// `StorageUtil`'s values ride the save; its keys and form values are plugin-and-id text, so a changed load
/// order still names the same forms.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<StorageUtilValues>("StorageUtilValues");
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
        assert_eq!(resolve("MyMod/Config").as_deref(), Some("skse/plugins/storageutildata/mymod/config.json"));
        assert_eq!(resolve("settings.json").as_deref(), Some("skse/plugins/storageutildata/settings.json"));
        assert_eq!(resolve("../StorageUtil.json").as_deref(), Some("skse/plugins/storageutil.json"));
        assert_eq!(resolve("../MyData/config").as_deref(), Some("skse/plugins/mydata/config.json"));
        assert_eq!(resolve("").as_deref(), Some("skse/plugins/storageutildata/noname.json"));
        assert_eq!(resolve_data_path("Data/SKSE/Plugins/JCData/x.json").as_deref(), Some("skse/plugins/jcdata/x.json"));
        assert_eq!(resolve_data_path("MyMod/presets.json").as_deref(), Some("mymod/presets.json"));
        assert_eq!(resolve_data_path("Data/../SkyrimSE.exe"), None, "never above Data");
        assert_eq!(resolve("../../escape"), None, "never above SKSE/Plugins");
        assert_eq!(resolve("C:/Windows/x"), None);
    }

    /// A shipped file reads in PapyrusUtil's format; a change waits for the game's save and then goes to the
    /// profile, never the mod; a load discards an unsaved change.
    #[test]
    fn files_read_shipped_write_profile_and_follow_the_save() {
        let files = Arc::new(MemoryJsonFiles {
            shipped: HashMap::from([(
                "skse/plugins/storageutildata/mymod.json".to_owned(),
                r#"{ "int": { "difficulty": 3 }, "form": { "item": "0x12eb7|Skyrim.esm" }, "intList": { "levels": [1, 2] } }"#.to_owned(),
            )]),
            ..Default::default()
        });
        let mut store = JsonUtilStore::new(files.clone());
        let path = resolve("MyMod").unwrap();
        let doc = store.doc(&path).unwrap();
        assert_eq!(doc.get(Kind::Int, "Difficulty"), Some(Json::from(3)), "keys are not case sensitive");
        assert_eq!(doc.get(Kind::Form, "item").and_then(|v| v.as_str().map(str::to_owned)), Some("0x12eb7|Skyrim.esm".to_owned()));
        assert_eq!(doc.list(Kind::Int, "levels"), Some(vec![Json::from(1), Json::from(2)]));
        assert!(store.is_good(&path));

        store.doc(&path).unwrap().set(Kind::Int, "Difficulty", Json::from(5));
        assert!(store.is_pending_save(&path));
        assert!(files.profile.lock().unwrap().is_empty(), "nothing written before the game saves");
        assert_eq!(store.save_modified(), 1);
        let written: Json = serde_json::from_str(&files.profile.lock().unwrap()[&path]).unwrap();
        assert_eq!(written["int"]["difficulty"], Json::from(5));
        assert_eq!(written["intList"]["levels"], serde_json::json!([1, 2]), "the shipped content is kept");

        store.doc(&path).unwrap().set(Kind::Int, "difficulty", Json::from(9));
        store.discard();
        assert_eq!(store.doc(&path).unwrap().get(Kind::Int, "difficulty"), Some(Json::from(5)), "a load reverts to the written file");
    }

    #[test]
    fn a_broken_file_reports_its_errors_and_forms_parse_as_written() {
        let files = Arc::new(MemoryJsonFiles {
            shipped: HashMap::from([("skse/plugins/storageutildata/broken.json".to_owned(), "{ not json".to_owned())]),
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

    /// `JsonUtil`'s path functions over jsoncpp paths: `.member` and `[index]`, created as needed on set.
    #[test]
    fn paths_resolve_set_and_clear() {
        let mut doc = ValueDoc::from_root(serde_json::from_str(r#"{ "foo": { "bar": [3, 10, 7] } }"#).unwrap());
        assert_eq!(doc.path(".foo.bar[1]"), Some(&Json::from(10)));
        assert_eq!(doc.path("foo.bar[1]"), Some(&Json::from(10)), "the leading dot is implied");
        assert_eq!(doc.path(".foo.missing"), None);
        assert!(doc.set_path(".a.b[2]", Json::from("x")));
        assert_eq!(doc.path(".a.b"), Some(&serde_json::json!([null, null, "x"])));
        assert!(doc.clear_path_index(".foo.bar", 0));
        assert_eq!(doc.path(".foo.bar"), Some(&serde_json::json!([10, 7])));
        assert!(doc.clear_path(".foo.bar"));
        assert_eq!(doc.path(".foo"), Some(&serde_json::json!({})));
        assert!(!doc.clear_path(".nothing.here"));
        assert!(doc.clear_path("."));
        assert!(doc.is_empty());
        assert_eq!(parse_path(""), None);
        assert_eq!(parse_path(".a[x]"), None);
    }

    /// StorageUtil's per-object values: one document per object key, pruned when emptied.
    #[test]
    fn storage_util_values_are_per_object() {
        let mut values = StorageUtilValues::default();
        values.doc("").set(Kind::Int, "Visits", Json::from(2));
        values.doc("0x14|Skyrim.esm").edit_list(Kind::Form, "Allies", |list| {
            list.push(Json::from("0x1a694|Skyrim.esm"));
            ((), true)
        });
        assert_eq!(values.objects_with(Kind::Int, false), vec!["".to_owned()]);
        assert_eq!(values.objects_with(Kind::Form, true), vec!["0x14|Skyrim.esm".to_owned()]);
        values.doc("").take(Kind::Int, "visits");
        values.prune();
        assert_eq!(values.objects_with(Kind::Int, false), Vec::<String>::new());
        values.delete_all();
        assert_eq!(values, StorageUtilValues::default());
    }
}
