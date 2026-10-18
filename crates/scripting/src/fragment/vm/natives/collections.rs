//! Collection natives: script-filled `RefCollectionAlias` members, array
//! methods and `InputEnableLayer` layers (#5800).

use super::*;

/// The `RefCollectionAlias` natives the VM runs.
/// An `InputEnableLayer` member function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::fragment::vm) enum LayerNative {
    Disable,
    Enable,
    Reset,
    Delete,
    /// `Enable<Control>(abEnable = true)`.
    Set(crate::Control),
    /// `Is<Control>Enabled()`.
    Query(crate::Control),
}

/// The layer function a lowercase name is, if any.
pub(in crate::fragment::vm) fn layer_native(name: &str) -> Option<LayerNative> {
    match name {
        "disableplayercontrols" => return Some(LayerNative::Disable),
        "enableplayercontrols" => return Some(LayerNative::Enable),
        "reset" => return Some(LayerNative::Reset),
        "delete" => return Some(LayerNative::Delete),
        _ => {}
    }
    if let Some(control) = name.strip_prefix("enable") {
        return crate::Control::of_name(control).map(LayerNative::Set);
    }
    let control = name.strip_prefix("is")?.strip_suffix("enabled")?;
    crate::Control::of_name(control).map(LayerNative::Query)
}

pub(in crate::fragment::vm) const COLLECTION_NATIVES: [&str; 13] = [
    "evaluateall",
    "getcount",
    "getat",
    "getarray",
    "getactorarray",
    "enableall",
    "disableall",
    "find",
    "addref",
    "addarray",
    "addrefcollection",
    "removeref",
    "removeall",
];

/// The array methods the VM runs (CK *Arrays (Papyrus)*).
pub(in crate::fragment::vm) const ARRAY_METHODS: [&str; 9] = [
    "find",
    "rfind",
    "findstruct",
    "rfindstruct",
    "add",
    "insert",
    "remove",
    "removelast",
    "clear",
];

impl Vm<'_> {
    /// An `InputEnableLayer` native on a layer handle (see [`crate::input_layers`]):
    /// `Disable` / `EnablePlayerControls`, `Enable<Control>`, `Is<Control>Enabled`,
    /// `Reset`, `Delete`. `None` when `callee` is not one on a layer.
    pub(in crate::fragment::vm) fn layer_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        let Some(native) = layer_native(&name) else {
            return Ok(None);
        };
        if static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Int(layer) = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let layer = layer as u32;
        let values = self.eval_args(frame, args)?;
        let flag = |index: usize, default: bool| values.get(index).map_or(default, Value::truthy);
        let Some(mut layers) = self.world.try_resource_mut::<crate::InputLayers>() else {
            return stop("an input layer without the layer store");
        };
        let result = match native {
            LayerNative::Query(control) => return Ok(Some(Value::Bool(layers.layer_enabled(layer, control)))),
            LayerNative::Disable | LayerNative::Enable => {
                let enabling = native == LayerNative::Enable;
                for (index, control) in crate::Control::PARAMETER_ORDER.into_iter().enumerate() {
                    let default = if enabling { true } else { crate::Control::DISABLE_DEFAULTS[index] };
                    if flag(index, default) {
                        layers.set_enabled(layer, control, enabling);
                    }
                }
                Value::None
            }
            LayerNative::Set(control) => {
                layers.set_enabled(layer, control, flag(0, true));
                Value::None
            }
            LayerNative::Reset => {
                layers.reset(layer);
                Value::None
            }
            LayerNative::Delete => {
                layers.delete(layer);
                Value::None
            }
        };
        if let Some(mut state) = self.world.try_resource_mut::<crate::player_control::PlayerControlState>() {
            layers.sync_state(&mut state);
        }
        Ok(Some(result))
    }

    /// A `RefCollectionAlias` native on a collection alias: `GetCount`,
    /// `GetAt`, `Find`, `AddRef`, `AddArray`, `AddRefCollection`, `RemoveRef`,
    /// `RemoveAll`. `None` when `callee` is not one on an alias. The collection
    /// must be one the quest leaves to its scripts (see
    /// [`crate::alias_collections`]).
    pub(in crate::fragment::vm) fn collection_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        if !COLLECTION_NATIVES.contains(&name.as_str()) || static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Alias { quest, alias } = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let key = (quest, alias);
        self.require_script_filled(key)?;
        if !self.collections.borrow().contains_key(&key) {
            let stored: Vec<u32> = self
                .world
                .try_resource::<crate::AliasCollections>()
                .map(|collections| collections.members(key).to_vec())
                .unwrap_or_default();
            self.collections.borrow_mut().insert(key, stored);
        }
        let values = self.eval_args(frame, args)?;
        let form_of = |value: Option<&Value>| match value {
            Some(Value::Form(form)) if *form != 0 => Some(*form),
            _ => None,
        };
        let mut collections = self.collections.borrow_mut();
        let members = collections.get_mut(&key).expect("loaded above");
        Ok(Some(match name.as_str() {
            "getcount" => Value::Int(members.len() as i32),
            "getat" => {
                let index = values.first().and_then(Value::int).unwrap_or(-1);
                usize::try_from(index)
                    .ok()
                    .and_then(|index| members.get(index))
                    .map_or(Value::None, |form| Value::Form(*form))
            }
            "getarray" | "getactorarray" => Value::Array(Rc::new(RefCell::new(
                members.iter().map(|form| Value::Form(*form)).collect(),
            ))),
            // CK *EvaluateAll - RefCollectionAlias*: "Calls EvaluatePackage on
            // all the actors in the collection."
            "evaluateall" => {
                if let Some(mut requests) = self.world.query_mut::<crate::EvaluatePackageRequest>() {
                    for form in members.iter() {
                        if let Some(entity) = crate::condition::resolve_entity_by_global_form_id(self.world, *form) {
                            requests.insert(entity, crate::EvaluatePackageRequest);
                        }
                    }
                }
                Value::None
            }
            // CK: "Calls EnableNoWait / DisableNoWait on all the references in
            // the collection."
            "enableall" | "disableall" => {
                let enabled = name == "enableall";
                for form in members.iter() {
                    self.deferred.reference_enable_changes.push((*form, enabled));
                }
                Value::None
            }
            "find" => Value::Int(match form_of(values.first()) {
                Some(form) => members.iter().position(|member| *member == form).map_or(-1, |i| i as i32),
                None => -1,
            }),
            "addref" => {
                if let Some(form) = form_of(values.first()) {
                    if !members.contains(&form) {
                        members.push(form);
                    }
                }
                Value::None
            }
            "addarray" => {
                // CK: "Reference Aliases inside arrays will not be added to a
                // collection if they are none."
                if let Some(Value::Array(items)) = values.first() {
                    for item in items.borrow().iter() {
                        if let Value::Form(form) = item {
                            if *form != 0 && !members.contains(form) {
                                members.push(*form);
                            }
                        }
                    }
                }
                Value::None
            }
            "addrefcollection" => {
                let Some(Value::Alias { quest: other_quest, alias: other_alias }) = values.first() else {
                    return Ok(Some(Value::None));
                };
                let other = (*other_quest, *other_alias);
                drop(collections);
                self.require_script_filled(other)?;
                let others: Vec<u32> = self
                    .collections
                    .borrow()
                    .get(&other)
                    .cloned()
                    .or_else(|| {
                        self.world
                            .try_resource::<crate::AliasCollections>()
                            .map(|collections| collections.members(other).to_vec())
                    })
                    .unwrap_or_default();
                let mut collections = self.collections.borrow_mut();
                let members = collections.get_mut(&key).expect("loaded above");
                for form in others {
                    if !members.contains(&form) {
                        members.push(form);
                    }
                }
                Value::None
            }
            "removeref" => {
                if let Some(form) = form_of(values.first()) {
                    members.retain(|member| *member != form);
                }
                Value::None
            }
            "removeall" => {
                members.clear();
                Value::None
            }
            _ => return Ok(None),
        }))
    }

    /// Stop unless the collection alias `key` is one the quest leaves to its
    /// scripts: a collection alias with no fill type and no match conditions.
    pub(in crate::fragment::vm) fn require_script_filled(&self, key: crate::alias_collections::CollectionKey) -> Run<()> {
        match self.deferred.collection_kinds.get(&key) {
            Some(true) => Ok(()),
            Some(false) => stop("a reference collection filled by the quest's conditions"),
            None => stop("a collection native on an alias that is not a known reference collection"),
        }
    }

    /// An array method (CK *Arrays (Papyrus)*), when `callee` calls one on an
    /// array: `Find` / `RFind`, `FindStruct` / `RFindStruct`, `Add`, `Insert`,
    /// `Remove`, `RemoveLast`, `Clear`.
    pub(in crate::fragment::vm) fn array_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        if !ARRAY_METHODS.contains(&name.as_str()) || static_class(&object.node).is_some() {
            return Ok(None);
        }
        let Value::Array(array) = self.eval(frame, &object.node)? else {
            return Ok(None);
        };
        let values = self.eval_args(frame, args)?;
        let int = |index: usize, default: i32| -> i32 { values.get(index).and_then(Value::int).unwrap_or(default) };
        let mut items = array.borrow_mut();
        let len = items.len() as i32;
        // The member a FindStruct compares.
        let matches = |item: &Value, member: Option<&str>, wanted: &Value| -> bool {
            match member {
                None => item.equals(wanted),
                Some(member) => match item {
                    Value::Struct(members) => {
                        let members = members.borrow();
                        Value::member(&members, member).is_some_and(|index| members[index].1.equals(wanted))
                    }
                    _ => false,
                },
            }
        };
        let (member, wanted, start_arg) = match name.as_str() {
            "findstruct" | "rfindstruct" => {
                let Some(Value::Str(member)) = values.first() else {
                    return stop(format!("{name} without a member name"));
                };
                (Some(member.clone()), values.get(1).cloned().unwrap_or(Value::None), 2)
            }
            _ => (None, values.first().cloned().unwrap_or(Value::None), 1),
        };
        Ok(Some(match name.as_str() {
            "find" | "findstruct" => {
                let start = int(start_arg, 0).max(0);
                (start..len)
                    .find(|&i| matches(&items[i as usize], member.as_deref(), &wanted))
                    .map_or(Value::Int(-1), Value::Int)
            }
            "rfind" | "rfindstruct" => {
                let start = int(start_arg, -1);
                let start = if start < 0 || start >= len { len - 1 } else { start };
                (0..=start)
                    .rev()
                    .find(|&i| matches(&items[i as usize], member.as_deref(), &wanted))
                    .map_or(Value::Int(-1), Value::Int)
            }
            "add" => {
                // "in the case of adding structs, they will all point at the
                // same struct" — the value is shared, not copied.
                let count = int(1, 1).max(0) as usize;
                if count > MAX_ARRAY_ELEMENTS.saturating_sub(items.len()) {
                    return stop(format!("array Add over {MAX_ARRAY_ELEMENTS} elements"));
                }
                for _ in 0..count {
                    items.push(wanted.clone());
                }
                Value::None
            }
            "insert" => {
                if items.len() >= MAX_ARRAY_ELEMENTS {
                    return stop(format!("array Insert over {MAX_ARRAY_ELEMENTS} elements"));
                }
                let index = int(1, 0).clamp(0, len) as usize;
                items.insert(index, wanted);
                Value::None
            }
            "remove" => {
                let index = int(0, 0);
                let count = int(1, 1).max(0);
                if index < 0 || index >= len {
                    return stop("array Remove out of range");
                }
                let end = index.saturating_add(count).min(len);
                items.drain(index as usize..end as usize);
                Value::None
            }
            "removelast" => {
                items.pop();
                Value::None
            }
            "clear" => {
                items.clear();
                Value::None
            }
            _ => return Ok(None),
        }))
    }
}

impl Checker<'_> {
    /// An array method's result kind (CK *Arrays (Papyrus)*).
    pub(in crate::fragment::vm) fn check_array_call(&mut self, name: &str, args: &[Arg], statement: bool) -> Option<Kind> {
        let num = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        let ok = match name {
            "find" | "rfind" => (1..=2).contains(&args.len()) && args.get(1).is_none_or(num),
            "findstruct" | "rfindstruct" => {
                (2..=3).contains(&args.len())
                    && matches!(args[0], Arg::Literal(Expr::StringLit(_)))
                    && args.get(2).is_none_or(num)
            }
            "add" => statement && (1..=2).contains(&args.len()) && args.get(1).is_none_or(num),
            "insert" => statement && args.len() == 2 && num(&args[1]),
            "remove" => statement && (1..=2).contains(&args.len()) && args.iter().all(num),
            "removelast" | "clear" => statement && args.is_empty(),
            _ => false,
        };
        if !ok {
            return self.fail(|| format!("array {name}/{}", args.len()));
        }
        Some(if name.contains("find") { Kind::Num } else { Kind::None })
    }
}
