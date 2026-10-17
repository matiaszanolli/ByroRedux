//! Papyrus values and their marshalling: the run's [`Value`], why a run
//! stops, one function activation, and a value as a saved continuation, a
//! script variable and an operand (#5800).

use super::*;

/// A Papyrus value.
#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    /// Any form by FormID — a reference, a base object, a quest; the
    /// player's reference is `0x14`.
    Form(u32),
    /// A quest alias.
    Alias { quest: u32, alias: i16 },
    Array(Rc<RefCell<Vec<Value>>>),
    /// A Fallout 4 struct: its members by name. Structs, like arrays, are
    /// shared by reference.
    Struct(Rc<RefCell<Vec<(String, Value)>>>),
}

impl Value {
    pub(super) fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Form(form) => *form != 0,
            Value::Alias { .. } | Value::Array(_) | Value::Struct(_) => true,
        }
    }

    pub(super) fn number(&self) -> Option<f32> {
        match self {
            Value::Bool(b) => Some(f32::from(u8::from(*b))),
            Value::Int(i) => Some(*i as f32),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Whether both are the same alias.
    pub(super) fn same_alias(&self, other: &Value) -> bool {
        matches!((self, other), (Value::Alias { quest: a, alias: x }, Value::Alias { quest: b, alias: y }) if a == b && x == y)
    }

    pub(super) fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    /// An `Int` (or `Bool`) exactly — `number()` goes through an `f32`,
    /// which cannot hold every 32-bit integer.
    pub(super) fn int(&self) -> Option<i32> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(i32::from(*b)),
            _ => None,
        }
    }

    pub(super) fn text(&self) -> String {
        match self {
            Value::None => "None".into(),
            Value::Bool(b) => if *b { "True" } else { "False" }.into(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{f:.6}"),
            Value::Str(s) => s.clone(),
            Value::Form(form) => format!("[Form <{form:08X}>]"),
            Value::Alias { quest, alias } => format!("[Alias <{quest:08X}:{alias}>]"),
            Value::Array(_) => "[Array]".into(),
            Value::Struct(_) => "[Struct]".into(),
        }
    }

    pub(super) fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::None, _) | (_, Value::None) => false,
            (Value::Str(a), Value::Str(b)) => a.eq_ignore_ascii_case(b),
            (Value::Form(a), Value::Form(b)) => a == b,
            (Value::Alias { quest: a, alias: x }, Value::Alias { quest: b, alias: y }) => a == b && x == y,
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Struct(a), Value::Struct(b)) => Rc::ptr_eq(a, b),
            (a, b) => match (a.int(), b.int()) {
                (Some(a), Some(b)) => a == b,
                _ => match (a.number(), b.number()) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                },
            },
        }
    }

    /// A struct's member `name`, case-insensitively.
    pub(super) fn member(members: &[(String, Value)], name: &str) -> Option<usize> {
        members.iter().position(|(member, _)| member.eq_ignore_ascii_case(name))
    }

    pub(super) fn default_for(ty: &Type) -> Value {
        match ty {
            Type::Bool => Value::Bool(false),
            Type::Int => Value::Int(0),
            Type::Float => Value::Float(0.0),
            Type::String => Value::Str(String::new()),
            _ => Value::None,
        }
    }

    pub(super) fn from_property(value: &PropertyValue) -> Value {
        match value {
            PropertyValue::Object { form_id: 0, alias: -1 } => Value::None,
            PropertyValue::Object { form_id, alias: -1 } => Value::Form(*form_id),
            PropertyValue::Object { form_id, alias } => Value::Alias {
                quest: *form_id,
                alias: *alias,
            },
            PropertyValue::String(s) => Value::Str(s.clone()),
            PropertyValue::Int32(i) => Value::Int(*i),
            PropertyValue::Float(f) => Value::Float(*f),
            PropertyValue::Bool(b) => Value::Bool(*b),
            PropertyValue::Array(items) => Value::Array(Rc::new(RefCell::new(
                items.iter().map(Value::from_property).collect(),
            ))),
            PropertyValue::Struct(members) => Value::Struct(Rc::new(RefCell::new(
                members
                    .iter()
                    .map(|member| (member.name.clone(), Value::from_property(&member.value)))
                    .collect(),
            ))),
            PropertyValue::Unknown(_) => Value::None,
        }
    }
}

/// Why a run stopped.
#[derive(Debug)]
pub(super) struct Stop(pub(super) String);

pub(super) type Run<T> = Result<T, Stop>;

pub(super) fn stop<T>(why: impl Into<String>) -> Run<T> {
    Err(Stop(why.into()))
}

/// One function activation.
pub(super) struct Frame {
    pub(super) script: Arc<Script>,
    /// The object the script is attached to (`Self`): a quest, or a
    /// reference for a reference script. Its script variables are keyed by
    /// it.
    pub(super) owner: QuestFormId,
    pub(super) vmad: Option<ScriptInstanceData>,
    pub(super) locals: HashMap<String, Value>,
    /// The script whose body is running — the instance's script or one it
    /// extends — which `Parent.Fn()` calls past. Set when a function starts.
    pub(super) defined_in: Option<Arc<Script>>,
}

pub(super) enum Flow {
    Normal,
    Return(Value),
    /// `Utility.Wait(seconds)`: the path down to it in this frame, and the
    /// suspended frames of the call it waits in (outermost first).
    Suspend(f32, Vec<VmStep>, Vec<VmFrame>),
}

pub(super) fn suspend_at(step: VmStep, flow: Flow) -> Flow {
    match flow {
        Flow::Suspend(seconds, mut path, frames) => {
            path.insert(0, step);
            Flow::Suspend(seconds, path, frames)
        }
        other => other,
    }
}

/// `frame`'s state at `path` in `code` for a [`VmResume`].
pub(super) fn frame_state(frame: &Frame, code: &Code, function: &str, path: Vec<VmStep>) -> Run<VmFrame> {
    let mut locals = Vec::with_capacity(frame.locals.len());
    for (name, value) in &frame.locals {
        let Some(value) = store(value) else {
            return stop("an array local across a wait");
        };
        locals.push((name.clone(), value));
    }
    Ok(VmFrame {
        script: frame.script.name.node.0.clone(),
        function: function.to_owned(),
        owner: frame.owner.0,
        defined_in: code.script.name.node.0.clone(),
        state: code.state.clone(),
        path,
        locals,
    })
}

/// Every local of `frame` can be stored in a [`VmFrame`] (no array or struct).
pub(super) fn locals_storable(frame: &Frame) -> bool {
    frame
        .locals
        .values()
        .all(|value| !matches!(value, Value::Array(_) | Value::Struct(_)))
}

pub(super) fn restore_locals(frame: &VmFrame) -> HashMap<String, Value> {
    frame
        .locals
        .iter()
        .map(|(name, value)| (name.clone(), restore(value)))
        .collect()
}

pub(super) fn store(value: &Value) -> Option<VmStored> {
    Some(match value {
        Value::None => VmStored::None,
        Value::Bool(b) => VmStored::Bool(*b),
        Value::Int(i) => VmStored::Int(*i),
        Value::Float(f) => VmStored::Float(*f),
        Value::Str(s) => VmStored::Str(s.clone()),
        Value::Form(form) => VmStored::Form(*form),
        Value::Alias { quest, alias } => VmStored::Alias {
            quest: *quest,
            alias: *alias,
        },
        Value::Array(_) | Value::Struct(_) => return None,
    })
}

pub(super) fn restore(stored: &VmStored) -> Value {
    match stored {
        VmStored::None => Value::None,
        VmStored::Bool(b) => Value::Bool(*b),
        VmStored::Int(i) => Value::Int(*i),
        VmStored::Float(f) => Value::Float(*f),
        VmStored::Str(s) => Value::Str(s.clone()),
        VmStored::Form(form) => Value::Form(*form),
        VmStored::Alias { quest, alias } => Value::Alias {
            quest: *quest,
            alias: *alias,
        },
        VmStored::Array(items) => Value::Array(Rc::new(RefCell::new(items.iter().map(restore).collect()))),
    }
}

/// `value` as an event argument: a scalar, or an array of scalars.
pub(super) fn event_arg(value: &Value) -> Option<VmStored> {
    match value {
        Value::Array(items) => items
            .borrow()
            .iter()
            .map(|item| if matches!(item, Value::Array(_)) { None } else { event_arg(item) })
            .collect::<Option<Vec<_>>>()
            .map(VmStored::Array),
        other => store(other),
    }
}

/// `value` as a stored object (see [`crate::ScriptObject`]); `None` for what
/// a script variable cannot hold (a nested array).
pub(super) fn to_stored(value: &Value) -> Option<crate::ScriptObject> {
    Some(match value {
        Value::None => crate::ScriptObject::None,
        Value::Bool(b) => crate::ScriptObject::Bool(*b),
        Value::Int(i) => crate::ScriptObject::Int(*i),
        Value::Float(f) => crate::ScriptObject::Float(*f),
        Value::Str(text) => crate::ScriptObject::Str(text.clone()),
        Value::Form(form) => crate::ScriptObject::Form(*form),
        Value::Alias { quest, alias } => crate::ScriptObject::Alias {
            quest: *quest,
            alias: *alias,
        },
        Value::Array(items) => {
            let mut stored = Vec::new();
            for item in items.borrow().iter() {
                if matches!(item, Value::Array(_)) {
                    return None;
                }
                stored.push(to_stored(item)?);
            }
            crate::ScriptObject::Array(stored)
        }
        Value::Struct(members) => {
            let mut stored = Vec::new();
            for (name, member) in members.borrow().iter() {
                if matches!(member, Value::Array(_)) {
                    return None;
                }
                stored.push((name.clone(), to_stored(member)?));
            }
            crate::ScriptObject::Struct(stored)
        }
    })
}

/// A stored object as a value; arrays and structs come back as fresh shared
/// values.
pub(super) fn from_stored(object: &crate::ScriptObject) -> Value {
    match object {
        crate::ScriptObject::None => Value::None,
        crate::ScriptObject::Form(form) => Value::Form(*form),
        crate::ScriptObject::Alias { quest, alias } => Value::Alias {
            quest: *quest,
            alias: *alias,
        },
        crate::ScriptObject::Str(text) => Value::Str(text.clone()),
        crate::ScriptObject::Int(i) => Value::Int(*i),
        crate::ScriptObject::Float(f) => Value::Float(*f),
        crate::ScriptObject::Bool(b) => Value::Bool(*b),
        crate::ScriptObject::Array(items) => {
            Value::Array(Rc::new(RefCell::new(items.iter().map(from_stored).collect())))
        }
        crate::ScriptObject::Struct(members) => Value::Struct(Rc::new(RefCell::new(
            members.iter().map(|(name, member)| (name.clone(), from_stored(member))).collect(),
        ))),
    }
}

pub(super) fn cast(value: Value, ty: &Type) -> Value {
    match ty {
        Type::Bool => Value::Bool(value.truthy()),
        Type::Int => match value {
            Value::Str(s) => Value::Int(s.trim().parse::<f32>().unwrap_or(0.0) as i32),
            other => Value::Int(other.number().unwrap_or(0.0) as i32),
        },
        Type::Float => match value {
            Value::Str(s) => Value::Float(s.trim().parse::<f32>().unwrap_or(0.0)),
            other => Value::Float(other.number().unwrap_or(0.0)),
        },
        Type::String => Value::Str(value.text()),
        // Object casts keep the object; Papyrus yields None on a mismatch,
        // which needs the type of a form this VM does not track.
        _ => value,
    }
}

pub(super) fn arithmetic(op: BinaryOp, l: &Value, r: &Value) -> Run<Value> {
    if op == BinaryOp::Add && (matches!(l, Value::Str(_)) || matches!(r, Value::Str(_))) {
        return concat_texts(l, r);
    }
    let (Some(a), Some(b)) = (l.number(), r.number()) else {
        return stop(format!("arithmetic on {} and {}", l.text(), r.text()));
    };
    if let (Some(a), Some(b)) = (l.int(), r.int()) {
        // Papyrus `Int` arithmetic is 32-bit and exact.
        return Ok(Value::Int(match op {
            BinaryOp::Add => a.wrapping_add(b),
            BinaryOp::Sub => a.wrapping_sub(b),
            BinaryOp::Mul => a.wrapping_mul(b),
            BinaryOp::Div if b == 0 => return stop("integer division by zero"),
            BinaryOp::Div => a.wrapping_div(b),
            BinaryOp::Mod if b == 0 => return stop("modulo by zero"),
            BinaryOp::Mod => a.wrapping_rem(b),
            _ => return stop("not an arithmetic operator"),
        }));
    }
    Ok(match op {
        BinaryOp::Add => Value::Float(a + b),
        BinaryOp::Sub => Value::Float(a - b),
        BinaryOp::Mul => Value::Float(a * b),
        BinaryOp::Div => Value::Float(a / b),
        BinaryOp::Mod => {
            if b as i32 == 0 {
                return stop("modulo by zero");
            }
            Value::Int((a as i32) % (b as i32))
        }
        _ => return stop("not an arithmetic operator"),
    })
}

pub(super) fn binary(op: BinaryOp, l: &Value, r: &Value) -> Run<Value> {
    Ok(match op {
        BinaryOp::Eq => Value::Bool(l.equals(r)),
        BinaryOp::Ne => Value::Bool(!l.equals(r)),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let ordering = match (l, r) {
                (Value::Str(a), Value::Str(b)) => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
                _ if l.int().is_some() && r.int().is_some() => l.int().cmp(&r.int()),
                _ => {
                    let (Some(a), Some(b)) = (l.number(), r.number()) else {
                        return stop(format!("comparing {} and {}", l.text(), r.text()));
                    };
                    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
                }
            };
            Value::Bool(match op {
                BinaryOp::Lt => ordering.is_lt(),
                BinaryOp::Le => ordering.is_le(),
                BinaryOp::Gt => ordering.is_gt(),
                _ => ordering.is_ge(),
            })
        }
        BinaryOp::StrCat => concat_texts(l, r)?,
        op => arithmetic(op, l, r)?,
    })
}

/// String concatenation under the sandbox's byte ceiling, so one statement
/// cannot more than double a string past `MAX_STRING_BYTES`.
pub(super) fn concat_texts(l: &Value, r: &Value) -> Run<Value> {
    let (a, b) = (l.text(), r.text());
    if a.len().saturating_add(b.len()) > MAX_STRING_BYTES {
        return stop(format!("string result over {MAX_STRING_BYTES} bytes"));
    }
    Ok(Value::Str(format!("{a}{b}")))
}
