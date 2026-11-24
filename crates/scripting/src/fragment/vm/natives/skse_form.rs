//! SKSE's `Form` reads (LMC-SCR-004 catalog growth, `docs/engine/extender-catalog.md` §9): a form's type, name,
//! weight, keywords and world model, answered from [`crate::form_facts`] on Skyrim.
//!
//! SKSE64's `PapyrusForm.cpp` casts the form to the component a read needs and answers the empty value ("", 0,
//! None, an empty array, false) for a class without it. A reference is a `TESObjectREFR` with none of them, so
//! `Form.GetName` on a placed actor answers "" in SKSE. A component the class carries but the engine does not read
//! stops the call with that reason.
//!
//! Papyrus calls a function by the object's own script, so SKSE's subclass functions of the same name answer
//! instead: `HeadPart.GetType` (the head part's type) on an `HDPT`, `ActorBase.GetWeight` (the body weight, not
//! served) on an `NPC_`.

use super::*;
use crate::form_facts::{form_type, FormFacts};

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "gettype",
    "getname",
    "getweight",
    "getnumkeywords",
    "getnthkeyword",
    "getkeywords",
    "hasworldmodel",
    "getworldmodelpath",
];

/// What a form is to these reads.
enum Receiver {
    /// A base form, by its record signature.
    Base([u8; 4]),
    /// A placed reference: `ACHR` (an actor) or `REFR`.
    Reference { actor: bool },
}

impl Vm<'_> {
    fn skse_receiver(&self, form: u32) -> Option<Receiver> {
        let facts = self.world.try_resource::<FormFacts>()?;
        if let Some(signature) = facts.signature(form) {
            return Some(Receiver::Base(signature));
        }
        self.entity_of(form)?;
        let actor = self.base_of(form).and_then(|base| facts.signature(base)) == Some(*b"NPC_");
        Some(Receiver::Reference { actor })
    }

    /// The `Form` reads of [`Vm::object_read`], on Skyrim.
    pub(in crate::fragment::vm) fn read_skse_form(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        let Value::Form(form) = *owner else {
            return Ok(None);
        };
        if !READS.contains(&name) {
            return Ok(None);
        }
        let skyrim = self.world.try_resource::<crate::ScriptGame>().is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Skyrim);
        if !skyrim {
            return stop(format!("Form.{name} outside Skyrim"));
        }
        let values = self.eval_args(frame, args)?;
        let Some(receiver) = self.skse_receiver(form) else {
            return stop(format!("Form.{name} on {form:08X}, a form the engine has no record of"));
        };
        let signature = match receiver {
            Receiver::Reference { actor } => {
                // A reference carries none of the components.
                return Ok(Some(match name {
                    "gettype" => Value::Int(if actor { 62 } else { 61 }),
                    "getname" | "getworldmodelpath" => Value::Str(String::new()),
                    "getweight" => Value::Float(0.0),
                    "getnumkeywords" => Value::Int(0),
                    "getnthkeyword" => Value::None,
                    "getkeywords" => Value::Array(Rc::new(RefCell::new(Vec::new()))),
                    _ => Value::Bool(false),
                }));
            }
            Receiver::Base(signature) => signature,
        };
        let label = String::from_utf8_lossy(&signature).into_owned();
        let Some(class) = form_type(&signature) else {
            return stop(format!("Form.{name} on a {label}, a record SKSE's form types do not name"));
        };
        let facts = self.world.resource::<FormFacts>();
        let fact = facts.get(form);
        let unread = |what: &str| stop(format!("Form.{name} on a {label}: the engine does not read its {what}"));
        Ok(Some(match name {
            "gettype" if &signature == b"HDPT" => match fact.and_then(|fact| fact.head_part_type) {
                Some(part) => Value::Int(part as i32),
                None => return unread("head part type"),
            },
            "gettype" => Value::Int(class.number),
            "getname" if !class.full_name => Value::Str(String::new()),
            "getname" => match fact.and_then(|fact| fact.name.clone()) {
                Some(full_name) => Value::Str(full_name),
                None => return unread("name"),
            },
            "getweight" if &signature == b"NPC_" => return stop("ActorBase.GetWeight (an NPC's body weight) is not served"),
            "getweight" if !class.weight => Value::Float(0.0),
            "getweight" => match fact.and_then(|fact| fact.weight) {
                Some(weight) => Value::Float(weight),
                None => return unread("weight"),
            },
            "getnumkeywords" | "getnthkeyword" | "getkeywords" if !class.keywords => match name {
                "getnumkeywords" => Value::Int(0),
                "getnthkeyword" => Value::None,
                _ => Value::Array(Rc::new(RefCell::new(Vec::new()))),
            },
            "getnumkeywords" | "getnthkeyword" | "getkeywords" => {
                let Some(keywords) = fact.and_then(|fact| fact.keywords.as_ref()) else {
                    return unread("keywords");
                };
                match name {
                    "getnumkeywords" => Value::Int(keywords.len() as i32),
                    // `UInt32` index: a negative one is past the end. A null entry is None.
                    "getnthkeyword" => {
                        let index = values.first().and_then(Value::int).unwrap_or(0) as u32 as usize;
                        keywords.get(index).filter(|keyword| **keyword != 0).map_or(Value::None, |keyword| Value::Form(*keyword))
                    }
                    // Null entries are skipped.
                    _ => Value::Array(Rc::new(RefCell::new(
                        keywords.iter().filter(|keyword| **keyword != 0).map(|keyword| Value::Form(*keyword)).collect(),
                    ))),
                }
            }
            "hasworldmodel" => Value::Bool(class.model),
            "getworldmodelpath" if !class.model => Value::Str(String::new()),
            "getworldmodelpath" => match fact.and_then(|fact| fact.model.clone()) {
                Some(path) => Value::Str(path),
                None => return unread("model"),
            },
            _ => return Ok(None),
        }))
    }
}

impl Checker<'_> {
    /// Whether the VM runs SKSE's `Form` read `name` on a form.
    pub(in crate::fragment::vm) fn check_skse_form(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        _statement: bool,
    ) -> Option<Kind> {
        let skyrim = self.game.is_none_or(|game| game == byroredux_plugin::esm::reader::GameKind::Skyrim);
        if !skyrim || !matches!(receiver_kind, Kind::Object | Kind::Player | Kind::Quest) {
            return None;
        }
        let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        match (name, arg_kinds) {
            ("gettype" | "getnumkeywords" | "getweight", []) => Some(Kind::Num),
            ("getname" | "getworldmodelpath", []) => Some(Kind::Str),
            ("hasworldmodel", []) => Some(Kind::Bool),
            ("getkeywords", []) => Some(Kind::Array),
            ("getnthkeyword", [index]) if numeric(index) => Some(Kind::Object),
            _ => None,
        }
    }
}
