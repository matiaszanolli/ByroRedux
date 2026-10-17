//! Receiver and argument resolvers (#5801, split from `effects.rs`): a call
//! receiver → [`QuestRef`] / [`ObjectRef`] / [`ActorRef`], shared by the
//! primitives and the guards, plus the static-call and optional-argument
//! helpers.

use super::*;

/// Lookup key for [`Scope::known_quest_properties`].
///
/// A decompiled `.pex` reads an auto-property through its *backing
/// variable* (`::MQ101_var`), not the authored name (`MQ101`) the property
/// table is keyed by — the same `::`/`_var` decoration
/// [`QuestRef::property_name`]/[`ObjectRef::property_name`] strip at
/// dispatch. Normalizing here is what makes the set match on real input
/// rather than only on `.psc`-built test bodies (#2657); the `.psc` `Ident`
/// regex cannot even produce a `::X_var` identifier.
fn quest_property_key(name: &str) -> String {
    name.strip_prefix("::")
        .and_then(|n| n.strip_suffix("_var"))
        .unwrap_or(name)
        .to_ascii_lowercase()
}

/// A receiver that is *unambiguously* a quest: `Self`, `GetOwningQuest()`,
/// a local explicitly declared `Quest k = …`, or a bare identifier the
/// containing script declares as a `Quest Property`.
///
/// Everything else declines. This exists because several modeled method
/// names (`Start`, `Stop`, `Reset`, `SetActive`) are declared on more than
/// one receiver type in the Papyrus API, and nothing in the AST shape
/// distinguishes them — `<ident>.Reset()` is `Quest.Reset()`,
/// `ObjectReference.Reset()` or `Cell.Reset()` depending only on the
/// property's declared type.
pub(super) fn explicit_quest_receiver(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    let quest = receiver_quest(object, scope)?;
    match &quest {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(quest),
        QuestRef::Property(_) => {
            let Expr::Ident(name) = object else {
                return None;
            };
            let key = quest_property_key(&name.0);
            (scope.quest_locals.contains_key(&key)
                || scope
                    .known_quest_properties
                    .contains(&quest_property_key(&name.0)))
            .then_some(quest)
        }
    }
}

/// A GlobalVariable named directly (`Global`) or through a property of the
/// fragment's own quest script (`kmyQuest.Global`) — for a quest fragment
/// the quest's `VMAD` carries that script's properties too, so both resolve
/// the same way at runtime.
pub(super) fn global_property(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    match object {
        Expr::Ident(_) => global_receiver(object, scope),
        Expr::MemberAccess { object, member } => {
            match explicit_quest_receiver(&object.node, scope)? {
                QuestRef::SelfRef | QuestRef::OwningQuest => {
                    Some(ObjectRef::Property(member.node.0.clone()))
                }
                QuestRef::Property(_) => None,
            }
        }
        _ => None,
    }
}

pub(super) fn actor_value_arg(e: &Expr, scope: &Scope) -> Option<ActorValueArg> {
    if let Expr::StringLit(name) = e {
        return Some(ActorValueArg::Named(name.clone()));
    }
    let mut inner = e;
    while let Expr::Cast { expr, .. } = inner {
        inner = &expr.node;
    }
    if let Expr::Call { callee, args } = inner {
        if let Expr::MemberAccess { object, member } = &callee.node {
            let accessor = member.node.0.as_str();
            let is_game = matches!(&object.node, Expr::Ident(name) if name.0.eq_ignore_ascii_case("game"));
            if is_game && args.is_empty() {
                let lower = accessor.to_ascii_lowercase();
                if lower.starts_with("get") && lower.ends_with("av") && accessor.len() > 5 {
                    return Some(ActorValueArg::Named(accessor[3..accessor.len() - 2].to_owned()));
                }
            }
        }
        return None;
    }
    receiver_object(e, scope).map(ActorValueArg::Form)
}

pub(super) fn game_call<'a>(e: &'a Expr, method: &str) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    static_call(e, "Game", method)
}

pub(super) fn static_call<'a>(
    e: &'a Expr,
    type_name: &str,
    method: &str,
) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    let (object, args) = method_call(e, method)?;
    matches!(object, Expr::Ident(name) if name.0.eq_ignore_ascii_case(type_name)).then_some(args)
}

pub(super) fn optional_int_arg(
    args: &[byroredux_papyrus::ast::CallArg],
    idx: usize,
    default: i32,
) -> Option<i32> {
    match args.get(idx) {
        None => Some(default),
        Some(_) => i32::try_from(int_arg(args, idx)?).ok(),
    }
}

// ── Receiver / quest-expr resolution ─────────────────────────────────

/// Resolve a call receiver to a [`QuestRef`]:
/// - a local bound to a quest → its bound `QuestRef`;
/// - a declared-but-not-quest-bound local used as a receiver → `None`
///   (decline — it is *not* a same-named property);
/// - otherwise classify directly (`Self` / `Self.GetOwningQuest()` / a
///   `Quest Property`).
pub(super) fn receiver_quest(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    if let Expr::MemberAccess { object: owner, member } = object {
        return owner_quest_property(&owner.node, &member.node.0, scope);
    }
    if let Expr::Ident(name) = object {
        let key = name.0.to_ascii_lowercase();
        if let Some(via) = scope.quest_locals.get(&key) {
            return Some(via.clone());
        }
        // A plain local, or the dialogue speaker (M47.2), is no quest.
        if scope.decl_locals.contains(&key) || scope.speaker_locals.contains(&key) {
            return None;
        }
    }
    match quest_via(object)? {
        // A fragment property, which `quest_vmad_statement` forbids.
        QuestRef::Property(_) if scope.fragment_properties_off => None,
        quest => Some(quest),
    }
}

/// M47.2 — `<quest>.Member` where `<quest>` is this fragment's own quest
/// (`kmyQuest`, `Self`, `GetOwningQuest() as …`) and `Member` is a property
/// of its script whose type is, or extends, Quest. The quest's own VMAD (the
/// fragment's, on the quest path) binds the property, so it resolves like a
/// `Quest Property`. Needs the script library, to read the property's
/// declared type; the INFO and scene paths carry none and decline.
fn owner_quest_property(owner: &Expr, member: &str, scope: &Scope) -> Option<QuestRef> {
    let ty = owner_property_type(owner, member, scope)?;
    crate::fragment::extends_quest(&ty, scope.library?)
        .then(|| QuestRef::Property(member.to_owned()))
}

/// M47.2 — the declared object type of `member`, a property of the fragment
/// quest's own script (on that script or an ancestor), when `owner` is the
/// fragment's quest (`kmyQuest`, `Self`, `GetOwningQuest() as …`) and the
/// fragment's VMAD is the quest's. Needs the script library.
fn owner_property_type(owner: &Expr, member: &str, scope: &Scope) -> Option<String> {
    if !scope.vmad_is_quest {
        return None;
    }
    let library = scope.library?;
    let owner_type = match owner {
        Expr::Ident(name) => {
            let key = name.0.to_ascii_lowercase();
            match scope.quest_locals.get(&key)? {
                QuestRef::SelfRef | QuestRef::OwningQuest => {}
                QuestRef::Property(_) => return None,
            }
            scope.local_types.get(&key)?.clone()
        }
        Expr::Cast { expr, target_type } => {
            let byroredux_papyrus::ast::Type::Object(ty) = &target_type.node else {
                return None;
            };
            match quest_expr_ref(&expr.node, scope)? {
                QuestRef::SelfRef | QuestRef::OwningQuest => {}
                QuestRef::Property(_) => return None,
            }
            ty.0.clone()
        }
        _ => return None,
    };
    let mut script_name = owner_type;
    for _ in 0..8 {
        let script = library.script(&script_name)?;
        let declared = script.body.iter().find_map(|item| match &item.node {
            byroredux_papyrus::ast::ScriptItem::Property(property)
                if property.name.node.0.eq_ignore_ascii_case(member) =>
            {
                Some(property.ty.node.clone())
            }
            _ => None,
        });
        if let Some(ty) = declared {
            let byroredux_papyrus::ast::Type::Object(ty) = ty else {
                return None;
            };
            return Some(ty.0);
        }
        script_name = script.parent.as_ref()?.node.0.clone();
    }
    None
}

/// Classify the RHS of a `local = <expr>` binding as a quest reference,
/// resolving a local-to-local copy through `scope`.
pub(super) fn quest_expr_ref(value: &Expr, scope: &Scope) -> Option<QuestRef> {
    match value {
        Expr::Cast { expr, .. } => quest_expr_ref(&expr.node, scope),
        _ => receiver_quest(value, scope),
    }
}

/// Resolve a pure object lookup used as a local initializer. Bare properties
/// are deliberately excluded here because the decompiled local declaration's
/// type is not carried into this helper; the unambiguous supported shapes are
/// alias `GetRef`/`GetActorRef` calls and copies of an already-known object
/// local.
pub(super) fn object_expr_ref(value: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = value {
        return object_expr_ref(&expr.node, scope);
    }
    if let Some(linked) = linked_reference(value, scope) {
        return Some(linked);
    }
    if let Expr::Ident(name) = value {
        return scope
            .object_locals
            .get(&name.0.to_ascii_lowercase())
            .cloned();
    }
    let (alias, args) =
        alias_reference_getter(value)?;
    if !args.is_empty() {
        return None;
    }
    receiver_object(alias, scope)
}

/// Whether `value` is the dialogue speaker: a speaker name, through casts.
pub(super) fn speaker_expr_ref(value: &Expr, scope: &Scope) -> bool {
    match value {
        Expr::Cast { expr, .. } => speaker_expr_ref(&expr.node, scope),
        Expr::Ident(name) => scope.speaker_locals.contains(&name.0.to_ascii_lowercase()),
        _ => false,
    }
}

/// `<alias>.GetRef()` / `.GetActorRef()` (Fallout 4 / Starfield) or
/// `.GetReference()` / `.GetActorReference()` (Skyrim's `ReferenceAlias`
/// spelling of the same getters, M47.2).
pub(super) fn alias_reference_getter(e: &Expr) -> Option<(&Expr, &[byroredux_papyrus::ast::CallArg])> {
    // M47.2 — Starfield's `ReferenceAlias.GetShipRef()` is
    // `GetShipReference()`, which is `GetReference() as SpaceshipReference`
    // (its shipped `ReferenceAlias.pex`).
    [
        "GetRef",
        "GetActorRef",
        "GetReference",
        "GetActorReference",
        "GetShipRef",
        "GetShipReference",
    ]
        .into_iter()
        .find_map(|getter| method_call(e, getter))
}

/// A location-typed receiver or argument: a `Location` property, or a
/// `LocationAlias` property's `GetLocation()` (the alias resolves at run
/// time through its bound location).
pub(super) fn receiver_location(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = object {
        return receiver_location(&expr.node, scope);
    }
    match method_call(object, "GetLocation") {
        Some((alias, [])) => receiver_object(alias, scope),
        Some(_) => None,
        None => receiver_object(object, scope),
    }
}

pub(super) fn player_expr_ref(value: &Expr, scope: &Scope) -> bool {
    match value {
        Expr::Cast { expr, .. } => player_expr_ref(&expr.node, scope),
        Expr::Ident(name) => scope.player_locals.contains(&name.0.to_ascii_lowercase()),
        _ => is_game_get_player(value),
    }
}

/// Resolve an object-targeting effect's receiver or argument to an
/// [`ObjectRef`]. Unlike [`receiver_quest`], there is no unambiguous
/// bare-identifier case (no `Self`/`GetOwningQuest()` equivalent — see
/// [`ObjectRef`]'s doc). A bare property is accepted, and a local is accepted
/// only when [`bind_local`] proved it came from a VMAD alias getter; unrelated
/// or plain locals still decline.
/// M47.2 — `<reference>.GetLinkedRef([apKeyword])`.
fn linked_reference(e: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let (from, args) = method_call(e, "GetLinkedRef")?;
    if args.len() > 1 {
        return None;
    }
    let keyword = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(keyword) => Some(Box::new(receiver_object(keyword, scope)?)),
    };
    Some(ObjectRef::Linked {
        from: Box::new(receiver_object(from, scope)?),
        keyword,
    })
}

pub(super) fn receiver_object(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = object {
        return receiver_object(&expr.node, scope);
    }
    if let Some(linked) = linked_reference(object, scope) {
        return Some(linked);
    }
    // M47.2 — `kmyQuest.Prop`, an object property of the fragment quest's
    // own script; the quest's VMAD binds it like a fragment property. A
    // quest-typed property is a quest, never an object receiver (#2538).
    if let Expr::MemberAccess { object: owner, member } = object {
        let ty = owner_property_type(&owner.node, &member.node.0, scope)?;
        return (!crate::fragment::extends_quest(&ty, scope.library?))
            .then(|| ObjectRef::Property(member.node.0.clone()));
    }
    let alias_getter = alias_reference_getter(object);
    if let Some((alias, args)) = alias_getter {
        if !args.is_empty() {
            return None;
        }
        return receiver_object(alias, scope);
    }
    let Expr::Ident(name) = object else {
        return None;
    };
    let key = name.0.to_ascii_lowercase();
    if let Some(via) = scope.object_locals.get(&key) {
        return Some(via.clone());
    }
    // `Self` is never the object here (see `ObjectRef`'s doc) — decline
    // explicitly rather than relying on no VMAD ever naming a property
    // "self".
    //
    // #2538 / SCR-D5-NEW10-01 — a bare identifier known (from the
    // containing script's own property declarations) to be `Quest`-typed
    // must decline here too: `Quest.Start()`/`Scene.Start()` share the
    // identical zero-arg AST shape, and without this check a Quest
    // Property that `explicit_quest_receiver` correctly declined (not
    // `Self`/`GetOwningQuest()`/a bound local) would fall through the
    // primitive table and get silently misclassified as a scene
    // reference instead of the whole statement declining.
    if key == "self"
        || scope.quest_locals.contains_key(&key)
        || scope.player_locals.contains(&key)
        || scope.speaker_locals.contains(&key)
        || scope.decl_locals.contains(&key)
        || scope
            .known_quest_properties
            .contains(&quest_property_key(&name.0))
    {
        return None;
    }
    if scope.fragment_properties_off {
        return None;
    }
    Some(ObjectRef::Property(name.0.clone()))
}

pub(super) fn receiver_actor(object: &Expr, scope: &Scope) -> Option<ActorRef> {
    if player_expr_ref(object, scope) {
        Some(ActorRef::Player)
    } else if speaker_expr_ref(object, scope) {
        scope.uses_speaker.set(true);
        Some(ActorRef::Speaker)
    } else {
        receiver_object(object, scope).map(ActorRef::Object)
    }
}

/// A boolean positional argument — `Bool`/`Int` literal, unwrapping a
/// cast (mirrors [`as_num`]'s tolerance).
///
/// Three-case contract (#2023 / SCR-D5-NEW2-01), matching the fix
/// [`rumble::bool_prop`]/`float_prop` already got under #1909: `None`
/// when the argument slot is present but `as_num` can't evaluate it (a
/// local variable, `Not(...)`, a copy-propagated temp) — decline, the
/// caller must NOT assume the Papyrus-side default applies, since the
/// real runtime value is unknown. `Some(None)` when the slot is
/// genuinely absent (the call omitted the optional argument) — safe to
/// apply the default. `Some(Some(v))` when present and a literal.
///
/// Pre-fix, both "absent" and "present but non-literal" collapsed into
/// a single `None` that every call site turned into the Papyrus default
/// via `.unwrap_or(true)` — silently discarding a real (just unresolved)
/// runtime value.
pub(super) fn bool_arg(args: &[byroredux_papyrus::ast::CallArg], idx: usize) -> Option<Option<bool>> {
    match args.get(idx) {
        None => Some(None),
        Some(arg) => as_num(&arg.value.node).map(|n| Some(n != 0.0)),
    }
}
