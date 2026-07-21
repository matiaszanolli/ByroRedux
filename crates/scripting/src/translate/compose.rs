//! The compositional recognizer toolkit — the shared mechanism behind
//! both the event-handler recognizers (b1) and the fragment lowerer (b2).
//!
//! The corpus survey ([`docs/engine/m47-2-recognizer-scaling.md`]) showed
//! the behavioral corpus is a heavy-tailed composition of a small
//! vocabulary of recurring *primitives* — atomic guard predicates and
//! leaf effect statements — rather than a small set of whole-body shapes.
//! Hand-writing a recognizer per shape scales linearly with the corpus;
//! recognizing per *primitive* scales sub-linearly, because one primitive
//! lifts every body that uses it.
//!
//! This module is that primitive layer for **guards**. It holds:
//!
//! - the low-level AST matchers every recognizer needs (`method_call`,
//!   `int_arg`, `as_num`, `quest_via`), previously private to
//!   `quest_stage_gate`;
//! - [`split_and`], which decomposes an `If`/`While` condition into atomic
//!   predicates — exactly the conservative `&&`-only split the original
//!   `classify_condition` walked (a `||` is left as one atom that no
//!   primitive claims, so the engine declines);
//! - the **guard-primitive table** ([`GUARD_PRIMITIVES`]): a slice of
//!   free fns, each matching one atomic predicate shape and binding its
//!   holes, mirroring the established `RECOGNIZERS: &[Recognizer]` pattern.
//!   [`classify_guard_atom`] runs the table first-match-wins; an atom no
//!   primitive claims returns `None`, and the caller declines the whole
//!   handler. That is the **decline-on-any-unmodeled-term** invariant,
//!   enforced per atom.
//!
//! The effect-primitive table (for the fragment lowerer) is a sibling that
//! reuses this same toolkit; see [`super::effects`].

use byroredux_papyrus::ast::{BinaryOp, CallArg, Expr, UnaryOp};

/// How a quest-stage call names its quest receiver — the one hole a
/// stage primitive can't resolve from the AST alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestRef {
    /// `Self.GetOwningQuest()` — the quest owning this alias. Resolved
    /// from the attach context's `owning_quest`.
    OwningQuest,
    /// `Self` — the script's own object. For a quest **stage/scene
    /// fragment** the script *is* the quest, so `Self.SetStage(..)`
    /// targets the quest the fragment runs in (the dispatch-context
    /// quest). Distinct from [`OwningQuest`] only for alias scripts,
    /// where `Self` is the alias and `GetOwningQuest()` is its quest;
    /// the fragment dispatcher resolves both to its context quest.
    SelfRef,
    /// A `Quest Property NAME` — resolved from VMAD by property name.
    Property(String),
}

/// The object-targeting sibling of [`QuestRef`] — how an `AddItem`/
/// `MoveTo`-family effect names its `ObjectReference`/`Actor` receiver or
/// argument.
///
/// Unlike a quest reference, there is no unambiguous bare-receiver case
/// (no `Self`/`GetOwningQuest()` equivalent): the fragment script
/// (`QF_…`) always `extends Quest`, so `Self` is never itself the object
/// being acted on. Every object reference is therefore VMAD-or-decline —
/// see [`super::effects`]'s resolution helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectRef {
    /// A property (`ObjectReference`/`Actor`/`Form`-typed) bound via
    /// VMAD by name.
    Property(String),
}

impl ObjectRef {
    pub fn property_name(&self) -> &str {
        let ObjectRef::Property(name) = self;
        name
    }
}

/// If `e` is `<object>.<method>(args)`, return `(&object, args)`.
pub fn method_call<'a>(e: &'a Expr, method: &str) -> Option<(&'a Expr, &'a [CallArg])> {
    let Expr::Call { callee, args } = e else {
        return None;
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return None;
    };
    member
        .node
        .eq_ignore_case(method)
        .then_some((&object.node, args.as_slice()))
}

/// The integer value of positional argument `idx`.
pub fn int_arg(args: &[CallArg], idx: usize) -> Option<i64> {
    match &args.get(idx)?.value.node {
        Expr::IntLit(n) => Some(*n),
        _ => None,
    }
}

/// A numeric literal — `Int`/`Float`/`Bool`, unwrapping a `… as Bool`
/// cast (the `== 1 as Bool` idiom Champollion emits).
pub fn as_num(e: &Expr) -> Option<f32> {
    match e {
        Expr::IntLit(n) => Some(*n as f32),
        Expr::FloatLit(f) => Some(*f as f32),
        Expr::BoolLit(b) => Some(if *b { 1.0 } else { 0.0 }),
        Expr::Cast { expr, .. } => as_num(&expr.node),
        _ => None,
    }
}

/// Classify the receiver of a `GetStageDone`/`SetStage` call into a
/// [`QuestRef`]. `Self.GetOwningQuest()` → [`QuestRef::OwningQuest`]; a
/// bare identifier (a `Quest Property`) → [`QuestRef::Property`].
pub fn quest_via(object: &Expr) -> Option<QuestRef> {
    if method_call(object, "GetOwningQuest").is_some() {
        Some(QuestRef::OwningQuest)
    } else if let Expr::Ident(name) = object {
        if name.0.eq_ignore_ascii_case("self") {
            Some(QuestRef::SelfRef)
        } else {
            Some(QuestRef::Property(name.0.clone()))
        }
    } else {
        None
    }
}

/// Decompose an `&&` conjunction into its atomic predicates. Anything
/// that is not an `And` node is an atom (pushed as-is).
///
/// Deliberately does **not** split `||`: a disjunction is left as a
/// single atom that no guard primitive matches, so the caller declines —
/// preserving the original conservative "an `||` we don't model means
/// decline" semantics. Recognizing a disjunction as a flat AND of its
/// terms would change behavior.
pub fn split_and<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    if let Expr::BinaryOp {
        op: BinaryOp::And,
        left,
        right,
    } = e
    {
        split_and(&left.node, out);
        split_and(&right.node, out);
    } else {
        out.push(e);
    }
}

/// What an atomic guard predicate lowered to. Extend this enum (and a
/// matching [`GuardPrimitive`]) to teach the engine a new guard shape.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardMatch {
    /// `<quest>.GetStageDone(stage) == expected` (or a bare
    /// `GetStageDone(stage)` used as a bool → `expected = 1`).
    StageDone {
        via: QuestRef,
        stage: u16,
        expected: f32,
    },
    /// `<activator> == Game.GetPlayer()` — the player-activator gate.
    PlayerGate,
}

/// A guard primitive: matches one atomic predicate shape and binds its
/// holes, or declines. `player_param` is the handler's first parameter
/// name (the activator/triggerer), needed by the player-gate primitive.
pub type GuardPrimitive = fn(&Expr, player_param: Option<&str>) -> Option<GuardMatch>;

/// The guard-primitive table. First match wins; an atom no primitive
/// claims makes [`classify_guard_atom`] return `None`.
pub const GUARD_PRIMITIVES: &[GuardPrimitive] = &[prim_player_gate, prim_stage_done];

/// Run the guard-primitive table against one atomic predicate.
pub fn classify_guard_atom(atom: &Expr, player_param: Option<&str>) -> Option<GuardMatch> {
    GUARD_PRIMITIVES.iter().find_map(|p| p(atom, player_param))
}

// ── Guard primitives ─────────────────────────────────────────────────

/// `<activator> == Game.GetPlayer()` (either operand order, tolerating an
/// `as Actor` cast on the player call).
fn prim_player_gate(e: &Expr, player_param: Option<&str>) -> Option<GuardMatch> {
    is_player_gate(e, player_param).then_some(GuardMatch::PlayerGate)
}

/// `<quest>.GetStageDone(stage) == expected`, either operand order, or a
/// bare `<quest>.GetStageDone(stage)` used as a boolean (→ `== 1`).
fn prim_stage_done(e: &Expr, _player_param: Option<&str>) -> Option<GuardMatch> {
    if let Expr::BinaryOp {
        op: BinaryOp::Eq,
        left,
        right,
    } = e
    {
        let staged = as_get_stage_done(&left.node)
            .map(|sv| (sv, as_num(&right.node)))
            .or_else(|| as_get_stage_done(&right.node).map(|sv| (sv, as_num(&left.node))));
        if let Some(((stage, via), Some(expected))) = staged {
            return Some(GuardMatch::StageDone {
                via,
                stage,
                expected,
            });
        }
        return None;
    }
    // A bare `GetStageDone(stage)` used as a boolean → `== 1`.
    let (stage, via) = as_get_stage_done(e)?;
    Some(GuardMatch::StageDone {
        via,
        stage,
        expected: 1.0,
    })
}

/// If `e` is a `…GetStageDone(stage)` call, return `(stage, quest_via)`.
fn as_get_stage_done(e: &Expr) -> Option<(u16, QuestRef)> {
    let (object, args) = method_call(e, "GetStageDone")?;
    let stage = int_arg(args, 0)?;
    Some((u16::try_from(stage).ok()?, quest_via(object)?))
}

/// True when `e` is `<player_param> == Game.GetPlayer()` (either operand
/// order, tolerating an `as Actor` cast on the player call).
pub fn is_player_gate(e: &Expr, player_param: Option<&str>) -> bool {
    let Expr::BinaryOp {
        op: BinaryOp::Eq,
        left,
        right,
    } = e
    else {
        return false;
    };
    let pair = |a: &Expr, b: &Expr| is_param_ref(a, player_param) && is_game_get_player(b);
    pair(&left.node, &right.node) || pair(&right.node, &left.node)
}

/// `e` is a reference to the handler's activator parameter.
fn is_param_ref(e: &Expr, player_param: Option<&str>) -> bool {
    matches!((e, player_param), (Expr::Ident(id), Some(p)) if id.0.eq_ignore_ascii_case(p))
}

/// `e` is a `Game.GetPlayer()` call (unwrapping an optional cast).
fn is_game_get_player(e: &Expr) -> bool {
    if let Expr::Cast { expr, .. } = e {
        return is_game_get_player(&expr.node);
    }
    let Some((object, _)) = method_call(e, "GetPlayer") else {
        return false;
    };
    matches!(object, Expr::Ident(id) if id.0.eq_ignore_ascii_case("Game"))
}

/// `!<inner>` unwrap helper — some guards arrive negated. Exposed for the
/// effect/fragment side, which classifies the same atom shapes.
pub fn strip_not(e: &Expr) -> &Expr {
    if let Expr::UnaryOp {
        op: UnaryOp::Not,
        operand,
    } = e
    {
        strip_not(&operand.node)
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::parse_script;
    use byroredux_papyrus::ast::{ScriptItem, Stmt};

    /// Parse a one-event script and hand back the `If` condition of its
    /// single statement, for primitive-level unit tests.
    fn first_if_condition(src: &str) -> Expr {
        let (script, errs) = parse_script(src).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        for item in &script.body {
            if let ScriptItem::Event(e) = &item.node {
                if let Some(stmt) = e.body.first() {
                    if let Stmt::If { condition, .. } = &stmt.node {
                        return condition.node.clone();
                    }
                }
            }
        }
        panic!("no If condition found");
    }

    #[test]
    fn split_and_flattens_conjunction_keeps_disjunction_whole() {
        let cond = first_if_condition(
            "ScriptName T extends ObjectReference\n\
             Event OnActivate(ObjectReference akActionRef)\n\
             If A() && B() && C()\n Debug.Notification(\"x\")\n EndIf\n EndEvent\n",
        );
        let mut atoms = Vec::new();
        split_and(&cond, &mut atoms);
        assert_eq!(atoms.len(), 3, "three &&-joined atoms");

        let or = first_if_condition(
            "ScriptName T extends ObjectReference\n\
             Event OnActivate(ObjectReference akActionRef)\n\
             If A() || B()\n Debug.Notification(\"x\")\n EndIf\n EndEvent\n",
        );
        let mut atoms = Vec::new();
        split_and(&or, &mut atoms);
        assert_eq!(atoms.len(), 1, "a disjunction is one un-split atom");
    }

    #[test]
    fn stage_done_primitive_binds_holes() {
        let cond = first_if_condition(
            "ScriptName T extends ObjectReference\n\
             Quest Property MyQuest Auto\n\
             Event OnActivate(ObjectReference akActionRef)\n\
             If MyQuest.GetStageDone(37) == 1\n MyQuest.SetStage(40)\n EndIf\n EndEvent\n",
        );
        assert_eq!(
            classify_guard_atom(&cond, Some("akActionRef")),
            Some(GuardMatch::StageDone {
                via: QuestRef::Property("MyQuest".into()),
                stage: 37,
                expected: 1.0,
            })
        );
    }

    #[test]
    fn player_gate_primitive_matches_both_orders() {
        for src in [
            "If akActionRef == Game.GetPlayer()\n",
            "If (Game.GetPlayer() as Actor) == akActionRef\n",
        ] {
            let full = format!(
                "ScriptName T extends ObjectReference\n\
                 Event OnActivate(ObjectReference akActionRef)\n\
                 {src} Debug.Notification(\"x\")\n EndIf\n EndEvent\n"
            );
            let cond = first_if_condition(&full);
            assert_eq!(
                classify_guard_atom(&cond, Some("akActionRef")),
                Some(GuardMatch::PlayerGate),
                "src: {src}"
            );
        }
    }

    #[test]
    fn unmodeled_atom_declines() {
        let cond = first_if_condition(
            "ScriptName T extends ObjectReference\n\
             Quest Property MyQuest Auto\n\
             Event OnActivate(ObjectReference akActionRef)\n\
             If MyQuest.GetStage() >= 10\n MyQuest.SetStage(20)\n EndIf\n EndEvent\n",
        );
        assert_eq!(classify_guard_atom(&cond, Some("akActionRef")), None);
    }
}
