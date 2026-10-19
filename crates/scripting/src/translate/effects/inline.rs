//! Quest-script inlining (#5801, split from `effects.rs`): the
//! [`ScriptLibrary`] a call into a quest's own script (`kmyQuest.Startup()`)
//! is inlined from, and re-lowering a statement against the quest's VMAD.

use super::*;

/// Decompiled scripts by name, for inlining calls into a quest's own script
/// (M47.2). The app implements it over the script archives.
pub trait ScriptLibrary {
    /// The script named `name` (case-insensitive), decompiled; `None` when
    /// the archives do not carry it or it does not decompile.
    fn script(&self, name: &str) -> Option<std::sync::Arc<byroredux_papyrus::ast::Script>>;
}

/// The Papyrus AST a [`ScriptLibrary`] serves.
pub use byroredux_papyrus::ast::Script as PapyrusScript;

/// Decompile compiled `.pex` bytes for a [`ScriptLibrary`]. `None` on a parse
/// or decompile failure, and on a decompiler panic (one bad callee must not
/// abort a population walk).
pub fn decompile_pex_for_library(bytes: &[u8]) -> Option<std::sync::Arc<PapyrusScript>> {
    let pex = byroredux_pex::parse(bytes).ok()?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        byroredux_pex::decompile::decompile_script(&pex)
    }))
    .ok()?
    .ok()
    .map(std::sync::Arc::new)
}

/// How deep calls into quest scripts are inlined (a callee calling another).
const MAX_INLINE_DEPTH: usize = 4;

/// `<quest>.Function()` where `<quest>` is this fragment's own quest (a quest
/// local bound to `Self` / `GetOwningQuest()`, or such a call cast to the
/// quest's script type) and the function takes no arguments: lower the
/// callee's body in its own script's scope and splice its effects in.
///
/// The callee's `Self` is the same quest, so its quest effects resolve to the
/// context quest at dispatch; its properties resolve through the quest's
/// `VMAD`, which carries every script attached to the quest. Declines when the
/// library is absent, the callee is missing / native / takes parameters,
/// the call would recurse, inlining is [`MAX_INLINE_DEPTH`] deep, or any
/// callee statement does not lower.
pub(super) fn inline_quest_script_call(
    e: &Expr,
    scope: &Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Vec<Effect>> {
    let library = scope.library?;
    let Expr::Call { callee, args } = e else {
        return None;
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return None;
    };
    let (quest, script_type) = match &object.node {
        Expr::Ident(name) => {
            let key = name.0.to_ascii_lowercase();
            let quest = match scope.quest_locals.get(&key)? {
                quest @ (QuestRef::SelfRef | QuestRef::OwningQuest) => quest.clone(),
                QuestRef::Property(_) => return None,
            };
            (quest, scope.local_types.get(&key)?.clone())
        }
        Expr::Cast { target_type, .. } => {
            let quest = match quest_expr_ref(&object.node, scope)? {
                quest @ (QuestRef::SelfRef | QuestRef::OwningQuest) => quest,
                QuestRef::Property(_) => return None,
            };
            let byroredux_papyrus::ast::Type::Object(ty) = &target_type.node else {
                return None;
            };
            (quest, ty.0.to_ascii_lowercase())
        }
        _ => return None,
    };
    let function_name = member.node.0.to_ascii_lowercase();
    let frame = (script_type.clone(), function_name.clone());
    if scope.inline_stack.len() >= MAX_INLINE_DEPTH || scope.inline_stack.contains(&frame) {
        return None;
    }
    let script = library.script(&script_type)?;
    let function = script.body.iter().find_map(|item| match &item.node {
        byroredux_papyrus::ast::ScriptItem::Function(f)
            if f.name.node.0.eq_ignore_ascii_case(&function_name) =>
        {
            Some(f)
        }
        _ => None,
    })?;
    if args.len() > function.params.len()
        || function
            .flags
            .intersects(byroredux_papyrus::ast::FunctionFlags::NATIVE | byroredux_papyrus::ast::FunctionFlags::GLOBAL)
    {
        return None;
    }
    let mut callee_scope = Scope {
        library: Some(library),
        vmad_is_quest: true,
        known_quest_properties: crate::fragment::quest_property_names_with_library(
            &script,
            Some(library),
        ),
        own_argless_functions: crate::fragment::argless_function_names(&script),
        inline_stack: scope.inline_stack.iter().cloned().chain([frame]).collect(),
        lowered_statements: scope.lowered_statements.clone(),
        script_variables: script_declarations_by_number_type(&script, true),
        non_numeric_script_variables: script_declarations_by_number_type(&script, false),
        ..Scope::default()
    };
    // Inside the callee, `Self` is the quest; bind it so `Self.Foo()` (a
    // call to another of its own functions) inlines through the same path.
    callee_scope.quest_locals.insert("self".into(), QuestRef::SelfRef);
    callee_scope.local_types.insert("self".into(), script_type.clone());
    // M47.2 — bind the arguments. A number becomes a run-time local set in
    // the caller's context; a reference binds the parameter to the same
    // thing — the player, the speaker, or a property (through the quest
    // scope's bindings when the property is the fragment's own). Anything
    // else declines.
    let mut prologue = Vec::new();
    let mut bindings = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        let arg = match args.get(index) {
            Some(arg) => &arg.value.node,
            None => &param.default.as_ref()?.node,
        };
        let key = param.name.node.0.to_ascii_lowercase();
        match &param.ty.node {
            byroredux_papyrus::ast::Type::Int
            | byroredux_papyrus::ast::Type::Float
            | byroredux_papyrus::ast::Type::Bool => {
                prologue.push(Effect::SetLocal {
                    local: format!("{script_type}.{function_name}.{key}"),
                    value: classify_value(arg, scope)?,
                });
                callee_scope.runtime_locals.insert(key);
            }
            byroredux_papyrus::ast::Type::Object(_) => match receiver_actor(arg, scope)? {
                ActorRef::Player => {
                    callee_scope.player_locals.insert(key);
                }
                ActorRef::Speaker => {
                    callee_scope.speaker_locals.insert(key);
                }
                ActorRef::Object(ObjectRef::Property(name)) if scope.vmad_is_quest => {
                    callee_scope
                        .object_locals
                        .insert(key, ObjectRef::Property(name));
                }
                // A linked reference resolves through its source's own
                // binding, which a quest scope cannot carry.
                ActorRef::Object(linked @ ObjectRef::Linked { .. }) if scope.vmad_is_quest => {
                    callee_scope.object_locals.insert(key, linked);
                }
                ActorRef::Object(ObjectRef::Linked { .. }) => return None,
                ActorRef::Object(ObjectRef::Property(name)) => {
                    let parameter = format!("inline.{script_type}.{function_name}.{key}");
                    bindings.push((
                        parameter.clone(),
                        crate::translate::compose::vmad_property_name(&name).to_owned(),
                    ));
                    callee_scope
                        .object_locals
                        .insert(key, ObjectRef::Property(parameter));
                }
            },
            _ => return None,
        }
    }
    let effects = lower_statements(&function.body, &mut callee_scope, providers, 0)?;
    if scope.vmad_is_quest {
        prologue.extend(effects);
        return Some(prologue);
    }
    // M47.2 — from an INFO or scene fragment the callee's properties live on
    // the quest's VMAD, not the fragment's: scope them. The scope runs in one
    // go, so a callee that would suspend declines.
    if effects.iter().any(has_latent_tail) {
        return None;
    }
    prologue.push(Effect::QuestScope {
        quest,
        effects,
        bindings,
    });
    Some(prologue)
}

/// M47.2 — an INFO or scene statement that reads the quest script's own
/// properties (`akSpeaker.AddToFaction((GetOwningQuest() as X).Faction)`),
/// lowered against the quest's VMAD and wrapped in [`Effect::QuestScope`].
/// The re-lowering forbids the fragment's own properties and the locals bound
/// from them, so everything it names resolves through the quest's VMAD or is
/// VMAD-free (the player, the speaker, literals).
pub(super) fn quest_vmad_statement(
    e: &Expr,
    scope: &Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Effect> {
    let quest_scope = quest_vmad_scope(scope)?;
    let effect = classify_effect_with_providers(e, &quest_scope, providers)?;
    if has_latent_tail(&effect) {
        return None;
    }
    Some(Effect::QuestScope {
        quest: QuestRef::OwningQuest,
        effects: vec![effect],
        bindings: Vec::new(),
    })
}

/// The scope an INFO / scene statement re-lowers in against its quest's VMAD:
/// the fragment's own properties, the locals bound from them and its Quest
/// properties forbidden. `None` where the VMAD already is the quest's, or
/// without a script library.
pub(super) fn quest_vmad_scope<'lib>(scope: &Scope<'lib>) -> Option<Scope<'lib>> {
    if scope.vmad_is_quest || scope.library.is_none() {
        return None;
    }
    Some(Scope {
        vmad_is_quest: true,
        fragment_properties_off: true,
        object_locals: HashMap::new(),
        known_quest_properties: HashSet::new(),
        ..scope.clone()
    })
}

/// The lowercased names of `script`'s own variables and properties whose
/// declared type is (`numeric == true`) or is not (`false`) a number the
/// quest-variable store holds: `Bool` / `Int` / `Float` (#5978). A
/// non-numeric property also lists its decompiled `::<name>_var` backing name.
fn script_declarations_by_number_type(
    script: &byroredux_papyrus::ast::Script,
    numeric: bool,
) -> HashSet<String> {
    use byroredux_papyrus::ast::{ScriptItem, Type};
    let holds_number = |ty: &Type| matches!(ty, Type::Bool | Type::Int | Type::Float);
    let mut names = HashSet::new();
    for item in &script.body {
        match &item.node {
            ScriptItem::Variable(v) if holds_number(&v.ty.node) == numeric => {
                names.insert(v.name.node.0.to_ascii_lowercase());
            }
            ScriptItem::Property(p) if holds_number(&p.ty.node) == numeric => {
                let name = p.name.node.0.to_ascii_lowercase();
                if !numeric {
                    names.insert(format!("::{name}_var"));
                }
                names.insert(name);
            }
            _ => {}
        }
    }
    names
}
