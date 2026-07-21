//! The effect-primitive table + fragment lowerer (b2) — the scaling
//! lever from [`docs/engine/m47-2-recognizer-scaling.md`].
//!
//! The corpus survey found 43,818 behavioral quest/scene/dialogue
//! `Fragment_*` functions (69.5% of the corpus), and that they are far
//! more compressible than event handlers: a ~500-primitive effect
//! vocabulary fully covers ~78% of them, because a fragment is almost
//! always a flat sequence of canonical effects (it is pre-gated by the
//! quest-stage contract, so it carries little control flow).
//!
//! This module lowers a fragment body to a `Vec<Effect>` through an
//! [`EffectPrimitive`] table — the effect sibling of
//! [`compose::GUARD_PRIMITIVES`](crate::translate::compose). It reuses
//! the same AST toolkit and the same **decline-on-any-unmodeled-term**
//! invariant: [`lower_fragment`] returns `None` the instant it meets a
//! statement no primitive claims, so a partially-understood fragment is
//! never partially applied.
//!
//! ## Scope (this increment)
//!
//! Only **quest-scoped** effects — those resolvable without runtime
//! FormID→entity binding — are modelled: `SetStage` and the objective
//! ops. Object-targeting effects (`Enable`/`Disable`/`MoveTo`/`AddItem`,
//! …) need a FormID→entity resolver that does not exist yet, so a
//! fragment that uses them declines (safe — no behavior attached) until
//! that resolver lands. The dominant fragment templates
//! (`{$=$;$.setstage(#)}`, `{self.setobjectivecompleted(#,#);…}`) are
//! covered.
//!
//! ## Local binding
//!
//! Champollion emits `Quest k = GetOwningQuest()` / `Quest k = MyQuestProp`
//! then `k.SetStage(..)`. [`lower_fragment`] tracks those binding
//! assignments in a small local environment so a later effect on `k`
//! resolves to the right [`QuestRef`]. A binding to anything it can't
//! classify (a non-quest expression) is itself an unmodeled statement →
//! decline.

use std::collections::{HashMap, HashSet};

use byroredux_papyrus::ast::{Expr, Stmt};
use byroredux_papyrus::span::Spanned;

use crate::translate::compose::{as_num, int_arg, method_call, quest_via, ObjectRef, QuestRef};

/// A canonical, quest-scoped effect a fragment statement lowers to. The
/// runtime applies these against [`QuestStageState`] /
/// [`QuestObjectiveState`]; see [`crate::fragment`].
///
/// [`QuestStageState`]: crate::quest_stages::QuestStageState
/// [`QuestObjectiveState`]: crate::quest_stages::QuestObjectiveState
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `<quest>.SetStage(stage)`.
    SetStage { quest: QuestRef, stage: u16 },
    /// `<quest>.SetObjectiveDisplayed(objective, displayed)`. Papyrus's
    /// optional `abForce` 3rd arg doesn't affect the stored state.
    SetObjectiveDisplayed {
        quest: QuestRef,
        objective: u16,
        displayed: bool,
    },
    /// `<quest>.SetObjectiveCompleted(objective, completed)`.
    SetObjectiveCompleted {
        quest: QuestRef,
        objective: u16,
        completed: bool,
    },
    /// `<quest>.SetObjectiveFailed(objective, failed)`.
    SetObjectiveFailed {
        quest: QuestRef,
        objective: u16,
        failed: bool,
    },
    /// `<quest>.CompleteAllObjectives()`.
    CompleteAllObjectives { quest: QuestRef },
    /// `<container>.AddItem(<item>, <count>)`. The optional 3rd
    /// (`abSilent`) argument is accepted (parsed, not applied — no pickup
    /// notification UI exists to suppress) but only as a literal; a
    /// non-literal 3rd arg, or a 4th, declines the whole fragment (the
    /// `bool_arg` discipline `SetObjectiveDisplayed` already uses).
    /// `item` resolves only to a FormID at dispatch, never to an entity —
    /// it names a *base record* (the item type), not a placed reference.
    AddItem {
        container: ObjectRef,
        item: ObjectRef,
        count: u32,
    },
    /// `<moved>.MoveTo(<destination>)` — the conservative 2-arg shape
    /// only. A 3rd+ argument (offsets / match-rotation) declines the
    /// whole fragment rather than silently dropping the offset and
    /// misplacing the object.
    MoveTo {
        moved: ObjectRef,
        destination: ObjectRef,
    },
}

/// The local-variable scope built while lowering a fragment body.
///
/// Distinguishes three name kinds so an effect receiver resolves
/// correctly: a local bound to a quest (`quest_locals`), a local of some
/// other type (`decl_locals` — `ObjectReference k = …`), and a bare
/// identifier that is neither (a script *property*, classified directly
/// by [`quest_via`]). The distinction matters: a declared local used as
/// an effect receiver but not quest-bound must **decline**, never be
/// misread as a same-named `Quest Property`.
#[derive(Default)]
struct Scope {
    quest_locals: HashMap<String, QuestRef>,
    decl_locals: HashSet<String>,
}

/// Lower a **flat** fragment body to its canonical effects, or decline.
///
/// Returns `None` (decline, the whole fragment) if the body contains any
/// control flow (`If`/`While`) or any statement no effect primitive
/// claims — never a partial lowering. An empty body lowers to an empty
/// effect list (a no-op fragment is trivially understood).
pub fn lower_fragment(body: &[Spanned<Stmt>]) -> Option<Vec<Effect>> {
    let mut scope = Scope::default();
    let mut effects = Vec::new();
    for stmt in body {
        match &stmt.node {
            // `Quest k = <quest-expr>` — a local quest binding. Other-typed
            // local decls are recorded (so a later misuse declines) but
            // contribute no effect. A bare decl (no initializer) is a
            // plain local.
            Stmt::VarDecl(var) => {
                let name = var.name.node.0.to_ascii_lowercase();
                match &var.initial_value {
                    Some(init) => bind_local(&mut scope, name, &init.node)?,
                    None => {
                        scope.decl_locals.insert(name);
                    }
                }
            }
            // Re-assignment to an existing local: same binding rule.
            Stmt::Assign { target, value, .. } => {
                let Expr::Ident(name) = &target.node else {
                    return None; // assignment to a field/index — unmodeled
                };
                bind_local(&mut scope, name.0.to_ascii_lowercase(), &value.node)?;
            }
            // `Return` with no value is Champollion's fragment terminator.
            Stmt::Return(None) => {}
            Stmt::ExprStmt(e) => effects.push(classify_effect(&e.node, &scope)?),
            // Control flow / valued return in a fragment are outside this
            // increment's flat-sequence model — decline.
            _ => return None,
        }
    }
    Some(effects)
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
fn bind_local(scope: &mut Scope, name: String, init: &Expr) -> Option<()> {
    if let Some(via) = quest_expr_ref(init, scope) {
        scope.quest_locals.insert(name, via);
    } else if is_side_effect_free(init) {
        scope.decl_locals.insert(name);
    } else {
        return None;
    }
    Some(())
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
        Expr::New { size, .. } => is_side_effect_free(&size.node),
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

/// Classify a single effect statement against the primitive table.
fn classify_effect(e: &Expr, scope: &Scope) -> Option<Effect> {
    EFFECT_PRIMITIVES.iter().find_map(|p| p(e, scope))
}

/// An effect primitive: matches one effect-call shape and binds its
/// holes (resolving the receiver to a [`QuestRef`] via `scope`), or
/// declines. Internal — the public surface is [`lower_fragment`].
type EffectPrimitive = fn(&Expr, &Scope) -> Option<Effect>;

/// The effect-primitive table. First match wins.
const EFFECT_PRIMITIVES: &[EffectPrimitive] = &[
    prim_set_stage,
    prim_set_objective_displayed,
    prim_set_objective_completed,
    prim_set_objective_failed,
    prim_complete_all_objectives,
    prim_add_item,
    prim_move_to,
];

// ── Effect primitives ────────────────────────────────────────────────

fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetStage")?;
    let stage = u16::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetStage {
        quest: receiver_quest(object, scope)?,
        stage,
    })
}

fn prim_set_objective_displayed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveDisplayed")?;
    let objective = u16::try_from(int_arg(args, 0)?).ok()?;
    // Optional 2nd arg `abDisplayed` defaults to true in Papyrus.
    let displayed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveDisplayed {
        quest: receiver_quest(object, scope)?,
        objective,
        displayed,
    })
}

fn prim_set_objective_completed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveCompleted")?;
    let objective = u16::try_from(int_arg(args, 0)?).ok()?;
    let completed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveCompleted {
        quest: receiver_quest(object, scope)?,
        objective,
        completed,
    })
}

fn prim_set_objective_failed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveFailed")?;
    let objective = u16::try_from(int_arg(args, 0)?).ok()?;
    let failed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveFailed {
        quest: receiver_quest(object, scope)?,
        objective,
        failed,
    })
}

fn prim_complete_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, _args) = method_call(e, "CompleteAllObjectives")?;
    Some(Effect::CompleteAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

fn prim_add_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddItem")?;
    let container = receiver_object(object, scope)?;
    let item = receiver_object(&args.first()?.value.node, scope)?;
    let count = u32::try_from(int_arg(args, 1)?).ok()?;
    // Optional 3rd arg (`abSilent`) — accepted only as a literal (parsed,
    // not applied; see the `Effect::AddItem` doc). A present-but-
    // non-literal 3rd arg declines via `bool_arg`'s `None`; a 4th arg
    // entirely is outside this primitive's understood shape.
    bool_arg(args, 2)?;
    if args.len() > 3 {
        return None;
    }
    Some(Effect::AddItem {
        container,
        item,
        count,
    })
}

fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveTo")?;
    // The conservative 2-arg shape only (receiver + destination) — a 3rd+
    // argument (offsets / match-rotation) declines rather than silently
    // dropping it and misplacing the object.
    if args.len() != 1 {
        return None;
    }
    let moved = receiver_object(object, scope)?;
    let destination = receiver_object(&args[0].value.node, scope)?;
    Some(Effect::MoveTo { moved, destination })
}

// ── Receiver / quest-expr resolution ─────────────────────────────────

/// Resolve a call receiver to a [`QuestRef`]:
/// - a local bound to a quest → its bound `QuestRef`;
/// - a declared-but-not-quest-bound local used as a receiver → `None`
///   (decline — it is *not* a same-named property);
/// - otherwise classify directly (`Self` / `Self.GetOwningQuest()` / a
///   `Quest Property`).
fn receiver_quest(object: &Expr, scope: &Scope) -> Option<QuestRef> {
    if let Expr::Ident(name) = object {
        let key = name.0.to_ascii_lowercase();
        if let Some(via) = scope.quest_locals.get(&key) {
            return Some(via.clone());
        }
        if scope.decl_locals.contains(&key) {
            return None;
        }
    }
    quest_via(object)
}

/// Classify the RHS of a `local = <expr>` binding as a quest reference,
/// resolving a local-to-local copy through `scope`.
fn quest_expr_ref(value: &Expr, scope: &Scope) -> Option<QuestRef> {
    receiver_quest(value, scope)
}

/// Resolve an object-targeting effect's receiver or argument to an
/// [`ObjectRef`]. Unlike [`receiver_quest`], there is no unambiguous
/// bare-identifier case (no `Self`/`GetOwningQuest()` equivalent — see
/// [`ObjectRef`]'s doc) — a bare property name is the *only* shape
/// accepted; a local variable (quest-bound or otherwise) declines,
/// because this increment doesn't track which property a local was
/// copied from.
fn receiver_object(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let Expr::Ident(name) = object else {
        return None;
    };
    let key = name.0.to_ascii_lowercase();
    // `Self` is never the object here (see `ObjectRef`'s doc) — decline
    // explicitly rather than relying on no VMAD ever naming a property
    // "self".
    if key == "self" || scope.quest_locals.contains_key(&key) || scope.decl_locals.contains(&key) {
        return None;
    }
    Some(ObjectRef::Property(name.0.clone()))
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
fn bool_arg(args: &[byroredux_papyrus::ast::CallArg], idx: usize) -> Option<Option<bool>> {
    match args.get(idx) {
        None => Some(None),
        Some(arg) => as_num(&arg.value.node).map(|n| Some(n != 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::ast::{ScriptItem, StateItem};
    use byroredux_papyrus::parse_script;

    /// Parse a script and return the body of its first function/event
    /// named like a fragment (or just the first function), to drive
    /// lowering on realistic shapes.
    fn first_fn_body(src: &str) -> Vec<Spanned<Stmt>> {
        let (script, errs) = parse_script(src).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        for item in &script.body {
            match &item.node {
                ScriptItem::Function(f) => return f.body.clone(),
                ScriptItem::Event(e) => return e.body.clone(),
                ScriptItem::State(st) => {
                    for si in &st.body {
                        match &si.node {
                            StateItem::Function(f) => return f.body.clone(),
                            StateItem::Event(e) => return e.body.clone(),
                        }
                    }
                }
                _ => {}
            }
        }
        panic!("no function/event body found");
    }

    #[test]
    fn lowers_self_set_stage() {
        // The `{self.setstage(#)}` family — Self is the quest.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20
            }])
        );
    }

    #[test]
    fn lowers_bound_local_then_set_stage() {
        // The top fragment template `{$=$;$.setstage(#)}`: a quest local
        // bound from GetOwningQuest then used.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Quest kmyQuest = Self.GetOwningQuest()\n\
             kmyQuest.SetStage(30)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::OwningQuest,
                stage: 30
            }])
        );
    }

    #[test]
    fn lowers_objective_pair() {
        // `{self.setobjectivecompleted(#,#);self.setobjectivedisplayed(#,#,#)}`.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_2()\n\
             Self.SetObjectiveCompleted(10)\n\
             Self.SetObjectiveDisplayed(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: 10,
                    completed: true,
                },
                Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: 20,
                    displayed: true,
                },
            ])
        );
    }

    #[test]
    fn objective_explicit_false_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_3()\n Self.SetObjectiveDisplayed(5, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 5,
                displayed: false,
            }])
        );
    }

    #[test]
    fn objective_non_literal_arg_declines_whole_fragment() {
        // #2023 / SCR-D5-NEW2-01 — a present-but-non-literal 2nd argument
        // (an ordinary local bool, unconstrained Papyrus unlike the
        // auto-property-initializer case `bool_prop` guards) must NOT
        // silently collapse into the Papyrus-side default `true`. Pre-fix
        // `bool_arg` returned a single `None` for both "absent" and
        // "present but unevaluable," so `.unwrap_or(true)` masked this
        // exact case — asserting `completed: true` here would have
        // passed both before and after the fix, so the meaningful
        // assertion is that the whole fragment declines (`None`), not a
        // specific wrong value.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             Bool bWasSuccessful = true\n\
             Self.SetObjectiveCompleted(20, bWasSuccessful)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn declines_on_unmodeled_effect() {
        // An object-targeting effect (Enable) isn't in this increment's
        // table — the whole fragment declines, never partially applies.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_4()\n\
             Self.SetStage(10)\n\
             akTarget.Enable()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn declines_on_side_effecting_binding() {
        // A non-quest binding whose initializer is a side-effecting call
        // (the spawn) must decline the whole fragment — otherwise the
        // spawn is silently dropped while the SetStage still applies (#1907).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             ObjectReference k = akActor.PlaceAtMe(SomeForm)\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn side_effect_free_binding_is_recorded_not_declined() {
        // A pure-value non-quest local (an ident copy) has no side-effect
        // to drop, so it is recorded and lowering continues (#1907).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_6()\n\
             ObjectReference k = akActor\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20
            }])
        );
    }

    #[test]
    fn lowers_add_item_on_bare_properties() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             SomeContainer.AddItem(SomeItem, 5)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::AddItem {
                container: ObjectRef::Property("SomeContainer".into()),
                item: ObjectRef::Property("SomeItem".into()),
                count: 5,
            }])
        );
    }

    #[test]
    fn lowers_add_item_with_literal_silent_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             SomeContainer.AddItem(SomeItem, 5, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::AddItem {
                container: ObjectRef::Property("SomeContainer".into()),
                item: ObjectRef::Property("SomeItem".into()),
                count: 5,
            }])
        );
    }

    #[test]
    fn add_item_declines_with_non_literal_silent_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_9()\n\
             Bool bQuiet = true\n\
             SomeContainer.AddItem(SomeItem, 5, bQuiet)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn add_item_declines_on_local_receiver() {
        // A local copy of a property is not tracked back to its name in
        // this increment — the receiver must be a bare property.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_10()\n\
             ObjectReference k = SomeContainer\n\
             k.AddItem(SomeItem, 5)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn lowers_move_to_two_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             SomeRef.MoveTo(SomeMarker)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::MoveTo {
                moved: ObjectRef::Property("SomeRef".into()),
                destination: ObjectRef::Property("SomeMarker".into()),
            }])
        );
    }

    #[test]
    fn move_to_declines_with_offset_args() {
        // The conservative 2-arg shape only — offsets/match-rotation
        // decline rather than silently misplacing the object.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 10.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn declines_on_control_flow() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Self.GetStageDone(5)\n Self.SetStage(10)\n EndIf\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn empty_fragment_is_understood_as_noop() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n Function Fragment_6()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), Some(vec![]));
    }
}
