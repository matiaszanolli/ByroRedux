//! JContainers (LMC PRD Q5, LMC-SCR-004; `docs/engine/jcontainers.md`): the object heap the Papyrus VM serves
//! `JValue`, `JArray`, `JMap`, `JFormMap`, `JIntMap`, `JDB`, `JFormDB` and `JAtomic` from, without the
//! JContainers DLL.
//!
//! **The contract** (JContainers `src/JContainers/src/api_3/*.h`, `ryobg/JContainers@0dcf61e0aa04`, MIT; its wiki
//! pages *Lifetime Management*, *Serialization* and *Path Resolving*):
//!
//! - An object is an integer handle. "Any container object may have zero, one or more owners. As long as an object
//!   has at least one owner, it continues to exist": a `retain`, a container holding it, or `JDB` reaching it.
//! - A new object, or one returned to Papyrus, is owned temporarily "for around 10 seconds"; "an object which is not
//!   referenced directly or indirectly by `JDB` or `JFormDB` gets destroyed after around 10 seconds".
//! - "Every container object persists in save file until the container gets destroyed."
//! - A `JMap`'s keys compare without case (`map_case_insensitive_comp`), keeping their spelling.
//!
//! **The engine's rules.** Collection is a mark from the owners (retains, `JDB`, and the temporary owners whose 10
//! seconds have not run out) and a sweep of the rest, run as the heap grows and when the game saves. Every handle a
//! script passes or receives renews its temporary ownership, so an object in use is never swept: the heap may keep
//! an object longer than JContainers would, never shorter. The heap is bounded at [`MAX_OBJECTS`].

use std::collections::{BTreeMap, HashMap};

use byroredux_core::ecs::resource::Resource;
use byroredux_sdk::identity::FormRef;
use serde_json::{Map, Value as Json};

/// How long a temporary owner keeps an object, in seconds (wiki *Lifetime Management*: "around 10 seconds").
pub const TEMPORARY_SECONDS: f64 = 10.0;
/// The most live objects (an engine bound; Q5 "bounded size").
pub const MAX_OBJECTS: usize = 262_144;
/// The fewest objects before the heap sweeps as it grows.
const FIRST_SWEEP: usize = 4_096;
/// The source a form outside the load order is kept under: its session FormID is the local id (JContainers writes
/// such "dynamic, 0xff*" forms as `__formData||formId`).
pub const SESSION_SOURCE: [u8; 16] = [0xFF; 16];

/// One value in a container. `None` is JContainers' "none" (value type 1); a missing value is "no value" (0).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum JcValue {
    None,
    Int(i32),
    Float(f32),
    Str(String),
    Form(FormRef),
    Object(i32),
}

impl JcValue {
    /// JContainers' value type: 1 none, 2 int, 3 float, 4 form, 5 object, 6 string (`VALUE_TYPE_COMMENT`).
    pub fn type_code(&self) -> i32 {
        match self {
            JcValue::None => 1,
            JcValue::Int(_) => 2,
            JcValue::Float(_) => 3,
            JcValue::Form(_) => 4,
            JcValue::Object(_) => 5,
            JcValue::Str(_) => 6,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            JcValue::Int(n) => Some(f64::from(*n)),
            JcValue::Float(f) => Some(f64::from(*f)),
            _ => None,
        }
    }
}

/// A form-keyed map, saved as pairs (a form is no JSON object key).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormKeyed(pub BTreeMap<FormRef, JcValue>);

#[cfg(feature = "save")]
impl serde::Serialize for FormKeyed {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter())
    }
}

#[cfg(feature = "save")]
impl<'de> serde::Deserialize<'de> for FormKeyed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(Vec::<(FormRef, JcValue)>::deserialize(deserializer)?.into_iter().collect()))
    }
}

/// One container object.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum JcObject {
    Array(Vec<JcValue>),
    /// Keyed by the lowercased key, holding the key as first spelled.
    Map(BTreeMap<String, (String, JcValue)>),
    FormMap(FormKeyed),
    IntMap(BTreeMap<i32, JcValue>),
}

impl JcObject {
    pub fn len(&self) -> usize {
        match self {
            JcObject::Array(items) => items.len(),
            JcObject::Map(items) => items.len(),
            JcObject::FormMap(items) => items.0.len(),
            JcObject::IntMap(items) => items.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every value held.
    pub fn values(&self) -> Box<dyn Iterator<Item = &JcValue> + '_> {
        match self {
            JcObject::Array(items) => Box::new(items.iter()),
            JcObject::Map(items) => Box::new(items.values().map(|(_, value)| value)),
            JcObject::FormMap(items) => Box::new(items.0.values()),
            JcObject::IntMap(items) => Box::new(items.values()),
        }
    }

    fn values_mut(&mut self) -> Box<dyn Iterator<Item = &mut JcValue> + '_> {
        match self {
            JcObject::Array(items) => Box::new(items.iter_mut()),
            JcObject::Map(items) => Box::new(items.values_mut().map(|(_, value)| value)),
            JcObject::FormMap(items) => Box::new(items.0.values_mut()),
            JcObject::IntMap(items) => Box::new(items.values_mut()),
        }
    }

    fn clear(&mut self) {
        match self {
            JcObject::Array(items) => items.clear(),
            JcObject::Map(items) => items.clear(),
            JcObject::FormMap(items) => items.0.clear(),
            JcObject::IntMap(items) => items.clear(),
        }
    }
}

/// A key into a container: an index, a string, a form or an integer.
#[derive(Debug, Clone, PartialEq)]
pub enum JcKey {
    Index(i64),
    Str(String),
    Form(FormRef),
    Int(i32),
}

/// One explicit owner count and its tag.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct Retention {
    count: u32,
    tag: Option<String>,
}

/// The session's JContainers objects. Saved ("all objects are saved and all objects are restored when the save file
/// gets loaded"); the temporary owners are the session's and are renewed for every object on a load.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct JcHeap {
    next: i32,
    objects: BTreeMap<i32, JcObject>,
    retains: BTreeMap<i32, Retention>,
    /// `JDB`'s root map, 0 until first used.
    jdb: i32,
    /// Handle -> the second its temporary owner lets go.
    #[cfg_attr(feature = "save", serde(skip))]
    temporary: HashMap<i32, f64>,
    #[cfg_attr(feature = "save", serde(skip))]
    sweep_at: usize,
    /// Whether this session has renewed the restored objects' temporary owners.
    #[cfg_attr(feature = "save", serde(skip))]
    session: bool,
}

impl Default for JcHeap {
    fn default() -> Self {
        Self {
            next: 1,
            objects: BTreeMap::new(),
            retains: BTreeMap::new(),
            jdb: 0,
            temporary: HashMap::new(),
            sweep_at: FIRST_SWEEP,
            session: false,
        }
    }
}

impl Resource for JcHeap {}

impl JcHeap {
    /// Start the session at `now`: every restored object gets its temporary owner, so a load sweeps nothing a
    /// script is about to use.
    fn begin(&mut self, now: f64) {
        if !self.session {
            self.session = true;
            let until = now + TEMPORARY_SECONDS;
            self.temporary = self.objects.keys().map(|&handle| (handle, until)).collect();
            self.sweep_at = FIRST_SWEEP.max(self.objects.len() * 2);
        }
    }

    pub fn contains(&self, handle: i32) -> bool {
        self.objects.contains_key(&handle)
    }

    pub fn object(&self, handle: i32) -> Option<&JcObject> {
        self.objects.get(&handle)
    }

    pub fn object_mut(&mut self, handle: i32) -> Option<&mut JcObject> {
        self.objects.get_mut(&handle)
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// A new object, temporarily owned; 0 at [`MAX_OBJECTS`].
    pub fn create(&mut self, object: JcObject, now: f64) -> i32 {
        self.begin(now);
        if self.objects.len() >= self.sweep_at {
            self.sweep(now);
            self.sweep_at = FIRST_SWEEP.max(self.objects.len() * 2);
        }
        if self.objects.len() >= MAX_OBJECTS {
            log::warn!("JContainers: {MAX_OBJECTS} objects are alive; no new object");
            return 0;
        }
        let handle = loop {
            let handle = self.next.max(1);
            self.next = handle.checked_add(1).unwrap_or(1);
            if !self.objects.contains_key(&handle) {
                break handle;
            }
        };
        self.objects.insert(handle, object);
        self.temporary.insert(handle, now + TEMPORARY_SECONDS);
        handle
    }

    /// Renew `handle`'s temporary owner: a script passed or received it.
    pub fn touch(&mut self, handle: i32, now: f64) {
        self.begin(now);
        if self.objects.contains_key(&handle) {
            self.temporary.insert(handle, now + TEMPORARY_SECONDS);
        }
    }

    /// `JValue.retain`: one more owner, tagged (`""` removes the tag); the object, or 0.
    pub fn retain(&mut self, handle: i32, tag: &str) -> i32 {
        if !self.contains(handle) {
            return 0;
        }
        let retention = self.retains.entry(handle).or_default();
        retention.count = retention.count.saturating_add(1);
        retention.tag = (!tag.is_empty()).then(|| tag.to_owned());
        handle
    }

    /// `JValue.release`: one owner fewer, and the temporary owner lets go ("JC stops to temporarily own an object if
    /// the user invokes `JValue.release`"). Returns 0.
    pub fn release(&mut self, handle: i32) -> i32 {
        self.temporary.remove(&handle);
        if let Some(retention) = self.retains.get_mut(&handle) {
            retention.count = retention.count.saturating_sub(1);
            if retention.count == 0 {
                self.retains.remove(&handle);
            }
        }
        0
    }

    /// `JValue.releaseObjectsWithTag`: every retain of the objects tagged `tag`, complemented.
    pub fn release_tagged(&mut self, tag: &str) {
        self.retains.retain(|_, retention| retention.tag.as_deref() != Some(tag));
    }

    /// `JValue.zeroLifetime`: the temporary owner lets go now.
    pub fn zero_lifetime(&mut self, handle: i32) -> i32 {
        self.temporary.remove(&handle);
        handle
    }

    /// Sweep the objects no owner reaches. Returns how many went.
    pub fn sweep(&mut self, now: f64) -> usize {
        self.temporary.retain(|handle, until| *until > now && self.objects.contains_key(handle));
        let mut marked: std::collections::HashSet<i32> = std::collections::HashSet::new();
        let mut stack: Vec<i32> = self.retains.keys().copied().chain(self.temporary.keys().copied()).collect();
        if self.jdb != 0 {
            stack.push(self.jdb);
        }
        while let Some(handle) = stack.pop() {
            if !marked.insert(handle) {
                continue;
            }
            if let Some(object) = self.objects.get(&handle) {
                stack.extend(object.values().filter_map(|value| match value {
                    JcValue::Object(child) if !marked.contains(child) => Some(*child),
                    _ => None,
                }));
            }
        }
        let before = self.objects.len();
        self.objects.retain(|handle, _| marked.contains(handle));
        self.retains.retain(|handle, _| marked.contains(handle));
        let swept = before - self.objects.len();
        if swept > 0 {
            log::debug!("JContainers: swept {swept} unowned object(s); {} alive", self.objects.len());
        }
        swept
    }

    /// `JDB`'s root map, made on first use (and owned by the heap).
    pub fn jdb(&mut self, now: f64) -> i32 {
        if !self.contains(self.jdb) {
            self.jdb = self.create(JcObject::Map(BTreeMap::new()), now);
        }
        self.jdb
    }

    /// Whether `handle` is `JDB`'s root.
    pub fn is_jdb(&self, handle: i32) -> bool {
        handle != 0 && handle == self.jdb
    }

    /// A copy of `handle`: its values as they are (shallow), or its objects copied too (deep). 0 if absent.
    pub fn copy(&mut self, handle: i32, deep: bool, now: f64) -> i32 {
        let Some(object) = self.objects.get(&handle).cloned() else {
            return 0;
        };
        if !deep {
            return self.create(object, now);
        }
        let mut copies: HashMap<i32, i32> = HashMap::new();
        self.deep_copy(handle, &mut copies, now).unwrap_or(0)
    }

    fn deep_copy(&mut self, handle: i32, copies: &mut HashMap<i32, i32>, now: f64) -> Option<i32> {
        if let Some(&copy) = copies.get(&handle) {
            return Some(copy);
        }
        let mut object = self.objects.get(&handle)?.clone();
        let copy = self.create(object.clone(), now);
        if copy == 0 {
            return None;
        }
        copies.insert(handle, copy);
        for value in object.values_mut() {
            if let JcValue::Object(child) = value {
                *child = self.deep_copy(*child, copies, now).unwrap_or(0);
            }
        }
        self.objects.insert(copy, object);
        Some(copy)
    }

    /// The value at `key` of `handle`.
    pub fn get(&self, handle: i32, key: &JcKey) -> Option<&JcValue> {
        match (self.objects.get(&handle)?, key) {
            (JcObject::Array(items), JcKey::Index(index)) => array_index(items.len(), *index).and_then(|i| items.get(i)),
            (JcObject::Map(items), JcKey::Str(key)) => items.get(&key.to_ascii_lowercase()).map(|(_, value)| value),
            (JcObject::FormMap(items), JcKey::Form(form)) => items.0.get(form),
            (JcObject::IntMap(items), JcKey::Int(key)) => items.get(key),
            _ => None,
        }
    }

    /// Set the value at `key` of `handle` (an array index must exist); whether it was set.
    pub fn set(&mut self, handle: i32, key: JcKey, value: JcValue) -> bool {
        if matches!(value, JcValue::Object(child) if child != 0 && !self.contains(child)) {
            return false;
        }
        match (self.objects.get_mut(&handle), key) {
            (Some(JcObject::Array(items)), JcKey::Index(index)) => match array_index(items.len(), index) {
                Some(i) => {
                    items[i] = value;
                    true
                }
                None => false,
            },
            (Some(JcObject::Map(items)), JcKey::Str(key)) => {
                let lower = key.to_ascii_lowercase();
                match items.get_mut(&lower) {
                    Some(slot) => slot.1 = value,
                    None => {
                        items.insert(lower, (key, value));
                    }
                }
                true
            }
            (Some(JcObject::FormMap(items)), JcKey::Form(form)) => {
                items.0.insert(form, value);
                true
            }
            (Some(JcObject::IntMap(items)), JcKey::Int(key)) => {
                items.insert(key, value);
                true
            }
            _ => false,
        }
    }

    /// Remove `key` from a map; whether it was there.
    pub fn remove(&mut self, handle: i32, key: &JcKey) -> bool {
        match (self.objects.get_mut(&handle), key) {
            (Some(JcObject::Map(items)), JcKey::Str(key)) => items.remove(&key.to_ascii_lowercase()).is_some(),
            (Some(JcObject::FormMap(items)), JcKey::Form(form)) => items.0.remove(form).is_some(),
            (Some(JcObject::IntMap(items)), JcKey::Int(key)) => items.remove(key).is_some(),
            _ => false,
        }
    }

    /// A map's keys, in the map's order.
    pub fn keys(&self, handle: i32) -> Vec<JcKey> {
        match self.objects.get(&handle) {
            Some(JcObject::Map(items)) => items.values().map(|(key, _)| JcKey::Str(key.clone())).collect(),
            Some(JcObject::FormMap(items)) => items.0.keys().map(|form| JcKey::Form(*form)).collect(),
            Some(JcObject::IntMap(items)) => items.keys().map(|key| JcKey::Int(*key)).collect(),
            _ => Vec::new(),
        }
    }

    /// `JValue.clear`.
    pub fn clear(&mut self, handle: i32) {
        if let Some(object) = self.objects.get_mut(&handle) {
            object.clear();
        }
    }

    /// The value a JContainers path reaches from `handle` (wiki *Path Resolving*: `.stringKey` into a `JMap`,
    /// `[index]` into a `JArray`, `[__formData|Plugin|formId]` into a `JFormMap`, `[int]` into a `JIntMap`).
    pub fn solve(&self, handle: i32, path: &str, form: &dyn Fn(&str) -> Option<FormRef>) -> Option<JcValue> {
        let mut current = JcValue::Object(handle);
        for element in parse_path(path)? {
            let JcValue::Object(object) = current else {
                return None;
            };
            current = self.get(object, &self.key_for(object, &element, form)?)?.clone();
        }
        Some(current)
    }

    /// Set the value a path reaches; with `create` each missing `JMap` key on the way is made ("will insert any
    /// missing `JMap` key during path traversal"). Whether it was set.
    pub fn solve_set(&mut self, handle: i32, path: &str, value: JcValue, create: bool, now: f64, form: &dyn Fn(&str) -> Option<FormRef>) -> bool {
        let Some(elements) = parse_path(path) else {
            return false;
        };
        let Some((last, walk)) = elements.split_last() else {
            return false;
        };
        let mut object = handle;
        for element in walk {
            let Some(key) = self.key_for(object, element, form) else {
                return false;
            };
            object = match self.get(object, &key) {
                Some(JcValue::Object(next)) if self.contains(*next) => *next,
                None if create && matches!(key, JcKey::Str(_)) => {
                    let made = self.create(JcObject::Map(BTreeMap::new()), now);
                    if made == 0 || !self.set(object, key, JcValue::Object(made)) {
                        return false;
                    }
                    made
                }
                _ => return false,
            };
        }
        let Some(key) = self.key_for(object, last, form) else {
            return false;
        };
        // "fail if `createMissingKeys` is disabled and any key in the path is missing".
        if !create && self.get(object, &key).is_none() {
            return false;
        }
        self.set(object, key, value)
    }

    /// The key an element names in `handle`'s kind of container.
    fn key_for(&self, handle: i32, element: &PathElement, form: &dyn Fn(&str) -> Option<FormRef>) -> Option<JcKey> {
        Some(match (self.objects.get(&handle)?, element) {
            (JcObject::Map(_), PathElement::Member(key)) => JcKey::Str(key.clone()),
            (JcObject::Array(_), PathElement::Bracket(text)) => JcKey::Index(parse_int(text)?),
            (JcObject::IntMap(_), PathElement::Bracket(text)) => JcKey::Int(i32::try_from(parse_int(text)?).ok()?),
            (JcObject::FormMap(_), PathElement::Bracket(text)) => JcKey::Form(form(text)?),
            _ => return None,
        })
    }

    /// An object (and what it holds) as JContainers writes JSON (wiki *Serialization*): an array `[]`, a map `{}`,
    /// a `JFormMap` / `JIntMap` an object marked `"__metaInfo": {"typeName": …}`, a form `"__formData|Plugin|0x…"`,
    /// none `null`, and an object met again a `"__reference|<path>"` link.
    pub fn to_json(&self, handle: i32, form_text: &dyn Fn(FormRef) -> String) -> Json {
        let mut seen: HashMap<i32, String> = HashMap::new();
        self.object_json(handle, String::new(), &mut seen, form_text)
    }

    fn object_json(&self, handle: i32, path: String, seen: &mut HashMap<i32, String>, form_text: &dyn Fn(FormRef) -> String) -> Json {
        if let Some(earlier) = seen.get(&handle) {
            return Json::from(format!("__reference|{earlier}"));
        }
        seen.insert(handle, path.clone());
        let mut value_json = |value: &JcValue, at: String| match value {
            JcValue::None => Json::Null,
            JcValue::Int(n) => Json::from(*n),
            JcValue::Float(f) => Json::from(f64::from(*f)),
            JcValue::Str(text) => Json::from(text.clone()),
            JcValue::Form(form) => Json::from(form_text(*form)),
            JcValue::Object(child) if self.contains(*child) => self.object_json(*child, at, seen, form_text),
            JcValue::Object(_) => Json::Null,
        };
        match self.objects.get(&handle) {
            None => Json::Null,
            Some(JcObject::Array(items)) => {
                Json::Array(items.iter().enumerate().map(|(i, value)| value_json(value, format!("{path}[{i}]"))).collect())
            }
            Some(JcObject::Map(items)) => Json::Object(
                items.values().map(|(key, value)| (key.clone(), value_json(value, format!("{path}.{key}")))).collect(),
            ),
            Some(JcObject::FormMap(items)) => {
                let mut map = Map::new();
                map.insert("__metaInfo".to_owned(), serde_json::json!({ "typeName": "JFormMap" }));
                for (key, value) in &items.0 {
                    let text = form_text(*key);
                    map.insert(text.clone(), value_json(value, format!("{path}[{text}]")));
                }
                Json::Object(map)
            }
            Some(JcObject::IntMap(items)) => {
                let mut map = Map::new();
                map.insert("__metaInfo".to_owned(), serde_json::json!({ "typeName": "JIntMap" }));
                for (key, value) in items {
                    map.insert(key.to_string(), value_json(value, format!("{path}[{key}]")));
                }
                Json::Object(map)
            }
        }
    }

    /// A new object from JSON in JContainers' format (an array or an object; `true` / `false` read as 1 / 0,
    /// "JSON -> Papyrus direction only"); `__reference|` links resolve once the whole tree is built. 0 for
    /// anything else.
    pub fn from_json(&mut self, json: &Json, now: f64, form: &dyn Fn(&str) -> Option<FormRef>) -> i32 {
        let mut links: Vec<(i32, JcKey, String)> = Vec::new();
        let root = self.build(json, now, form, &mut links);
        if root == 0 {
            return 0;
        }
        for (owner, key, path) in links {
            let target = if path.is_empty() { Some(JcValue::Object(root)) } else { self.solve(root, &path, form) };
            if let Some(target @ JcValue::Object(_)) = target {
                self.set(owner, key, target);
            }
        }
        root
    }

    fn build(&mut self, json: &Json, now: f64, form: &dyn Fn(&str) -> Option<FormRef>, links: &mut Vec<(i32, JcKey, String)>) -> i32 {
        let handle = match json {
            Json::Array(_) => self.create(JcObject::Array(Vec::new()), now),
            Json::Object(map) => {
                let type_name = map.get("__metaInfo").and_then(|meta| meta.get("typeName")).and_then(Json::as_str);
                let object = match type_name {
                    Some("JFormMap") => JcObject::FormMap(FormKeyed::default()),
                    Some("JIntMap") => JcObject::IntMap(BTreeMap::new()),
                    _ => JcObject::Map(BTreeMap::new()),
                };
                self.create(object, now)
            }
            _ => return 0,
        };
        if handle == 0 {
            return 0;
        }
        match json {
            Json::Array(items) => {
                for (i, value) in items.iter().enumerate() {
                    self.build_item(handle, JcKey::Index(i as i64), value, now, form, links);
                }
            }
            Json::Object(map) => {
                let kind = self.objects.get(&handle).cloned();
                for (key, value) in map.iter().filter(|(key, _)| *key != "__metaInfo") {
                    let key = match kind {
                        Some(JcObject::FormMap(_)) => match form(key) {
                            Some(form) => JcKey::Form(form),
                            None => continue,
                        },
                        Some(JcObject::IntMap(_)) => match parse_int(key).and_then(|n| i32::try_from(n).ok()) {
                            Some(n) => JcKey::Int(n),
                            None => continue,
                        },
                        _ => JcKey::Str(key.clone()),
                    };
                    self.build_item(handle, key, value, now, form, links);
                }
            }
            _ => {}
        }
        handle
    }

    /// One value from JSON into `handle` at `key`.
    fn build_item(&mut self, handle: i32, key: JcKey, value: &Json, now: f64, form: &dyn Fn(&str) -> Option<FormRef>, links: &mut Vec<(i32, JcKey, String)>) {
        let value = match value {
            Json::Null => JcValue::None,
            Json::Bool(flag) => JcValue::Int(i32::from(*flag)),
            Json::Number(number) => match number.as_i64() {
                Some(n) => JcValue::Int(n as i32),
                None => JcValue::Float(number.as_f64().unwrap_or(0.0) as f32),
            },
            Json::String(text) => {
                if let Some(path) = text.strip_prefix("__reference|") {
                    links.push((handle, key.clone(), path.to_owned()));
                    JcValue::None
                } else if text.starts_with("__formData|") {
                    form(text).map_or(JcValue::None, JcValue::Form)
                } else {
                    JcValue::Str(text.clone())
                }
            }
            nested => JcValue::Object(self.build(nested, now, form, links)),
        };
        self.insert_built(handle, key, value);
    }

    /// Add a value built from JSON (an array grows; a map sets).
    fn insert_built(&mut self, handle: i32, key: JcKey, value: JcValue) {
        match self.objects.get_mut(&handle) {
            Some(JcObject::Array(items)) => items.push(value),
            _ => {
                self.set(handle, key, value);
            }
        }
    }
}

/// One element of a JContainers path.
#[derive(Debug, Clone, PartialEq)]
enum PathElement {
    Member(String),
    Bracket(String),
}

/// A path's elements; `None` for a malformed one or a collection operator (`@…`, not served). An empty path is the
/// object itself.
fn parse_path(path: &str) -> Option<Vec<PathElement>> {
    if path.contains('@') {
        return None;
    }
    let mut elements = Vec::new();
    let mut rest = path;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('[') {
            let (inner, tail) = after.split_once(']')?;
            elements.push(PathElement::Bracket(inner.to_owned()));
            rest = tail;
        } else if let Some(after) = rest.strip_prefix('.') {
            let end = after.find(['.', '[']).unwrap_or(after.len());
            if end == 0 {
                return None;
            }
            elements.push(PathElement::Member(after[..end].to_owned()));
            rest = &after[end..];
        } else {
            return None;
        }
    }
    Some(elements)
}

/// An integer written in decimal or `0x` hex.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// An array index, a negative one counting from the end (JContainers' `JArray` "negative index accesses items
/// from the end").
pub fn array_index(len: usize, index: i64) -> Option<usize> {
    let index = if index < 0 { len as i64 + index } else { index };
    usize::try_from(index).ok().filter(|&i| i < len)
}

/// JContainers' form text: `__formData|<plugin file>|0x<local id>`, or `__formData||0x<id>` for a form outside the
/// load order (wiki *Serialization*).
pub fn form_data(plugin: Option<&str>, id: u32) -> String {
    format!("__formData|{}|0x{id:x}", plugin.unwrap_or(""))
}

/// A form text's plugin file (empty for a dynamic form) and id: `formId` "is a hex number prefixed with `0x` or a
/// decimal number".
pub fn parse_form_data(text: &str) -> Option<(&str, u32)> {
    let rest = text.strip_prefix("__formData|")?;
    let (plugin, id) = rest.split_once('|')?;
    Some((plugin, u32::try_from(parse_int(id)?).ok()?))
}

/// This subsystem's world setup: the heap (saved).
pub fn install(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<JcHeap>();
}

/// The heap rides the save; its forms are plugin-and-id identities ([`FormRef`]), so a changed load order names the
/// same forms.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_resource::<JcHeap>("JcHeap");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(text: &str) -> Option<FormRef> {
        let (plugin, id) = parse_form_data(text)?;
        Some(FormRef::new([plugin.len() as u8; 16], id))
    }

    fn text(form: FormRef) -> String {
        form_data(Some("Skyrim.esm"), form.local())
    }

    /// Owners keep objects: a retain, a container, JDB, and a temporary owner for 10 seconds after a use.
    #[test]
    fn objects_live_while_owned_and_are_swept_after() {
        let mut heap = JcHeap::default();
        let kept = heap.create(JcObject::Array(Vec::new()), 0.0);
        let child = heap.create(JcObject::Map(BTreeMap::new()), 0.0);
        let temp = heap.create(JcObject::Array(Vec::new()), 0.0);
        let in_db = heap.create(JcObject::IntMap(BTreeMap::new()), 0.0);
        heap.retain(kept, "mymod");
        assert!(heap.set(kept, JcKey::Index(0), JcValue::None) == false, "no index 0 yet");
        if let Some(JcObject::Array(items)) = heap.object_mut(kept) {
            items.push(JcValue::Object(child));
        }
        let db = heap.jdb(0.0);
        heap.set(db, JcKey::Str("Frost".into()), JcValue::Object(in_db));
        assert_eq!(heap.sweep(5.0), 0, "inside 10 seconds everything lives");
        assert_eq!(heap.sweep(11.0), 1, "only the unowned one goes");
        assert!(!heap.contains(temp) && heap.contains(kept) && heap.contains(child) && heap.contains(in_db));
        heap.release_tagged("mymod");
        assert_eq!(heap.sweep(12.0), 2, "the released array and what it held");
        heap.touch(in_db, 12.0);
        assert!(heap.contains(in_db));
    }

    #[test]
    fn paths_reach_and_create_values() {
        let mut heap = JcHeap::default();
        let root = heap.create(JcObject::Map(BTreeMap::new()), 0.0);
        assert!(heap.solve_set(root, ".frost.keyB", JcValue::Int(10), true, 0.0, &form));
        assert_eq!(heap.solve(root, ".Frost.KEYB", &form), Some(JcValue::Int(10)), "JMap keys compare without case");
        assert!(!heap.solve_set(root, ".missing.key", JcValue::Int(1), false, 0.0, &form));
        assert_eq!(heap.solve(root, ".frost.keyB.deeper", &form), None);
        assert_eq!(heap.solve(root, ".a@maxNum", &form), None, "collection operators are not served");
        assert_eq!(array_index(3, -1), Some(2));
    }

    /// JSON in JContainers' format both ways, including typed maps, forms and a shared object written as a link.
    #[test]
    fn json_round_trips_in_the_jcontainers_format() {
        let mut heap = JcHeap::default();
        let source = serde_json::json!({
            "name": "Elsa",
            "level": 2,
            "actor": "__formData|Skyrim.esm|0x14",
            "inner": [[1, 2.5, null]],
            "link": "__reference|.inner[0]",
            "byForm": { "__metaInfo": { "typeName": "JFormMap" }, "__formData|Skyrim.esm|0x14": "player" },
            "byInt": { "__metaInfo": { "typeName": "JIntMap" }, "7": true }
        });
        let root = heap.from_json(&source, 0.0, &form);
        assert_eq!(heap.solve(root, ".name", &form), Some(JcValue::Str("Elsa".into())));
        assert_eq!(heap.solve(root, ".actor", &form), form("__formData|Skyrim.esm|0x14").map(JcValue::Form));
        assert_eq!(heap.solve(root, ".inner[0][1]", &form), Some(JcValue::Float(2.5)));
        assert_eq!(heap.solve(root, ".link", &form), heap.solve(root, ".inner[0]", &form), "the link is the same object");
        assert_eq!(heap.solve(root, ".byForm[__formData|Skyrim.esm|0x14]", &form), Some(JcValue::Str("player".into())));
        assert_eq!(heap.solve(root, ".byInt[7]", &form), Some(JcValue::Int(1)));
        let written = heap.to_json(root, &text);
        assert_eq!(written["byInt"]["__metaInfo"]["typeName"], serde_json::json!("JIntMap"));
        assert_eq!(written["actor"], serde_json::json!("__formData|Skyrim.esm|0x14"));
        assert_eq!(written["link"], serde_json::json!("__reference|.inner[0]"));
    }
}
