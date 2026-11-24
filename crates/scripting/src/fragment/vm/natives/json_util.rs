//! PapyrusUtil's `JsonUtil` and `StorageUtil` ([`crate::json_util`], LMC PRD Q5), served by the engine.
//!
//! Both work on documents of typed values ([`ValueDoc`]): a `JsonUtil` call names a file, a `StorageUtil`
//! call a form (`None` for its global values), and the shared ops follow `StorageUtil.psc`'s documentation,
//! which `JsonUtil.psc` points to ("These functions all work in exactly the same way as their
//! StorageUtil.psc equivalents"). `StorageUtil`'s `File*` functions are `JsonUtil` on `"../StorageUtil.json"`
//! (their bodies in `StorageUtil.psc`). `JsonUtil` adds the file and path functions, `StorageUtil` plucks,
//! shifts, pops, sorts, prefix work across every object, and its `debug_` listings.
//!
//! Not served, each a stop that names it: `FormListFilterByType(s)` (Papyrus form-type ids the engine does
//! not map), `ImportFile` / `ExportFile`, `debug_Cleanup`, `debug_SetDebugMode` and the `debug_File*` family.

use super::*;
use crate::json_util::{self, Kind as JsonKind, JsonUtilStore, StorageUtilValues, ValueDoc, MAX_RESIZE};
use serde_json::Value as Json;

/// The file `StorageUtil`'s `File*` functions use.
const STORAGE_UTIL_FILE: &str = "../StorageUtil.json";

/// The class a call is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
    /// `JsonUtil`, or `StorageUtil.File*`: the first argument names a file.
    Json,
    /// `StorageUtil`: the first argument is a form, or `None`.
    Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListOp {
    Add,
    Get,
    Set,
    Pluck,
    Shift,
    Pop,
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
    Sort,
    Random,
}

/// The JSON kind a path's value is asked to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathType {
    String,
    Number,
    Form,
    Bool,
    Array,
    Object,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    // JsonUtil's files.
    Load,
    Save,
    Unload,
    IsPendingSave,
    IsGood,
    GetErrors,
    JsonInFolder,
    JsonExists,
    ClearAll,
    // Values.
    Set(JsonKind),
    Get(JsonKind),
    Pluck(JsonKind),
    Unset(JsonKind),
    Has(JsonKind),
    Adjust(JsonKind),
    List(JsonKind, ListOp),
    // Prefixes: in the target (a file, or one object), or across every object.
    CountPrefix(JsonKind, bool),
    CountAllPrefix,
    CountEveryPrefix(JsonKind, bool),
    CountEveryAllPrefix,
    ClearPrefix(JsonKind, bool),
    ClearAllPrefix,
    ClearEveryPrefix(JsonKind, bool),
    ClearEveryAllPrefix,
    // JsonUtil's paths.
    SetPath(JsonKind),
    GetPath(JsonKind),
    GetPathBool,
    PathElements(JsonKind),
    FindPathElement(JsonKind),
    SetPathArray(JsonKind),
    PathCount,
    PathMembers,
    CanResolvePath,
    IsPath(PathType),
    ClearPath,
    ClearPathIndex,
    SetRawPathValue,
    // StorageUtil's debug listings.
    DeleteValues,
    DeleteAllValues,
    ObjectCount(JsonKind, bool),
    Object(JsonKind, bool),
    AllObjects(JsonKind, bool),
    KeysCount(JsonKind, bool),
    Key(JsonKind, bool),
    AllKeys(JsonKind, bool),
}

/// `rest` without its leading value-type word, and that type.
fn split_kind(rest: &str) -> Option<(JsonKind, &str)> {
    [("string", JsonKind::String), ("float", JsonKind::Float), ("form", JsonKind::Form), ("int", JsonKind::Int)]
        .into_iter()
        .find_map(|(word, kind)| Some((kind, rest.strip_prefix(word)?)))
}

/// `<Kind>` or `<Kind>List` at the start of `rest`, and what follows.
fn split_kind_list(rest: &str) -> Option<(JsonKind, bool, &str)> {
    let (kind, tail) = split_kind(rest)?;
    Some(match tail.strip_prefix("list") {
        Some(after) => (kind, true, after),
        None => (kind, false, tail),
    })
}

/// The op a lowercase function name names on `flavor`'s class.
fn parse_op(flavor: Flavor, name: &str) -> Option<Op> {
    let json = flavor == Flavor::Json;
    let storage = flavor == Flavor::Storage;
    let whole = match name {
        "load" if json => Some(Op::Load),
        "save" if json => Some(Op::Save),
        "unload" if json => Some(Op::Unload),
        "ispendingsave" if json => Some(Op::IsPendingSave),
        "isgood" if json => Some(Op::IsGood),
        "geterrors" if json => Some(Op::GetErrors),
        "jsoninfolder" if json => Some(Op::JsonInFolder),
        "jsonexists" if json => Some(Op::JsonExists),
        "clearall" if json => Some(Op::ClearAll),
        "countallprefix" if json => Some(Op::CountAllPrefix),
        "countallprefix" => Some(Op::CountEveryAllPrefix),
        "countallobjprefix" if storage => Some(Op::CountAllPrefix),
        "clearallprefix" if storage => Some(Op::ClearEveryAllPrefix),
        "clearallobjprefix" if storage => Some(Op::ClearAllPrefix),
        "pathcount" if json => Some(Op::PathCount),
        "pathmembers" if json => Some(Op::PathMembers),
        "canresolvepath" if json => Some(Op::CanResolvePath),
        "ispathstring" if json => Some(Op::IsPath(PathType::String)),
        "ispathnumber" if json => Some(Op::IsPath(PathType::Number)),
        "ispathform" if json => Some(Op::IsPath(PathType::Form)),
        "ispathbool" if json => Some(Op::IsPath(PathType::Bool)),
        "ispatharray" if json => Some(Op::IsPath(PathType::Array)),
        "ispathobject" if json => Some(Op::IsPath(PathType::Object)),
        "clearpath" if json => Some(Op::ClearPath),
        "clearpathindex" if json => Some(Op::ClearPathIndex),
        "setrawpathvalue" if json => Some(Op::SetRawPathValue),
        "getpathboolvalue" if json => Some(Op::GetPathBool),
        "debug_deletevalues" if storage => Some(Op::DeleteValues),
        "debug_deleteallvalues" if storage => Some(Op::DeleteAllValues),
        _ => None,
    };
    if whole.is_some() {
        return whole;
    }
    if storage && let Some(rest) = name.strip_prefix("debug_") {
        // `debug_Get<Kind>[List]ObjectCount`, `debug_Get<Kind>[List]Object`, `debug_All<Kind>[List]Objs`,
        // `debug_Get<Kind>[List]KeysCount`, `debug_Get<Kind>[List]Key`, `debug_AllObj<Kind>[List]Keys`.
        if let Some(rest) = rest.strip_prefix("allobj") {
            let (kind, list, tail) = split_kind_list(rest)?;
            return (tail == "keys").then_some(Op::AllKeys(kind, list));
        }
        if let Some(rest) = rest.strip_prefix("all") {
            let (kind, list, tail) = split_kind_list(rest)?;
            return (tail == "objs").then_some(Op::AllObjects(kind, list));
        }
        let (kind, list, tail) = split_kind_list(rest.strip_prefix("get")?)?;
        return match tail {
            "objectcount" => Some(Op::ObjectCount(kind, list)),
            "object" => Some(Op::Object(kind, list)),
            "keyscount" => Some(Op::KeysCount(kind, list)),
            "key" => Some(Op::Key(kind, list)),
            _ => None,
        };
    }
    // Prefixes: `Count<Kind>ValuePrefix` / `Count<Kind>ListPrefix` (a file's for JsonUtil, every object's for
    // StorageUtil), `CountObj…` (one object's), `Clear…` / `ClearObj…` likewise (StorageUtil only).
    for (verb, count) in [("count", true), ("clear", false)] {
        let Some(rest) = name.strip_prefix(verb).and_then(|rest| rest.strip_suffix("prefix")) else {
            continue;
        };
        let (obj, rest) = match rest.strip_prefix("obj") {
            Some(after) if storage => (true, after),
            _ => (false, rest),
        };
        let (kind, tail) = split_kind(rest)?;
        let list = match tail {
            "value" => false,
            "list" => true,
            _ => return None,
        };
        return match (count, json, obj) {
            (true, true, _) | (true, false, true) => Some(Op::CountPrefix(kind, list)),
            (true, false, false) => Some(Op::CountEveryPrefix(kind, list)),
            (false, false, true) => Some(Op::ClearPrefix(kind, list)),
            (false, false, false) => Some(Op::ClearEveryPrefix(kind, list)),
            (false, true, _) => None,
        };
    }
    // JsonUtil's paths: `SetPath<Kind>Value`, `SetPath<Kind>Array`, `GetPath<Kind>Value`, `Path<Kind>Elements`,
    // `FindPath<Kind>Element`.
    if json {
        if let Some((kind, tail)) = name.strip_prefix("setpath").and_then(split_kind) {
            return match tail {
                "value" => Some(Op::SetPath(kind)),
                "array" => Some(Op::SetPathArray(kind)),
                _ => None,
            };
        }
        if let Some((kind, "value")) = name.strip_prefix("getpath").and_then(split_kind) {
            return Some(Op::GetPath(kind));
        }
        if let Some((kind, "elements")) = name.strip_prefix("path").and_then(split_kind) {
            return Some(Op::PathElements(kind));
        }
        if let Some((kind, "element")) = name.strip_prefix("findpath").and_then(split_kind) {
            return Some(Op::FindPathElement(kind));
        }
    }
    // `<Verb><Kind>Value`.
    for (verb, op) in [
        ("set", Op::Set as fn(JsonKind) -> Op),
        ("get", Op::Get),
        ("unset", Op::Unset),
        ("has", Op::Has),
        ("adjust", Op::Adjust),
        ("pluck", Op::Pluck),
    ] {
        if let Some((kind, "value")) = name.strip_prefix(verb).and_then(split_kind) {
            if (verb == "adjust" && !matches!(kind, JsonKind::Int | JsonKind::Float)) || (verb == "pluck" && json) {
                return None;
            }
            return Some(op(kind));
        }
    }
    // `<Kind>List<Verb>`.
    let (kind, rest) = split_kind(name)?;
    let verb = rest.strip_prefix("list")?;
    let number = matches!(kind, JsonKind::Int | JsonKind::Float);
    let op = match verb {
        "add" => ListOp::Add,
        "get" => ListOp::Get,
        "set" => ListOp::Set,
        "remove" => ListOp::Remove,
        "insertat" if json => ListOp::InsertAt,
        "insert" if storage => ListOp::InsertAt,
        "removeat" => ListOp::RemoveAt,
        "clear" => ListOp::Clear,
        "count" => ListOp::Count,
        "countvalue" => ListOp::CountValue,
        "find" => ListOp::Find,
        "has" => ListOp::Has,
        "toarray" => ListOp::ToArray,
        "resize" => ListOp::Resize,
        "adjust" if number => ListOp::Adjust,
        "slice" => ListOp::Slice,
        "copy" => ListOp::Copy,
        "random" => ListOp::Random,
        "pluck" if storage => ListOp::Pluck,
        "shift" if storage => ListOp::Shift,
        "pop" if storage => ListOp::Pop,
        "sort" if storage => ListOp::Sort,
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
        Op::GetErrors | Op::Key(..) => Kind::Str,
        Op::JsonInFolder | Op::PathMembers | Op::PathElements(_) | Op::AllObjects(..) | Op::AllKeys(..) => Kind::Array,
        Op::ClearAll | Op::SetPath(_) | Op::SetPathArray(_) | Op::ClearPath | Op::ClearPathIndex | Op::DeleteValues | Op::DeleteAllValues => {
            Kind::None
        }
        Op::Set(kind) | Op::Get(kind) | Op::Pluck(kind) | Op::GetPath(kind) | Op::Adjust(kind) => value_kind(kind),
        Op::Unset(_) | Op::Has(_) | Op::GetPathBool | Op::CanResolvePath | Op::IsPath(_) | Op::SetRawPathValue => Kind::Bool,
        Op::Object(..) => Kind::Object,
        Op::CountPrefix(..)
        | Op::CountAllPrefix
        | Op::CountEveryPrefix(..)
        | Op::CountEveryAllPrefix
        | Op::ClearPrefix(..)
        | Op::ClearAllPrefix
        | Op::ClearEveryPrefix(..)
        | Op::ClearEveryAllPrefix
        | Op::FindPathElement(_)
        | Op::PathCount
        | Op::ObjectCount(..)
        | Op::KeysCount(..) => Kind::Num,
        Op::List(kind, list) => match list {
            ListOp::Get | ListOp::Set | ListOp::Pluck | ListOp::Shift | ListOp::Pop | ListOp::Random | ListOp::Adjust => {
                value_kind(kind)
            }
            ListOp::InsertAt | ListOp::RemoveAt | ListOp::Has | ListOp::Copy => Kind::Bool,
            ListOp::ToArray => Kind::Array,
            ListOp::Slice | ListOp::Sort => Kind::None,
            ListOp::Add | ListOp::Remove | ListOp::Clear | ListOp::Count | ListOp::CountValue | ListOp::Find | ListOp::Resize => {
                Kind::Num
            }
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

/// The "nothing" a call answers when its target cannot be reached.
fn empty_of(kind: Kind) -> Value {
    match kind {
        Kind::Bool => Value::Bool(false),
        Kind::Num => Value::Int(0),
        Kind::Str => Value::Str(String::new()),
        Kind::Array => Value::Array(Rc::new(RefCell::new(Vec::new()))),
        _ => Value::None,
    }
}

/// Whether two stored values of `kind` are equal: numbers by value, strings exactly, forms by their text.
fn same(kind: JsonKind, a: &Json, b: &Json) -> bool {
    match kind {
        JsonKind::Int | JsonKind::Float => a.as_f64() == b.as_f64(),
        JsonKind::String => a.as_str() == b.as_str(),
        JsonKind::Form => match (a.as_str(), b.as_str()) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (a, b) => a.is_none() && b.is_none(),
        },
    }
}

/// A stored value that is "zero, empty, or none" (`StorageUtil.psc`: setting one unsets the key).
fn is_blank(json: &Json) -> bool {
    match json {
        Json::Null => true,
        Json::Number(number) => number.as_f64() == Some(0.0),
        Json::String(text) => text.is_empty() || text == "0",
        _ => false,
    }
}

/// `old` adjusted by `amount` as `kind` stores it.
fn adjusted(kind: JsonKind, old: f64, amount: f64) -> Json {
    match kind {
        JsonKind::Int => Json::from((old as i64).wrapping_add(amount as i64) as i32),
        _ => Json::from(f64::from((old + amount) as f32)),
    }
}

/// A path's value read as `kind`, as PapyrusUtil's `GetPath*Value` does: a number (or bool) as a number, a
/// string as itself (a number or bool as its text), a form from its form string; `None` otherwise.
fn path_value(kind: JsonKind, json: &Json) -> Option<Json> {
    match kind {
        JsonKind::Int | JsonKind::Float => match json {
            Json::Number(_) => Some(json.clone()),
            Json::Bool(flag) => Some(Json::from(i32::from(*flag))),
            _ => None,
        },
        JsonKind::String => match json {
            Json::String(_) => Some(json.clone()),
            Json::Number(_) | Json::Bool(_) => Some(Json::from(json.to_string())),
            _ => None,
        },
        JsonKind::Form => json.as_str().map(|text| Json::from(text.to_owned())),
    }
}

/// Where a call's values live.
enum Target {
    File(String),
    Object(String),
}

/// A read answered after the document is released, so a form converts with no guard held.
enum Out {
    Value(Value),
    Json(JsonKind, Option<Json>, Value),
    Jsons(JsonKind, Vec<Json>),
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
        self.read_papyrus_util(frame, Flavor::Json, name, args, storage_file)
    }

    /// A `StorageUtil.<name>(…)` call on its form-keyed values.
    pub(in crate::fragment::vm) fn read_storage_util(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<Option<Value>> {
        self.read_papyrus_util(frame, Flavor::Storage, name, args, false)
    }

    fn read_papyrus_util(
        &mut self,
        frame: &mut Frame,
        flavor: Flavor,
        name: &str,
        args: &[CallArg],
        storage_file: bool,
    ) -> Run<Option<Value>> {
        let class = if flavor == Flavor::Json && !storage_file { "JsonUtil" } else { "StorageUtil" };
        let Some(op) = parse_op(flavor, name) else {
            return stop(format!("{class}.{name}"));
        };
        let mut values = self.eval_args(frame, args)?;
        if storage_file {
            values.insert(0, Value::Str(STORAGE_UTIL_FILE.to_owned()));
        }
        let target = match flavor {
            Flavor::Json => {
                if self.world.try_resource::<JsonUtilStore>().is_none() {
                    return stop("JsonUtil without the file store");
                }
                let file_name = values.first().map(Value::text).unwrap_or_default();
                if op == Op::JsonInFolder {
                    let names = json_util::resolve_folder(&file_name)
                        .map(|folder| self.world.resource::<JsonUtilStore>().json_in_folder(&folder))
                        .unwrap_or_default();
                    return Ok(Some(Value::Array(Rc::new(RefCell::new(names.into_iter().map(Value::Str).collect())))));
                }
                match json_util::resolve(&file_name) {
                    Some(path) => Target::File(path),
                    None => {
                        log::warn!("JsonUtil.{name}: {file_name:?} leaves SKSE/Plugins; refused");
                        return Ok(Some(empty_of(return_kind(op))));
                    }
                }
            }
            Flavor::Storage => {
                if self.world.try_resource::<StorageUtilValues>().is_none() {
                    return stop("StorageUtil without its values");
                }
                // The calls on every object, or none, take no target.
                if let Some(answer) = self.storage_every(op, &values) {
                    return Ok(Some(answer));
                }
                Target::Object(self.object_key(values.first()))
            }
        };
        if let Target::File(path) = &target {
            let mut store = self.world.resource_mut::<JsonUtilStore>();
            let flag = |i: usize, default: bool| values.get(i).map_or(default, Value::truthy);
            let answer = match op {
                Op::Load => Some(Value::Bool(store.load(path))),
                Op::Save => Some(Value::Bool(store.save(path, flag(1, false)))),
                Op::Unload => Some(Value::Bool(store.unload(path, flag(1, true), flag(2, false)))),
                Op::IsPendingSave => Some(Value::Bool(store.is_pending_save(path))),
                Op::IsGood => Some(Value::Bool(store.is_good(path))),
                Op::GetErrors => Some(Value::Str(store.errors(path))),
                Op::JsonExists => Some(Value::Bool(store.exists(path))),
                _ => None,
            };
            if answer.is_some() {
                return Ok(answer);
            }
        }
        let answer = self.doc_op(&target, flavor, op, &values);
        if let Target::Object(key) = &target {
            self.world.resource_mut::<StorageUtilValues>().prune_key(key);
        }
        Ok(Some(answer.unwrap_or_else(|| empty_of(return_kind(op)))))
    }

    /// The `StorageUtil` calls on every object or none: `None` for the ones that name an object.
    fn storage_every(&self, op: Op, values: &[Value]) -> Option<Value> {
        let text = |i: usize| values.get(i).map(Value::text).unwrap_or_default();
        let index = |i: usize| values.get(i).and_then(Value::number).and_then(|n| usize::try_from(n as i64).ok());
        let kinds = [JsonKind::Int, JsonKind::Float, JsonKind::String, JsonKind::Form];
        let every = |doc: &ValueDoc, prefix: &str| {
            kinds.iter().flat_map(|&kind| [doc.count_prefix(kind, false, prefix), doc.count_prefix(kind, true, prefix)]).sum::<usize>()
        };
        let mut store = self.world.try_resource_mut::<StorageUtilValues>()?;
        Some(match op {
            Op::CountEveryPrefix(kind, list) => {
                Value::Int(store.objects_mut().map(|(_, doc)| doc.count_prefix(kind, list, &text(0))).sum::<usize>() as i32)
            }
            Op::CountEveryAllPrefix => Value::Int(store.objects_mut().map(|(_, doc)| every(doc, &text(0))).sum::<usize>() as i32),
            Op::ClearEveryPrefix(kind, list) => {
                let cleared = store.objects_mut().map(|(_, doc)| doc.clear_prefix(kind, list, &text(0))).sum::<usize>();
                store.prune();
                Value::Int(cleared as i32)
            }
            Op::ClearEveryAllPrefix => {
                let prefix = text(0);
                let cleared = store
                    .objects_mut()
                    .map(|(_, doc)| {
                        kinds.iter().flat_map(|&kind| [(kind, false), (kind, true)]).map(|(kind, list)| doc.clear_prefix(kind, list, &prefix)).sum::<usize>()
                    })
                    .sum::<usize>();
                store.prune();
                Value::Int(cleared as i32)
            }
            Op::DeleteAllValues => {
                store.delete_all();
                Value::None
            }
            Op::ObjectCount(kind, list) => Value::Int(store.objects_with(kind, list).len() as i32),
            Op::Object(kind, list) => {
                let key = index(0).and_then(|i| store.objects_with(kind, list).get(i).cloned());
                drop(store);
                return Some(key.map_or(Value::None, |key| self.form_of_key(&key)));
            }
            Op::AllObjects(kind, list) => {
                let keys = store.objects_with(kind, list);
                drop(store);
                let forms: Vec<Value> = keys.iter().map(|key| self.form_of_key(key)).collect();
                return Some(Value::Array(Rc::new(RefCell::new(forms))));
            }
            _ => return None,
        })
    }

    /// The `StorageUtil` object key of a form argument: `""` for `None` (its global values).
    fn object_key(&self, value: Option<&Value>) -> String {
        match value {
            Some(Value::Form(form)) => self.form_text(*form),
            _ => String::new(),
        }
    }

    /// The form an object key names, `None` for the global key.
    fn form_of_key(&self, key: &str) -> Value {
        if key.is_empty() {
            return Value::None;
        }
        self.value_of(JsonKind::Form, &Json::from(key))
    }

    /// One op on the target's document; `None` when the document cannot be opened.
    fn doc_op(&self, target: &Target, flavor: Flavor, op: Op, values: &[Value]) -> Option<Value> {
        let text = |i: usize| values.get(i).map(Value::text).unwrap_or_default();
        let int = |i: usize, default: i64| match values.get(i) {
            Some(Value::Int(n)) => i64::from(*n),
            other => other.and_then(Value::number).map_or(default, |n| n as i64),
        };
        let flag = |i: usize, default: bool| values.get(i).map_or(default, Value::truthy);
        let index = |i: usize| usize::try_from(int(i, -1)).ok();
        let key = text(1);
        let kinds = [JsonKind::Int, JsonKind::Float, JsonKind::String, JsonKind::Form];
        // Values crossing into the document, converted before it is borrowed.
        let json_at = |kind: JsonKind, i: usize| self.json_of(kind, values.get(i));
        let array_at = |kind: JsonKind, i: usize| match values.get(i) {
            Some(Value::Array(array)) => array.borrow().iter().map(|value| self.json_of(kind, Some(value))).collect::<Vec<Json>>(),
            _ => Vec::new(),
        };
        let missing = |kind: JsonKind, i: usize| values.get(i).cloned().unwrap_or_else(|| default_of(kind));
        let out = match op {
            Op::Set(kind) => {
                let json = json_at(kind, 2);
                let blank = flavor == Flavor::Storage && is_blank(&json);
                self.with_doc(target, |doc| {
                    if blank {
                        doc.take(kind, &key);
                    } else {
                        doc.set(kind, &key, json);
                    }
                })?;
                Out::Value(values.get(2).cloned().unwrap_or_else(|| default_of(kind)))
            }
            Op::Get(kind) => Out::Json(kind, self.with_doc(target, |doc| doc.get(kind, &key))?, missing(kind, 2)),
            Op::Pluck(kind) => Out::Json(kind, self.with_doc(target, |doc| doc.take(kind, &key))?, missing(kind, 2)),
            Op::Unset(kind) => Out::Value(Value::Bool(self.with_doc(target, |doc| doc.take(kind, &key).is_some())?)),
            Op::Has(kind) => Out::Value(Value::Bool(self.with_doc(target, |doc| doc.get(kind, &key).is_some())?)),
            Op::Adjust(kind) => {
                let amount = values.get(2).and_then(Value::number).unwrap_or(0.0) as f64;
                let new = self.with_doc(target, |doc| {
                    let old = doc.get(kind, &key).and_then(|json| json.as_f64()).unwrap_or(0.0);
                    let new = adjusted(kind, old, amount);
                    doc.set(kind, &key, new.clone());
                    new
                })?;
                Out::Json(kind, Some(new), default_of(kind))
            }
            Op::ClearAll => {
                self.with_doc(target, ValueDoc::clear_all)?;
                Out::Value(Value::None)
            }
            Op::CountPrefix(kind, list) => Out::Value(Value::Int(self.with_doc(target, |doc| doc.count_prefix(kind, list, &key))? as i32)),
            Op::CountAllPrefix => Out::Value(Value::Int(self.with_doc(target, |doc| {
                kinds.iter().flat_map(|&kind| [doc.count_prefix(kind, false, &key), doc.count_prefix(kind, true, &key)]).sum::<usize>()
            })? as i32)),
            Op::ClearPrefix(kind, list) => Out::Value(Value::Int(self.with_doc(target, |doc| doc.clear_prefix(kind, list, &key))? as i32)),
            Op::ClearAllPrefix => Out::Value(Value::Int(self.with_doc(target, |doc| {
                kinds.iter().flat_map(|&kind| [(kind, false), (kind, true)]).map(|(kind, list)| doc.clear_prefix(kind, list, &key)).sum::<usize>()
            })? as i32)),
            Op::DeleteValues => {
                if let Target::Object(object) = target {
                    self.world.resource_mut::<StorageUtilValues>().delete(object);
                }
                Out::Value(Value::None)
            }
            Op::KeysCount(kind, list) => Out::Value(Value::Int(self.with_doc(target, |doc| doc.keys(kind, list).len())? as i32)),
            Op::Key(kind, list) => {
                let at = index(1);
                Out::Value(Value::Str(self.with_doc(target, |doc| at.and_then(|i| doc.keys(kind, list).get(i).cloned()))?.unwrap_or_default()))
            }
            Op::AllKeys(kind, list) => {
                let keys = self.with_doc(target, |doc| doc.keys(kind, list))?;
                Out::Value(Value::Array(Rc::new(RefCell::new(keys.into_iter().map(Value::Str).collect()))))
            }
            // Paths: the second argument is the path.
            Op::SetPath(kind) => {
                let json = json_at(kind, 2);
                self.with_doc(target, |doc| doc.set_path(&key, json))?;
                Out::Value(Value::None)
            }
            Op::SetPathArray(kind) => {
                let items = array_at(kind, 2);
                let append = flag(3, false);
                self.with_doc(target, |doc| {
                    let mut array = match doc.path(&key) {
                        Some(Json::Array(array)) if append => array.clone(),
                        _ => Vec::new(),
                    };
                    array.extend(items);
                    doc.set_path(&key, Json::Array(array))
                })?;
                Out::Value(Value::None)
            }
            Op::SetRawPathValue => {
                let Ok(json) = serde_json::from_str::<Json>(&text(2)) else {
                    return Some(Value::Bool(false));
                };
                Out::Value(Value::Bool(self.with_doc(target, |doc| doc.set_path(&key, json))?))
            }
            Op::GetPath(kind) => {
                let found = self.with_doc(target, |doc| doc.path(&key).cloned())?;
                Out::Json(kind, found.and_then(|json| path_value(kind, &json)), missing(kind, 2))
            }
            Op::GetPathBool => {
                let found = self.with_doc(target, |doc| doc.path(&key).cloned())?;
                let number = found.and_then(|json| path_value(JsonKind::Int, &json)).and_then(|json| json.as_f64());
                Out::Value(Value::Bool(number.map_or_else(|| flag(2, false), |n| n != 0.0)))
            }
            Op::PathElements(kind) => {
                let invalid = json_at(kind, 2);
                let items = self.with_doc(target, |doc| match doc.path(&key) {
                    Some(Json::Array(array)) => array.iter().map(|item| path_value(kind, item).unwrap_or_else(|| invalid.clone())).collect(),
                    _ => Vec::new(),
                })?;
                Out::Jsons(kind, items)
            }
            Op::FindPathElement(kind) => {
                // jsoncpp's equality, or strings without case.
                let needle = json_at(kind, 2);
                let found = self.with_doc(target, |doc| match doc.path(&key) {
                    Some(Json::Array(array)) => array.iter().position(|item| {
                        item == &needle
                            || (item.is_number() && item.as_f64() == needle.as_f64())
                            || matches!((item.as_str(), needle.as_str()), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b))
                    }),
                    _ => None,
                })?;
                Out::Value(Value::Int(found.map_or(-1, |i| i as i32)))
            }
            Op::PathCount => Out::Value(Value::Int(self.with_doc(target, |doc| match doc.path(&key) {
                Some(Json::Array(array)) => array.len() as i32,
                Some(Json::Object(map)) => map.len() as i32,
                _ => -1,
            })?)),
            Op::PathMembers => {
                let members = self.with_doc(target, |doc| match doc.path(&key) {
                    Some(Json::Object(map)) => map.keys().cloned().collect(),
                    _ => Vec::new(),
                })?;
                Out::Value(Value::Array(Rc::new(RefCell::new(members.into_iter().map(Value::Str).collect()))))
            }
            Op::CanResolvePath => Out::Value(Value::Bool(self.with_doc(target, |doc| doc.path(&key).is_some())?)),
            Op::IsPath(kind) => Out::Value(Value::Bool(self.with_doc(target, |doc| match (kind, doc.path(&key)) {
                (PathType::String, Some(json)) => json.is_string(),
                (PathType::Number, Some(json)) => json.is_number(),
                (PathType::Bool, Some(json)) => json.is_boolean(),
                (PathType::Array, Some(json)) => json.is_array(),
                (PathType::Object, Some(json)) => json.is_object(),
                (PathType::Form, Some(Json::String(text))) => {
                    json_util::parse_form_string(text).is_some() || json_util::parse_session_form_string(text).is_some()
                }
                _ => false,
            })?)),
            Op::ClearPath => {
                self.with_doc(target, |doc| doc.clear_path(&key))?;
                Out::Value(Value::None)
            }
            Op::ClearPathIndex => {
                let at = index(2);
                self.with_doc(target, |doc| at.is_some_and(|i| doc.clear_path_index(&key, i)))?;
                Out::Value(Value::None)
            }
            Op::List(kind, list) => return self.list_op(target, kind, list, &key, values),
            _ => return None,
        };
        Some(match out {
            Out::Value(value) => value,
            Out::Json(kind, json, missing) => json.map_or(missing, |json| self.value_of(kind, &json)),
            Out::Jsons(kind, items) => Value::Array(Rc::new(RefCell::new(items.iter().map(|json| self.value_of(kind, json)).collect()))),
        })
    }

    /// One `<Kind>List<Verb>` op on the target's document.
    fn list_op(&self, target: &Target, kind: JsonKind, op: ListOp, key: &str, values: &[Value]) -> Option<Value> {
        let int = |i: usize, default: i64| match values.get(i) {
            Some(Value::Int(n)) => i64::from(*n),
            other => other.and_then(Value::number).map_or(default, |n| n as i64),
        };
        let flag = |i: usize, default: bool| values.get(i).map_or(default, Value::truthy);
        let index = |i: usize| usize::try_from(int(i, -1)).ok();
        let needle = |i: usize| self.json_of(kind, values.get(i));
        let out = |json: Option<Json>, missing: Value| json.map_or(missing, |json| self.value_of(kind, &json));
        let list = || self.with_doc(target, |doc| doc.list(kind, key).unwrap_or_default());
        Some(match op {
            ListOp::Add => {
                let value = needle(2);
                let allow_duplicate = flag(3, true);
                Value::Int(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        if !allow_duplicate && list.iter().any(|item| same(kind, item, &value)) {
                            return (-1, false);
                        }
                        list.push(value);
                        (list.len() as i32 - 1, true)
                    })
                })?)
            }
            ListOp::Get => {
                let at = index(2);
                out(at.and_then(|i| list()?.get(i).cloned()), default_of(kind))
            }
            ListOp::Set => {
                // StorageUtil.psc: "This will return the previous value or 0 if there was a problem."
                let value = needle(3);
                let at = index(2);
                let previous = self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| match at.and_then(|i| list.get_mut(i)) {
                        Some(slot) => (Some(std::mem::replace(slot, value)), true),
                        None => (None, false),
                    })
                })?;
                out(previous, default_of(kind))
            }
            ListOp::Pluck | ListOp::Shift | ListOp::Pop => {
                // Pluck: "The index is removed from the list's storage after returning it's value"; Shift the
                // first, Pop the last.
                let at = index(2);
                let missing = if op == ListOp::Pluck { values.get(3).cloned().unwrap_or_else(|| default_of(kind)) } else { default_of(kind) };
                let taken = self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        let i = match op {
                            ListOp::Shift => (!list.is_empty()).then_some(0),
                            ListOp::Pop => list.len().checked_sub(1),
                            _ => at.filter(|&i| i < list.len()),
                        };
                        match i {
                            Some(i) => (Some(list.remove(i)), true),
                            None => (None, false),
                        }
                    })
                })?;
                out(taken, missing)
            }
            ListOp::Remove => {
                let value = needle(2);
                let all = flag(3, true);
                Value::Int(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        let mut removed = 0;
                        list.retain(|item| {
                            let drop = same(kind, item, &value) && (all || removed == 0);
                            removed += i32::from(drop);
                            !drop
                        });
                        (removed, removed > 0)
                    })
                })?)
            }
            ListOp::InsertAt => {
                let value = needle(3);
                let at = index(2);
                Value::Bool(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| match at.filter(|&i| i <= list.len()) {
                        Some(i) => {
                            list.insert(i, value);
                            (true, true)
                        }
                        None => (false, false),
                    })
                })?)
            }
            ListOp::RemoveAt => {
                let at = index(2);
                Value::Bool(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| match at.filter(|&i| i < list.len()) {
                        Some(i) => {
                            list.remove(i);
                            (true, true)
                        }
                        None => (false, false),
                    })
                })?)
            }
            ListOp::Clear => Value::Int(self.with_doc(target, |doc| {
                doc.edit_list(kind, key, |list| {
                    let count = list.len();
                    list.clear();
                    (count as i32, count > 0)
                })
            })?),
            ListOp::Count => Value::Int(list()?.len() as i32),
            ListOp::CountValue => {
                let value = needle(2);
                let exclude = flag(3, false);
                Value::Int(list()?.iter().filter(|item| same(kind, item, &value) != exclude).count() as i32)
            }
            ListOp::Find => {
                let value = needle(2);
                Value::Int(list()?.iter().position(|item| same(kind, item, &value)).map_or(-1, |i| i as i32))
            }
            ListOp::Has => {
                let value = needle(2);
                Value::Bool(list()?.iter().any(|item| same(kind, item, &value)))
            }
            ListOp::ToArray => Value::Array(Rc::new(RefCell::new(list()?.iter().map(|json| self.value_of(kind, json)).collect()))),
            ListOp::Resize => {
                // "Returns the number of elements truncated (signed) or added"; at most 500 long.
                let length = int(2, 0).clamp(0, MAX_RESIZE as i64) as usize;
                let filler = needle(3);
                Value::Int(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        let before = list.len() as i32;
                        list.resize(length, filler);
                        (list.len() as i32 - before, list.len() as i32 != before)
                    })
                })?)
            }
            ListOp::Adjust => {
                // "returns 0 if index does not exists".
                let amount = values.get(3).and_then(Value::number).unwrap_or(0.0) as f64;
                let at = index(2);
                let new = self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| match at.and_then(|i| list.get_mut(i)) {
                        Some(slot) => {
                            *slot = adjusted(kind, slot.as_f64().unwrap_or(0.0), amount);
                            (Some(slot.clone()), true)
                        }
                        None => (None, false),
                    })
                })?;
                out(new, default_of(kind))
            }
            ListOp::Slice => {
                // Fills the given array from `startIndex` on.
                let start = index(3).unwrap_or(0);
                let items = list()?;
                if let Some(Value::Array(array)) = values.get(2) {
                    let mut array = array.borrow_mut();
                    for (i, slot) in array.iter_mut().enumerate() {
                        if let Some(json) = items.get(start + i) {
                            *slot = self.value_of(kind, json);
                        }
                    }
                }
                Value::None
            }
            ListOp::Copy => {
                // The list becomes the given array's values.
                let Some(Value::Array(array)) = values.get(2) else {
                    return Some(Value::Bool(false));
                };
                let items: Vec<Json> = array.borrow().iter().map(|value| self.json_of(kind, Some(value))).collect();
                Value::Bool(self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        *list = items;
                        (true, true)
                    })
                })?)
            }
            ListOp::Sort => {
                // Ascending: numbers by value, strings and forms by their text without case.
                self.with_doc(target, |doc| {
                    doc.edit_list(kind, key, |list| {
                        list.sort_by(|a, b| match kind {
                            JsonKind::Int | JsonKind::Float => a.as_f64().partial_cmp(&b.as_f64()).unwrap_or(std::cmp::Ordering::Equal),
                            _ => a.as_str().unwrap_or("").to_ascii_lowercase().cmp(&b.as_str().unwrap_or("").to_ascii_lowercase()),
                        });
                        ((), true)
                    })
                })?;
                Value::None
            }
            ListOp::Random => {
                let items = list()?;
                if items.is_empty() {
                    return Some(default_of(kind));
                }
                let draw = self.world.try_resource_mut::<crate::story_manager::StoryManagerRng>().map_or(0.0, |mut rng| rng.unit_inclusive());
                let i = ((draw * items.len() as f32) as usize).min(items.len() - 1);
                self.value_of(kind, &items[i])
            }
        })
    }

    /// Run `edit` on the target's document: the file's (opened on first use), or the object's.
    fn with_doc<R>(&self, target: &Target, edit: impl FnOnce(&mut ValueDoc) -> R) -> Option<R> {
        match target {
            Target::File(path) => {
                let mut store = self.world.try_resource_mut::<JsonUtilStore>()?;
                Some(edit(store.doc(path)?))
            }
            Target::Object(key) => {
                let mut values = self.world.try_resource_mut::<StorageUtilValues>()?;
                Some(edit(values.doc(key)))
            }
        }
    }

    /// A form as the documents store it: its plugin and local id ([`json_util::form_string`]), else its session
    /// id for a form outside the load order.
    fn form_text(&self, form: u32) -> String {
        let named = self.world.try_resource::<crate::load_order::LoadOrderIdentity>().and_then(|identity| {
            let pair = identity.pair(form)?;
            let catalog = self.world.try_resource::<crate::LegacyObscriptContentCatalog>()?;
            let plugin = catalog.0.iter().find(|plugin| plugin.source() == pair.plugin.0.to_be_bytes())?;
            Some(json_util::form_string(pair.local.0, plugin.name()))
        });
        named.unwrap_or_else(|| json_util::session_form_string(form))
    }

    /// A script value as the documents store it ([`json_util`]'s format).
    fn json_of(&self, kind: JsonKind, value: Option<&Value>) -> Json {
        match kind {
            JsonKind::Int => Json::from(match value {
                Some(Value::Int(int)) => *int,
                other => other.and_then(Value::number).map_or(0, |n| n as i32),
            }),
            JsonKind::Float => Json::from(f64::from(value.and_then(Value::number).unwrap_or(0.0))),
            JsonKind::String => Json::from(value.map(Value::text).unwrap_or_default()),
            JsonKind::Form => match value {
                Some(Value::Form(form)) => Json::from(self.form_text(*form)),
                _ => Json::from("0"),
            },
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
            JsonKind::Form => {
                let Some(text) = json.as_str() else {
                    return Value::None;
                };
                if let Some(form) = json_util::parse_session_form_string(text) {
                    return Value::Form(form);
                }
                json_util::parse_form_string(text)
                    .and_then(|(local, plugin)| {
                        self.world
                            .try_resource::<crate::load_order::LoadOrderIdentity>()?
                            .form_from_file(local, byroredux_core::form_id::PluginId::from_filename(plugin))
                    })
                    .map_or(Value::None, Value::Form)
            }
        }
    }
}

impl Checker<'_> {
    /// A `JsonUtil` or `StorageUtil` call the VM serves, and what it returns.
    pub(in crate::fragment::vm) fn check_json_util(&mut self, class: &str, name: &str) -> Option<Kind> {
        let op = match (class, name.strip_prefix("file")) {
            // `StorageUtil.File*` is JsonUtil's value and list surface on one file.
            ("storageutil", Some(rest)) => parse_op(Flavor::Json, rest).filter(|op| matches!(op, Op::Set(_) | Op::Get(_) | Op::Unset(_) | Op::Has(_) | Op::Adjust(_) | Op::List(..) | Op::CountPrefix(..) | Op::CountAllPrefix | Op::ClearAll)),
            ("storageutil", None) => parse_op(Flavor::Storage, name),
            _ => parse_op(Flavor::Json, name),
        };
        match op {
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
    fn papyrus_util_names_parse_to_their_ops() {
        use Flavor::{Json as J, Storage as S};
        assert_eq!(parse_op(J, "setintvalue"), Some(Op::Set(JsonKind::Int)));
        assert_eq!(parse_op(J, "getformvalue"), Some(Op::Get(JsonKind::Form)));
        assert_eq!(parse_op(J, "adjustfloatvalue"), Some(Op::Adjust(JsonKind::Float)));
        assert_eq!(parse_op(J, "adjuststringvalue"), None);
        assert_eq!(parse_op(J, "stringlistadd"), Some(Op::List(JsonKind::String, ListOp::Add)));
        assert_eq!(parse_op(J, "intlistrandom"), Some(Op::List(JsonKind::Int, ListOp::Random)));
        assert_eq!(parse_op(J, "countfloatlistprefix"), Some(Op::CountPrefix(JsonKind::Float, true)));
        assert_eq!(parse_op(J, "setpathintvalue"), Some(Op::SetPath(JsonKind::Int)));
        assert_eq!(parse_op(J, "pathformelements"), Some(Op::PathElements(JsonKind::Form)));
        assert_eq!(parse_op(J, "findpathstringelement"), Some(Op::FindPathElement(JsonKind::String)));
        assert_eq!(parse_op(J, "setpathfloatarray"), Some(Op::SetPathArray(JsonKind::Float)));
        assert_eq!(parse_op(J, "ispathform"), Some(Op::IsPath(PathType::Form)));
        assert_eq!(parse_op(J, "pluckintvalue"), None, "JsonUtil has no pluck");
        assert_eq!(parse_op(J, "intlistpop"), None);
        assert_eq!(parse_op(S, "pluckintvalue"), Some(Op::Pluck(JsonKind::Int)));
        assert_eq!(parse_op(S, "formlistshift"), Some(Op::List(JsonKind::Form, ListOp::Shift)));
        assert_eq!(parse_op(S, "intlistinsert"), Some(Op::List(JsonKind::Int, ListOp::InsertAt)));
        assert_eq!(parse_op(S, "stringlistsort"), Some(Op::List(JsonKind::String, ListOp::Sort)));
        assert_eq!(parse_op(S, "countintvalueprefix"), Some(Op::CountEveryPrefix(JsonKind::Int, false)));
        assert_eq!(parse_op(S, "countobjintlistprefix"), Some(Op::CountPrefix(JsonKind::Int, true)));
        assert_eq!(parse_op(S, "clearformvalueprefix"), Some(Op::ClearEveryPrefix(JsonKind::Form, false)));
        assert_eq!(parse_op(S, "clearobjformvalueprefix"), Some(Op::ClearPrefix(JsonKind::Form, false)));
        assert_eq!(parse_op(S, "countallprefix"), Some(Op::CountEveryAllPrefix));
        assert_eq!(parse_op(S, "clearallobjprefix"), Some(Op::ClearAllPrefix));
        assert_eq!(parse_op(S, "debug_getintlistobjectcount"), Some(Op::ObjectCount(JsonKind::Int, true)));
        assert_eq!(parse_op(S, "debug_allobjstringkeys"), Some(Op::AllKeys(JsonKind::String, false)));
        assert_eq!(parse_op(S, "debug_allformlistobjs"), Some(Op::AllObjects(JsonKind::Form, true)));
        assert_eq!(parse_op(S, "debug_getfloatkey"), Some(Op::Key(JsonKind::Float, false)));
        assert_eq!(parse_op(S, "formlistfilterbytypes"), None, "not served");
        assert_eq!(parse_op(S, "debug_cleanup"), None);
        assert_eq!(parse_op(S, "setpathintvalue"), None, "paths are JsonUtil's");
    }
}
