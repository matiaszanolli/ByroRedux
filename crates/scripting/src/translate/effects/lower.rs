//! Statement lowering (#5801, split from `effects.rs`): the
//! `lower_*_fragment` entry points, `lower_statements` / `lower_if`, local
//! binding, quest / self / reference variable writes, the latent `While`
//! polls, latent-tail analysis and unread-local pruning.

use super::*;

/// Lower a predominantly flat fragment body to its canonical effects, or
/// decline.
///
/// Returns `None` (decline, the whole fragment) if the body contains any
/// control flow outside MQ101's exact `Is3DLoaded` polling-loop shape, or any
/// statement no effect primitive claims — never a partial lowering. An empty
/// body lowers to an empty effect list (a no-op fragment is trivially
/// understood).
///
/// Thin wrapper over [`lower_fragment_with_quest_properties`] with an
/// empty quest-property set, for the many call sites (mostly tests) with
/// no script-level property metadata available — preserves this
/// function's pre-#2538 behavior exactly for those.
///
/// #2658 (SCR-D5-NEW11-03) — `#[doc(hidden)]` because the context-free
/// path is test-only by design: the single production caller
/// (`fragment::populate_quest_fragments_from_script`) always calls
/// [`lower_fragment_with_quest_properties`] with a real set instead. Two
/// coverage/conformance measurement harnesses called this one anyway,
/// which for a time made them measure a lowering path production never
/// runs — hiding it from docs is a nudge against a future call site
/// picking it by mistake, not an access restriction (both examples and
/// every test in this crate can still see and call it fine).
#[doc(hidden)]
pub fn lower_fragment(body: &[Spanned<Stmt>]) -> Option<Vec<Effect>> {
    lower_fragment_with_quest_properties(body, &HashSet::new())
}

/// [`lower_fragment`], with the containing script's `Quest Property`
/// names (lowercased) made available to break the `Quest.Start()` vs
/// `Scene.Start()` ambiguity — see [`Scope::known_quest_properties`].
/// #2538 / SCR-D5-NEW10-01.
pub fn lower_fragment_with_quest_properties(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
) -> Option<Vec<Effect>> {
    lower_fragment_with_quest_properties_and_providers(body, quest_property_names, None)
}

/// Lower a fragment with an exact provider catalog. Top-level provider calls
/// become sequencing barriers; the runtime resumes later effects only after
/// guard-free host dispatch completes.
pub fn lower_fragment_with_quest_properties_and_providers(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Vec<Effect>> {
    lower_fragment_with_script_metadata(body, quest_property_names, &HashSet::new(), providers)
}

/// [`lower_fragment_with_quest_properties_and_providers`] with the
/// containing script's own function table available (#5407): the
/// lowercased names of its argless functions, built by
/// [`crate::fragment::populate::argless_function_names`]. Production
/// (quest/scene/alias fragment population) passes it; bare harnesses
/// keep the empty-set behavior.
pub fn lower_fragment_with_script_metadata(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    own_argless_function_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Vec<Effect>> {
    let mut scope = Scope {
        known_quest_properties: quest_property_names.clone(),
        own_argless_functions: own_argless_function_names.clone(),
        ..Scope::default()
    };
    lower_statements(body, &mut scope, providers, 0)
        .map(prune_unread_locals)
        .filter(|effects| locals_never_suspend(effects))
}

/// [`lower_fragment_with_script_metadata`] for a quest-stage fragment, with a
/// [`ScriptLibrary`] to inline calls into the quest's own script.
pub fn lower_quest_fragment(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    own_argless_function_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
    library: Option<&dyn ScriptLibrary>,
) -> Option<Vec<Effect>> {
    let mut scope = Scope {
        library,
        vmad_is_quest: true,
        known_quest_properties: quest_property_names.clone(),
        own_argless_functions: own_argless_function_names.clone(),
        ..Scope::default()
    };
    lower_statements(body, &mut scope, providers, 0)
        .map(prune_unread_locals)
        .filter(|effects| locals_never_suspend(effects))
}

/// M47.2 — [`lower_fragment_with_script_metadata`] for a scene (`SF_…`)
/// fragment, with a [`ScriptLibrary`] so a call into the owning quest's
/// script inlines as an [`Effect::QuestScope`].
pub fn lower_scene_fragment(
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    own_argless_function_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
    library: Option<&dyn ScriptLibrary>,
) -> Option<Vec<Effect>> {
    let mut scope = Scope {
        library,
        known_quest_properties: quest_property_names.clone(),
        own_argless_functions: own_argless_function_names.clone(),
        ..Scope::default()
    };
    lower_statements(body, &mut scope, providers, 0)
        .map(prune_unread_locals)
        .filter(|effects| locals_never_suspend(effects))
}

/// M47.2 — `(<reference> as Script).<Variable> = literal`, a variable of a
/// script attached to a reference. Stored under the `::Name_var` spelling the
/// CTDA `GetVMScriptVariable` names it by.
fn lower_reference_variable_write(
    target: &Expr,
    op: byroredux_papyrus::ast::AssignOp,
    value: &Expr,
    scope: &Scope,
) -> Option<Effect> {
    if op != byroredux_papyrus::ast::AssignOp::Eq {
        return None;
    }
    let Expr::MemberAccess { object, member } = target else {
        return None;
    };
    let value = as_num(value)?;
    // The receiver must be cast to a script the library knows, that is no
    // quest (#2538) and that declares the variable (on it or an ancestor) —
    // an untyped property could be a quest, whose variables live elsewhere.
    let Expr::Cast { target_type, .. } = &object.node else {
        return None;
    };
    let byroredux_papyrus::ast::Type::Object(script_type) = &target_type.node else {
        return None;
    };
    let library = scope.library?;
    if crate::fragment::extends_quest(&script_type.0, library) {
        return None;
    }
    let name = &member.node.0;
    let declared = |script: &byroredux_papyrus::ast::Script| {
        script.body.iter().any(|item| match &item.node {
            byroredux_papyrus::ast::ScriptItem::Property(p) => {
                p.name.node.0.eq_ignore_ascii_case(name)
            }
            byroredux_papyrus::ast::ScriptItem::Variable(v) => {
                v.name.node.0.eq_ignore_ascii_case(name)
            }
            _ => false,
        })
    };
    let mut script_name = script_type.0.clone();
    let mut found = false;
    for _ in 0..8 {
        let Some(script) = library.script(&script_name) else {
            break;
        };
        if declared(&script) {
            found = true;
            break;
        }
        let Some(parent) = script.parent.as_ref() else {
            break;
        };
        script_name = parent.node.0.clone();
    }
    if !found {
        return None;
    }
    let variable = if is_backing_variable(name) {
        name.clone()
    } else {
        format!("::{name}_var")
    };
    Some(Effect::SetReferenceVariable {
        object: receiver_object(&object.node, scope)?,
        variable,
        value,
    })
}

/// The parameter through which an INFO (`TIF_…`) fragment receives the
/// speaking actor — `Function Fragment_N(ObjectReference akSpeakerRef)` in
/// every Skyrim / Fallout 4 / Starfield topic-info fragment script.
pub const INFO_SPEAKER_PARAMETER: &str = "akSpeakerRef";

/// Lower one INFO (`TIF_…`) fragment function (M47.2): like
/// [`lower_fragment_with_script_metadata`], with the function's
/// [`INFO_SPEAKER_PARAMETER`] (and every local bound from it) lowering to
/// [`ActorRef::Speaker`].
///
/// The speaker is a live entity of the dialogue call, not something a saved
/// continuation can name, so a fragment that uses it and also carries a
/// latent tail (a `Utility.Wait`, a 3D-loaded poll, a provider barrier) —
/// whose remainder would be queued and resumed later — declines.
pub fn lower_info_fragment(
    params: &[byroredux_papyrus::ast::Param],
    body: &[Spanned<Stmt>],
    quest_property_names: &HashSet<String>,
    own_argless_function_names: &HashSet<String>,
    providers: Option<&PapyrusProviderCatalog>,
    library: Option<&dyn ScriptLibrary>,
) -> Option<Vec<Effect>> {
    let mut scope = Scope {
        library,
        known_quest_properties: quest_property_names.clone(),
        own_argless_functions: own_argless_function_names.clone(),
        ..Scope::default()
    };
    for param in params {
        if param.name.node.0.eq_ignore_ascii_case(INFO_SPEAKER_PARAMETER) {
            scope.speaker_locals.insert(param.name.node.0.to_ascii_lowercase());
        }
    }
    let effects = lower_statements(body, &mut scope, providers, 0)
        .map(prune_unread_locals)
        .filter(|effects| locals_never_suspend(effects))?;
    if scope.uses_speaker.get() && effects.iter().any(has_latent_tail) {
        return None;
    }
    Some(effects)
}

/// M47.2 — drop the [`Effect::SetLocal`]s no [`ValueExpr::Local`] reads (a
/// local assigned a number and never compared or used).
fn prune_unread_locals(effects: Vec<Effect>) -> Vec<Effect> {
    fn read_value(value: &ValueExpr, out: &mut HashSet<String>) {
        match value {
            ValueExpr::Local(local) => {
                out.insert(local.clone());
            }
            ValueExpr::Arith(left, _, right) => {
                read_value(left, out);
                read_value(right, out);
            }
            ValueExpr::Abs(inner) => read_value(inner, out),
            _ => {}
        }
    }
    fn read_guard(guard: &FragmentGuard, out: &mut HashSet<String>) {
        match guard {
            FragmentGuard::Compare { left, right, .. } => {
                read_value(left, out);
                read_value(right, out);
            }
            FragmentGuard::Any(alternatives) => {
                alternatives.iter().flatten().for_each(|guard| read_guard(guard, out));
            }
            _ => {}
        }
    }
    fn reads(effects: &[Effect], out: &mut HashSet<String>) {
        for effect in effects {
            match effect {
                Effect::SetLocal { value, .. } => read_value(value, out),
                Effect::Conditional {
                    guards,
                    then_effects,
                    else_effects,
                } => {
                    guards.iter().for_each(|guard| read_guard(guard, out));
                    reads(then_effects, out);
                    reads(else_effects, out);
                }
                Effect::QuestScope { effects, .. } => reads(effects, out),
                _ => {}
            }
        }
    }
    fn prune(effects: Vec<Effect>, read: &HashSet<String>) -> Vec<Effect> {
        effects
            .into_iter()
            .filter(|effect| !matches!(effect, Effect::SetLocal { local, .. } if !read.contains(local)))
            .map(|effect| match effect {
                Effect::Conditional {
                    guards,
                    then_effects,
                    else_effects,
                } => Effect::Conditional {
                    guards,
                    then_effects: prune(then_effects, read),
                    else_effects: prune(else_effects, read),
                },
                Effect::QuestScope {
                    quest,
                    effects,
                    bindings,
                } => Effect::QuestScope {
                    quest,
                    effects: prune(effects, read),
                    bindings,
                },
                other => other,
            })
            .collect()
    }
    let mut read = HashSet::new();
    reads(&effects, &mut read);
    prune(effects, &read)
}

/// M47.2 — a fragment's run-time locals ([`Effect::SetLocal`]) live for one
/// run; a continuation resumed after a suspension would not carry them, so a
/// fragment holding both declines.
fn locals_never_suspend(effects: &[Effect]) -> bool {
    fn sets_local(effect: &Effect) -> bool {
        match effect {
            Effect::SetLocal { .. } => true,
            Effect::Conditional {
                then_effects,
                else_effects,
                ..
            } => then_effects.iter().chain(else_effects).any(sets_local),
            Effect::QuestScope { effects, .. } => effects.iter().any(sets_local),
            _ => false,
        }
    }
    !(effects.iter().any(sets_local) && effects.iter().any(has_latent_tail))
}

/// Whether running `effect` can suspend the fragment and queue the rest.
/// M47.2 — whether `effect` suspends a run (a wait, a provider barrier),
/// for the Papyrus VM, which cannot.
pub fn is_latent(effect: &Effect) -> bool {
    has_latent_tail(effect)
}

pub(super) fn has_latent_tail(effect: &Effect) -> bool {
    match effect {
        Effect::Wait { .. }
        | Effect::WaitForActors3DLoaded { .. }
        | Effect::WaitWhile { .. }
        | Effect::ProviderCall(_) => true,
        Effect::Conditional {
            then_effects,
            else_effects,
            ..
        } => then_effects.iter().chain(else_effects).any(has_latent_tail),
        Effect::QuestScope { effects, .. } => effects.iter().any(has_latent_tail),
        _ => false,
    }
}

/// Maximum `Stmt::If` nesting depth [`lower_statements`] recurses through
/// before declining rather than recursing further.
///
/// #3279 (SCR-D5-2026-08-24-02) — unlike every sibling recursive pass in
/// this domain (`crates/pex/src/decompile/control_flow.rs`'s
/// `MAX_REBUILD_DEPTH = 1024`, `crates/papyrus/src/parser/{expr,stmt}.rs`'s
/// `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`), this recursion carried no
/// depth cap of its own. Not independently unbounded in practice — every
/// reachable input path is itself already bounded by one of those two
/// upstream caps (a `.psc`-sourced AST by `MAX_STMT_DEPTH`, a
/// `.pex`-sourced one, this feature's actual target, by
/// `MAX_REBUILD_DEPTH` reconstructing the nested-`If` shape) — but a
/// defense-in-depth gap: if either upstream cap is ever loosened without
/// re-deriving whether this function's own stack budget still holds, this
/// would be the first place that finds out, silently, via a crash.
/// Capped at `MAX_STMT_DEPTH`, the smaller of the two upstream bounds.
///
/// #3945 — referenced from `byroredux_papyrus` rather than restated as a
/// literal. "Capped at `MAX_STMT_DEPTH`" was a claim no compiler checked.
pub(super) const MAX_CONDITIONAL_DEPTH: u32 = byroredux_papyrus::parser::stmt::MAX_STMT_DEPTH;

pub(super) fn lower_statements(
    body: &[Spanned<Stmt>],
    scope: &mut Scope,
    providers: Option<&PapyrusProviderCatalog>,
    depth: u32,
) -> Option<Vec<Effect>> {
    if depth >= MAX_CONDITIONAL_DEPTH {
        return None;
    }
    scope.spend_statements(body.len())?;
    let mut effects = Vec::new();
    for (position, stmt) in body.iter().enumerate() {
        match &stmt.node {
            // `Quest k = <quest-expr>` — a local quest binding. Other-typed
            // local decls are recorded (so a later misuse declines) but
            // contribute no effect. A bare decl (no initializer) is a
            // plain local.
            Stmt::VarDecl(var) => {
                let name = var.name.node.0.to_ascii_lowercase();
                if let byroredux_papyrus::ast::Type::Object(ty) = &var.ty.node {
                    scope.local_types.insert(name.clone(), ty.0.to_ascii_lowercase());
                }
                match &var.initial_value {
                    Some(init) => {
                        if let Some(set) = bind_local(scope, name, &init.node)? {
                            effects.push(set);
                        }
                    }
                    None => {
                        scope.decl_locals.insert(name);
                    }
                }
            }
            // M47.2 — `quest.Property = literal`, a quest-script variable.
            Stmt::Assign { target, op, value } if matches!(target.node, Expr::MemberAccess { .. }) => {
                let write = lower_quest_variable_write(&target.node, *op, &value.node, scope)
                    .or_else(|| lower_reference_variable_write(&target.node, *op, &value.node, scope))
                    .or_else(|| {
                        // M47.2 — an INFO / scene write through the quest
                        // script's own properties, against the quest's VMAD.
                        let quest_scope = quest_vmad_scope(scope)?;
                        let write =
                            lower_quest_variable_write(&target.node, *op, &value.node, &quest_scope)?;
                        Some(Effect::QuestScope {
                            quest: QuestRef::OwningQuest,
                            effects: vec![write],
                            bindings: Vec::new(),
                        })
                    })?;
                effects.push(write);
            }
            // Re-assignment to an existing local: same binding rule.
            Stmt::Assign { target, op, value } => {
                let Expr::Ident(ident) = &target.node else {
                    return None; // assignment to an index — unmodeled
                };
                let name = ident.0.to_ascii_lowercase();
                // Only a local this function declared can be re-bound. A bare
                // identifier that is none of them is a script variable or a
                // property: that write is persistent state `bind_local` would
                // record as a side-effect-free local and drop (#5638).
                if !scope.declares(&name) {
                    // M47.2 — where the VMAD is the quest's (a quest
                    // fragment, an inlined quest-script body) the script is
                    // attached to the quest, so `::X_var` is one of the
                    // quest's script variables. An INFO's or scene's own
                    // variables are not, and still decline.
                    if scope.vmad_is_quest && is_script_variable(&ident.0, scope) {
                        effects.push(lower_self_variable_write(&ident.0, *op, &value.node, scope)?);
                        continue;
                    }
                    return None;
                }
                // A compound assignment (`n += 2`) is not a re-bind to the
                // right-hand side: `bind_local` would record `n = 2`. Every
                // sibling variable writer declines `op != Eq` (#5976).
                if *op != byroredux_papyrus::ast::AssignOp::Eq {
                    return None;
                }
                if let Some(set) = bind_local(scope, name, &value.node)? {
                    effects.push(set);
                }
            }
            // `Return` with no value is Champollion's fragment terminator —
            // and only as the body's final statement. Anywhere else (an
            // early `Return` inside an `If`, or before more statements) it
            // is control flow: lowering it as nothing would run the rest
            // unconditionally, so decline (#5631). The provider lowerer
            // draws the same line (`lower_program.rs`).
            Stmt::Return(None) if depth == 0 && position + 1 == body.len() => {}
            Stmt::ExprStmt(e) => match classify_effect_with_providers(&e.node, scope, providers) {
                Some(effect) => effects.push(effect),
                // M47.2 — a call into the quest's own script, inlined.
                None => match inline_quest_script_call(&e.node, scope, providers) {
                    Some(inlined) => effects.extend(inlined),
                    None => effects.push(quest_vmad_statement(&e.node, scope, providers)?),
                },
            },
            Stmt::While { condition, body } => {
                effects.push(
                    lower_3d_loaded_wait(&condition.node, body, scope)
                        .or_else(|| lower_wait_while(&condition.node, body, scope))?,
                );
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                effects.push(lower_if(
                    &condition.node,
                    body,
                    elseif_clauses,
                    else_body.as_deref(),
                    scope,
                    providers,
                    depth,
                )?);
            }
            // Other control flow / valued return remain outside this
            // increment's conservative sequence model — decline.
            _ => return None,
        }
    }
    Some(effects)
}

/// `If` / `ElseIf` / `Else` as an [`Effect::Conditional`]; each `ElseIf`
/// (M47.2) is the else branch of the one before it — exactly Papyrus's
/// first-true-wins order.
fn lower_if(
    condition: &Expr,
    body: &[Spanned<Stmt>],
    elseif_clauses: &[(Spanned<Expr>, Vec<Spanned<Stmt>>)],
    else_body: Option<&[Spanned<Stmt>]>,
    scope: &mut Scope,
    providers: Option<&PapyrusProviderCatalog>,
    depth: u32,
) -> Option<Effect> {
    // An `ElseIf` chain nests one level per clause without passing through
    // `lower_statements`, so it is bounded here by the same cap.
    if depth >= MAX_CONDITIONAL_DEPTH {
        return None;
    }
    let mut guards = Vec::new();
    classify_condition(condition, scope, false, &mut guards)?;
    let mut then_scope = scope.clone();
    let then_effects = lower_statements(body, &mut then_scope, providers, depth + 1)?;
    let mut else_scope = scope.clone();
    let else_effects = match elseif_clauses.split_first() {
        Some(((condition, body), rest)) => vec![lower_if(
            &condition.node,
            body,
            rest,
            else_body,
            &mut else_scope,
            providers,
            depth + 1,
        )?],
        None => match else_body {
            Some(body) => lower_statements(body, &mut else_scope, providers, depth + 1)?,
            None => Vec::new(),
        },
    };
    // The branches lowered against clones, so a declaration inside one
    // correctly stays inside it. But Papyrus locals are function-scoped: an
    // assignment in a branch to a local declared BEFORE the `If` changes what
    // that name means after `EndIf`, and the clone is dropped. Decline rather
    // than lower the tail against the pre-branch binding (#5632).
    if scope.rebound_by(&then_scope) || scope.rebound_by(&else_scope) {
        return None;
    }
    // M47.2 — a branch may wait: `apply_effects`' cursor stack (#3935)
    // suspends with the rest of the branch followed by the rest of the
    // program, so the resume continues exactly there. The callers still
    // refuse a latent effect where it cannot suspend (a quest scope, a
    // speaker line, run-time locals) — `has_latent_tail` looks into branches.
    Some(Effect::Conditional {
        guards,
        then_effects,
        else_effects,
    })
}

/// `<quest>.<Property> = <literal>` (M47.2). The receiver must be a quest the
/// lowerer can name — a quest local (`kmyQuest`, bound from `Self as Quest`
/// or `GetOwningQuest()`), `GetOwningQuest()` itself through its cast, or a
/// known `Quest` property — never a bare `Self`, which in a topic-info or
/// scene fragment is that fragment's own script, not the quest. Only a plain
/// `=` of a numeric / Bool literal lowers: a computed value (`x.value + 1`)
/// needs the read the lowerer does not model.
fn lower_quest_variable_write(
    target: &Expr,
    op: byroredux_papyrus::ast::AssignOp,
    value: &Expr,
    scope: &Scope,
) -> Option<Effect> {
    if op != byroredux_papyrus::ast::AssignOp::Eq {
        return None;
    }
    let Expr::MemberAccess { object, member } = target else {
        return None;
    };
    // M47.2 — `<global>.Value = …`, the GlobalVariable property's write.
    if member.node.0.eq_ignore_ascii_case("value") {
        if let Some(global) = global_property(&object.node, scope) {
            return Some(Effect::SetGlobalValue {
                global,
                value: classify_value(value, scope)?,
            });
        }
    }
    let quest = quest_variable_receiver(&object.node, scope)?;
    if let Some(value) = as_num(value) {
        return Some(Effect::SetQuestVariable {
            quest,
            variable: member.node.0.clone(),
            value,
        });
    }
    // `X = X + c` / `X = c + X` / `X = X - c` on the same variable.
    let mut node = value;
    while let Expr::Cast { expr, .. } = node {
        node = &expr.node;
    }
    let Expr::BinaryOp { op, left, right } = node else {
        return None;
    };
    let same = |e: &Expr| {
        let mut e = e;
        while let Expr::Cast { expr, .. } = e {
            e = &expr.node;
        }
        matches!(e, Expr::MemberAccess { object: o, member: m }
            if m.node.0.eq_ignore_ascii_case(&member.node.0)
                && quest_variable_receiver(&o.node, scope).as_ref() == Some(&quest))
    };
    let delta = match op {
        BinaryOp::Add if same(&left.node) => as_num(&right.node)?,
        BinaryOp::Add if same(&right.node) => as_num(&left.node)?,
        BinaryOp::Sub if same(&left.node) => -as_num(&right.node)?,
        _ => return None,
    };
    Some(Effect::ModQuestVariable {
        quest,
        variable: member.node.0.clone(),
        delta,
    })
}

/// A decompiled auto-property / script-variable backing name, `::Name_var`.
fn is_backing_variable(name: &str) -> bool {
    name.starts_with("::") && name.len() > 6 && name.to_ascii_lowercase().ends_with("_var")
}

/// Inside an inlined quest-script body: whether `name` is one of that
/// script's own variables — a `::Name_var` backing name, or a declared
/// variable / property — and not a local.
fn is_script_variable(name: &str, scope: &Scope) -> bool {
    !scope.declares(&name.to_ascii_lowercase())
        && (is_backing_variable(name)
            || scope
                .script_variables
                .contains(&name.to_ascii_lowercase()))
}

/// M47.2 — a write to the quest script's own variable `name` (a
/// `::Name_var`) inside an inlined quest-script body: a literal, the same
/// variable plus or minus a literal, or a copy of another of its variables.
fn lower_self_variable_write(
    name: &str,
    op: byroredux_papyrus::ast::AssignOp,
    value: &Expr,
    scope: &Scope,
) -> Option<Effect> {
    if op != byroredux_papyrus::ast::AssignOp::Eq {
        return None;
    }
    let quest = QuestRef::SelfRef;
    if let Some(value) = as_num(value) {
        return Some(Effect::SetQuestVariable {
            quest,
            variable: name.to_owned(),
            value,
        });
    }
    fn strip(mut e: &Expr) -> &Expr {
        while let Expr::Cast { expr, .. } = e {
            e = &expr.node;
        }
        e
    }
    let own_variable = |e: &Expr| match strip(e) {
        Expr::Ident(other) if is_script_variable(&other.0, scope) => Some(other.0.clone()),
        _ => None,
    };
    if let Some(from) = own_variable(value) {
        return Some(Effect::CopyQuestVariable {
            quest,
            from,
            to: name.to_owned(),
        });
    }
    let Expr::BinaryOp { op, left, right } = strip(value) else {
        return None;
    };
    let same = |e: &Expr| own_variable(e).is_some_and(|other| other.eq_ignore_ascii_case(name));
    let delta = match op {
        BinaryOp::Add if same(&left.node) => as_num(&right.node)?,
        BinaryOp::Add if same(&right.node) => as_num(&left.node)?,
        BinaryOp::Sub if same(&left.node) => -as_num(&right.node)?,
        _ => return None,
    };
    Some(Effect::ModQuestVariable {
        quest,
        variable: name.to_owned(),
        delta,
    })
}

/// The quest whose script variable `<object>.<name>` names: the fragment's
/// own quest (`kmyQuest`, `(GetOwningQuest() as X)`) or a known quest
/// property; `Self` (a fragment-script variable) declines.
fn quest_variable_receiver(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    if matches!(object, Expr::Ident(name) if name.0.eq_ignore_ascii_case("self")) {
        return None;
    }
    match object {
        Expr::Ident(_) => explicit_quest_receiver(object, scope),
        Expr::Cast { .. } => match quest_expr_ref(object, scope)? {
            quest @ (QuestRef::OwningQuest | QuestRef::SelfRef) => Some(quest),
            // M47.2 — a property cast to a script that extends Quest, or a
            // typed quest property of the fragment's own quest.
            quest @ QuestRef::Property(_) => cast_to_quest_script(object, scope).then_some(quest),
        },
        Expr::MemberAccess { .. } => receiver_quest(object, scope),
        _ => None,
    }
}

/// M47.2 — whether `object`, a cast, names a script type that extends Quest
/// (resolved through the script library; `false` without one).
pub(super) fn cast_to_quest_script(object: &Expr, scope: &Scope) -> bool {
    let Expr::Cast { target_type, .. } = object else {
        return false;
    };
    let byroredux_papyrus::ast::Type::Object(ty) = &target_type.node else {
        return false;
    };
    scope
        .library
        .is_some_and(|library| crate::fragment::extends_quest(&ty.0, library))
}

/// Record a local's binding, or decline the whole fragment.
///
/// - A quest expression → `quest_locals`.
/// - A side-effect-free non-quest value (a literal, ident, member read,
///   arithmetic) → `decl_locals` (a plain local; a later misuse as a
///   quest receiver still declines via [`receiver_quest`]).
/// - A non-quest *side-effecting* initializer (e.g.
///   `k = akActor.PlaceAtMe(...)`) is an unmodeled statement whose effect
///   this table can't lower — decline rather than silently drop the
///   side-effect (#1907), matching the flat-sequence decline contract.
fn bind_local(scope: &mut Scope, name: String, init: &Expr) -> Option<Option<Effect>> {
    enum Binding {
        Quest(QuestRef),
        Player,
        Speaker,
        Object(ObjectRef),
        /// M47.2 — a number computed at run time ([`Effect::SetLocal`]).
        Value(ValueExpr),
        Plain,
    }
    let binding = if let Some(via) = quest_expr_ref(init, scope) {
        Binding::Quest(via)
    } else if player_expr_ref(init, scope) {
        Binding::Player
    } else if speaker_expr_ref(init, scope) {
        Binding::Speaker
    } else if let Some(via) = object_expr_ref(init, scope) {
        Binding::Object(via)
    } else if let Some(value) = classify_value(init, scope) {
        Binding::Value(value)
    } else if is_side_effect_free(init) {
        Binding::Plain
    } else {
        return None;
    };

    scope.quest_locals.remove(&name);
    scope.object_locals.remove(&name);
    scope.player_locals.remove(&name);
    scope.speaker_locals.remove(&name);
    scope.decl_locals.remove(&name);
    let mut set = None;
    match binding {
        Binding::Quest(via) => {
            scope.quest_locals.insert(name, via);
        }
        Binding::Player => {
            scope.player_locals.insert(name);
        }
        Binding::Speaker => {
            scope.speaker_locals.insert(name);
        }
        Binding::Object(via) => {
            scope.object_locals.insert(name, via);
        }
        Binding::Value(value) => {
            set = Some(Effect::SetLocal {
                local: local_slot(scope, &name),
                value,
            });
            scope.runtime_locals.insert(name.clone());
            scope.decl_locals.insert(name);
        }
        Binding::Plain => {
            scope.decl_locals.insert(name);
        }
    }
    Some(set)
}

/// M47.2 — the run-time slot of local `name`: qualified by the inlined
/// function it belongs to, so a callee's locals never meet its caller's.
pub(super) fn local_slot(scope: &Scope, name: &str) -> String {
    match scope.inline_stack.last() {
        Some((script, function)) => format!("{script}.{function}.{name}"),
        None => name.to_owned(),
    }
}

/// Whether evaluating `e` invokes no function/method call. Papyrus side
/// effects come from calls, so a non-quest initializer that contains a
/// `Call` can't be lowered to an effect and must decline (#1907).
fn is_side_effect_free(e: &Expr) -> bool {
    match e {
        Expr::Call { .. } => false,
        Expr::MemberAccess { object, .. } => is_side_effect_free(&object.node),
        Expr::Index { object, index } => {
            is_side_effect_free(&object.node) && is_side_effect_free(&index.node)
        }
        Expr::UnaryOp { operand, .. } => is_side_effect_free(&operand.node),
        Expr::BinaryOp { left, right, .. } => {
            is_side_effect_free(&left.node) && is_side_effect_free(&right.node)
        }
        Expr::Cast { expr, .. } => is_side_effect_free(&expr.node),
        Expr::Is { expr, .. } => is_side_effect_free(&expr.node),
        Expr::New { size, .. } => is_side_effect_free(&size.node),
        Expr::NewStruct { .. } => true,
        Expr::ArrayLit(items) => items.iter().all(|i| is_side_effect_free(&i.node)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => true,
    }
}

/// Recognize MQ101's actor-load gate without opening the general-purpose
/// control-flow surface. The condition must be an OR tree whose leaves are
/// exactly `!<actor>.Is3DLoaded()`, and the loop body must be one positive
/// `Utility.Wait` call.
/// M47.2 — `While <condition>` whose body is only `Utility.Wait(<literal>)`:
/// a poll on any condition the guards model (`While !Quest.IsRunning()`,
/// `While Alias.GetReference() == None`).
fn lower_wait_while(condition: &Expr, body: &[Spanned<Stmt>], scope: &Scope) -> Option<Effect> {
    let [statement] = body else {
        return None;
    };
    let Stmt::ExprStmt(wait) = &statement.node else {
        return None;
    };
    let Effect::Wait {
        seconds: poll_seconds,
    } = prim_wait(&wait.node, scope)?
    else {
        return None;
    };
    if poll_seconds <= 0.0 {
        return None;
    }
    let mut guards = Vec::new();
    classify_condition(condition, scope, false, &mut guards)?;
    Some(Effect::WaitWhile {
        guards,
        poll_seconds,
    })
}

fn lower_3d_loaded_wait(condition: &Expr, body: &[Spanned<Stmt>], scope: &Scope) -> Option<Effect> {
    let [statement] = body else {
        return None;
    };
    let Stmt::ExprStmt(wait) = &statement.node else {
        return None;
    };
    let Effect::Wait {
        seconds: poll_seconds,
    } = prim_wait(&wait.node, scope)?
    else {
        return None;
    };
    if poll_seconds <= 0.0 {
        return None;
    }

    let mut actors = Vec::new();
    collect_not_3d_loaded_actors(condition, scope, &mut actors)?;
    (!actors.is_empty()).then_some(Effect::WaitForActors3DLoaded {
        actors,
        poll_seconds,
    })
}

fn collect_not_3d_loaded_actors(
    condition: &Expr,
    scope: &Scope,
    actors: &mut Vec<ActorRef>,
) -> Option<()> {
    if let Expr::BinaryOp {
        left,
        op: BinaryOp::Or,
        right,
    } = condition
    {
        collect_not_3d_loaded_actors(&left.node, scope, actors)?;
        collect_not_3d_loaded_actors(&right.node, scope, actors)?;
        return Some(());
    }

    let Expr::UnaryOp {
        op: UnaryOp::Not,
        operand,
    } = condition
    else {
        return None;
    };
    let (receiver, args) = method_call(&operand.node, "Is3DLoaded")?;
    if !args.is_empty() {
        return None;
    }
    actors.push(receiver_actor(receiver, scope)?);
    Some(())
}
