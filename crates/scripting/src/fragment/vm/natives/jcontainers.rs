//! JContainers' Papyrus classes ([`crate::jcontainers`], LMC PRD Q5), served by the engine: `JValue`, `JArray`,
//! `JMap`, `JFormMap`, `JIntMap`, `JDB`, `JFormDB`, `JAtomic`, `JString` and `JContainers`, as JContainers'
//! `src/JContainers/src/api_3/tes_*.h` declare them (`ryobg/JContainers@0dcf61e0aa04`, version 4.3.4.0).
//!
//! An object is an `Int` handle. Every handle a call passes or returns renews the object's temporary owner
//! ([`JcHeap::touch`]). A form is held by its plugin and local id ([`FormRef`]); a form outside the load order by its
//! session id. Files resolve under `Data/` ([`json_util::resolve_data_path`]) through the profile's mod data
//! files, as `JsonUtil`'s do; `userDirectory()` is `Data/SKSE/Plugins/JCUser/` there.
//!
//! Not served, each a stop that names it: `JLua` and `JValue.evalLua*`, collection operators in paths (`@…`),
//! `JArray.addFromFormList`, `JArray.writeTo*PArray`, `JContainers.removeFileAtPath` and
//! `JContainers.contentsOfDirectoryAtPath`.

use super::*;
use crate::jcontainers::{array_index, form_data, parse_form_data, FormKeyed, JcHeap, JcKey, JcObject, JcValue, SESSION_SOURCE};
use crate::json_util;
use byroredux_sdk::identity::FormRef;
use serde_json::Value as Json;
use std::collections::BTreeMap;

/// The version served (`jcontainers_constants.h`: API 4, feature 3, minor 4, patch 0).
const VERSION: (i32, i32, i32, i32) = (4, 3, 4, 0);
/// Where `JContainers.userDirectory()` points, under `Data/` in the profile's mod data.
const USER_DIRECTORY: &str = "Data/SKSE/Plugins/JCUser/";

/// The JContainers classes the VM serves, lowercase.
pub(in crate::fragment::vm) const CLASSES: &[&str] =
    &["jvalue", "jarray", "jmap", "jformmap", "jintmap", "jdb", "jformdb", "jatomic", "jcontainers", "jstring"];

/// The heap's clock: seconds since the process started (temporary owners count real seconds).
fn now() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// A value type a function name ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Flt,
    Str,
    Obj,
    Form,
}

impl Ty {
    fn kind(self) -> Kind {
        match self {
            Ty::Int | Ty::Flt | Ty::Obj => Kind::Num,
            Ty::Str => Kind::Str,
            Ty::Form => Kind::Object,
        }
    }

    fn default(self) -> Value {
        match self {
            Ty::Int | Ty::Obj => Value::Int(0),
            Ty::Flt => Value::Float(0.0),
            Ty::Str => Value::Str(String::new()),
            Ty::Form => Value::None,
        }
    }
}

/// `name` without its trailing value type (`…Int`, `…Flt`, `…Str`, `…Obj`, `…Form`, and `JArray`'s
/// `…Integer`, `…Float`, `…String`, `…Object`), and that type.
fn split_ty(name: &str) -> Option<(&str, Ty)> {
    [
        ("integer", Ty::Int),
        ("float", Ty::Flt),
        ("string", Ty::Str),
        ("object", Ty::Obj),
        ("int", Ty::Int),
        ("flt", Ty::Flt),
        ("str", Ty::Str),
        ("obj", Ty::Obj),
        ("form", Ty::Form),
    ]
    .into_iter()
    .find_map(|(suffix, ty)| Some((name.strip_suffix(suffix)?, ty)))
}

/// What a served call returns, by class and lowercase name; `None` for a call not served.
fn jc_kind(class: &str, name: &str) -> Option<Kind> {
    let typed = |prefix: &str| split_ty(name).filter(|(head, _)| *head == prefix).map(|(_, ty)| ty);
    let map_key_kind = match class {
        "jmap" => Kind::Str,
        "jformmap" => Kind::Object,
        _ => Kind::Num,
    };
    Some(match class {
        "jvalue" => match name {
            "retain" | "release" | "releaseandretain" | "zerolifetime" | "addtopool" | "shallowcopy" | "deepcopy"
            | "readfromfile" | "readfromdirectory" | "objectfromprototype" | "count" | "solvedvaluetype" => Kind::Num,
            "releaseobjectswithtag" | "cleanpool" | "enableapilog" | "clear" | "writetofile" => Kind::None,
            "isexists" | "isarray" | "ismap" | "isformmap" | "isintegermap" | "empty" | "haspath" => Kind::Bool,
            "tojsonstring" => Kind::Str,
            _ => match (typed("solve"), name.strip_suffix("setter").and_then(split_ty)) {
                (Some(ty), _) => ty.kind(),
                (None, Some(("solve", _))) => Kind::Bool,
                _ => return None,
            },
        },
        "jarray" => match name {
            "object" | "objectwithsize" | "objectwithints" | "objectwithstrings" | "objectwithfloats" | "objectwithbooleans"
            | "objectwithforms" | "subarray" | "count" | "valuetype" | "sort" | "unique" | "reverse" => Kind::Num,
            "addfromarray" | "clear" | "eraseindex" | "eraserange" | "swapitems" => Kind::None,
            "asintarray" | "asfloatarray" | "asstringarray" | "asformarray" => Kind::Array,
            _ => {
                let (head, ty) = split_ty(name)?;
                match head {
                    "get" => ty.kind(),
                    "find" | "count" | "erase" => Kind::Num,
                    "set" | "add" => Kind::None,
                    _ => return None,
                }
            }
        },
        "jmap" | "jformmap" | "jintmap" => match name {
            "object" | "valuetype" | "count" | "allkeys" | "allvalues" => Kind::Num,
            "haskey" | "removekey" => Kind::Bool,
            "clear" | "addpairs" => Kind::None,
            "allkeysparray" => Kind::Array,
            "nextkey" | "getnthkey" => map_key_kind,
            _ => {
                let (head, ty) = split_ty(name)?;
                match head {
                    "get" => ty.kind(),
                    "set" | "insert" => Kind::None,
                    _ => return None,
                }
            }
        },
        "jdb" => match name {
            "setobj" | "writetofile" => Kind::None,
            "haspath" => Kind::Bool,
            "allkeys" | "allvalues" | "root" => Kind::Num,
            _ => match (typed("solve"), name.strip_suffix("setter").and_then(split_ty)) {
                (Some(ty), _) => ty.kind(),
                (None, Some(("solve", _))) => Kind::Bool,
                _ => return None,
            },
        },
        "jformdb" => match name {
            "setentry" => Kind::None,
            "makeentry" | "findentry" | "allkeys" | "allvalues" => Kind::Num,
            "haspath" => Kind::Bool,
            _ => match (split_ty(name), name.strip_suffix("setter").and_then(split_ty)) {
                (_, Some(("solve", _))) => Kind::Bool,
                (Some(("solve" | "get", ty)), _) => ty.kind(),
                (Some(("set", _)), _) => Kind::None,
                _ => return None,
            },
        },
        "jatomic" => {
            let rest = name.strip_prefix("fetch").or_else(|| name.strip_prefix("compareexchange")).or_else(|| name.strip_prefix("exchange"))?;
            let (op, ty) = split_ty(rest)?;
            let fetch = name.starts_with("fetch");
            match (fetch, op, ty) {
                (true, "add" | "mult" | "div", Ty::Int | Ty::Flt) | (true, "mod" | "and" | "xor" | "or", Ty::Int) => Kind::Num,
                (false, "", _) => ty.kind(),
                _ => return None,
            }
        }
        "jcontainers" => match name {
            "isinstalled" | "__isinstalled" | "versionatleast" | "fileexistsatpath" => Kind::Bool,
            "apiversion" | "featureversion" | "minorversion" | "patchversion" | "versionint" | "lasterror" => Kind::Num,
            "versionstring" | "userdirectory" | "lasterrorstring" => Kind::Str,
            _ => return None,
        },
        "jstring" => match name {
            "wrap" => Kind::Num,
            _ => return None,
        },
        _ => return None,
    })
}

/// Whether two values are equal for `find` / `count` / `erase`: numbers by value, strings without case, forms and
/// objects by identity.
fn same(a: &JcValue, b: &JcValue) -> bool {
    match (a, b) {
        (JcValue::Str(a), JcValue::Str(b)) => a.eq_ignore_ascii_case(b),
        _ => match (a.as_number(), b.as_number()) {
            (Some(a), Some(b)) => a == b,
            _ => a == b,
        },
    }
}

/// `JArray.sort`'s order: "none < int < float < form < object < string".
fn order(a: &JcValue, b: &JcValue) -> std::cmp::Ordering {
    let rank = |value: &JcValue| match value {
        JcValue::None => 0,
        JcValue::Int(_) => 1,
        JcValue::Float(_) => 2,
        JcValue::Form(_) => 3,
        JcValue::Object(_) => 4,
        JcValue::Str(_) => 5,
    };
    rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
        (JcValue::Int(a), JcValue::Int(b)) => a.cmp(b),
        (JcValue::Float(a), JcValue::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (JcValue::Form(a), JcValue::Form(b)) => a.cmp(b),
        (JcValue::Object(a), JcValue::Object(b)) => a.cmp(b),
        (JcValue::Str(a), JcValue::Str(b)) => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
        _ => std::cmp::Ordering::Equal,
    })
}

/// `text` wrapped to lines of at most `width` characters at spaces (`JString.wrap`).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    lines
}

impl Vm<'_> {
    /// A JContainers class call: `class` one of [`CLASSES`].
    pub(in crate::fragment::vm) fn read_jcontainers(&mut self, frame: &mut Frame, class: &str, name: &str, args: &[CallArg]) -> Run<Option<Value>> {
        if jc_kind(class, name).is_none() {
            return stop(format!("{class}.{name}"));
        }
        let values = self.eval_args(frame, args)?;
        if self.world.try_resource::<JcHeap>().is_none() {
            return stop("JContainers without its heap");
        }
        let now = now();
        // Every handle passed renews its temporary owner.
        {
            let mut heap = self.world.resource_mut::<JcHeap>();
            for value in &values {
                if let Value::Int(handle) = value {
                    heap.touch(*handle, now);
                }
            }
        }
        let answer = match class {
            "jvalue" => self.jc_value(name, &values, now),
            "jarray" => self.jc_array(name, &values, now),
            "jmap" | "jformmap" | "jintmap" => self.jc_map(class, name, &values, now),
            "jdb" => self.jc_db(name, &values, now),
            "jformdb" => self.jc_form_db(name, &values, now),
            "jatomic" => self.jc_atomic(name, &values, now),
            "jcontainers" => self.jc_containers(name, &values),
            _ => {
                let lines = wrap(&values.first().map(Value::text).unwrap_or_default(), self.jc_int(&values, 1, 60).max(1) as usize);
                let items = lines.into_iter().map(JcValue::Str).collect();
                Some(Value::Int(self.world.resource_mut::<JcHeap>().create(JcObject::Array(items), now)))
            }
        };
        let Some(answer) = answer else {
            return stop(format!("{class}.{name}"));
        };
        // And every handle returned.
        if let Value::Int(handle) = answer {
            self.world.resource_mut::<JcHeap>().touch(handle, now);
        }
        Ok(Some(answer))
    }

    fn jc_int(&self, values: &[Value], i: usize, default: i32) -> i32 {
        match values.get(i) {
            Some(Value::Int(n)) => *n,
            Some(value) => value.number().map_or(default, |n| n as i32),
            None => default,
        }
    }

    fn jc_text(&self, values: &[Value], i: usize) -> String {
        values.get(i).map(Value::text).unwrap_or_default()
    }

    fn jc_flag(&self, values: &[Value], i: usize, default: bool) -> bool {
        values.get(i).map_or(default, Value::truthy)
    }

    /// A form's identity: its plugin and local id, else its session id.
    fn jc_form_ref(&self, form: u32) -> FormRef {
        self.world
            .try_resource::<crate::load_order::LoadOrderIdentity>()
            .and_then(|identity| identity.pair(form))
            .map_or_else(|| FormRef::new(SESSION_SOURCE, form), |pair| FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0))
    }

    /// The session form an identity names, if it is loaded.
    fn jc_global(&self, form: FormRef) -> Option<u32> {
        if form.source() == SESSION_SOURCE {
            return Some(form.local());
        }
        self.world.try_resource::<crate::load_order::LoadOrderIdentity>()?.global_form_id(form)
    }

    /// A form as JContainers writes it (`__formData|Plugin|0x…`).
    fn jc_form_text(&self, form: FormRef) -> String {
        if form.source() == SESSION_SOURCE {
            return form_data(None, form.local());
        }
        let name = self
            .world
            .try_resource::<crate::LegacyObscriptContentCatalog>()
            .and_then(|catalog| catalog.0.iter().find(|plugin| plugin.source() == form.source()).map(|plugin| plugin.name().to_owned()));
        form_data(name.as_deref(), form.local())
    }

    /// The identity a JContainers form text names.
    fn jc_parse_form(text: &str) -> Option<FormRef> {
        let (plugin, id) = parse_form_data(text)?;
        if plugin.is_empty() {
            return Some(FormRef::new(SESSION_SOURCE, id));
        }
        Some(FormRef::new(byroredux_core::form_id::PluginId::from_filename(plugin).0.to_be_bytes(), id & 0x00FF_FFFF))
    }

    /// A script value stored as `ty`.
    fn jc_store(&self, ty: Ty, value: Option<&Value>) -> JcValue {
        match (ty, value) {
            (Ty::Int, value) => JcValue::Int(match value {
                Some(Value::Int(n)) => *n,
                other => other.and_then(Value::number).map_or(0, |n| n as i32),
            }),
            (Ty::Flt, value) => JcValue::Float(value.and_then(Value::number).unwrap_or(0.0)),
            (Ty::Str, value) => JcValue::Str(value.map(Value::text).unwrap_or_default()),
            (Ty::Obj, Some(Value::Int(handle))) if *handle != 0 => JcValue::Object(*handle),
            (Ty::Form, Some(Value::Form(form))) => JcValue::Form(self.jc_form_ref(*form)),
            _ => JcValue::None,
        }
    }

    /// A stored value read as `ty`, or `default` when it is not one.
    fn jc_load(&self, ty: Ty, value: Option<&JcValue>, default: Value) -> Value {
        match (ty, value) {
            (Ty::Int, Some(JcValue::Int(n))) => Value::Int(*n),
            (Ty::Int, Some(JcValue::Float(f))) => Value::Int(*f as i32),
            (Ty::Flt, Some(JcValue::Float(f))) => Value::Float(*f),
            (Ty::Flt, Some(JcValue::Int(n))) => Value::Float(*n as f32),
            (Ty::Str, Some(JcValue::Str(text))) => Value::Str(text.clone()),
            (Ty::Obj, Some(JcValue::Object(handle))) => Value::Int(*handle),
            (Ty::Form, Some(JcValue::Form(form))) => self.jc_global(*form).map_or(Value::None, Value::Form),
            _ => default,
        }
    }

    /// A map key from a script value, for `class`'s kind of map.
    fn jc_key(&self, class: &str, value: Option<&Value>) -> Option<JcKey> {
        Some(match class {
            "jmap" => JcKey::Str(value.map(Value::text).unwrap_or_default()),
            "jformmap" => match value {
                Some(Value::Form(form)) => JcKey::Form(self.jc_form_ref(*form)),
                _ => return None,
            },
            _ => JcKey::Int(match value {
                Some(Value::Int(n)) => *n,
                other => other.and_then(Value::number).map_or(0, |n| n as i32),
            }),
        })
    }

    /// A map key as a script sees it.
    fn jc_key_value(&self, key: &JcKey) -> Value {
        match key {
            JcKey::Str(text) => Value::Str(text.clone()),
            JcKey::Form(form) => self.jc_global(*form).map_or(Value::None, Value::Form),
            JcKey::Int(n) => Value::Int(*n),
            JcKey::Index(i) => Value::Int(*i as i32),
        }
    }

    /// A key stored as a value (`allKeys`).
    fn jc_key_stored(key: &JcKey) -> JcValue {
        match key {
            JcKey::Str(text) => JcValue::Str(text.clone()),
            JcKey::Form(form) => JcValue::Form(*form),
            JcKey::Int(n) => JcValue::Int(*n),
            JcKey::Index(i) => JcValue::Int(*i as i32),
        }
    }

    fn jc_new(&self, object: JcObject, now: f64) -> Value {
        Value::Int(self.world.resource_mut::<JcHeap>().create(object, now))
    }

    /// A Papyrus array of stored values read as `ty`.
    fn jc_papyrus_array(&self, items: &[JcValue], ty: Ty) -> Value {
        let values = items.iter().map(|item| self.jc_load(ty, Some(item), ty.default())).collect();
        Value::Array(Rc::new(RefCell::new(values)))
    }

    /// The values of a Papyrus array argument stored as `ty`.
    fn jc_from_papyrus(&self, value: Option<&Value>, ty: Ty) -> Vec<JcValue> {
        match value {
            Some(Value::Array(items)) => items.borrow().iter().map(|item| self.jc_store(ty, Some(item))).collect(),
            _ => Vec::new(),
        }
    }

    /// `solve*` from `handle` at `path`, read as `ty` (default otherwise).
    fn jc_solve(&self, handle: i32, path: &str, ty: Ty, default: Value) -> Value {
        let found = self.world.resource::<JcHeap>().solve(handle, path, &Self::jc_parse_form);
        self.jc_load(ty, found.as_ref(), default)
    }

    fn jc_solve_set(&self, handle: i32, path: &str, value: JcValue, create: bool, now: f64) -> bool {
        self.world.resource_mut::<JcHeap>().solve_set(handle, path, value, create, now, &Self::jc_parse_form)
    }

    /// The JSON text of a file under `Data/`, through the profile's mod data files.
    fn jc_read_file(&self, path: &str) -> Option<Json> {
        let resolved = json_util::resolve_data_path(path)?;
        let files = self.world.try_resource::<json_util::JsonUtilStore>()?.files()?;
        let bytes = files.read(&resolved)?;
        if bytes.len() > json_util::MAX_FILE_BYTES {
            log::warn!("JContainers: {path} is over {} bytes", json_util::MAX_FILE_BYTES);
            return None;
        }
        serde_json::from_slice(&bytes).map_err(|error| log::warn!("JContainers: {path}: {error}")).ok()
    }

    /// Write `handle` as JSON to a file under `Data/` (the profile's copy).
    fn jc_write_file(&self, handle: i32, path: &str) {
        let Some(resolved) = json_util::resolve_data_path(path) else {
            log::warn!("JContainers: {path:?} leaves Data/; not written");
            return;
        };
        let json = self.world.resource::<JcHeap>().to_json(handle, &|form| self.jc_form_text(form));
        let Ok(text) = serde_json::to_string_pretty(&json) else {
            return;
        };
        if text.len() > json_util::MAX_FILE_BYTES {
            log::warn!("JContainers: {path} not written: over {} bytes", json_util::MAX_FILE_BYTES);
            return;
        }
        let files = self.world.try_resource::<json_util::JsonUtilStore>().and_then(|store| store.files());
        if let Some(Err(error)) = files.map(|files| files.write(&resolved, &text)) {
            log::warn!("JContainers: {path} not written: {error}");
        }
    }

    /// `JValue`: lifetime, copies, type tests, files and paths on any object.
    fn jc_value(&self, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let handle = self.jc_int(values, 0, 0);
        let text = |i| self.jc_text(values, i);
        let heap = || self.world.resource_mut::<JcHeap>();
        Some(match name {
            "retain" => Value::Int(heap().retain(handle, &text(1))),
            "release" => Value::Int(heap().release(handle)),
            "releaseandretain" => {
                let next = self.jc_int(values, 1, 0);
                let mut heap = heap();
                if handle != next && handle != 0 {
                    heap.release(handle);
                }
                Value::Int(heap.retain(next, &text(2)))
            }
            "releaseobjectswithtag" => {
                heap().release_tagged(&text(0));
                Value::None
            }
            "zerolifetime" => Value::Int(heap().zero_lifetime(handle)),
            "addtopool" | "cleanpool" => {
                // A pool is an array JDB holds at `.__tempPools.<name>` (JContainers' `JC_OBJECT_POOL_KEY`).
                let pool_name = if name == "addtopool" { text(1) } else { text(0) };
                let root = heap().jdb(now);
                let path = format!(".__tempPools.{pool_name}");
                let mut pool = match heap().solve(root, &path, &Self::jc_parse_form) {
                    Some(JcValue::Object(pool)) => pool,
                    _ => 0,
                };
                if pool == 0 {
                    pool = heap().create(JcObject::Array(Vec::new()), now);
                    self.jc_solve_set(root, &path, JcValue::Object(pool), true, now);
                }
                if name == "cleanpool" {
                    heap().clear(pool);
                    return Some(Value::None);
                }
                if let Some(JcObject::Array(items)) = heap().object_mut(pool) {
                    items.push(JcValue::Object(handle));
                }
                Value::Int(handle)
            }
            "shallowcopy" => Value::Int(heap().copy(handle, false, now)),
            "deepcopy" => Value::Int(heap().copy(handle, true, now)),
            "isexists" => Value::Bool(heap().contains(handle)),
            "isarray" => Value::Bool(matches!(heap().object(handle), Some(JcObject::Array(_)))),
            "ismap" => Value::Bool(matches!(heap().object(handle), Some(JcObject::Map(_)))),
            "isformmap" => Value::Bool(matches!(heap().object(handle), Some(JcObject::FormMap(_)))),
            "isintegermap" => Value::Bool(matches!(heap().object(handle), Some(JcObject::IntMap(_)))),
            "empty" => Value::Bool(heap().object(handle).is_none_or(JcObject::is_empty)),
            "count" => Value::Int(heap().object(handle).map_or(0, JcObject::len) as i32),
            "clear" => {
                heap().clear(handle);
                Value::None
            }
            "enableapilog" => Value::None,
            "readfromfile" => match self.jc_read_file(&text(0)) {
                Some(json) => Value::Int(heap().from_json(&json, now, &Self::jc_parse_form)),
                None => Value::Int(0),
            },
            "readfromdirectory" => {
                // A map of file name -> the object its JSON makes.
                let folder = json_util::resolve_data_path(&text(0))?;
                let extension = text(1).to_ascii_lowercase();
                let files = self.world.try_resource::<json_util::JsonUtilStore>().and_then(|store| store.files());
                let names = files.map(|files| files.list(&folder)).unwrap_or_default();
                let map = heap().create(JcObject::Map(BTreeMap::new()), now);
                for file in names.into_iter().filter(|file| extension.is_empty() || file.ends_with(&extension)) {
                    if let Some(json) = self.jc_read_file(&format!("{folder}/{file}")) {
                        let object = heap().from_json(&json, now, &Self::jc_parse_form);
                        heap().set(map, JcKey::Str(file), JcValue::Object(object));
                    }
                }
                Value::Int(map)
            }
            "objectfromprototype" => match serde_json::from_str::<Json>(&text(0)) {
                Ok(json) => Value::Int(heap().from_json(&json, now, &Self::jc_parse_form)),
                Err(_) => Value::Int(0),
            },
            "writetofile" => {
                self.jc_write_file(handle, &text(1));
                Value::None
            }
            "tojsonstring" => {
                let json = heap().to_json(handle, &|form| self.jc_form_text(form));
                Value::Str(serde_json::to_string(&json).unwrap_or_default())
            }
            "solvedvaluetype" => Value::Int(heap().solve(handle, &text(1), &Self::jc_parse_form).map_or(0, |value| value.type_code())),
            "haspath" => Value::Bool(heap().solve(handle, &text(1), &Self::jc_parse_form).is_some()),
            _ => {
                if let Some(("solve", ty)) = name.strip_suffix("setter").and_then(split_ty) {
                    let value = self.jc_store(ty, values.get(2));
                    return Some(Value::Bool(self.jc_solve_set(handle, &text(1), value, self.jc_flag(values, 3, false), now)));
                }
                let (_, ty) = split_ty(name)?;
                let default = values.get(2).cloned().unwrap_or_else(|| ty.default());
                self.jc_solve(handle, &text(1), ty, default)
            }
        })
    }

    /// `JArray`.
    fn jc_array(&self, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let handle = self.jc_int(values, 0, 0);
        let heap = || self.world.resource_mut::<JcHeap>();
        let items = || match heap().object(handle) {
            Some(JcObject::Array(items)) => Some(items.clone()),
            _ => None,
        };
        let edit = |f: &mut dyn FnMut(&mut Vec<JcValue>)| {
            if let Some(JcObject::Array(items)) = heap().object_mut(handle) {
                f(items);
            }
        };
        Some(match name {
            "object" => self.jc_new(JcObject::Array(Vec::new()), now),
            "objectwithsize" => self.jc_new(JcObject::Array(vec![JcValue::None; self.jc_int(values, 0, 0).clamp(0, 1 << 16) as usize]), now),
            "objectwithints" => self.jc_new(JcObject::Array(self.jc_from_papyrus(values.first(), Ty::Int)), now),
            "objectwithfloats" => self.jc_new(JcObject::Array(self.jc_from_papyrus(values.first(), Ty::Flt)), now),
            "objectwithstrings" => self.jc_new(JcObject::Array(self.jc_from_papyrus(values.first(), Ty::Str)), now),
            "objectwithforms" => self.jc_new(JcObject::Array(self.jc_from_papyrus(values.first(), Ty::Form)), now),
            "objectwithbooleans" => self.jc_new(JcObject::Array(self.jc_from_papyrus(values.first(), Ty::Int)), now),
            "subarray" => {
                // "[startIndex, endIndex)".
                let items = items().unwrap_or_default();
                let start = self.jc_int(values, 1, 0).max(0) as usize;
                let end = (self.jc_int(values, 2, 0).max(0) as usize).min(items.len());
                self.jc_new(JcObject::Array(items.get(start..end).map(<[JcValue]>::to_vec).unwrap_or_default()), now)
            }
            "addfromarray" => {
                let source = match heap().object(self.jc_int(values, 1, 0)) {
                    Some(JcObject::Array(source)) => source.clone(),
                    _ => Vec::new(),
                };
                let at = self.jc_int(values, 2, -1);
                edit(&mut |items| {
                    let index = if at < 0 { items.len() } else { (at as usize).min(items.len()) };
                    items.splice(index..index, source.iter().cloned());
                });
                Value::None
            }
            "count" => Value::Int(items().map_or(0, |items| items.len()) as i32),
            "clear" => {
                heap().clear(handle);
                Value::None
            }
            "eraseindex" => {
                let at = self.jc_int(values, 1, 0) as i64;
                edit(&mut |items| {
                    if let Some(i) = array_index(items.len(), at) {
                        items.remove(i);
                    }
                });
                Value::None
            }
            "eraserange" => {
                // "Erases [first, last] index range"; negative indexes from the end.
                let (first, last) = (self.jc_int(values, 1, 0) as i64, self.jc_int(values, 2, 0) as i64);
                edit(&mut |items| {
                    if let (Some(first), Some(last)) = (array_index(items.len(), first), array_index(items.len(), last))
                        && first <= last
                    {
                        items.drain(first..=last);
                    }
                });
                Value::None
            }
            "swapitems" => {
                let (a, b) = (self.jc_int(values, 1, 0) as i64, self.jc_int(values, 2, 0) as i64);
                edit(&mut |items| {
                    if let (Some(a), Some(b)) = (array_index(items.len(), a), array_index(items.len(), b)) {
                        items.swap(a, b);
                    }
                });
                Value::None
            }
            "valuetype" => {
                let at = self.jc_int(values, 1, 0) as i64;
                Value::Int(items().and_then(|items| array_index(items.len(), at).map(|i| items[i].type_code())).unwrap_or(0))
            }
            "sort" | "unique" | "reverse" => {
                edit(&mut |items| match name {
                    "reverse" => items.reverse(),
                    _ => {
                        items.sort_by(order);
                        if name == "unique" {
                            items.dedup_by(|a, b| order(a, b) == std::cmp::Ordering::Equal);
                        }
                    }
                });
                Value::Int(handle)
            }
            "asintarray" => self.jc_papyrus_array(&items().unwrap_or_default(), Ty::Int),
            "asfloatarray" => self.jc_papyrus_array(&items().unwrap_or_default(), Ty::Flt),
            "asstringarray" => self.jc_papyrus_array(&items().unwrap_or_default(), Ty::Str),
            "asformarray" => self.jc_papyrus_array(&items().unwrap_or_default(), Ty::Form),
            _ => {
                let (head, ty) = split_ty(name)?;
                match head {
                    "get" => {
                        let at = self.jc_int(values, 1, 0) as i64;
                        let item = items().and_then(|items| array_index(items.len(), at).map(|i| items[i].clone()));
                        self.jc_load(ty, item.as_ref(), values.get(2).cloned().unwrap_or_else(|| ty.default()))
                    }
                    "set" => {
                        let value = self.jc_store(ty, values.get(2));
                        heap().set(handle, JcKey::Index(self.jc_int(values, 1, 0) as i64), value);
                        Value::None
                    }
                    "add" => {
                        let value = self.jc_store(ty, values.get(1));
                        let at = self.jc_int(values, 2, -1);
                        edit(&mut |items| {
                            let index = if at < 0 { items.len() } else { (at as usize).min(items.len()) };
                            items.insert(index, value.clone());
                        });
                        Value::None
                    }
                    "find" => {
                        // "searchStartIndex - index of the array where to start search"; a negative one searches
                        // backwards from that index counted from the end.
                        let needle = self.jc_store(ty, values.get(1));
                        let start = self.jc_int(values, 2, 0) as i64;
                        let items = items().unwrap_or_default();
                        let found = if start >= 0 {
                            items.iter().enumerate().skip(start as usize).find(|(_, item)| same(item, &needle)).map(|(i, _)| i)
                        } else {
                            array_index(items.len(), start).and_then(|from| (0..=from).rev().find(|&i| same(&items[i], &needle)))
                        };
                        Value::Int(found.map_or(-1, |i| i as i32))
                    }
                    "count" => {
                        let needle = self.jc_store(ty, values.get(1));
                        Value::Int(items().unwrap_or_default().iter().filter(|item| same(item, &needle)).count() as i32)
                    }
                    "erase" => {
                        // "Erase all elements of given value. Returns the number of erased elements."
                        let needle = self.jc_store(ty, values.get(1));
                        let mut erased = 0;
                        edit(&mut |items| {
                            let before = items.len();
                            items.retain(|item| !same(item, &needle));
                            erased = before - items.len();
                        });
                        Value::Int(erased as i32)
                    }
                    _ => return None,
                }
            }
        })
    }

    /// `JMap`, `JFormMap`, `JIntMap`.
    fn jc_map(&self, class: &str, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let handle = self.jc_int(values, 0, 0);
        let heap = || self.world.resource_mut::<JcHeap>();
        let empty_key = || match class {
            "jmap" => Value::Str(String::new()),
            "jformmap" => Value::None,
            _ => Value::Int(0),
        };
        Some(match name {
            "object" => self.jc_new(
                match class {
                    "jmap" => JcObject::Map(BTreeMap::new()),
                    "jformmap" => JcObject::FormMap(FormKeyed::default()),
                    _ => JcObject::IntMap(BTreeMap::new()),
                },
                now,
            ),
            "haskey" => Value::Bool(self.jc_key(class, values.get(1)).is_some_and(|key| heap().get(handle, &key).is_some())),
            "valuetype" => Value::Int(self.jc_key(class, values.get(1)).and_then(|key| heap().get(handle, &key).map(JcValue::type_code)).unwrap_or(0)),
            "removekey" => Value::Bool(self.jc_key(class, values.get(1)).is_some_and(|key| heap().remove(handle, &key))),
            "count" => Value::Int(heap().object(handle).map_or(0, JcObject::len) as i32),
            "clear" => {
                heap().clear(handle);
                Value::None
            }
            "allkeys" => {
                let keys: Vec<JcValue> = heap().keys(handle).iter().map(Self::jc_key_stored).collect();
                self.jc_new(JcObject::Array(keys), now)
            }
            "allkeysparray" => {
                let keys: Vec<Value> = heap().keys(handle).iter().map(|key| self.jc_key_value(key)).collect();
                Value::Array(Rc::new(RefCell::new(keys)))
            }
            "allvalues" => {
                let values: Vec<JcValue> = heap().object(handle).map(|object| object.values().cloned().collect()).unwrap_or_default();
                self.jc_new(JcObject::Array(values), now)
            }
            "addpairs" => {
                let source = self.jc_int(values, 1, 0);
                let replace = self.jc_flag(values, 2, false);
                let pairs: Vec<(JcKey, JcValue)> = {
                    let heap = heap();
                    heap.keys(source).into_iter().filter_map(|key| Some((key.clone(), heap.get(source, &key)?.clone()))).collect()
                };
                let mut heap = heap();
                for (key, value) in pairs {
                    if replace || heap.get(handle, &key).is_none() {
                        heap.set(handle, key, value);
                    }
                }
                Value::None
            }
            "nextkey" => {
                // "If @previousKey == @endKey the function returns the first key ... returns @endKey only once to
                // signal that iteration has reached its end."
                let keys = heap().keys(handle);
                let end = values.get(2).cloned().unwrap_or_else(empty_key);
                let previous = self.jc_key(class, values.get(1));
                let end_key = self.jc_key(class, Some(&end));
                let next = match previous {
                    Some(previous) if Some(&previous) != end_key.as_ref() => {
                        keys.iter().position(|key| key_eq(key, &previous)).and_then(|i| keys.get(i + 1))
                    }
                    _ => keys.first(),
                };
                next.map_or(end, |key| self.jc_key_value(key))
            }
            "getnthkey" => {
                let keys = heap().keys(handle);
                let at = self.jc_int(values, 1, 0) as i64;
                array_index(keys.len(), at).map_or_else(empty_key, |i| self.jc_key_value(&keys[i]))
            }
            _ => {
                let (head, ty) = split_ty(name)?;
                let key = self.jc_key(class, values.get(1));
                match head {
                    "get" => {
                        let item = key.and_then(|key| heap().get(handle, &key).cloned());
                        self.jc_load(ty, item.as_ref(), values.get(2).cloned().unwrap_or_else(|| ty.default()))
                    }
                    "set" | "insert" => {
                        // insert: "Does nothing if the @key already exists".
                        let value = self.jc_store(ty, values.get(2));
                        if let Some(key) = key {
                            let mut heap = heap();
                            if head == "set" || heap.get(handle, &key).is_none() {
                                heap.set(handle, key, value);
                            }
                        }
                        Value::None
                    }
                    _ => return None,
                }
            }
        })
    }

    /// `JDB`: the root map every session object can hang from.
    fn jc_db(&self, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let root = self.world.resource_mut::<JcHeap>().jdb(now);
        let text = |i| self.jc_text(values, i);
        Some(match name {
            "root" => Value::Int(root),
            "setobj" => {
                // "attaches the object under the key"; 0 detaches it.
                let object = self.jc_int(values, 1, 0);
                let mut heap = self.world.resource_mut::<JcHeap>();
                if object == 0 {
                    heap.remove(root, &JcKey::Str(text(0)));
                } else {
                    heap.set(root, JcKey::Str(text(0)), JcValue::Object(object));
                }
                Value::None
            }
            "haspath" => Value::Bool(self.world.resource::<JcHeap>().solve(root, &text(0), &Self::jc_parse_form).is_some()),
            "allkeys" | "allvalues" => {
                let mut heap = self.world.resource_mut::<JcHeap>();
                let items: Vec<JcValue> = if name == "allkeys" {
                    heap.keys(root).iter().map(Self::jc_key_stored).collect()
                } else {
                    heap.object(root).map(|object| object.values().cloned().collect()).unwrap_or_default()
                };
                Value::Int(heap.create(JcObject::Array(items), now))
            }
            "writetofile" => {
                self.jc_write_file(root, &text(0));
                Value::None
            }
            _ => {
                if let Some(("solve", ty)) = name.strip_suffix("setter").and_then(split_ty) {
                    let value = self.jc_store(ty, values.get(1));
                    return Some(Value::Bool(self.jc_solve_set(root, &text(0), value, self.jc_flag(values, 2, false), now)));
                }
                let (_, ty) = split_ty(name)?;
                self.jc_solve(root, &text(0), ty, values.get(1).cloned().unwrap_or_else(|| ty.default()))
            }
        })
    }

    /// `JFormDB`: per-form entries, `JDB[storageName]` a `JFormMap` of form -> entry `JMap`; a path is
    /// `.storageName.rest`.
    fn jc_form_db(&self, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let form_key = |i: usize| match values.get(i) {
            Some(Value::Form(form)) => Some(self.jc_form_ref(*form)),
            _ => None,
        };
        // The storage, its form map (made when `make`), and the entry for `form` (made when `make`).
        let entry = |storage: &str, form: Option<FormRef>, make: bool| -> i32 {
            let (Some(form), false) = (form, storage.is_empty()) else {
                return 0;
            };
            let mut heap = self.world.resource_mut::<JcHeap>();
            let root = heap.jdb(now);
            let mut map = match heap.get(root, &JcKey::Str(storage.to_owned())) {
                Some(JcValue::Object(map)) if matches!(heap.object(*map), Some(JcObject::FormMap(_))) => *map,
                _ => 0,
            };
            if map == 0 && make {
                map = heap.create(JcObject::FormMap(FormKeyed::default()), now);
                heap.set(root, JcKey::Str(storage.to_owned()), JcValue::Object(map));
            }
            if map == 0 {
                return 0;
            }
            match heap.get(map, &JcKey::Form(form)) {
                Some(JcValue::Object(entry)) if heap.contains(*entry) => *entry,
                _ if make => {
                    let entry = heap.create(JcObject::Map(BTreeMap::new()), now);
                    heap.set(map, JcKey::Form(form), JcValue::Object(entry));
                    entry
                }
                _ => 0,
            }
        };
        Some(match name {
            "setentry" => {
                // "set entry to zero to destroy association".
                let storage = self.jc_text(values, 0);
                let object = self.jc_int(values, 2, 0);
                if let Some(form) = form_key(1).filter(|_| !storage.is_empty()) {
                    let mut heap = self.world.resource_mut::<JcHeap>();
                    let root = heap.jdb(now);
                    let map = match heap.get(root, &JcKey::Str(storage.clone())) {
                        Some(JcValue::Object(map)) => *map,
                        _ if object != 0 => {
                            let map = heap.create(JcObject::FormMap(FormKeyed::default()), now);
                            heap.set(root, JcKey::Str(storage), JcValue::Object(map));
                            map
                        }
                        _ => 0,
                    };
                    if object == 0 {
                        heap.remove(map, &JcKey::Form(form));
                    } else {
                        heap.set(map, JcKey::Form(form), JcValue::Object(object));
                    }
                }
                Value::None
            }
            "makeentry" => Value::Int(entry(&self.jc_text(values, 0), form_key(1), true)),
            "findentry" => Value::Int(entry(&self.jc_text(values, 0), form_key(1), false)),
            _ => {
                // The rest name a form, then `.storageName…`.
                let (storage, rest) = split_storage(&self.jc_text(values, 1))?;
                let form = form_key(0);
                if let Some(("solve", ty)) = name.strip_suffix("setter").and_then(split_ty) {
                    let value = self.jc_store(ty, values.get(2));
                    let target = entry(&storage, form, true);
                    return Some(Value::Bool(target != 0 && self.jc_solve_set(target, &rest, value, self.jc_flag(values, 3, false), now)));
                }
                match name {
                    "haspath" => {
                        let target = entry(&storage, form, false);
                        Value::Bool(target != 0 && self.world.resource::<JcHeap>().solve(target, &rest, &Self::jc_parse_form).is_some())
                    }
                    "allkeys" | "allvalues" => {
                        let target = entry(&storage, form, false);
                        let object = match self.world.resource::<JcHeap>().solve(target, &rest, &Self::jc_parse_form) {
                            Some(JcValue::Object(object)) => object,
                            _ => target,
                        };
                        let mut heap = self.world.resource_mut::<JcHeap>();
                        let items: Vec<JcValue> = if name == "allkeys" {
                            heap.keys(object).iter().map(Self::jc_key_stored).collect()
                        } else {
                            heap.object(object).map(|object| object.values().cloned().collect()).unwrap_or_default()
                        };
                        Value::Int(heap.create(JcObject::Array(items), now))
                    }
                    _ => {
                        let (head, ty) = split_ty(name)?;
                        match head {
                            "solve" | "get" => {
                                let target = entry(&storage, form, false);
                                let default = values.get(2).cloned().unwrap_or_else(|| ty.default());
                                if target == 0 { default } else { self.jc_solve(target, &rest, ty, default) }
                            }
                            "set" => {
                                let value = self.jc_store(ty, values.get(2));
                                let target = entry(&storage, form, true);
                                if target != 0 {
                                    self.jc_solve_set(target, &rest, value, true, now);
                                }
                                Value::None
                            }
                            _ => return None,
                        }
                    }
                }
            }
        })
    }

    /// `JAtomic`: read-modify-write at a path, returning the previous value.
    fn jc_atomic(&self, name: &str, values: &[Value], now: f64) -> Option<Value> {
        let handle = self.jc_int(values, 0, 0);
        let path = self.jc_text(values, 1);
        let current = self.world.resource::<JcHeap>().solve(handle, &path, &Self::jc_parse_form);
        if let Some(rest) = name.strip_prefix("fetch") {
            // (object, path, value, initialValue, createMissingKeys, onErrorReturn).
            let (op, ty) = split_ty(rest)?;
            let operand = values.get(2).and_then(Value::number).unwrap_or(0.0) as f64;
            let previous = current.as_ref().and_then(JcValue::as_number).unwrap_or_else(|| values.get(3).and_then(Value::number).unwrap_or(0.0) as f64);
            let next = match (op, ty) {
                ("add", _) => previous + operand,
                ("mult", _) => previous * operand,
                ("div", _) if operand != 0.0 => previous / operand,
                ("mod", _) if operand != 0.0 => ((previous as i32) % (operand as i32)) as f64,
                ("and", _) => ((previous as i64 as u32) & (operand as i64 as u32)) as i32 as f64,
                ("xor", _) => ((previous as i64 as u32) ^ (operand as i64 as u32)) as i32 as f64,
                ("or", _) => ((previous as i64 as u32) | (operand as i64 as u32)) as i32 as f64,
                _ => return Some(values.get(5).cloned().unwrap_or_else(|| ty.default())),
            };
            let stored = match ty {
                Ty::Int => JcValue::Int(next as i32),
                _ => JcValue::Float(next as f32),
            };
            if !self.jc_solve_set(handle, &path, stored, self.jc_flag(values, 4, false), now) {
                return Some(values.get(5).cloned().unwrap_or_else(|| ty.default()));
            }
            return Some(match ty {
                Ty::Int => Value::Int(previous as i32),
                _ => Value::Float(previous as f32),
            });
        }
        let compare = name.starts_with("compareexchange");
        let (_, ty) = split_ty(name.strip_prefix("compareexchange").or_else(|| name.strip_prefix("exchange"))?)?;
        let new = self.jc_store(ty, values.get(2));
        // exchange (object, path, value, createMissingKeys, onError); compareExchange (…, comparer, createMissingKeys,
        // onError).
        let (create_at, error_at) = if compare { (4, 5) } else { (3, 4) };
        let on_error = values.get(error_at).cloned().unwrap_or_else(|| ty.default());
        let previous = self.jc_load(ty, current.as_ref(), ty.default());
        let matches = !compare || {
            let comparer = self.jc_store(ty, values.get(3));
            current.as_ref().is_some_and(|current| same(current, &comparer)) || (current.is_none() && comparer == JcValue::None)
        };
        if matches && !self.jc_solve_set(handle, &path, new, self.jc_flag(values, create_at, false), now) {
            return Some(on_error);
        }
        Some(previous)
    }

    /// `JContainers`: the version and file tests.
    fn jc_containers(&self, name: &str, values: &[Value]) -> Option<Value> {
        let (api, feature, minor, patch) = VERSION;
        Some(match name {
            "isinstalled" | "__isinstalled" => Value::Bool(true),
            "apiversion" => Value::Int(api),
            "featureversion" => Value::Int(feature),
            "minorversion" => Value::Int(minor),
            "patchversion" => Value::Int(patch),
            "versionint" => Value::Int(api * 1_000_000 + feature * 10_000 + minor * 100 + patch),
            "versionstring" => Value::Str(format!("{api}.{feature}.{minor}.{patch}")),
            "versionatleast" => {
                let asked = (self.jc_int(values, 0, 0), self.jc_int(values, 1, 0), self.jc_int(values, 2, 0), self.jc_int(values, 3, 0));
                Value::Bool(VERSION >= asked)
            }
            "fileexistsatpath" => Value::Bool(
                json_util::resolve_data_path(&self.jc_text(values, 0))
                    .zip(self.world.try_resource::<json_util::JsonUtilStore>().and_then(|store| store.files()))
                    .is_some_and(|(path, files)| files.read(&path).is_some()),
            ),
            "userdirectory" => Value::Str(USER_DIRECTORY.to_owned()),
            "lasterror" => Value::Int(0),
            "lasterrorstring" => Value::Str(String::new()),
            _ => return None,
        })
    }
}

/// Whether two map keys are the same key (string keys without case).
fn key_eq(a: &JcKey, b: &JcKey) -> bool {
    match (a, b) {
        (JcKey::Str(a), JcKey::Str(b)) => a.eq_ignore_ascii_case(b),
        _ => a == b,
    }
}

/// A `JFormDB` path's storage name and the rest of the path from its separator on (`.frostfall.keyB` ->
/// `frostfall`, `.keyB`).
fn split_storage(path: &str) -> Option<(String, String)> {
    let after = path.strip_prefix('.')?;
    let end = after.find(['.', '['])?;
    (end > 0).then(|| (after[..end].to_owned(), after[end..].to_owned()))
}

impl Checker<'_> {
    /// A JContainers call the VM serves, and what it returns.
    pub(in crate::fragment::vm) fn check_jcontainers(&mut self, class: &str, name: &str) -> Option<Kind> {
        match jc_kind(class, name) {
            Some(kind) => Some(kind),
            None => self.fail(|| format!("{class}.{name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn served_names_and_their_kinds() {
        assert_eq!(jc_kind("jvalue", "retain"), Some(Kind::Num));
        assert_eq!(jc_kind("jvalue", "solvestr"), Some(Kind::Str));
        assert_eq!(jc_kind("jvalue", "solveformsetter"), Some(Kind::Bool));
        assert_eq!(jc_kind("jvalue", "evalluaint"), None, "Lua is not served");
        assert_eq!(jc_kind("jarray", "getform"), Some(Kind::Object));
        assert_eq!(jc_kind("jarray", "countinteger"), Some(Kind::Num));
        assert_eq!(jc_kind("jarray", "eraseobject"), Some(Kind::Num));
        assert_eq!(jc_kind("jarray", "addfromformlist"), None);
        assert_eq!(jc_kind("jformmap", "nextkey"), Some(Kind::Object));
        assert_eq!(jc_kind("jintmap", "getnthkey"), Some(Kind::Num));
        assert_eq!(jc_kind("jmap", "insertstr"), Some(Kind::None));
        assert_eq!(jc_kind("jdb", "solveintsetter"), Some(Kind::Bool));
        assert_eq!(jc_kind("jformdb", "getflt"), Some(Kind::Num));
        assert_eq!(jc_kind("jatomic", "fetchaddint"), Some(Kind::Num));
        assert_eq!(jc_kind("jatomic", "compareexchangestr"), Some(Kind::Str));
        assert_eq!(jc_kind("jatomic", "fetchmodflt"), None);
        assert_eq!(jc_kind("jcontainers", "versionatleast"), Some(Kind::Bool));
        assert_eq!(split_storage(".frostfall.keyB"), Some(("frostfall".to_owned(), ".keyB".to_owned())));
        assert_eq!(wrap("the quick brown fox", 9), vec!["the quick", "brown fox"]);
    }
}
