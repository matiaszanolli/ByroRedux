//! The script model: what a call runs (state blocks, the `extends` chain),
//! declared members and constants, and the shapes of the calls the VM and
//! the check both recognise (#5800).

use super::*;

/// What a call runs: a function's or an event's parameters and body.
pub(super) struct Callable<'s> {
    pub(super) params: &'s [Param],
    pub(super) body: &'s [Spanned<Stmt>],
    /// A function's declared return type; an event returns nothing.
    pub(super) returns: Option<&'s Spanned<Type>>,
}

/// `name` defined in `script` — in the state block `state`, or in the empty
/// state for `None` — as a function or an event. A `Native` function is not
/// one: its declaration (in `Quest`, `ObjectReference`, … which the script
/// library also serves) has no body, and the native bridge runs it.
pub(super) fn callable_in<'s>(script: &'s Script, state: Option<&str>, name: &str) -> Option<Callable<'s>> {
    let named = |n: &Identifier| n.0.eq_ignore_ascii_case(name);
    let scripted = |f: &byroredux_papyrus::ast::Function| !f.flags.contains(FunctionFlags::NATIVE);
    match state {
        None => script.body.iter().find_map(|item| match &item.node {
            ScriptItem::Function(f) if named(&f.name.node) && scripted(f) => Some(Callable {
                params: &f.params,
                body: &f.body,
                returns: f.return_type.as_ref(),
            }),
            ScriptItem::Event(e) if named(&e.name.node) => Some(Callable {
                params: &e.params,
                body: &e.body,
                returns: None,
            }),
            _ => None,
        }),
        Some(state) => script.body.iter().find_map(|item| match &item.node {
            ScriptItem::State(block) if block.name.node.0.eq_ignore_ascii_case(state) => {
                block.body.iter().find_map(|item| match &item.node {
                    StateItem::Function(f) if named(&f.name.node) && scripted(f) => Some(Callable {
                        params: &f.params,
                        body: &f.body,
                        returns: f.return_type.as_ref(),
                    }),
                    StateItem::Event(e) if named(&e.name.node) => Some(Callable {
                        params: &e.params,
                        body: &e.body,
                        returns: None,
                    }),
                    _ => None,
                })
            }
            _ => None,
        }),
    }
}

/// Where a running body is: the script that defines it and its state block
/// (`None`: the empty state).
#[derive(Clone)]
pub(super) struct Code {
    pub(super) script: Arc<Script>,
    pub(super) state: Option<String>,
}

impl Code {
    pub(super) fn callable(&self, name: &str) -> Run<Callable<'_>> {
        match callable_in(&self.script, self.state.as_deref(), name) {
            Some(callable) => Ok(callable),
            None => stop(format!("no function {name}")),
        }
    }
}

/// `script` and the scripts it extends that `scripts` carries, most derived
/// first.
pub(super) fn script_chain(script: &Arc<Script>, scripts: &HashMap<String, Arc<Script>>) -> Vec<Arc<Script>> {
    let mut chain = vec![Arc::clone(script)];
    while chain.len() < 16 {
        let Some(parent) = chain.last().and_then(|last| last.parent.as_ref()) else {
            break;
        };
        match scripts.get(&parent.node.0.to_ascii_lowercase()) {
            Some(parent) => chain.push(Arc::clone(parent)),
            None => break,
        }
    }
    chain
}

/// An engine class — `Form`, `ObjectReference`, `Quest`, … — declares
/// `Native` functions; no game script does. Its API, scripted wrappers
/// included (`Quest.SetStage` → `SetCurrentStageID`), is the native
/// bridge's, so calls never resolve into it.
pub(super) fn is_engine_class(script: &Script) -> bool {
    let native = |f: &byroredux_papyrus::ast::Function| f.flags.contains(FunctionFlags::NATIVE);
    script.body.iter().any(|item| match &item.node {
        ScriptItem::Function(f) => native(f),
        ScriptItem::State(block) => block.body.iter().any(|item| matches!(&item.node, StateItem::Function(f) if native(f))),
        _ => false,
    })
}

/// The body a call of `name` runs on an instance of `script` in `state`.
/// CK *States (Papyrus)*: the current state down the `extends` chain, then
/// the empty state down the chain — stopping at the engine classes.
pub(super) fn resolve_in(script: &Arc<Script>, state: &str, name: &str, scripts: &HashMap<String, Arc<Script>>) -> Option<Code> {
    let chain: Vec<_> = script_chain(script, scripts)
        .into_iter()
        .take_while(|script| !is_engine_class(script))
        .collect();
    let in_state = (!state.is_empty())
        .then(|| {
            chain.iter().find(|s| callable_in(s, Some(state), name).is_some()).map(|s| Code {
                script: Arc::clone(s),
                state: Some(state.to_owned()),
            })
        })
        .flatten();
    in_state.or_else(|| {
        chain.iter().find(|s| callable_in(s, None, name).is_some()).map(|s| Code {
            script: Arc::clone(s),
            state: None,
        })
    })
}

/// The state an instance of `script` starts in: its `Auto State`, or one a
/// script it extends declares; else the empty state.
pub(super) fn auto_state(script: &Arc<Script>, scripts: &HashMap<String, Arc<Script>>) -> String {
    script_chain(script, scripts)
        .iter()
        .find_map(|s| {
            s.body.iter().find_map(|item| match &item.node {
                ScriptItem::State(block) if block.is_auto => Some(block.name.node.0.clone()),
                _ => None,
            })
        })
        .unwrap_or_default()
}

/// The names of `script`'s state blocks.
pub(super) fn state_names(script: &Script) -> impl Iterator<Item = &str> {
    script.body.iter().filter_map(|item| match &item.node {
        ScriptItem::State(block) => Some(block.name.node.0.as_str()),
        _ => None,
    })
}

/// The decompiled `GotoState` / `GetState` keep a script's state in the
/// `::State` variable.
pub(super) fn is_state_variable(name: &str) -> bool {
    name.eq_ignore_ascii_case("::state")
}

/// Where `script`'s state is kept among its owner's script variables — per
/// script, since one reference may carry several.
pub(super) fn state_key(script: &Script) -> ConditionStringId {
    ConditionStringId::from_text(&format!("::state::{}", script.name.node.0.to_ascii_lowercase()))
}

/// The value a script variable or property declares (`Int n = 5`,
/// `Bool Property P = True Auto`), or an `AutoReadOnly` property's constant,
/// when it is a literal.
pub(super) fn declared_default(script: &Script, name: &str) -> Option<Value> {
    let bare = vmad_property_name(name);
    let named = |n: &Identifier| n.0.eq_ignore_ascii_case(name) || n.0.eq_ignore_ascii_case(bare);
    script.body.iter().find_map(|item| {
        let initial = match &item.node {
            ScriptItem::Variable(v) if named(&v.name.node) => v.initial_value.as_ref(),
            ScriptItem::Property(p) if named(&p.name.node) => match &p.initial_value {
                Some(initial) => Some(initial),
                None => return getter_constant(p),
            },
            _ => return None,
        };
        literal_value(&initial?.node)
    })
}

pub(super) fn literal_value(e: &Expr) -> Option<Value> {
    Some(match literal_of(e)? {
        Expr::IntLit(i) => Value::Int(i as i32),
        Expr::FloatLit(f) => Value::Float(f as f32),
        Expr::BoolLit(b) => Value::Bool(b),
        Expr::StringLit(text) => Value::Str(text),
        _ => Value::None,
    })
}

/// An `AutoReadOnly` property's constant: the compiler gives it a getter
/// that returns a literal (`ObjectReference.Motion_Keyframed` → 2).
pub(super) fn getter_constant(property: &byroredux_papyrus::ast::Property) -> Option<Value> {
    let getter = property.getter.as_ref()?;
    match getter.body.as_slice() {
        [statement] => match &statement.node {
            Stmt::Return(Some(value)) => literal_value(&value.node),
            _ => None,
        },
        _ => None,
    }
}

/// The declaration of the struct type `name`: `Script#Struct` names its
/// script; a bare name is a struct of `script`'s chain.
pub(super) fn struct_declaration(
    script: &Arc<Script>,
    name: &str,
    scripts: &HashMap<String, Arc<Script>>,
) -> Option<byroredux_papyrus::ast::Struct> {
    let (owners, struct_name) = match name.split_once('#') {
        Some((owner, struct_name)) => (vec![scripts.get(&owner.to_ascii_lowercase())?.clone()], struct_name),
        None => (script_chain(script, scripts), name),
    };
    owners.iter().find_map(|owner| {
        owner.body.iter().find_map(|item| match &item.node {
            ScriptItem::Struct(declaration) if declaration.name.node.0.eq_ignore_ascii_case(struct_name) => {
                Some(declaration.clone())
            }
            _ => None,
        })
    })
}

/// The engine classes whose `AutoReadOnly` constants a script reads on any
/// reference (`akRef.Motion_Keyframed`).
pub(super) const CONSTANT_CLASSES: [&str; 3] = ["objectreference", "actor", "form"];

/// A constant property's value: an `AutoReadOnly` one's literal, as source
/// declares it or as the compiled getter returns it.
pub(super) fn property_constant(property: &byroredux_papyrus::ast::Property) -> Option<Value> {
    let read_only = property
        .flags
        .contains(byroredux_papyrus::ast::PropertyFlags::AUTO_READ_ONLY);
    match &property.initial_value {
        Some(initial) if read_only => literal_value(&initial.node),
        _ => getter_constant(property),
    }
}

/// `name` as a constant property of `script` (not a parent).
pub(super) fn constant_property<'s>(script: &'s Script, name: &str) -> Option<&'s byroredux_papyrus::ast::Property> {
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Property(p) if p.name.node.0.eq_ignore_ascii_case(name) && property_constant(p).is_some() => Some(&**p),
        _ => None,
    })
}

/// The declared type of `name` in `script` or a parent script `scripts`
/// carries.
pub(super) fn member_type_in(script: &Script, name: &str, scripts: &HashMap<String, Arc<Script>>) -> Option<Type> {
    let mut current: Option<&Script> = Some(script);
    for _ in 0..16 {
        let here = current?;
        if let Some(ty) = script_member_type(here, name) {
            return Some(ty);
        }
        current = here
            .parent
            .as_ref()
            .and_then(|parent| scripts.get(&parent.node.0.to_ascii_lowercase()))
            .map(|parent| &**parent);
    }
    None
}

/// Whether `receiver` names a property or variable of `script` declared a
/// `MusicType`: the one receiver `Add()` / `Remove()` is a music-stack call on
/// (a script's own `Add()` function is a different call).
pub(super) fn declares_music_type(script: &Script, receiver: &Expr) -> bool {
    let Expr::Ident(name) = receiver else {
        return false;
    };
    matches!(
        script_member_type(script, &name.0),
        Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("musictype")
    )
}

/// The script's declared type for `name` (a property or a script variable).
pub(super) fn script_member_type(script: &Script, name: &str) -> Option<Type> {
    let bare = vmad_property_name(name);
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Property(p)
            if p.name.node.0.eq_ignore_ascii_case(bare) || p.name.node.0.eq_ignore_ascii_case(name) =>
        {
            Some(p.ty.node.clone())
        }
        ScriptItem::Variable(v)
            if v.name.node.0.eq_ignore_ascii_case(name) || v.name.node.0.eq_ignore_ascii_case(bare) =>
        {
            Some(v.ty.node.clone())
        }
        _ => None,
    })
}

/// `Game` / `Utility` / `Debug` / `Math` — the global-function classes.
pub(super) fn static_class(e: &Expr) -> Option<&str> {
    let Expr::Ident(name) = e else {
        return None;
    };
    ["game", "utility", "debug", "math", "weather"]
        .into_iter()
        .find(|class| name.0.eq_ignore_ascii_case(class))
}

pub(super) fn spanned<T>(node: T) -> Spanned<T> {
    Spanned::new(node, Span::new(0, 0))
}

pub(super) fn ident(name: &str) -> Spanned<Expr> {
    spanned(Expr::Ident(Identifier(name.to_owned())))
}

/// A Starfield guard operation, as the decompiler lifts it.
pub(super) enum GuardCall {
    Lock,
    Unlock,
    TryLock,
}

/// `Guard.lock(...)` / `.unlock(...)` / `.trylock(...)`: the lifted form of
/// `LockGuards` / `UnlockGuards` / `TryLockGuards`, whose arguments are guard
/// names, not values. A guard serialises threads that interleave only when a run
/// suspends; this VM's runs interleave at a wait or a budget slice, and a guard
/// held across either is not modelled, so every operation succeeds at once (the
/// try-lock reads True).
pub(super) fn guard_call(callee: &Expr) -> Option<GuardCall> {
    let Expr::MemberAccess { object, member } = callee else {
        return None;
    };
    if !matches!(&object.node, Expr::Ident(id) if id.0 == "Guard") {
        return None;
    }
    match member.node.0.as_str() {
        "lock" => Some(GuardCall::Lock),
        "unlock" => Some(GuardCall::Unlock),
        "trylock" => Some(GuardCall::TryLock),
        _ => None,
    }
}

/// `Global.Value = n` as the native it is: `Global.SetValue(n)`.
pub(super) fn set_value_call(global: &Spanned<Expr>, value: &Spanned<Expr>) -> (Box<Spanned<Expr>>, Vec<CallArg>) {
    let callee = spanned(Expr::MemberAccess {
        object: Box::new(global.clone()),
        member: spanned(Identifier("SetValue".into())),
    });
    let args = vec![CallArg {
        name: None,
        value: value.clone(),
    }];
    (Box::new(callee), args)
}

/// A call of `name` (lowercase) on the script itself.
pub(super) fn self_call_named(callee: &Expr, name: &str) -> bool {
    let (receiver, called) = match callee {
        Expr::Ident(called) => (None, called),
        Expr::MemberAccess { object, member } => (Some(&object.node), &member.node),
        _ => return false,
    };
    receiver.is_none_or(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")))
        && called.0.eq_ignore_ascii_case(name)
}

/// `OnBeginState()` / `OnEndState()` on the script itself.
pub(super) fn is_state_event(callee: &Expr) -> bool {
    let (receiver, name) = match callee {
        Expr::Ident(name) => (None, name),
        Expr::MemberAccess { object, member } => (Some(&object.node), &member.node),
        _ => return false,
    };
    let on_self = receiver.is_none_or(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")));
    on_self && (name.0.eq_ignore_ascii_case("onbeginstate") || name.0.eq_ignore_ascii_case("onendstate"))
}

/// Whether `name` is written like an event (`OnInit`, `OnActivate`).
pub(super) fn is_event_name(name: &str) -> bool {
    name.len() > 2 && name[..2].eq_ignore_ascii_case("on") && name[2..3].chars().all(|c| c.is_ascii_uppercase())
}

/// `Message.ResetHelpMessage(asEvent)`, `<message>.UnshowAsHelpMessage()` and
/// `Message.ClearHelpMessages()`, with the number of arguments each takes. A help
/// message is shown once here, with no repeat interval or event to end it, so
/// there is no shown-count to reset ("allowing a message to be displayed for that
/// input event") and nothing standing to hide: each is a no-op (an
/// `UnshowAsHelpMessage` / `ClearHelpMessages` that arrives while the one-shot is
/// still on screen does not dismiss it).
pub(super) fn help_message_housekeeping(callee: &Expr) -> Option<usize> {
    let Expr::MemberAccess { member, .. } = callee else {
        return None;
    };
    match member.node.0.to_ascii_lowercase().as_str() {
        "resethelpmessage" => Some(1),
        "unshowashelpmessage" | "clearhelpmessages" => Some(0),
        _ => None,
    }
}

/// Whether `callee` is a member call named `name` (lowercase).
pub(super) fn callee_member_is(callee: &Expr, name: &str) -> bool {
    matches!(callee, Expr::MemberAccess { member, .. } if member.node.0.eq_ignore_ascii_case(name))
}

pub(super) fn is_trace(callee: &Expr) -> bool {
    let Expr::MemberAccess { object, member } = callee else {
        return false;
    };
    let name = member.node.0.to_ascii_lowercase();
    // `Debug.Trace…` write the script log; `OpenUserLog` / `CloseUserLog` (CK:
    // "Opens a new user log on disk") open and close a file of the same kind.
    // Nothing else reads either, so a statement call has no game effect.
    static_class(&object.node) == Some("debug")
        && (name.starts_with("trace") || name == "openuserlog" || name == "closeuserlog")
}

pub(super) fn callee_name(callee: &Expr) -> String {
    match callee {
        Expr::MemberAccess { member, .. } => member.node.0.clone(),
        Expr::Ident(name) => name.0.clone(),
        _ => "<call>".into(),
    }
}
