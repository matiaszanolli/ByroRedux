//! `JsonUtil` and `StorageUtil.File*`: PapyrusUtil's file-backed values ([`crate::json_util`], LMC PRD Q5),
//! served by the engine. `StorageUtil`'s file functions are `JsonUtil` on `"../StorageUtil.json"`
//! (`StorageUtil.psc`: `FileSetIntValue(KeyName, value)` returns `JsonUtil.SetIntValue("../StorageUtil.json",
//! KeyName, value)`); its form-keyed values are save state and are not served here.
//!
//! The ops follow `StorageUtil.psc`'s documentation, which `JsonUtil.psc` points to ("These functions all
//! work in exactly the same way as their StorageUtil.psc equivalents"). The experimental path functions
//! (`SetPath*` / `GetPath*` …) and `*ListRandom` are not served and stop.

use super::*;
use crate::json_util::{self, Kind as JsonKind, JsonUtilStore, MAX_RESIZE};
use serde_json::Value as Json;

/// The file `StorageUtil`'s `File*` functions use.
const STORAGE_UTIL_FILE: &str = "../StorageUtil.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListOp {
    Add,
    Get,
    Set,
    Remove,
    InsertAt,
    RemoveAt,
    Clear,
    Count,
    CountValue,
    Find,
    Has,
    ToArray,
    Resize,
    Adjust,
    Slice,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Load,
    Save,
    Unload,
    IsPendingSave,
    IsGood,
    GetErrors,
    JsonInFolder,
    JsonExists,
    ClearAll,
    Set(JsonKind),
    Get(JsonKind),
    Unset(JsonKind),
    Has(JsonKind),
    Adjust(JsonKind),
    List(JsonKind, ListOp),
    CountPrefix(JsonKind, bool),
    CountAllPrefix,
}

/// `rest` without its leading value-type word, and that type.
fn split_kind(rest: &str) -> Option<(JsonKind, &str)> {
    [("string", JsonKind::String), ("float", JsonKind::Float), ("form", JsonKind::Form), ("int", JsonKind::Int)]
        .into_iter()
        .find_map(|(word, kind)| Some((kind, rest.strip_prefix(word)?)))
}

/// The op a lowercase `JsonUtil` function name names.
fn parse_op(name: &str) -> Option<Op> {
    let whole = match name {
        "load" => Some(Op::Load),
        "save" => Some(Op::Save),
        "unload" => Some(Op::Unload),
        "ispendingsave" => Some(Op::IsPendingSave),
        "isgood" => Some(Op::IsGood),
        "geterrors" => Some(Op::GetErrors),
        "jsoninfolder" => Some(Op::JsonInFolder),
        "jsonexists" => Some(Op::JsonExists),
        "clearall" => Some(Op::ClearAll),
        "countallprefix" => Some(Op::CountAllPrefix),
        _ => None,
    };
    if whole.is_some() {
        return whole;
    }
    // `Count<Kind>ValuePrefix` / `Count<Kind>ListPrefix`.
    if let Some(rest) = name.strip_prefix("count").and_then(|rest| rest.strip_suffix("prefix")) {
        let (kind, tail) = split_kind(rest)?;
        return match tail {
            "value" => Some(Op::CountPrefix(kind, false)),
            "list" => Some(Op::CountPrefix(kind, true)),
            _ => None,
        };
    }
    // `<Verb><Kind>Value`.
    for (verb, op) in [
        ("set", Op::Set as fn(JsonKind) -> Op),
        ("get", Op::Get),
        ("unset", Op::Unset),
        ("has", Op::Has),
        ("adjust", Op::Adjust),
    ] {
        if let Some((kind, "value")) = name.strip_prefix(verb).and_then(split_kind) {
            if verb == "adjust" && !matches!(kind, JsonKind::Int | JsonKind::Float) {
                return None;
            }
            return Some(op(kind));
        }
    }
    // `<Kind>List<Verb>`.
    let (kind, rest) = split_kind(name)?;
    let verb = rest.strip_prefix("list")?;
    let op = match verb {
        "add" => ListOp::Add,
        "get" => ListOp::Get,
        "set" => ListOp::Set,
        "remove" => ListOp::Remove,
        "insertat" => ListOp::InsertAt,
        "removeat" => ListOp::RemoveAt,
        "clear" => ListOp::Clear,
        "count" => ListOp::Count,
        "countvalue" => ListOp::CountValue,
        "find" => ListOp::Find,
        "has" => ListOp::Has,
        "toarray" => ListOp::ToArray,
        "resize" => ListOp::Resize,
        "adjust" if matches!(kind, JsonKind::Int | JsonKind::Float) => ListOp::Adjust,
        "slice" => ListOp::Slice,
        "copy" => ListOp::Copy,
        _ => return None,
    };
    Some(Op::List(kind, op))
}

/// The checker's kind of a value of `kind`.
fn value_kind(kind: JsonKind) -> Kind {
    match kind {
        JsonKind::Int | JsonKind::Float => Kind::Num,
        JsonKind::String => Kind::Str,
        JsonKind::Form => Kind::Object,
    }
}

/// What `op` returns to the script.
fn return_kind(op: Op) -> Kind {
    match op {
        Op::Load | Op::Save | Op::Unload | Op::IsPendingSave | Op::IsGood | Op::JsonExists => Kind::Bool,
        Op::GetErrors => Kind::Str,
        Op::JsonInFolder => Kind::Array,
        Op::ClearAll => Kind::None,
        Op::Set(kind) | Op::Get(kind) => value_kind(kind),
        Op::Unset(_) | Op::Has(_) => Kind::Bool,
        Op::Adjust(_) | Op::CountPrefix(..) | Op::CountAllPrefix => Kind::Num,
        Op::List(kind, list) => match list {
            ListOp::Get | ListOp::Set => value_kind(kind),
            ListOp::InsertAt | ListOp::RemoveAt | ListOp::Has | ListOp::Copy => Kind::Bool,
            ListOp::ToArray => Kind::Array,
            ListOp::Slice => Kind::None,
            ListOp::Add
            | ListOp::Remove
            | ListOp::Clear
            | ListOp::Count
            | ListOp::CountValue
            | ListOp::Find
            | ListOp::Resize
            | ListOp::Adjust => Kind::Num,
        },
    }
}

/// The default a missing value reads as.
fn default_of(kind: JsonKind) -> Value {
    match kind {
        JsonKind::Int => Value::Int(0),
        JsonKind::Float => Value::Float(0.0),
        JsonKind::String => Value::Str(String::new()),
        JsonKind::Form => Value::None,
    }
}

/// Whether two stored values of `kind` are equal: numbers by value, forms by plugin and id.
fn same(kind: JsonKind, a: &Json, b: &Json) -> bool {
    match kind {
        JsonKind::Int | JsonKind::Float => a.as_f64() == b.as_f64(),
        JsonKind::String => a.as_str() == b.as_str(),
        JsonKind::Form => {
            let form = |value: &Json| {
                value.as_str().and_then(json_util::parse_form_string).map(|(local, plugin)| (local, plugin.to_ascii_lowercase()))
            };
            match (form(a), form(b)) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                _ => false,
            }
        }
    }
}

impl Vm<'_> {
    /// A `JsonUtil.<name>(…)` call, or a `StorageUtil.File<name>(…)` one (`storage_file`).
    pub(in crate::fragment::vm) fn read_json_util(
        &mut self,
        frame: &mut Frame,
        name: &str,
        args: &[CallArg],
        storage_file: bool,
    ) -> Run<Option<Value>> {
        let Some(op) = parse_op(name) else {
            return stop(format!("JsonUtil.{name}"));
        };
        let mut values = self.eval_args(frame, args)?;
        if storage_file {
            values.insert(0, Value::Str(STORAGE_UTIL_FILE.to_owned()));
        }
        if self.world.try_resource::<JsonUtilStore>().is_none() {
            return stop("JsonUtil without the file store");
        }
        let text = |i: usize| values.get(i).map(Value::text).unwrap_or_default();
        let int = |i: usize, default: i64| values.get(i).and_then(Value::number).map_or(default, |n| n as i64);
        let flag = |i: usize, default: bool| values.get(i).map_or(default, Value::truthy);
        let file_name = text(0);
        if op == Op::JsonInFolder {
            let Some(folder) = json_util::resolve_folder(&file_name) else {
                return Ok(Some(Value::Array(Rc::new(RefCell::new(Vec::new())))));
            };
            let names = self.world.resource::<JsonUtilStore>().json_in_folder(&folder);
            return Ok(Some(Value::Array(Rc::new(RefCell::new(names.into_iter().map(Value::Str).collect())))));
        }
        let Some(path) = json_util::resolve(&file_name) else {
            log::warn!("JsonUtil.{name}: {file_name:?} leaves SKSE/Plugins; refused");
            return Ok(Some(match return_kind(op) {
                Kind::Bool => Value::Bool(false),
                Kind::Num => Value::Int(0),
                Kind::Str => Value::Str(String::new()),
                Kind::Array => Value::Array(Rc::new(RefCell::new(Vec::new()))),
                _ => Value::None,
            }));
        };
        let key = text(1);
        // Values crossing into the file, converted before the store is borrowed.
        let to_json = |vm: &Self, kind: JsonKind, value: Option<&Value>| vm.json_of(kind, value);
        let mut store = self.world.resource_mut::<JsonUtilStore>();
        let result: Value = match op {
            Op::Load => Value::Bool(store.load(&path)),
            Op::Save => Value::Bool(store.save(&path, flag(1, false))),
            Op::Unload => Value::Bool(store.unload(&path, flag(1, true), flag(2, false))),
            Op::IsPendingSave => Value::Bool(store.is_pending_save(&path)),
            Op::IsGood => Value::Bool(store.is_good(&path)),
            Op::GetErrors => Value::Str(store.errors(&path)),
            Op::JsonExists => Value::Bool(store.exists(&path)),
            Op::JsonInFolder => unreachable!("answered above"),
            Op::ClearAll => {
                store.clear_all(&path);
                Value::None
            }
            Op::Set(kind) => {
                drop(store);
                let json = to_json(self, kind, values.get(2));
                self.world.resource_mut::<JsonUtilStore>().set(&path, kind, &key, json);
                return Ok(Some(values.get(2).cloned().unwrap_or_else(|| default_of(kind))));
            }
            Op::Get(kind) => {
                let found = store.get(&path, kind, &key);
                drop(store);
                return Ok(Some(match found {
                    Some(json) => self.value_of(kind, &json),
                    None => values.get(2).cloned().unwrap_or_else(|| default_of(kind)),
                }));
            }
            Op::Unset(kind) => Value::Bool(store.unset(&path, kind, &key)),
            Op::Has(kind) => Value::Bool(store.get(&path, kind, &key).is_some()),
            Op::Adjust(kind) => {
                let amount = values.get(2).and_then(Value::number).unwrap_or(0.0) as f64;
                let old = store.get(&path, kind, &key).and_then(|json| json.as_f64()).unwrap_or(0.0);
                let (json, value) = match kind {
                    JsonKind::Int => {
                        let new = (old as i64).wrapping_add(amount as i64) as i32;
                        (Json::from(new), Value::Int(new))
                    }
                    _ => {
                        let new = (old + amount) as f32;
                        (Json::from(f64::from(new)), Value::Float(new))
                    }
                };
                store.set(&path, kind, &key, json);
                value
            }
            Op::CountPrefix(kind, list) => Value::Int(store.count_prefix(&path, kind, list, &key) as i32),
            Op::CountAllPrefix => Value::Int(
                [JsonKind::Int, JsonKind::Float, JsonKind::String, JsonKind::Form]
                    .into_iter()
                    .flat_map(|kind| [(kind, false), (kind, true)])
                    .map(|(kind, list)| store.count_prefix(&path, kind, list, &key))
                    .sum::<usize>() as i32,
            ),
            Op::List(kind, list) => {
                drop(store);
                return self.json_list(&path, kind, list, &key, &values, int, flag);
            }
        };
        Ok(Some(result))
    }

    /// One `<Kind>List<Verb>` op; `values` are the call's arguments, the file name first.
    #[allow(clippy::too_many_arguments)] // The call's parsed parts.
    fn json_list(
        &mut self,
        path: &str,
        kind: JsonKind,
        op: ListOp,
        key: &str,
        values: &[Value],
        int: impl Fn(usize, i64) -> i64,
        flag: impl Fn(usize, bool) -> bool,
    ) -> Run<Option<Value>> {
        let index = |i: usize| usize::try_from(int(i, -1)).ok();
        let needle = |i: usize| self.json_of(kind, values.get(i));
        let world = self.world;
        let store = || world.resource_mut::<JsonUtilStore>();
        let list = || store().list(path, kind, key).unwrap_or_default();
        let out = |vm: &Self, json: Option<&Json>| json.map_or_else(|| default_of(kind), |json| vm.value_of(kind, json));
        Ok(Some(match op {
            ListOp::Add => {
                let value = needle(2);
                let allow_duplicate = flag(3, true);
                let added = store().edit_list(path, kind, key, |list| {
                    if !allow_duplicate && list.iter().any(|item| same(kind, item, &value)) {
                        return (-1, false);
                    }
                    list.push(value);
                    (list.len() as i32 - 1, true)
                });
                Value::Int(added.unwrap_or(-1))
            }
            ListOp::Get => {
                let list = list();
                out(self, index(2).and_then(|i| list.get(i)))
            }
            ListOp::Set => {
                // StorageUtil.psc: "This will return the previous value or 0 if there was a problem."
                let value = needle(3);
                let at = index(2);
                let previous = store().edit_list(path, kind, key, |list| match at.and_then(|i| list.get_mut(i)) {
                    Some(slot) => (Some(std::mem::replace(slot, value)), true),
                    None => (None, false),
                });
                out(self, previous.flatten().as_ref())
            }
            ListOp::Remove => {
                let value = needle(2);
                let all = flag(3, true);
                let removed = store().edit_list(path, kind, key, |list| {
                    let mut removed = 0;
                    list.retain(|item| {
                        let drop = same(kind, item, &value) && (all || removed == 0);
                        removed += i32::from(drop);
                        !drop
                    });
                    (removed, removed > 0)
                });
                Value::Int(removed.unwrap_or(0))
            }
            ListOp::InsertAt => {
                let value = needle(3);
                let at = index(2);
                let inserted = store().edit_list(path, kind, key, |list| match at.filter(|&i| i <= list.len()) {
                    Some(i) => {
                        list.insert(i, value);
                        (true, true)
                    }
                    None => (false, false),
                });
                Value::Bool(inserted.unwrap_or(false))
            }
            ListOp::RemoveAt => {
                let at = index(2);
                let removed = store().edit_list(path, kind, key, |list| match at.filter(|&i| i < list.len()) {
                    Some(i) => {
                        list.remove(i);
                        (true, true)
                    }
                    None => (false, false),
                });
                Value::Bool(removed.unwrap_or(false))
            }
            ListOp::Clear => Value::Int(store().clear_list(path, kind, key) as i32),
            ListOp::Count => Value::Int(list().len() as i32),
            ListOp::CountValue => {
                let value = needle(2);
                let exclude = flag(3, false);
                Value::Int(list().iter().filter(|item| same(kind, item, &value) != exclude).count() as i32)
            }
            ListOp::Find => {
                let value = needle(2);
                Value::Int(list().iter().position(|item| same(kind, item, &value)).map_or(-1, |i| i as i32))
            }
            ListOp::Has => {
                let value = needle(2);
                Value::Bool(list().iter().any(|item| same(kind, item, &value)))
            }
            ListOp::ToArray => {
                let items: Vec<Value> = list().iter().map(|json| self.value_of(kind, json)).collect();
                Value::Array(Rc::new(RefCell::new(items)))
            }
            ListOp::Resize => {
                // "Returns the number of elements truncated (signed) or added"; at most 500 long.
                let length = int(2, 0).clamp(0, MAX_RESIZE as i64) as usize;
                let filler = needle(3);
                let change = store().edit_list(path, kind, key, |list| {
                    let before = list.len() as i32;
                    list.resize(length, filler);
                    (list.len() as i32 - before, list.len() as i32 != before)
                });
                Value::Int(change.unwrap_or(0))
            }
            ListOp::Adjust => {
                // "returns 0 if index does not exists".
                let amount = values.get(3).and_then(Value::number).unwrap_or(0.0) as f64;
                let at = index(2);
                let adjusted = store().edit_list(path, kind, key, |list| match at.and_then(|i| list.get_mut(i)) {
                    Some(slot) => {
                        let old = slot.as_f64().unwrap_or(0.0);
                        *slot = match kind {
                            JsonKind::Int => Json::from((old as i64).wrapping_add(amount as i64) as i32),
                            _ => Json::from(f64::from((old + amount) as f32)),
                        };
                        (Some(slot.clone()), true)
                    }
                    None => (None, false),
                });
                out(self, adjusted.flatten().as_ref())
            }
            ListOp::Slice => {
                // Fills the given array from `startIndex` on.
                let start = index(3).unwrap_or(0);
                let list = list();
                if let Some(Value::Array(array)) = values.get(2) {
                    let mut array = array.borrow_mut();
                    for (i, slot) in array.iter_mut().enumerate() {
                        if let Some(json) = list.get(start + i) {
                            *slot = self.value_of(kind, json);
                        }
                    }
                }
                Value::None
            }
            ListOp::Copy => {
                // The list becomes the given array's values.
                let Some(Value::Array(array)) = values.get(2) else {
                    return Ok(Some(Value::Bool(false)));
                };
                let items: Vec<Json> = array.borrow().iter().map(|value| self.json_of(kind, Some(value))).collect();
                let copied = store().edit_list(path, kind, key, |list| {
                    *list = items;
                    (true, true)
                });
                Value::Bool(copied.unwrap_or(false))
            }
        }))
    }

    /// A script value as the file stores it ([`json_util`]'s format).
    fn json_of(&self, kind: JsonKind, value: Option<&Value>) -> Json {
        match kind {
            JsonKind::Int => Json::from(match value {
                Some(Value::Int(int)) => *int,
                other => other.and_then(Value::number).map_or(0, |n| n as i32),
            }),
            JsonKind::Float => Json::from(f64::from(value.and_then(Value::number).unwrap_or(0.0))),
            JsonKind::String => Json::from(value.map(Value::text).unwrap_or_default()),
            JsonKind::Form => {
                let Some(Value::Form(form)) = value else {
                    return Json::from("0");
                };
                let name = self.world.try_resource::<crate::load_order::LoadOrderIdentity>().and_then(|identity| {
                    let pair = identity.pair(*form)?;
                    let catalog = self.world.try_resource::<crate::LegacyObscriptContentCatalog>()?;
                    let plugin = catalog.0.iter().find(|plugin| plugin.source() == pair.plugin.0.to_be_bytes())?;
                    Some(json_util::form_string(pair.local.0, plugin.name()))
                });
                Json::from(name.unwrap_or_else(|| "0".to_owned()))
            }
        }
    }

    /// A stored value as the script reads it.
    fn value_of(&self, kind: JsonKind, json: &Json) -> Value {
        match kind {
            JsonKind::Int => Value::Int(json.as_i64().or_else(|| json.as_f64().map(|f| f as i64)).unwrap_or(0) as i32),
            JsonKind::Float => Value::Float(json.as_f64().unwrap_or(0.0) as f32),
            JsonKind::String => Value::Str(match json {
                Json::String(text) => text.clone(),
                Json::Null => String::new(),
                other => other.to_string(),
            }),
            JsonKind::Form => json
                .as_str()
                .and_then(json_util::parse_form_string)
                .and_then(|(local, plugin)| {
                    self.world
                        .try_resource::<crate::load_order::LoadOrderIdentity>()?
                        .form_from_file(local, byroredux_core::form_id::PluginId::from_filename(plugin))
                })
                .map_or(Value::None, Value::Form),
        }
    }
}

impl Checker<'_> {
    /// A `JsonUtil` (or `StorageUtil.File*`) call the VM serves, and what it returns.
    pub(in crate::fragment::vm) fn check_json_util(&mut self, class: &str, name: &str) -> Option<Kind> {
        let op = if class == "storageutil" { name.strip_prefix("file").and_then(parse_op) } else { parse_op(name) };
        match op {
            Some(Op::JsonInFolder | Op::Load | Op::Save | Op::Unload) if class == "storageutil" => {
                self.fail(|| format!("StorageUtil.{name}"))
            }
            Some(op) => Some(return_kind(op)),
            None if class == "storageutil" => self.fail(|| format!("StorageUtil.{name}")),
            None => self.fail(|| format!("JsonUtil.{name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_util_names_parse_to_their_ops() {
        assert_eq!(parse_op("setintvalue"), Some(Op::Set(JsonKind::Int)));
        assert_eq!(parse_op("getformvalue"), Some(Op::Get(JsonKind::Form)));
        assert_eq!(parse_op("adjustfloatvalue"), Some(Op::Adjust(JsonKind::Float)));
        assert_eq!(parse_op("adjuststringvalue"), None);
        assert_eq!(parse_op("stringlistadd"), Some(Op::List(JsonKind::String, ListOp::Add)));
        assert_eq!(parse_op("formlistcountvalue"), Some(Op::List(JsonKind::Form, ListOp::CountValue)));
        assert_eq!(parse_op("intlistadjust"), Some(Op::List(JsonKind::Int, ListOp::Adjust)));
        assert_eq!(parse_op("countfloatlistprefix"), Some(Op::CountPrefix(JsonKind::Float, true)));
        assert_eq!(parse_op("countallprefix"), Some(Op::CountAllPrefix));
        assert_eq!(parse_op("unload"), Some(Op::Unload));
        assert_eq!(parse_op("setpathintvalue"), None, "the path functions are not served");
        assert_eq!(parse_op("intlistrandom"), None);
    }
}
