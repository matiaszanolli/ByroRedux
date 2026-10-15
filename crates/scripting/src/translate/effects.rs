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
//! Quest-scoped stage/objective operations and a conservative set of
//! VMAD-resolved object effects are modelled. Object receivers can be direct
//! FormID properties, quest-alias `GetRef()`/`GetActorRef()` results, or locals
//! proven to hold those results; every other shape still declines as a whole.
//! The table covers quest/object state, scene control, player controls, and the
//! MQ101 cart cinematic boundary (idle, vehicle, motion, sitting, latent wait).
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

use byroredux_core::ecs::components::MotionType;
use byroredux_papyrus::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use byroredux_papyrus::span::Spanned;

use crate::cinematic::CinematicAnimationEvent;
use crate::papyrus_provider::{lower_provider_call, MAX_PROVIDER_WAIT_SECONDS, PapyrusProviderCatalog};
use crate::player_control::PlayerControlSelection;
use crate::translate::compose::{
    as_num, int_arg, is_game_get_player, method_call, method_call_any, quest_via, ObjectRef,
    QuestRef,
};

/// A comparison operator of a fragment guard, applied as
/// `reading <op> value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum GuardComparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl GuardComparison {
    fn from_op(op: byroredux_papyrus::ast::BinaryOp) -> Option<Self> {
        use byroredux_papyrus::ast::BinaryOp as B;
        Some(match op {
            B::Eq => Self::Eq,
            B::Ne => Self::Ne,
            B::Lt => Self::Lt,
            B::Le => Self::Le,
            B::Gt => Self::Gt,
            B::Ge => Self::Ge,
            _ => return None,
        })
    }

    /// The operator with its operands swapped (`5 < x` is `x > 5`).
    fn swapped(self) -> Self {
        match self {
            Self::Eq | Self::Ne => self,
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
        }
    }

    /// The logical negation (`!(x < 5)` is `x >= 5`).
    fn negated(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::Lt => Self::Ge,
            Self::Le => Self::Gt,
            Self::Gt => Self::Le,
            Self::Ge => Self::Lt,
        }
    }

    pub fn apply(self, reading: f32, value: f32) -> bool {
        match self {
            Self::Eq => reading == value,
            Self::Ne => reading != value,
            Self::Lt => reading < value,
            Self::Le => reading <= value,
            Self::Gt => reading > value,
            Self::Ge => reading >= value,
        }
    }
}

/// One conservatively lowered predicate guarding a fragment branch (M47.2
/// widened it from `GetStageDone` alone). Every guard reads quest, objective,
/// global or quest-variable state the runtime holds; anything else declines.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum FragmentGuard {
    /// `<quest>.GetStageDone(stage)` (FO4: `IsStageDone`) is `done`.
    StageDone {
        quest: QuestRef,
        stage: u16,
        done: bool,
    },
    /// `<quest>.IsRunning()` is `running`.
    QuestRunning { quest: QuestRef, running: bool },
    /// `<quest>.GetStage() <op> value`.
    Stage {
        quest: QuestRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.IsObjectiveDisplayed(objective)` is `expected`.
    ObjectiveDisplayed {
        quest: QuestRef,
        objective: i32,
        expected: bool,
    },
    /// `<quest>.IsObjectiveCompleted(objective)` is `expected`.
    ObjectiveCompleted {
        quest: QuestRef,
        objective: i32,
        expected: bool,
    },
    /// `<global>.GetValue() / GetValueInt() / .Value <op> value`.
    Global {
        global: ObjectRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.<Property> <op> value` — a quest script variable
    /// ([`Effect::SetQuestVariable`]'s read side).
    QuestVariable {
        quest: QuestRef,
        variable: String,
        comparison: GuardComparison,
        value: f32,
    },
    /// M47.2 — a disjunction: holds when every guard of at least one
    /// alternative holds (`A || (B && C)`).
    Any(Vec<Vec<FragmentGuard>>),
    /// M47.2 — two run-time numbers compared (`GetLevel() < Global.GetValue()`,
    /// `Utility.RandomInt(1, 3) == 1`) when no specific guard fits.
    Compare {
        left: ValueExpr,
        comparison: GuardComparison,
        right: ValueExpr,
    },
    /// `<actor>.IsDead()` is `dead`.
    ActorDead { actor: ActorRef, dead: bool },
    /// `<actor>.HasPerk(perk)` is `expected`.
    HasPerk {
        actor: ActorRef,
        perk: ObjectRef,
        expected: bool,
    },
    /// `<container>.GetItemCount(item) <op> value`.
    ItemCount {
        container: ActorRef,
        item: ObjectRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<quest>.IsCompleted()` is `completed`.
    QuestCompleted { quest: QuestRef, completed: bool },
    /// `<scene>.IsPlaying()` is `playing`.
    ScenePlaying { scene: ObjectRef, playing: bool },
    /// `<marker>.IsMapMarkerVisible()` is `expected`.
    MapMarkerVisible { marker: ObjectRef, expected: bool },
    /// `<marker>.CanFastTravelToMarker()` is `expected`.
    MapMarkerFastTravel { marker: ObjectRef, expected: bool },
    /// `<location>.IsCleared()` is `expected`.
    LocationCleared { location: ObjectRef, expected: bool },
    /// `<actor>.IsBleedingOut()` is `expected`.
    ActorBleedingOut { actor: ActorRef, expected: bool },
    /// `<actor>.IsInCombat()` is `expected` ([`crate::is_in_combat`]).
    ActorInCombat { actor: ActorRef, expected: bool },
    /// `<actor>.IsPlayerTeammate()` / `IsDoingFavor()` is `expected`
    /// ([`crate::FollowerState`]).
    ActorFollower {
        actor: ActorRef,
        flag: crate::FollowerFlag,
        expected: bool,
    },
    /// `<reference>.IsEnabled()` is `expected` (`IsDisabled()` inverted).
    ReferenceEnabled { object: ObjectRef, expected: bool },
    /// `<actor>.IsInFaction(faction)` is `expected`.
    InFaction {
        actor: ActorRef,
        faction: ObjectRef,
        expected: bool,
    },
    /// `<reference>.Is3DLoaded()` is `expected`.
    Loaded3D { object: ObjectRef, expected: bool },
    /// `<reference>.HasLocRefType(refType)` is `expected`: the placement's
    /// XLRT tags ([`crate::scene::SceneAliasCandidate`]).
    HasLocRefType {
        actor: ActorRef,
        ref_type: ObjectRef,
        expected: bool,
    },
    /// `<actor>.GetRelationshipRank(<other>)` compared with `value`.
    RelationshipRank {
        actor: ActorRef,
        other: ActorRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<from>.GetDistance(<to>)` compared with `value`.
    Distance {
        from: ActorRef,
        to: ActorRef,
        comparison: GuardComparison,
        value: f32,
    },
    /// `<actor>.IsEssential()` is `expected`.
    ActorEssential { actor: ActorRef, expected: bool },
    /// `<actor>.GetCurrentLocation().HasKeyword(keyword)` is `expected` — the
    /// actor's current location itself (the M47.2 location runtime).
    LocationKeyword {
        actor: ActorRef,
        keyword: ObjectRef,
        expected: bool,
    },
    /// M47.2 — `<reference>.HasKeyword(keyword)` is `expected`
    /// ([`crate::keywords`]).
    HasKeyword {
        object: ActorRef,
        keyword: ObjectRef,
        expected: bool,
    },
    /// M47.2 — `<reference> == <reference>` (or `== None`, `right: None`) is
    /// `expected`: the two name the same reference, by FormID, so a direct
    /// reference compares whether or not it is loaded and an unfilled alias
    /// is `None`.
    SameReference {
        left: ActorRef,
        right: Option<ActorRef>,
        expected: bool,
    },
}

/// M47.2 — a number a fragment reads at run time, for
/// [`FragmentGuard::Compare`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ValueExpr {
    Literal(f32),
    /// `<quest>.GetStage()`.
    Stage(QuestRef),
    /// `<global>.GetValue()` / `.Value`.
    Global(ObjectRef),
    /// `<quest>.<Variable>`, a quest script variable.
    QuestVariable(QuestRef, String),
    /// `<container>.GetItemCount(item)`.
    ItemCount(ActorRef, ObjectRef),
    /// `<from>.GetDistance(<to>)`.
    Distance(ActorRef, ActorRef),
    /// `<actor>.GetRelationshipRank(<other>)`.
    RelationshipRank(ActorRef, ActorRef),
    /// `<actor>.GetLevel()`.
    Level(ActorRef),
    /// `Utility.RandomInt(min, max)`, both inclusive.
    RandomInt(i32, i32),
    /// A fragment local assigned a number ([`Effect::SetLocal`]); 0 until
    /// assigned, Papyrus's default.
    Local(String),
    /// Arithmetic over two values.
    Arith(Box<ValueExpr>, ArithOp, Box<ValueExpr>),
    /// M47.2 — `<actor>.GetValue(<actor value>)`, the current value.
    ActorValue(ActorRef, ObjectRef),
    /// M47.2 — `<actor>.GetAV("Name")` / `GetValue(Game.GetHealthAV())`, an
    /// actor value by name ([`crate::ActorValueNames`]).
    ActorValueNamed(ActorRef, String),
    /// M47.2 — `<actor>.GetBaseValue(av)` / `GetValuePercentage(av)` and
    /// Skyrim's `GetBaseActorValue("Name")` / `GetActorValuePercentage("Name")`.
    ActorValueReading(ActorValueReading, ActorRef, ActorValueArg),
    /// M47.2 — `Game.QueryStat("Houses Owned")`, a script-kept misc stat
    /// ([`crate::misc_stats::SCRIPT_STATS`]).
    MiscStat(String),
    /// M47.2 — `<location>.GetKeywordData(keyword)`
    /// ([`crate::LocationKeywordData`]).
    LocationKeywordData(ObjectRef, ObjectRef),
}

/// The arithmetic a [`ValueExpr::Arith`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl ArithOp {
    pub fn apply(self, left: f32, right: f32) -> Option<f32> {
        Some(match self {
            Self::Add => left + right,
            Self::Sub => left - right,
            Self::Mul => left * right,
            Self::Div if right == 0.0 => return None,
            Self::Div => left / right,
        })
    }
}

/// M47.2 — a numeric expression as a [`ValueExpr`]: a literal, one of the
/// numeric guard readings, `GetLevel()`, `Utility.RandomInt` with literal
/// bounds, or arithmetic over those.
fn classify_value(e: &Expr, scope: &Scope) -> Option<ValueExpr> {
    let mut e = e;
    while let Expr::Cast { expr, .. } = e {
        e = &expr.node;
    }
    if let Some(value) = as_num(e) {
        return Some(ValueExpr::Literal(value));
    }
    if let Expr::Ident(name) = e {
        let key = name.0.to_ascii_lowercase();
        if scope.runtime_locals.contains(&key) {
            return Some(ValueExpr::Local(local_slot(scope, &key)));
        }
        // M47.2 — a number held by the quest's own script: a property's
        // `::X_var` backing variable, or an inlined quest script's variable.
        // Against the quest's VMAD that is the quest script variable
        // `GetVMQuestVariable` reads (its authored value until written).
        if scope.vmad_is_quest
            && !scope.fragment_properties_off
            && (key.starts_with("::") || scope.script_variables.contains(&key))
        {
            return Some(ValueExpr::QuestVariable(QuestRef::SelfRef, name.0.clone()));
        }
    }
    if let Expr::BinaryOp { op, left, right } = e {
        let op = match op {
            BinaryOp::Add => ArithOp::Add,
            BinaryOp::Sub => ArithOp::Sub,
            BinaryOp::Mul => ArithOp::Mul,
            BinaryOp::Div => ArithOp::Div,
            _ => return None,
        };
        return Some(ValueExpr::Arith(
            Box::new(classify_value(&left.node, scope)?),
            op,
            Box::new(classify_value(&right.node, scope)?),
        ));
    }
    if let Some(args) = static_call(e, "Utility", "RandomInt") {
        if args.len() > 2 {
            return None;
        }
        // `RandomInt(int aiMin = 0, int aiMax = 100)`.
        let bound = |index: usize, default: i32| -> Option<i32> {
            match args.get(index) {
                None => Some(default),
                Some(_) => i32::try_from(int_arg(args, index)?).ok(),
            }
        };
        return Some(ValueExpr::RandomInt(bound(0, 0)?, bound(1, 100)?));
    }
    if let Some((object, args)) = method_call(e, "GetLevel") {
        return args
            .is_empty()
            .then(|| receiver_actor(object, scope).map(ValueExpr::Level))
            .flatten();
    }
    if let Some(args) = static_call(e, "Game", "QueryStat") {
        let [stat] = args else {
            return None;
        };
        let Expr::StringLit(stat) = &stat.value.node else {
            return None;
        };
        return crate::misc_stats::script_stat(stat).map(|stat| ValueExpr::MiscStat(stat.to_owned()));
    }
    // M47.2 — CK *GetKeywordData - Location*.
    if let Some((object, [keyword])) = method_call(e, "GetKeywordData") {
        return Some(ValueExpr::LocationKeywordData(
            receiver_object(object, scope)?,
            receiver_object(&keyword.value.node, scope)?,
        ));
    }
    // M47.2 — an actor value: `GetValue(av)` (one argument; the bare
    // `GetValue()` is a global's), or `GetAV` / `GetActorValue("Name")`.
    if let Some((object, [av])) = method_call_any(e, &["GetValue", "GetAV", "GetActorValue"]) {
        let actor = receiver_actor(object, scope)?;
        return Some(match actor_value_arg(&av.value.node, scope)? {
            ActorValueArg::Form(form) => ValueExpr::ActorValue(actor, form),
            ActorValueArg::Named(name) => ValueExpr::ActorValueNamed(actor, name),
        });
    }
    for (names, reading) in [
        (&["GetBaseValue", "GetBaseActorValue"][..], ActorValueReading::Base),
        (&["GetValuePercentage", "GetActorValuePercentage"][..], ActorValueReading::Percent),
    ] {
        if let Some((object, [av])) = method_call_any(e, names) {
            let actor = receiver_actor(object, scope)?;
            return Some(ValueExpr::ActorValueReading(reading, actor, actor_value_arg(&av.value.node, scope)?));
        }
    }
    Some(match guard_reading(e, scope)? {
        GuardReading::Stage(quest) => ValueExpr::Stage(quest),
        GuardReading::Global(global) => ValueExpr::Global(global),
        GuardReading::QuestVariable(quest, variable) => ValueExpr::QuestVariable(quest, variable),
        GuardReading::ItemCount(container, item) => ValueExpr::ItemCount(container, item),
        GuardReading::Distance(from, to) => ValueExpr::Distance(from, to),
        GuardReading::RelationshipRank(actor, other) => ValueExpr::RelationshipRank(actor, other),
        _ => return None,
    })
}

/// What a guard atom reads, before its comparison is attached.
enum GuardReading {
    StageDone(QuestRef, u16),
    QuestRunning(QuestRef),
    /// `<quest>.IsStopped()`: "completely stopped" is the opposite of running,
    /// since a quest here starts and stops in one step (CK *IsStopped - Quest*).
    QuestStopped(QuestRef),
    Stage(QuestRef),
    ObjectiveDisplayed(QuestRef, i32),
    ObjectiveCompleted(QuestRef, i32),
    Global(ObjectRef),
    QuestVariable(QuestRef, String),
    ActorDead(ActorRef),
    HasPerk(ActorRef, ObjectRef),
    ItemCount(ActorRef, ObjectRef),
    QuestCompleted(QuestRef),
    ScenePlaying(ObjectRef),
    LocationKeyword(ActorRef, ObjectRef),
    HasKeyword(ActorRef, ObjectRef),
    MapMarkerVisible(ObjectRef),
    MapMarkerFastTravel(ObjectRef),
    LocationCleared(ObjectRef),
    ActorBleedingOut(ActorRef),
    ActorInCombat(ActorRef),
    ActorFollower(ActorRef, crate::FollowerFlag),
    ActorEssential(ActorRef),
    /// `IsEnabled()` reads true when enabled; `IsDisabled()` is the negation.
    ReferenceEnabled(ObjectRef, bool),
    InFaction(ActorRef, ObjectRef),
    HasLocRefType(ActorRef, ObjectRef),
    Loaded3D(ObjectRef),
    Distance(ActorRef, ActorRef),
    RelationshipRank(ActorRef, ActorRef),
}

/// Classify a whole `If` condition as a conjunction of guards: `&&` joins,
/// `||` becomes a [`FragmentGuard::Any`] of its alternatives, and `Not` is
/// pushed down to the atoms by De Morgan (`!(A || B)` is `!A && !B`,
/// `!(A && B)` is `!A || !B`). Any atom that does not classify declines.
fn classify_condition(
    condition: &Expr,
    scope: &Scope,
    negated: bool,
    out: &mut Vec<FragmentGuard>,
) -> Option<()> {
    use byroredux_papyrus::ast::UnaryOp;
    match condition {
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
        } => classify_condition(&operand.node, scope, !negated, out),
        Expr::BinaryOp {
            op: op @ (BinaryOp::And | BinaryOp::Or),
            left,
            right,
        } => {
            // `&&`, or a negated `||`, is a conjunction; the other two are
            // disjunctions.
            let conjunction = (*op == BinaryOp::And) != negated;
            if conjunction {
                classify_condition(&left.node, scope, negated, out)?;
                classify_condition(&right.node, scope, negated, out)
            } else {
                let mut alternatives = Vec::new();
                for side in [&left.node, &right.node] {
                    let mut conjunct = Vec::new();
                    classify_condition(side, scope, negated, &mut conjunct)?;
                    // Flatten a nested disjunction into this one.
                    match conjunct.as_slice() {
                        [FragmentGuard::Any(inner)] => alternatives.extend(inner.iter().cloned()),
                        _ => alternatives.push(conjunct),
                    }
                }
                out.push(FragmentGuard::Any(alternatives));
                Some(())
            }
        }
        atom => {
            let negate = |comparison: GuardComparison| {
                if negated {
                    comparison.negated()
                } else {
                    comparison
                }
            };
            if let Some(guard) = classify_comparison(atom, scope)
                .and_then(|(reading, comparison, value)| guard_from(reading, negate(comparison), value))
            {
                out.push(guard);
                return Some(());
            }
            // M47.2 — no specific guard fits: compare two run-time numbers.
            let Expr::BinaryOp { op, left, right } = atom else {
                return None;
            };
            // M47.2 — or two references.
            if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                if let (Some(l), Some(r)) =
                    (reference_side(&left.node, scope), reference_side(&right.node, scope))
                {
                    let (left, right) = match (l, r) {
                        (Some(left), right) => (left, right),
                        (None, Some(right)) => (right, None),
                        (None, None) => return None,
                    };
                    out.push(FragmentGuard::SameReference {
                        left,
                        right,
                        expected: (*op == BinaryOp::Eq) != negated,
                    });
                    return Some(());
                }
            }
            let comparison = GuardComparison::from_op(*op)?;
            out.push(FragmentGuard::Compare {
                left: classify_value(&left.node, scope)?,
                comparison: negate(comparison),
                right: classify_value(&right.node, scope)?,
            });
            Some(())
        }
    }
}

/// M47.2 — one side of a reference comparison: `None` (`Some(None)`), or an
/// expression that can only be a reference — an alias's
/// `GetActorRef()` / `GetActorReference()` / `GetReference()` / `GetRef()`,
/// `Game.GetPlayer()`, or a local bound to a reference. A bare property is
/// not taken: by name alone it could as well be a number.
fn reference_side(e: &Expr, scope: &Scope) -> Option<Option<ActorRef>> {
    let mut e = e;
    while let Expr::Cast { expr, .. } = e {
        e = &expr.node;
    }
    match e {
        Expr::NoneLit => Some(None),
        Expr::Ident(name) => {
            let key = name.0.to_ascii_lowercase();
            let reference = scope.object_locals.contains_key(&key)
                || scope.player_locals.contains(&key)
                || scope.speaker_locals.contains(&key);
            reference.then(|| receiver_actor(e, scope)).flatten().map(Some)
        }
        Expr::Call { .. } => {
            let reference_call = alias_reference_getter(e).is_some_and(|(_, args)| args.is_empty())
                || player_expr_ref(e, scope);
            reference_call.then(|| receiver_actor(e, scope)).flatten().map(Some)
        }
        _ => None,
    }
}

/// `reading <op> literal` in either operand order, or a bare reading used as a
/// boolean (`!= 0`).
fn classify_comparison(atom: &Expr, scope: &Scope) -> Option<(GuardReading, GuardComparison, f32)> {
    if let Expr::BinaryOp { op, left, right } = atom {
        let comparison = GuardComparison::from_op(*op)?;
        if let (Some(reading), Some(value)) = (guard_reading(&left.node, scope), as_num(&right.node)) {
            return Some((reading, comparison, value));
        }
        if let (Some(value), Some(reading)) = (as_num(&left.node), guard_reading(&right.node, scope)) {
            return Some((reading, comparison.swapped(), value));
        }
        return None;
    }
    Some((guard_reading(atom, scope)?, GuardComparison::Ne, 0.0))
}

fn guard_reading(e: &Expr, scope: &Scope) -> Option<GuardReading> {
    if let Expr::Cast { expr, .. } = e {
        return guard_reading(&expr.node, scope);
    }
    let quest = |object: &Expr| -> Option<QuestRef> {
        if matches!(object, Expr::Ident(name) if name.0.eq_ignore_ascii_case("self")) {
            return Some(QuestRef::SelfRef);
        }
        match object {
            Expr::Ident(_) => explicit_quest_receiver(object, scope),
            Expr::Cast { .. } | Expr::Call { .. } => match quest_expr_ref(object, scope)? {
                quest @ (QuestRef::OwningQuest | QuestRef::SelfRef) => Some(quest),
                QuestRef::Property(_) => None,
            },
            _ => None,
        }
    };
    let int = |args: &[byroredux_papyrus::ast::CallArg]| -> Option<i64> {
        if args.len() == 1 {
            int_arg(args, 0)
        } else {
            None
        }
    };
    for name in ["GetStageDone", "IsStageDone"] {
        if let Some((object, args)) = method_call(e, name) {
            let stage = u16::try_from(int(args)?).ok()?;
            return Some(GuardReading::StageDone(quest(object)?, stage));
        }
    }
    if let Some((object, args)) = method_call(e, "IsRunning") {
        return args.is_empty().then_some(())
            .and_then(|()| Some(GuardReading::QuestRunning(quest(object)?)));
    }
    if let Some((object, args)) = method_call(e, "IsStopped") {
        return args.is_empty().then_some(())
            .and_then(|()| Some(GuardReading::QuestStopped(quest(object)?)));
    }
    if let Some((object, args)) = method_call(e, "GetStage") {
        return args.is_empty().then_some(())
            .and_then(|()| Some(GuardReading::Stage(quest(object)?)));
    }
    if let Some((object, args)) = method_call(e, "IsObjectiveDisplayed") {
        let objective = i32::try_from(int(args)?).ok()?;
        return Some(GuardReading::ObjectiveDisplayed(quest(object)?, objective));
    }
    if let Some((object, args)) = method_call(e, "IsObjectiveCompleted") {
        let objective = i32::try_from(int(args)?).ok()?;
        return Some(GuardReading::ObjectiveCompleted(quest(object)?, objective));
    }
    for name in ["GetValue", "GetValueInt"] {
        if let Some((object, args)) = method_call(e, name) {
            if !args.is_empty() {
                return None;
            }
            return global_receiver(object, scope).map(GuardReading::Global);
        }
    }
    // M47.2 — actor, quest, scene and location reads.
    let one_object = |args: &[byroredux_papyrus::ast::CallArg]| {
        (args.len() == 1)
            .then(|| receiver_object(&args[0].value.node, scope))
            .flatten()
    };
    if let Some((object, args)) = method_call(e, "HasKeyword") {
        if let Some((location_of, location_args)) = method_call(object, "GetCurrentLocation") {
            if !location_args.is_empty() {
                return None;
            }
            return Some(GuardReading::LocationKeyword(
                receiver_actor(location_of, scope)?,
                one_object(args)?,
            ));
        }
        // M47.2 — the reference's own keywords.
        return Some(GuardReading::HasKeyword(
            receiver_actor(object, scope)?,
            one_object(args)?,
        ));
    }
    if let Some((object, args)) = method_call(e, "IsDead") {
        return args
            .is_empty()
            .then(|| receiver_actor(object, scope).map(GuardReading::ActorDead))
            .flatten();
    }
    for (method, enabled) in [("IsEnabled", true), ("IsDisabled", false)] {
        if let Some((object, args)) = method_call(e, method) {
            return args
                .is_empty()
                .then(|| {
                    receiver_object(object, scope)
                        .map(|object| GuardReading::ReferenceEnabled(object, enabled))
                })
                .flatten();
        }
    }
    if let Some((object, args)) = method_call(e, "IsInFaction") {
        return Some(GuardReading::InFaction(
            receiver_actor(object, scope)?,
            one_object(args)?,
        ));
    }
    if let Some((object, args)) = method_call(e, "HasLocRefType") {
        return Some(GuardReading::HasLocRefType(
            receiver_actor(object, scope)?,
            one_object(args)?,
        ));
    }
    if let Some((object, args)) = method_call(e, "Is3DLoaded") {
        return args
            .is_empty()
            .then(|| receiver_object(object, scope).map(GuardReading::Loaded3D))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "GetRelationshipRank") {
        let [other] = args else {
            return None;
        };
        let mut other = &other.value.node;
        while let Expr::Cast { expr, .. } = other {
            other = &expr.node;
        }
        return Some(GuardReading::RelationshipRank(
            receiver_actor(object, scope)?,
            receiver_actor(other, scope)?,
        ));
    }
    if let Some((object, args)) = method_call(e, "GetDistance") {
        let [target] = args else {
            return None;
        };
        let mut target = &target.value.node;
        while let Expr::Cast { expr, .. } = target {
            target = &expr.node;
        }
        return Some(GuardReading::Distance(
            receiver_actor(object, scope)?,
            receiver_actor(target, scope)?,
        ));
    }
    if let Some((object, args)) = method_call(e, "IsBleedingOut") {
        return args
            .is_empty()
            .then(|| receiver_actor(object, scope).map(GuardReading::ActorBleedingOut))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "IsInCombat") {
        return args
            .is_empty()
            .then(|| receiver_actor(object, scope).map(GuardReading::ActorInCombat))
            .flatten();
    }
    for (method, flag) in [
        ("IsPlayerTeammate", crate::FollowerFlag::Teammate),
        ("IsDoingFavor", crate::FollowerFlag::DoingFavor),
    ] {
        if let Some((object, args)) = method_call(e, method) {
            return args
                .is_empty()
                .then(|| receiver_actor(object, scope).map(|actor| GuardReading::ActorFollower(actor, flag)))
                .flatten();
        }
    }
    if let Some((object, args)) = method_call(e, "IsEssential") {
        return args
            .is_empty()
            .then(|| receiver_actor(object, scope).map(GuardReading::ActorEssential))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "HasPerk") {
        return Some(GuardReading::HasPerk(receiver_actor(object, scope)?, one_object(args)?));
    }
    if let Some((object, args)) = method_call(e, "GetItemCount") {
        return Some(GuardReading::ItemCount(receiver_actor(object, scope)?, one_object(args)?));
    }
    if let Some((object, args)) = method_call(e, "IsCompleted") {
        return args
            .is_empty()
            .then(|| quest(object).map(GuardReading::QuestCompleted))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "IsMapMarkerVisible") {
        return args
            .is_empty()
            .then(|| receiver_object(object, scope).map(GuardReading::MapMarkerVisible))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "IsCleared") {
        return args
            .is_empty()
            .then(|| receiver_location(object, scope).map(GuardReading::LocationCleared))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "CanFastTravelToMarker") {
        return args
            .is_empty()
            .then(|| receiver_object(object, scope).map(GuardReading::MapMarkerFastTravel))
            .flatten();
    }
    if let Some((object, args)) = method_call(e, "IsPlaying") {
        return args
            .is_empty()
            .then(|| receiver_object(object, scope).map(GuardReading::ScenePlaying))
            .flatten();
    }
    if let Expr::MemberAccess { object, member } = e {
        // `<global>.Value` — the GlobalVariable property spelling.
        if member.node.0.eq_ignore_ascii_case("value") {
            if let Some(global) = global_receiver(&object.node, scope) {
                return Some(GuardReading::Global(global));
            }
        }
        if matches!(&object.node, Expr::Ident(name) if name.0.eq_ignore_ascii_case("self")) {
            return None;
        }
        let quest = match &object.node {
            Expr::Ident(_) => explicit_quest_receiver(&object.node, scope)?,
            Expr::Cast { .. } => match quest_expr_ref(&object.node, scope)? {
                quest @ (QuestRef::OwningQuest | QuestRef::SelfRef) => quest,
                quest @ QuestRef::Property(_) => {
                    if !cast_to_quest_script(&object.node, scope) {
                        return None;
                    }
                    quest
                }
            },
            Expr::MemberAccess { .. } => receiver_quest(&object.node, scope)?,
            _ => return None,
        };
        return Some(GuardReading::QuestVariable(quest, member.node.0.clone()));
    }
    None
}

/// A bare identifier naming a VMAD property (the GLOB a `GetValue()` reads):
/// not a local of any kind, not a known quest property.
fn global_receiver(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let Expr::Ident(name) = object else {
        return None;
    };
    let key = name.0.to_ascii_lowercase();
    if scope.declares(&key) || scope.speaker_locals.contains(&key) {
        return None;
    }
    receiver_object(object, scope)
}

/// A boolean reading compared with a literal becomes the bool it must equal;
/// a comparison every bool satisfies, or none does, declines.
fn bool_expectation(comparison: GuardComparison, value: f32) -> Option<bool> {
    let when_true = comparison.apply(1.0, value);
    let when_false = comparison.apply(0.0, value);
    (when_true != when_false).then_some(when_true)
}

fn guard_from(reading: GuardReading, comparison: GuardComparison, value: f32) -> Option<FragmentGuard> {
    Some(match reading {
        GuardReading::StageDone(quest, stage) => FragmentGuard::StageDone {
            quest,
            stage,
            done: bool_expectation(comparison, value)?,
        },
        GuardReading::QuestRunning(quest) => FragmentGuard::QuestRunning {
            quest,
            running: bool_expectation(comparison, value)?,
        },
        GuardReading::QuestStopped(quest) => FragmentGuard::QuestRunning {
            quest,
            running: !bool_expectation(comparison, value)?,
        },
        GuardReading::ObjectiveDisplayed(quest, objective) => FragmentGuard::ObjectiveDisplayed {
            quest,
            objective,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ObjectiveCompleted(quest, objective) => FragmentGuard::ObjectiveCompleted {
            quest,
            objective,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::Stage(quest) => FragmentGuard::Stage {
            quest,
            comparison,
            value,
        },
        GuardReading::Global(global) => FragmentGuard::Global {
            global,
            comparison,
            value,
        },
        GuardReading::QuestVariable(quest, variable) => FragmentGuard::QuestVariable {
            quest,
            variable,
            comparison,
            value,
        },
        GuardReading::ActorDead(actor) => FragmentGuard::ActorDead {
            actor,
            dead: bool_expectation(comparison, value)?,
        },
        GuardReading::ActorBleedingOut(actor) => FragmentGuard::ActorBleedingOut {
            actor,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ActorInCombat(actor) => FragmentGuard::ActorInCombat {
            actor,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ActorFollower(actor, flag) => FragmentGuard::ActorFollower {
            actor,
            flag,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ActorEssential(actor) => FragmentGuard::ActorEssential {
            actor,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ReferenceEnabled(object, enabled) => FragmentGuard::ReferenceEnabled {
            object,
            expected: bool_expectation(comparison, value)? == enabled,
        },
        GuardReading::HasLocRefType(actor, ref_type) => FragmentGuard::HasLocRefType {
            actor,
            ref_type,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::InFaction(actor, faction) => FragmentGuard::InFaction {
            actor,
            faction,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::Loaded3D(object) => FragmentGuard::Loaded3D {
            object,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::RelationshipRank(actor, other) => FragmentGuard::RelationshipRank {
            actor,
            other,
            comparison,
            value,
        },
        GuardReading::Distance(from, to) => FragmentGuard::Distance {
            from,
            to,
            comparison,
            value,
        },
        GuardReading::HasPerk(actor, perk) => FragmentGuard::HasPerk {
            actor,
            perk,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::ItemCount(container, item) => FragmentGuard::ItemCount {
            container,
            item,
            comparison,
            value,
        },
        GuardReading::QuestCompleted(quest) => FragmentGuard::QuestCompleted {
            quest,
            completed: bool_expectation(comparison, value)?,
        },
        GuardReading::ScenePlaying(scene) => FragmentGuard::ScenePlaying {
            scene,
            playing: bool_expectation(comparison, value)?,
        },
        GuardReading::MapMarkerVisible(marker) => FragmentGuard::MapMarkerVisible {
            marker,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::MapMarkerFastTravel(marker) => FragmentGuard::MapMarkerFastTravel {
            marker,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::LocationCleared(location) => FragmentGuard::LocationCleared {
            location,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::LocationKeyword(actor, keyword) => FragmentGuard::LocationKeyword {
            actor,
            keyword,
            expected: bool_expectation(comparison, value)?,
        },
        GuardReading::HasKeyword(object, keyword) => FragmentGuard::HasKeyword {
            object,
            keyword,
            expected: bool_expectation(comparison, value)?,
        },
    })
}

/// Typed provider invocation deferred until fragment ECS guards are released.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FragmentProviderCall {
    pub route: String,
    pub arguments: Vec<byroredux_sdk::script_function::ScriptValue>,
    pub principal: Option<byroredux_sdk::identity::PrincipalId>,
}

/// Attribute every provider barrier in a lowered fragment, including calls in
/// nested conditional branches, to the archive package that supplied its PEX.
pub(crate) fn attribute_provider_calls(
    effects: &mut [Effect],
    principal: &byroredux_sdk::identity::PrincipalId,
) {
    for effect in effects {
        match effect {
            Effect::ProviderCall(call) => call.principal = Some(principal.clone()),
            Effect::Conditional {
                then_effects,
                else_effects,
                ..
            } => {
                attribute_provider_calls(then_effects, principal);
                attribute_provider_calls(else_effects, principal);
            }
            _ => {}
        }
    }
}

/// A canonical effect a fragment statement lowers to. The runtime applies
/// quest-scoped variants against [`QuestStageState`] / [`QuestObjectiveState`]
/// and object variants through the fragment's VMAD; see [`crate::fragment`].
///
/// [`QuestStageState`]: crate::quest_stages::QuestStageState
/// [`QuestObjectiveState`]: crate::quest_stages::QuestObjectiveState
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum Effect {
    /// `If <guard && ...> ... [Else ...] EndIf`. Only conjunctions of
    /// [`FragmentGuard`]s lower; elseif/disjunction and latent effects inside
    /// either branch remain decline-on-unknown.
    Conditional {
        guards: Vec<FragmentGuard>,
        then_effects: Vec<Effect>,
        else_effects: Vec<Effect>,
    },
    /// `<global>.SetValue(value)` — resolves the VMAD-bound GLOB FormID and
    /// updates the canonical runtime [`crate::Globals`] table.
    SetGlobalValue {
        global: ObjectRef,
        /// M47.2 — a run-time number since FORMAT 60: a literal, a read
        /// global plus an offset (`G.Mod(x)`, `G.SetValue(S.GetValue() + 1)`),
        /// or any other [`ValueExpr`].
        value: ValueExpr,
    },
    /// `<quest>.SetStage(stage)`.
    SetStage { quest: QuestRef, stage: u16 },
    /// `<quest>.Start()`.
    StartQuest { quest: QuestRef },
    /// `<quest>.Stop()`.
    StopQuest { quest: QuestRef },
    /// `<quest>.CompleteQuest()`.
    CompleteQuest { quest: QuestRef },
    /// `<quest>.Reset()`.
    ResetQuest { quest: QuestRef },
    /// `<quest>.SetActive(active)`.
    SetQuestActive { quest: QuestRef, active: bool },
    /// `<quest>.SetObjectiveDisplayed(objective, displayed)`. Papyrus's
    /// optional `abForce` 3rd arg doesn't affect the stored state, but is
    /// accepted only as a literal (declining non-literal 3rd expressions,
    /// #5652).
    SetObjectiveDisplayed {
        quest: QuestRef,
        objective: i32,
        displayed: bool,
    },
    /// `<quest>.SetObjectiveCompleted(objective, completed)`.
    SetObjectiveCompleted {
        quest: QuestRef,
        objective: i32,
        completed: bool,
    },
    /// `<quest>.SetObjectiveFailed(objective, failed)`.
    SetObjectiveFailed {
        quest: QuestRef,
        objective: i32,
        failed: bool,
    },
    /// `<quest>.CompleteAllObjectives()`.
    CompleteAllObjectives { quest: QuestRef },
    /// `<quest>.FailAllObjectives()`.
    FailAllObjectives { quest: QuestRef },
    /// `<container>.AddItem(<item>, <count = 1>)`. The optional 3rd
    /// (`abSilent`) argument is accepted (parsed, not applied — no pickup
    /// notification UI exists to suppress) but only as a literal; a
    /// non-literal 3rd arg, or a 4th, declines the whole fragment (the
    /// `bool_arg` discipline `SetObjectiveDisplayed` already uses).
    /// `item` resolves only to a FormID at dispatch, never to an entity —
    /// it names a *base record* (the item type), not a placed reference.
    /// The container is an [`ActorRef`] so `Game.GetPlayer().AddItem(..)` —
    /// the corpus's commonest reward shape — lowers (M47.2).
    AddItem {
        container: ActorRef,
        item: ObjectRef,
        count: ItemCount,
    },
    /// `<actor>.EquipItem(<item>, false, <silent>)`. The supported shape
    /// deliberately declines `abPreventUnequip=true` until locked equipment
    /// has canonical state; the runtime updates `Inventory` +
    /// `EquipmentSlots` using the plugin-derived `EquipItemCatalog`.
    EquipItem {
        actor: ActorRef,
        item: ObjectRef,
        silent: bool,
    },
    /// `<moved>.MoveTo(<destination>[, x, y, z, matchRotation])` (CK *MoveTo -
    /// ObjectReference*): the destination's position plus the offset (Gamebryo
    /// Z-up world axes), with the destination's rotation when `match_rotation`
    /// (the default). Starfield's `abRotateOffset` is not modelled and declines
    /// (see [`prim_move_to`]).
    MoveTo {
        moved: ObjectRef,
        destination: ObjectRef,
        offset: [f32; 3],
        match_rotation: bool,
    },
    /// `Game.GetPlayer().MoveTo(<destination>)` (M47.2) — the player's
    /// teleport, with [`Self::MoveTo`]'s default-offsets rule. The player
    /// may land in another cell, so the runtime records the destination
    /// reference for the engine ([`crate::fragment::PlayerMoveRequest`]),
    /// which moves the player in place or transitions to the reference's
    /// cell.
    MovePlayerTo { destination: ObjectRef },
    /// `<object>.Disable([fadeOut])` — records the placed reference as
    /// disabled even when its cell/entity is not currently loaded.
    Disable { object: ObjectRef, fade_out: bool },
    /// `<object>.Enable([fadeIn])` — the symmetric counterpart to
    /// [`Self::Disable`]; clears the same [`crate::fragment::
    /// ReferenceEnableState`] entry so a reference a script disabled can be
    /// re-enabled by script. #3489 — shipped alongside `Disable` (`5f38402e`)
    /// without this half, so `ReferenceEnableState::set_enabled`'s
    /// `enabled == true` branch had no production caller and every
    /// `Enable()` call (3,005 in the real corpus, more common than
    /// `Disable()`'s 2,587) declined the whole fragment.
    Enable { object: ObjectRef, fade_in: bool },
    /// `<scene>.Start()` — resolves a VMAD-bound SCEN FormID and queues the
    /// canonical scene start request at dispatch.
    StartScene { scene: ObjectRef },
    /// `<scene>.Stop()` — the symmetric explicit stop request.
    StopScene { scene: ObjectRef },
    /// `<target>.Activate()` (or an explicit `Game.GetPlayer()` activator).
    /// Dispatch emits the canonical one-frame [`crate::ActivateEvent`].
    Activate {
        target: ObjectRef,
        /// `None` means the player/default activator; `Some` resolves a
        /// VMAD property or quest-alias `GetRef()` expression.
        activator: Option<ObjectRef>,
        /// `abDefaultProcessingOnly`: the engine's own activation runs, no script hears
        /// `OnActivate` and the block flag is ignored.
        native_only: bool,
    },
    /// `<target>.SetOpen(open)` — synchronizes the canonical two-state
    /// activator and its `::isOpen_var` CTDA-visible VM variable.
    SetOpen { target: ObjectRef, open: bool },
    /// `<target>.Lock(abLock)` — sets or clears the authored lock on a door
    /// or container.
    ///
    /// Papyrus defaults `abLock` to `true`, so a bare `Lock()` locks.
    /// `Lock(false)` is the unlock that #3159 was filed for: before this
    /// primitive existed nothing in the engine could remove a `Locked`
    /// component, and a fragment containing the call declined *wholesale* —
    /// taking its sibling `SetStage` / `SetObjectiveCompleted` with it.
    /// The declared second parameter `abAsOwner` (cell public/private
    /// flagging, #5653) is parsed as a literal and not applied.
    SetLocked { target: ObjectRef, locked: bool },
    /// `<target>.SetLockLevel(aiLockLevel)` — sets lock *difficulty*.
    ///
    /// Deliberately a separate effect from [`Self::SetLocked`], not a second
    /// spelling of it: in Papyrus `SetLockLevel` does **not** lock or unlock
    /// anything, it only changes how hard an already-locked object is to
    /// pick. Folding the two together would make a fragment that merely
    /// re-tunes a lock silently lock or unlock a door.
    ///
    /// Applying it to an unlocked object is a no-op, matching the component
    /// model: `Locked`'s absence *is* the unlocked state, so there is no
    /// record to carry a difficulty on.
    SetLockLevel { target: ObjectRef, level: u8 },
    /// `Game.GetPlayer().SetRestrained(restrained)`.
    SetPlayerRestrained { restrained: bool },
    /// Selectively enable/disable the domains named by the two Papyrus
    /// `*PlayerControls` global functions.
    SetPlayerControls {
        enabled: bool,
        selection: PlayerControlSelection,
    },
    /// `Game.SetPlayerAIDriven(ai_driven)`.
    SetPlayerAiDriven { ai_driven: bool },
    /// `Game.SetHudCartMode(cart_mode)` presentation state.
    SetHudCartMode { cart_mode: bool },
    /// `Game.SetInChargen(abDisableSaving, abDisableWaiting,
    /// abShowControlsDisabledMessage)` — Skyrim's own declaration (#4322).
    /// The aliases decode saves written under the earlier invented names,
    /// which held the same positional arguments.
    SetInChargen {
        #[cfg_attr(feature = "save", serde(alias = "enabled"))]
        disable_saving: bool,
        #[cfg_attr(feature = "save", serde(alias = "wait_for_race_sex"))]
        disable_waiting: bool,
        #[cfg_attr(feature = "save", serde(alias = "stay_in_first_person"))]
        show_controls_disabled_message: bool,
    },
    /// `Game.ShowRaceMenu()`. No interactive race-menu UI exists yet (a full
    /// slider-based character creator is its own milestone); the runtime
    /// treats the call as an instant auto-accept of the player's current
    /// appearance so the fragment it's authored in — MQ101's execution-block
    /// scene — no longer declines wholesale over an unmodeled statement.
    ShowRaceMenu,
    /// `Game.RequestSave()` / `Game.RequestAutoSave()`. Deliberately does
    /// not write a save file — see `CinematicPresentationState::
    /// request_save`'s doc for why a mid-fragment write is out of scope.
    RequestSave { auto: bool },
    /// `<faction>.SetEnemy(<other_faction>, abSelfIsNeutralToOther,
    /// abOtherIsNeutralToSelf)`. Each flag makes its direction *neutral*
    /// instead of hostile (#4318); the two directions are independent, which
    /// [`crate::FactionRelations`] records as directed reactions (#4414).
    SetEnemy {
        faction: ObjectRef,
        other_faction: ObjectRef,
        self_neutral: bool,
        other_neutral: bool,
    },
    /// M47.2 — `<faction>.SetAlly(<other>, abSelfIsFriendToOther = false,
    /// abOtherIsFriendToSelf = false)`: each direction becomes an ally, or a
    /// friend when its flag is set (CK *SetAlly - Faction*).
    SetAlly {
        faction: ObjectRef,
        other_faction: ObjectRef,
        self_friend: bool,
        other_friend: bool,
    },
    /// `<actor>.StartCombat(<target>)`. See [`crate::AiCombatState`] for
    /// the runtime chase-and-strike behavior this arms.
    StartCombat { actor: ActorRef, target: ActorRef },
    /// M47.2 — `<reference>.<Variable> = literal` on a script attached to a
    /// reference (`::MQ203PlateTrapMarker_var.plateTriggered = False`): the
    /// reference's `ScriptVariables`, which `GetVMScriptVariable` reads.
    SetReferenceVariable {
        object: ObjectRef,
        variable: String,
        value: f32,
    },
    /// M47.2 — `<message>.Show([afArg1 … afArg9])` as a statement: the
    /// message's notification or message box, its `printf` spots filled from
    /// the literal arguments ([`crate::messages`]).
    ShowMessage { message: ObjectRef, args: Vec<f32> },
    /// M47.2 — `<message>.ShowAsHelpMessage(asEvent, …)`: the help text,
    /// shown once (the repeat interval and the ending event are not
    /// modelled).
    ShowHelpMessage { message: ObjectRef },
    /// M47.2 — `Debug.Notification(asText)` with a literal text.
    Notification { text: String },
    /// M47.2 — a fragment the lowering declines, or a reference script's
    /// event, run by the Papyrus VM ([`crate::fragment::vm`]): `script`'s
    /// `function` (or event), installed with the scripts it reaches.
    Interpret {
        script: String,
        function: String,
        /// The event's arguments (`akActionRef`); none for a fragment.
        args: Vec<VmStored>,
        /// Where a run suspended by `Utility.Wait` resumes; `None` to start.
        resume: Option<VmResume>,
    },
    /// M47.2 — `<reference>.DeleteWhenAble()`: a created reference is deleted
    /// once its cell is gone ([`crate::CreatedDeletion`]); an authored one
    /// cannot be deleted.
    DeleteWhenAble { object: ObjectRef },
    /// M47.2 — `<actor>.ShowBarterMenu()`: open the barter menu with the
    /// actor's vendor faction ([`crate::menus`]).
    ShowBarterMenu { vendor: ActorRef },
    /// M47.2 — `<reference>.AddKeyword(kw)` / `RemoveKeyword(kw)` /
    /// `ResetKeyword(kw)` ([`crate::keywords`]).
    ChangeKeyword {
        object: ActorRef,
        keyword: ObjectRef,
        change: crate::keywords::KeywordChange,
    },
    /// M47.2 — a local assigned a number computed at run time
    /// (`int n = Utility.RandomInt(0, 3)`), read by later [`ValueExpr::Local`]s
    /// in the same run. A fragment holding one never suspends.
    SetLocal { local: String, value: ValueExpr },
    /// M47.2 — effects inlined from the quest script's function
    /// (`kmyQuest.UpdateRadio()`) into an INFO or scene fragment: they run
    /// with `quest` as their context and its VMAD for their properties.
    /// Never holds a latent effect.
    ///
    /// `bindings` pass the inlined function's object arguments that name the
    /// *fragment's* properties: `(parameter property, fragment property)`.
    /// At run time each becomes a property of the quest's VMAD, valued as the
    /// fragment's (`MoveSpouse(akSpeaker, SolitudeHouse)` from an INFO).
    QuestScope {
        quest: QuestRef,
        effects: Vec<Effect>,
        bindings: Vec<(String, String)>,
    },
    /// M47.2 — `Game.RewardPlayerXP(auiXPAmount, abDirect = false)` (FO4 /
    /// Starfield): experience through the engine's level roll; `direct`
    /// bypasses the XP modifiers.
    RewardPlayerXp { amount: ItemCount, direct: bool },
    /// M47.2 — `Game.IncrementStat(asStatName, aiModAmount = 1)` with
    /// literal arguments ([`crate::misc_stats`]).
    IncrementStat { stat: String, amount: i32 },
    /// M47.2 — `Game.AddAchievement(aiAchievementID)` with a literal ID
    /// ([`crate::Achievements`]).
    AddAchievement { id: i32 },
    /// M47.2 — `<location>.SetKeywordData(keyword, value)`.
    SetLocationKeywordData {
        location: ObjectRef,
        keyword: ObjectRef,
        value: ValueExpr,
    },
    /// M47.2 — `<reference>.MoveToMyEditorLocation()`: back to where the plugin
    /// placed it ([`crate::EditorLocation`]).
    MoveToEditorLocation { moved: ObjectRef },
    /// M47.2 — `<reference>.SetLinkedRef(apRef, apKeyword = None)`
    /// ([`crate::LinkedRefOverrides`]); a `None` target clears the link.
    SetLinkedRef {
        object: ObjectRef,
        target: Option<ObjectRef>,
        keyword: Option<ObjectRef>,
    },
    /// M47.2 — `<actor>.DamageValue(av, amount)` / `RestoreValue(av, amount)`
    /// (Skyrim: `DamageActorValue` / `RestoreActorValue("Name", amount)`): the
    /// actor value's damage layer grows by `amount`, or `restore` takes up to
    /// `amount` of it back (CK *DamageValue* / *RestoreValue - ObjectReference*).
    DamageActorValue {
        actor: ActorRef,
        actor_value: ActorValueArg,
        amount: ValueExpr,
        restore: bool,
    },
    /// M47.2 — `<reference>.SetLocRefType(akLoc, akRefType)`
    /// ([`crate::LocRefTypeOverrides`]); a `None` ref type clears what scripts
    /// assigned. The location is not part of the key.
    SetLocRefType {
        object: ObjectRef,
        ref_type: Option<ObjectRef>,
    },
    /// M47.2 — `Game.AdvanceSkill(asSkillName, afMagnitude)` with literal
    /// arguments ([`crate::skills`]).
    AdvanceSkill { skill: String, magnitude: f32 },
    /// M47.2 — `<actor>.SetPlayerTeammate([abTeammate], [abCanDoFavor])`
    /// ([`crate::FollowerState`]).
    SetPlayerTeammate {
        actor: ActorRef,
        teammate: bool,
        can_do_favor: bool,
    },
    /// M47.2 — `<actor>.SetDoingFavor([abDoingFavor])`.
    SetDoingFavor { actor: ActorRef, doing: bool },
    /// M47.2 — `Game.GivePlayerCaps(n)` / `RemovePlayerCaps(n)` (FO4): the
    /// script bodies add / remove `n` of `Game.GetCaps()` (form `0xF`) on the
    /// player when `n > 0`. Negative `delta` removes.
    PlayerCaps { delta: i32 },
    /// M47.2 — `<actor>.SetRelationshipRank(akOther, aiRank)` (+4 Lover …
    /// −4 Archnemesis), on the two actors' bases.
    SetRelationshipRank {
        actor: ActorRef,
        other: ActorRef,
        rank: i8,
    },
    /// M47.2 — `<actor>.Kill([akKiller = None])`: the actor dies through the
    /// engine's death path, or goes into bleedout when essential (CK *Kill*).
    /// `silent` is `KillSilent`: the same death "without a kill event" (CK
    /// *KillSilent - Actor*), so the Story Manager's KILL event is not raised.
    Kill {
        actor: ActorRef,
        killer: Option<ActorRef>,
        silent: bool,
    },
    /// #4816 — `<actor>.StopCombat()`: end the combat [`Self::StartCombat`]
    /// (or ambient faction hostility) armed on this actor. The ambient
    /// hostility system may start a fresh one if the actor still sees
    /// someone it attacks, as the original does.
    StopCombat { actor: ActorRef },
    /// M47.2 — `<actor>.SetCrimeFaction(akFaction)` ([`crate::CrimeFactionOverrides`]);
    /// `None` clears it. A faction the actor is not in is added (CK *SetCrimeFaction
    /// - Actor*).
    SetCrimeFaction {
        actor: ActorRef,
        faction: Option<ObjectRef>,
    },
    /// M47.2 — `<actor>.StopCombatAlarm()` (CK: "Stop all combat and alarms
    /// against this actor"): every actor fighting this one stands down. The CK's
    /// alarms are the crime system's, which the engine does not have.
    StopCombatAlarm { actor: ActorRef },
    /// #5017 — `<actor>.SetUnconscious([abUnconscious = true])`. See
    /// [`crate::ActorControlState::unconscious`] for what the state
    /// suppresses. Wakes FO4's dormant "Starts Unconscious" robots and
    /// turrets.
    SetUnconscious { actor: ActorRef, unconscious: bool },
    /// M47.2 — `<actor>.SetGhost([abIsGhost = true])`
    /// ([`crate::ActorControlState::ghost`]).
    SetGhost { actor: ActorRef, ghost: bool },
    /// #4415 — `<actor>.AddSpell(<spell>[, abVerbose])`: put the spell on the
    /// actor's `SpellList`, applying a constant spell's value changes. The
    /// verbose flag only chooses whether a HUD message shows.
    AddSpell { actor: ActorRef, spell: ObjectRef },
    /// #4415 — `<actor>.RemoveSpell(<spell>)`, undoing the above.
    RemoveSpell { actor: ActorRef, spell: ObjectRef },
    /// #4415 — `AddRaceSpells()` / `<actor>.AddRaceSpells()`: re-apply the
    /// actor's RACE-authored `SPLO` set (level-resolved at spawn and held
    /// as the `RaceSpells` component). Vanilla calls it unqualified from a
    /// quest script's own helper (MQ101QuestScript), which is why the
    /// unqualified spelling is the one that must lower.
    AddRaceSpells { actor: ActorRef },
    /// #4415 — `RemoveRaceSpells()`: clear exactly the racial set above
    /// from the actor's live `SpellList` (a scripted `AddSpell` survives).
    RemoveRaceSpells { actor: ActorRef },
    /// `<actor>.PlayIdle(<idle>)`. The runtime preserves the IDLE FormID as
    /// an animation-backend request even when the current game uses HKX.
    PlayIdle { actor: ActorRef, idle: ObjectRef },
    /// `<actor>.SetVehicle(<vehicle>)`; `None` detaches the actor.
    SetVehicle {
        actor: ActorRef,
        vehicle: Option<ObjectRef>,
    },
    /// `<cart>.TetherToHorse(<horse>)`. The app preserves the captured cart
    /// pose relative to the package-driven horse.
    TetherToHorse { cart: ObjectRef, horse: ObjectRef },
    /// `<object>.SetMotionType(...)`, bridged to the app physics layer by a
    /// one-shot `MotionTypeChangeRequest`.
    SetMotionType {
        target: ObjectRef,
        motion_type: MotionType,
        allow_activate: bool,
    },
    /// `Game.SetSittingRotation(degrees)` camera/presentation state.
    SetSittingRotation { degrees: f32 },
    /// MQ101's `ExitCart(alias, seat)` helper. It detaches the actor and
    /// requests the helper's exact seat-specific exit IDLE.
    ExitCart { actor: ObjectRef, seat: u8 },
    /// MQ101 helper registrations for `PlayImod` / `IdleFurnitureExit`.
    RegisterPlayerAnimationEvent { event: CinematicAnimationEvent },
    /// `<actor>.EvaluatePackage()` — queues the actor's active scene package
    /// for condition re-selection and command restart on the next package tick.
    /// An [`ActorRef`] since M47.2, so a dialogue line's speaker re-evaluates.
    EvaluatePackage { actor: ActorRef },
    /// Latent `Utility.Wait(seconds)`. Production dispatch pauses here and
    /// resumes the remaining effects through `FragmentExecutionQueue`.
    Wait { seconds: f32 },
    /// The exact bounded-work polling loop used by MQ101 Fragment_175:
    /// `While !actor.Is3DLoaded() || ...; Utility.Wait(poll); EndWhile`.
    /// Each continuation tick evaluates the list once and reschedules itself,
    /// avoiding a blocking script loop while preserving the authored gate.
    WaitForActors3DLoaded {
        actors: Vec<ActorRef>,
        poll_seconds: f32,
    },
    /// M47.2 — `While <condition>` / `Utility.Wait(poll)` / `EndWhile`: while
    /// the guards hold the fragment suspends for `poll_seconds` and checks
    /// again; once they fail it continues. A guard that cannot be read ends
    /// the wait.
    WaitWhile {
        guards: Vec<FragmentGuard>,
        poll_seconds: f32,
    },
    /// `<container>.RemoveItem(<item>, <count = 1>, <silent = false>,
    /// <other container = None>)` (M47.2). Removes up to `count` of the base
    /// item (Papyrus removes what is there when there is less); with an
    /// other container the removed items move there instead of vanishing.
    /// `abSilent` is accepted as a literal and not applied, as for `AddItem`.
    RemoveItem {
        container: ActorRef,
        item: ObjectRef,
        count: ItemCount,
        destination: Option<ActorRef>,
    },
    /// M47.2 — `<quest>.<Property> = <literal>`: a write to a quest script's
    /// auto property, recorded in [`crate::QuestScriptVariables`] under the
    /// backing `::Property_var` name `GetVMQuestVariable` conditions read.
    /// The value is a numeric / Bool literal (Bool as 0 / 1).
    /// M47.2 — `<quest>.<Property> = <quest>.<Property> ± delta`: a relative
    /// quest-script variable write (counters).
    ModQuestVariable {
        quest: QuestRef,
        variable: String,
        delta: f32,
    },
    /// M47.2 — `to = from` between two of a quest script's own variables
    /// (`::LastSong02_var = ::LastSong01_var` in an inlined quest-script
    /// body). A source with no value leaves the target untouched.
    CopyQuestVariable {
        quest: QuestRef,
        from: String,
        to: String,
    },
    SetQuestVariable {
        quest: QuestRef,
        variable: String,
        value: f32,
    },
    /// M47.2 — `<actor>.AddToFaction(f)` / `.RemoveFromFaction(f)` /
    /// `.SetFactionRank(f, rank)`: the live `FactionRanks` edit, kept by the
    /// saved [`crate::ScriptedFactionChanges`] ledger across re-derivation.
    ChangeFaction {
        actor: ActorRef,
        faction: ObjectRef,
        change: crate::FactionChange,
    },
    /// M47.2 — FO4 / Starfield `<actor>.SetValue(akActorValue, afValue)`:
    /// sets the actor value's base (CK *SetValue - ObjectReference*). The
    /// actor value is a VMAD `ActorValue` property (an AVIF FormID).
    SetActorValue {
        actor: ActorRef,
        actor_value: ObjectRef,
        /// M47.2 — a run-time number since FORMAT 59 (`GetValue(av) + 1`).
        value: ValueExpr,
    },
    /// M47.2 — `<marker>.AddToMap([abFastTravel = false])`.
    AddToMap { marker: ObjectRef, fast_travel: bool },
    /// M47.2 — `<location>.SetCleared([abCleared = true])`: a location
    /// property, or a location alias's `GetLocation()`.
    SetLocationCleared { location: ObjectRef, cleared: bool },
    /// M47.2 — `<base>.SetEssential([abEssential = true])` (`protected:
    /// false`) or `SetProtected([abProtected = true])`. Setting one flag
    /// clears the other (CK *SetEssential* / *SetProtected - ActorBase*).
    SetBaseProtection {
        base: ActorBaseRef,
        protected: bool,
        value: bool,
    },
    /// M47.2 — `<keyword>.SendStoryEvent(akLoc, akRef1, akRef2, aiValue1,
    /// aiValue2)`: raises a script (`SCPT`) Story Manager event carrying the
    /// keyword, the location, the two references and the two integers.
    SendStoryEvent {
        keyword: ObjectRef,
        location: Option<ObjectRef>,
        references: [Option<ActorRef>; 2],
        values: [i32; 2],
    },
    /// M47.2 — `<object>.BlockActivation([abBlocked = true][,
    /// abHideActivateText])`: blocks or unblocks the reference's default
    /// activation, recorded in [`crate::ReferenceActivationBlocks`].
    BlockActivation { object: ObjectRef, blocked: bool },
    /// M47.2 — Skyrim's `<actor>.SetAV("Name", value)` /
    /// `SetActorValue("Name", value)`: [`Self::SetActorValue`] with the actor
    /// value named as Papyrus spells it, resolved through
    /// [`crate::ActorValueNames`].
    SetActorValueNamed {
        actor: ActorRef,
        name: String,
        value: ValueExpr,
    },
    /// M47.2 — `<alias>.ForceRefTo(akRef)` (`target` = the reference) or
    /// `<alias>.Clear()` (`target` = `None`): a scripted alias override,
    /// recorded in [`crate::AliasOverrides`] for the alias refresh.
    ForceAlias {
        alias: ObjectRef,
        target: Option<ActorRef>,
    },
    /// M47.2 — `<location alias>.ForceLocationTo(akLocation)`: the location
    /// alias holds the location until the quest stops (CK *ForceLocationTo -
    /// LocationAlias*).
    ForceLocationTo { alias: ObjectRef, location: ObjectRef },
    /// M47.2 — `<actor>.AddPerk(akPerk[, abNotify])` / `RemovePerk(akPerk)`:
    /// the CK's "adds the specified perk to this actor" (rank 1 when not
    /// owned, else unchanged) and its removal, on the actor's `Perks`.
    ChangePerk {
        actor: ActorRef,
        perk: ObjectRef,
        add: bool,
    },
    /// A manifest or engine-owned static provider call used as a sequencing
    /// barrier. Runtime dispatch occurs only after quest/object ECS guards are
    /// released, then resumes the remaining top-level effects in order.
    ProviderCall(FragmentProviderCall),
}

/// M47.2 — an `ActorBase` receiver: an actor's `GetActorBase()`, or an
/// ActorBase property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorBaseRef {
    OfActor(ActorRef),
    Form(ObjectRef),
}

/// An actor receiver is either the canonical player or a VMAD-resolved
/// object/alias. Keeping the player explicit avoids inventing a VMAD property
/// for `Game.GetPlayer()`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorRef {
    Player,
    Object(ObjectRef),
    /// M47.2 — the speaker of the dialogue line whose `TIF` fragment this is
    /// (`akSpeakerRef`, and the `akSpeaker = akSpeakerRef as Actor` local
    /// Champollion emits). Only INFO fragments lower it, and never alongside
    /// a latent tail, so it is always resolved from the live dialogue call.
    Speaker,
}

impl Effect {
    /// Whether this effect's runtime arm only *records* that the call ran —
    /// a counter or flag nothing reads — without doing what the call does
    /// (#4328). `Game.ShowRaceMenu` opens no menu; `Game.RequestSave` and
    /// `RequestAutoSave` write no save; `Game.SetHudCartMode` sets
    /// `PlayerControlState::hud_cart_mode`, which no HUD reads (#4372).
    /// `Game.SetInChargen` is not a placeholder: its `abDisableSaving` flag
    /// gates the save command.
    ///
    /// Lowering them is deliberate: declining would discard every sibling
    /// effect in MQ101's chargen fragments. But a fragment claimed only by
    /// way of one is coverage with a hole in it, so the coverage harnesses
    /// count those fragments separately rather than letting the stub pass
    /// for the real behavior.
    /// #5807 — the variant-name table the two conformance harnesses
    /// (`mq101_conformance.rs`, `fragment_coverage.rs`) each pasted as a
    /// ~99-line match, edited by hand on every new variant (a missed copy
    /// mis-reports that harness's coverage). One home, next to
    /// `is_placeholder`; the exhaustive match means a new variant without
    /// a row here is a compile error.
    pub fn kind(&self) -> &'static str {
        match self {
        Effect::Conditional { .. } => "Conditional",
        Effect::SetGlobalValue { .. } => "SetGlobalValue",
        Effect::SetStage { .. } => "SetStage",
        Effect::StartQuest { .. } => "StartQuest",
        Effect::StopQuest { .. } => "StopQuest",
        Effect::CompleteQuest { .. } => "CompleteQuest",
        Effect::ResetQuest { .. } => "ResetQuest",
        Effect::SetQuestActive { .. } => "SetQuestActive",
        Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
        Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
        Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
        Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
        Effect::FailAllObjectives { .. } => "FailAllObjectives",
        Effect::AddItem { .. } => "AddItem",
        Effect::RemoveItem { .. } => "RemoveItem",
        Effect::SetQuestVariable { .. } => "SetQuestVariable",
        Effect::ChangeFaction { .. } => "ChangeFaction",
        Effect::SetActorValue { .. } => "SetActorValue",
        Effect::EquipItem { .. } => "EquipItem",
        Effect::MoveTo { .. } => "MoveTo",
        Effect::MovePlayerTo { .. } => "MovePlayerTo",
        Effect::ChangePerk { .. } => "ChangePerk",
        Effect::ForceAlias { .. } => "ForceAlias",
        Effect::ModQuestVariable { .. } => "ModQuestVariable",
        Effect::SetActorValueNamed { .. } => "SetActorValueNamed",
        Effect::DamageActorValue { .. } => "DamageActorValue",
        Effect::BlockActivation { .. } => "BlockActivation",
        Effect::SendStoryEvent { .. } => "SendStoryEvent",
        Effect::AddToMap { .. } => "AddToMap",
        Effect::SetLocationCleared { .. } => "SetCleared",
        Effect::SetBaseProtection { .. } => "SetEssential",
        Effect::Kill { .. } => "Kill",
        Effect::QuestScope { .. } => "QuestScope",
        Effect::CopyQuestVariable { .. } => "CopyQuestVariable",
        Effect::SetRelationshipRank { .. } => "SetRelationshipRank",
        Effect::RewardPlayerXp { .. } => "RewardPlayerXP",
        Effect::IncrementStat { .. } => "IncrementStat",
        Effect::AddAchievement { .. } => "AddAchievement",
        Effect::AdvanceSkill { .. } => "AdvanceSkill",
        Effect::SetLinkedRef { .. } => "SetLinkedRef",
        Effect::SetLocRefType { .. } => "SetLocRefType",
        Effect::MoveToEditorLocation { .. } => "MoveToEditorLocation",
        Effect::SetLocationKeywordData { .. } => "SetLocationKeywordData",
        Effect::ForceLocationTo { .. } => "ForceLocationTo",
        Effect::SetPlayerTeammate { .. } => "SetPlayerTeammate",
        Effect::SetDoingFavor { .. } => "SetDoingFavor",
        Effect::PlayerCaps { .. } => "PlayerCaps",
        Effect::SetLocal { .. } => "SetLocal",
        Effect::SetReferenceVariable { .. } => "SetReferenceVariable",
        Effect::ShowMessage { .. } => "ShowMessage",
        Effect::ShowHelpMessage { .. } => "ShowHelpMessage",
        Effect::Notification { .. } => "Notification",
        Effect::ShowBarterMenu { .. } => "ShowBarterMenu",
        Effect::ChangeKeyword { .. } => "ChangeKeyword",
        Effect::SetGhost { .. } => "SetGhost",
        Effect::SetAlly { .. } => "SetAlly",
        Effect::DeleteWhenAble { .. } => "DeleteWhenAble",
        Effect::WaitWhile { .. } => "WaitWhile",
        Effect::Interpret { .. } => "Interpret",
        Effect::Disable { .. } => "Disable",
        Effect::Enable { .. } => "Enable",
        Effect::StartScene { .. } => "StartScene",
        Effect::StopScene { .. } => "StopScene",
        Effect::Activate { .. } => "Activate",
        Effect::SetOpen { .. } => "SetOpen",
        Effect::SetLocked { .. } => "SetLocked",
        Effect::SetLockLevel { .. } => "SetLockLevel",
        Effect::SetPlayerRestrained { .. } => "SetPlayerRestrained",
        Effect::SetPlayerControls { .. } => "SetPlayerControls",
        Effect::SetPlayerAiDriven { .. } => "SetPlayerAiDriven",
        Effect::SetHudCartMode { .. } => "SetHudCartMode",
        Effect::SetInChargen { .. } => "SetInChargen",
        Effect::ShowRaceMenu => "ShowRaceMenu",
        Effect::RequestSave { .. } => "RequestSave",
        Effect::SetEnemy { .. } => "SetEnemy",
        Effect::StartCombat { .. } => "StartCombat",
        Effect::StopCombat { .. } => "StopCombat",
        Effect::StopCombatAlarm { .. } => "StopCombatAlarm",
        Effect::SetCrimeFaction { .. } => "SetCrimeFaction",
        Effect::AddSpell { .. } => "AddSpell",
        Effect::RemoveSpell { .. } => "RemoveSpell",
        Effect::PlayIdle { .. } => "PlayIdle",
        Effect::SetVehicle { .. } => "SetVehicle",
        Effect::TetherToHorse { .. } => "TetherToHorse",
        Effect::SetMotionType { .. } => "SetMotionType",
        Effect::SetSittingRotation { .. } => "SetSittingRotation",
        Effect::ExitCart { .. } => "ExitCart",
        Effect::RegisterPlayerAnimationEvent { .. } => "RegisterPlayerAnimationEvent",
        Effect::EvaluatePackage { .. } => "EvaluatePackage",
        Effect::Wait { .. } => "Wait",
        Effect::WaitForActors3DLoaded { .. } => "WaitForActors3DLoaded",
        Effect::ProviderCall(_) => "ProviderCall",
        Effect::SetUnconscious { .. } => "SetUnconscious",
        Effect::AddRaceSpells { .. } => "AddRaceSpells",
        Effect::RemoveRaceSpells { .. } => "RemoveRaceSpells",
        }
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(
            self,
            Effect::ShowRaceMenu | Effect::RequestSave { .. } | Effect::SetHudCartMode { .. }
        )
    }
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
#[derive(Clone, Default)]
struct Scope<'lib> {
    /// M47.2 — where a call into the quest's own script (`kmyQuest.Startup()`)
    /// finds the callee's decompiled body to inline. `None` in every bare
    /// harness.
    library: Option<&'lib dyn ScriptLibrary>,
    /// M47.2 — whether the fragment's VMAD is its quest's, so that the
    /// quest script's properties resolve through it. True for quest fragments
    /// and inlined quest-script bodies. On the INFO and scene paths (an
    /// INFO's or SCEN's own VMAD) an inlined callee is wrapped in
    /// [`Effect::QuestScope`] instead, and a quest script's own property
    /// (`kmyQuest.CW`) declines.
    vmad_is_quest: bool,
    /// M47.2 — set while an INFO / scene statement is re-lowered against its
    /// quest's VMAD ([`quest_vmad_statement`]): the fragment's own properties
    /// (and locals bound from them) would resolve against the wrong VMAD
    /// there, so they decline.
    fragment_properties_off: bool,
    /// The declared object type (lowercased script name) of each local
    /// (`MyQuestScript kmyQuest`), for resolving an inlined call's callee.
    local_types: HashMap<String, String>,
    /// `(script, function)` frames currently being inlined — the cycle guard
    /// and depth cap of [`inline_quest_script_call`].
    inline_stack: Vec<(String, String)>,
    /// M47.2 — inside an inlined quest-script body, the lowercased names of
    /// that script's own variables and properties (its quest variables).
    script_variables: HashSet<String>,
    quest_locals: HashMap<String, QuestRef>,
    object_locals: HashMap<String, ObjectRef>,
    player_locals: HashSet<String>,
    decl_locals: HashSet<String>,
    /// M47.2 — locals (lowercased) holding a run-time number
    /// ([`Effect::SetLocal`]).
    runtime_locals: HashSet<String>,
    /// M47.2 — names (lowercased) that denote the dialogue line's speaker:
    /// an INFO fragment's `akSpeakerRef` parameter and every local bound
    /// from it (`akSpeaker = akSpeakerRef as Actor`). Empty outside INFO
    /// fragments.
    speaker_locals: HashSet<String>,
    /// Set when a receiver lowered to [`ActorRef::Speaker`], so the INFO
    /// entry point can refuse a speaker fragment with a latent tail. Shared
    /// (not copied) by the clones an `If` branch lowers against.
    uses_speaker: std::rc::Rc<std::cell::Cell<bool>>,
    /// #2538 / SCR-D5-NEW10-01 — lowercased names of the containing
    /// script's `Quest Property` declarations (from VMAD-equivalent
    /// property-type metadata, threaded in from `&Script` at
    /// `lower_fragment_with_quest_properties`'s entry point). Nothing in
    /// the AST shape alone distinguishes a bare `Quest Property`
    /// reference from a bare scene-form property — `Quest.Start()` and
    /// `Scene.Start()` produce the identical zero-arg method-call AST —
    /// so `receiver_object` (the resolver `prim_start_scene`/
    /// `prim_stop_scene` and friends share) declines any bare identifier
    /// that appears here, rather than letting a name we *know* is a
    /// `Quest` property fall through and get silently accepted as a
    /// scene reference. Empty (via `lower_fragment`'s thin wrapper) for
    /// every call site that doesn't have script-level property metadata
    /// available — matches the pre-fix behavior for those.
    known_quest_properties: HashSet<String>,
    /// #5407 — lowercased names of the containing script's own ARGLESS
    /// functions, threaded from `&Script` the way
    /// [`Self::known_quest_properties`] is. `AddRaceSpells` /
    /// `RemoveRaceSpells` are script-defined helpers (MQ101QuestScript
    /// authors both), not `actor.psc` natives: the race-spells
    /// primitives accept the call only when the calling script itself
    /// defines that name with zero parameters, so a mod's same-named
    /// helper with other semantics never lowers onto the player. Empty
    /// for call sites without script metadata — pre-fix behavior.
    own_argless_functions: HashSet<String>,
}

/// What a declared local currently means to the lowerer — the identity
/// [`Scope::rebound_by`] compares across a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalBinding {
    Quest(QuestRef),
    Object(ObjectRef),
    Player,
    Speaker,
    Plain,
}

impl Scope<'_> {
    /// The binding of a name declared in this function, or `None` when no
    /// `VarDecl`/`Assign` has bound it.
    fn binding_of(&self, name: &str) -> Option<LocalBinding> {
        if let Some(via) = self.quest_locals.get(name) {
            Some(LocalBinding::Quest(via.clone()))
        } else if let Some(via) = self.object_locals.get(name) {
            Some(LocalBinding::Object(via.clone()))
        } else if self.player_locals.contains(name) {
            Some(LocalBinding::Player)
        } else if self.speaker_locals.contains(name) {
            Some(LocalBinding::Speaker)
        } else if self.decl_locals.contains(name) {
            Some(LocalBinding::Plain)
        } else {
            None
        }
    }

    /// Whether `name` is a local declared by this function (as opposed to a
    /// script variable or property).
    fn declares(&self, name: &str) -> bool {
        self.binding_of(name).is_some()
    }

    /// Whether `branch` — a clone of `self` that a nested body was lowered
    /// against — rebinds any name `self` already declares.
    fn rebound_by(&self, branch: &Scope) -> bool {
        let names = self
            .quest_locals
            .keys()
            .chain(self.object_locals.keys())
            .chain(self.player_locals.iter())
            .chain(self.speaker_locals.iter())
            .chain(self.decl_locals.iter());
        names.into_iter().any(|name| self.binding_of(name) != branch.binding_of(name))
    }
}

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
fn inline_quest_script_call(
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
        script_variables: script
            .body
            .iter()
            .filter_map(|item| match &item.node {
                byroredux_papyrus::ast::ScriptItem::Variable(v) => {
                    Some(v.name.node.0.to_ascii_lowercase())
                }
                byroredux_papyrus::ast::ScriptItem::Property(p) => {
                    Some(p.name.node.0.to_ascii_lowercase())
                }
                _ => None,
            })
            .collect(),
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
fn quest_vmad_statement(
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
fn quest_vmad_scope<'lib>(scope: &Scope<'lib>) -> Option<Scope<'lib>> {
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

fn has_latent_tail(effect: &Effect) -> bool {
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
const MAX_CONDITIONAL_DEPTH: u32 = byroredux_papyrus::parser::stmt::MAX_STMT_DEPTH;

fn lower_statements(
    body: &[Spanned<Stmt>],
    scope: &mut Scope,
    providers: Option<&PapyrusProviderCatalog>,
    depth: u32,
) -> Option<Vec<Effect>> {
    if depth >= MAX_CONDITIONAL_DEPTH {
        return None;
    }
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
fn cast_to_quest_script(object: &Expr, scope: &Scope) -> bool {
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
fn local_slot(scope: &Scope, name: &str) -> String {
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

/// Classify a single effect statement against the primitive table.
/// M47.2 — where a Papyrus VM run suspended by `Utility.Wait` resumes: per
/// suspended frame, the path of block positions down to the wait or the call
/// it waits in (each block's statement index, and for an `If` / `While` the
/// branch the run was in), and the frame's locals.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct VmResume {
    /// The suspended frames, outermost (the fragment) first; each caller's
    /// path ends at the call statement its callee suspended in.
    pub frames: Vec<VmFrame>,
    /// Statements the logical run has already executed when the frame budget
    /// (not a `Utility.Wait`) suspended it, so the per-run statement cap
    /// still bounds the whole run; 0 after a real wait, which starts the
    /// count afresh as it always did.
    pub carried_steps: u32,
}

/// One suspended function activation of a [`VmResume`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct VmFrame {
    /// The script attached to `owner` (its variables and properties).
    pub script: String,
    pub function: String,
    /// The object the script is attached to (`Self`): a quest or a
    /// reference.
    pub owner: u32,
    /// The script that defines the running body — `script` or one it
    /// extends — and the state block it is in (`None`: the empty state).
    /// The body is kept across the wait even when the state changes.
    pub defined_in: String,
    pub state: Option<String>,
    pub path: Vec<VmStep>,
    pub locals: Vec<(String, VmStored)>,
}

/// One position on a [`VmResume`] path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum VmStep {
    /// The statement at this index of the current block.
    Statement(u32),
    /// Inside an `If` (0 its body, `k` its `k`-th `ElseIf`, `u32::MAX` its
    /// `Else`) or a `While` (0, its body).
    Branch(u32),
}

/// A local's value across a suspension.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum VmStored {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Form(u32),
    Alias { quest: u32, alias: i16 },
    /// An array an event passes (`Var[] akArgs`): its elements, each scalar.
    Array(Vec<VmStored>),
}

/// M47.2 — how the Papyrus VM ([`crate::vm`]) names the values it binds for
/// one native call or read it hands to this module's primitives. Every value
/// is a synthetic identifier: a quest or a player through these sets, any
/// other form as a property of the synthetic VMAD the VM applies the result
/// against; numbers, Bools and strings stay literals.
#[derive(Debug, Default, Clone)]
pub struct NativeBindings {
    pub players: HashSet<String>,
    pub quests: HashSet<String>,
}

impl NativeBindings {
    fn scope(&self) -> Scope<'static> {
        let mut scope = Scope {
            vmad_is_quest: true,
            ..Scope::default()
        };
        for name in &self.players {
            scope.player_locals.insert(name.to_ascii_lowercase());
        }
        for name in &self.quests {
            scope
                .quest_locals
                .insert(name.to_ascii_lowercase(), QuestRef::Property(name.clone()));
        }
        scope
    }
}

/// M47.2 — a native statement call with VM-bound values as an [`Effect`].
pub fn classify_native_effect(e: &Expr, bindings: &NativeBindings) -> Option<Effect> {
    let scope = bindings.scope();
    classify_effect(e, &scope)
}

/// M47.2 — a native numeric read with VM-bound values as a [`ValueExpr`].
pub fn classify_native_value(e: &Expr, bindings: &NativeBindings) -> Option<ValueExpr> {
    classify_value(e, &bindings.scope())
}

/// M47.2 — a native Bool read with VM-bound values as guards that hold
/// exactly when it is true.
pub fn classify_native_guard(e: &Expr, bindings: &NativeBindings) -> Option<Vec<FragmentGuard>> {
    let mut guards = Vec::new();
    classify_condition(e, &bindings.scope(), false, &mut guards)?;
    Some(guards)
}

fn classify_effect(e: &Expr, scope: &Scope) -> Option<Effect> {
    EFFECT_PRIMITIVES.iter().find_map(|p| p(e, scope))
}

fn classify_effect_with_providers(
    expression: &Expr,
    scope: &Scope,
    providers: Option<&PapyrusProviderCatalog>,
) -> Option<Effect> {
    // #3934 — the canonical effect table is consulted FIRST, and a manifest
    // provider alias only ever gets a statement no primitive claimed.
    //
    // The reverse order let one installed extension change how vanilla
    // content lowers, two ways. `PapyrusProviderCatalog::insert` records the
    // *provider identifier* of every strictly-inserted alias, and
    // `lower_provider_call` hard-errors (`UnknownFunction`) on a miss whose
    // identifier is any known provider — so a single alias published under
    // the reserved `Self` receiver (which the SDK prescribes for instance
    // methods), or under `Game`/`Utility`, made every `Self.SetStage(..)`,
    // `Game.*` or `Utility.Wait(..)` in the corpus decline its whole
    // fragment. And an alias spelled exactly like a modeled effect silently
    // *replaced* it with a deferred host barrier — for `Utility.Wait` that
    // also slipped past `has_latent`, which only matches `Effect::Wait`.
    // Primitives-first makes both impossible: canonical semantics win, and
    // an alias can only ever add a spelling the engine does not model.
    if let Some(effect) = classify_effect(expression, scope) {
        return Some(effect);
    }
    let providers = providers?;
    match lower_provider_call(expression, providers) {
        Ok(Some(call)) => Some(Effect::ProviderCall(FragmentProviderCall {
            route: call.route.qualified_name().to_owned(),
            arguments: call.arguments,
            principal: None,
        })),
        // Unclaimed by both tables, or a bound-but-unresolvable extender
        // call: decline the fragment, same as any unmodeled statement.
        Ok(None) | Err(_) => None,
    }
}

/// An effect primitive: matches one effect-call shape and binds its
/// holes (resolving the receiver to a [`QuestRef`] via `scope`), or
/// declines. Internal — the public surface is [`lower_fragment`].
type EffectPrimitive = fn(&Expr, &Scope) -> Option<Effect>;

/// The effect-primitive table. First match wins.
const EFFECT_PRIMITIVES: &[EffectPrimitive] = &[
    prim_set_global_value,
    prim_set_stage,
    prim_start_quest,
    prim_stop_quest,
    prim_complete_quest,
    prim_reset_quest,
    prim_set_quest_active,
    prim_set_objective_displayed,
    prim_set_objective_completed,
    prim_set_objective_failed,
    prim_complete_all_objectives,
    prim_fail_all_objectives,
    prim_add_item,
    prim_remove_item,
    prim_change_faction,
    prim_set_actor_value,
    prim_change_perk,
    prim_force_alias,
    prim_block_activation,
    prim_send_story_event,
    prim_add_to_map,
    prim_set_location_cleared,
    prim_set_base_protection,
    prim_kill,
    prim_set_relationship_rank,
    prim_show_message,
    prim_notification,
    prim_show_barter_menu,
    prim_delete_when_able,
    prim_change_keyword,
    prim_reward_player_xp,
    prim_increment_stat,
    prim_add_achievement,
    prim_set_linked_ref,
    prim_set_loc_ref_type,
    prim_damage_actor_value,
    prim_advance_skill,
    prim_set_player_teammate,
    prim_set_doing_favor,
    prim_player_caps,
    prim_set_objective_skipped,
    prim_equip_item,
    prim_move_to,
    prim_move_to_editor_location,
    prim_force_location_to,
    prim_set_location_keyword_data,
    prim_disable,
    prim_enable,
    prim_start_scene,
    prim_stop_scene,
    prim_activate,
    prim_set_open,
    prim_lock,
    prim_set_lock_level,
    prim_set_player_restrained,
    prim_disable_player_controls,
    prim_enable_player_controls,
    prim_set_player_ai_driven,
    prim_set_hud_cart_mode,
    prim_set_in_chargen,
    prim_show_race_menu,
    prim_request_save,
    prim_request_auto_save,
    prim_set_enemy,
    prim_set_ally,
    prim_start_combat,
    prim_stop_combat,
    prim_stop_combat_alarm,
    prim_set_crime_faction,
    prim_set_unconscious,
    prim_set_ghost,
    prim_add_spell,
    prim_remove_spell,
    prim_add_race_spells,
    prim_remove_race_spells,
    prim_play_idle,
    prim_set_vehicle,
    prim_tether_to_horse,
    prim_set_motion_type,
    prim_set_sitting_rotation,
    prim_exit_cart,
    prim_player_imod_animation,
    prim_player_furniture_animation,
    prim_evaluate_package,
    prim_wait,
];

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

// ── Effect primitives ────────────────────────────────────────────────

/// `ObjectReference.MoveTo`'s full parameter count: `akTarget` plus three
/// float offsets plus `abMatchRotation` is five on Skyrim/FO4, and
/// Starfield's `abRotateOffset` makes six. Anything longer is not a shape
/// any shipped game's compiler can emit (#3487).
const MOVE_TO_MAX_ARGS: usize = 6;

/// `Game.DisablePlayerControls`/`EnablePlayerControls` take nine
/// parameters, and exist only on Skyrim — see [`prim_player_controls`]
/// for why this is not the FO4/Starfield count (#3487).
const SKYRIM_PLAYER_CONTROLS_ARGS: usize = 9;

/// `GlobalVariable.SetValue(afValue)` / `SetValueInt(aiValue)` with a
/// run-time number — a literal, `S.GetValue() + 1.0` (the radio-ready and
/// counter idioms), any [`ValueExpr`] — and `G.Mod(afHowMuch)`, which the CK
/// defines as adding to the value.
fn prim_set_global_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    if let Some((object, args)) = method_call(e, "Mod") {
        if args.len() != 1 {
            return None;
        }
        let global = receiver_object(object, scope)?;
        return Some(Effect::SetGlobalValue {
            value: ValueExpr::Arith(
                Box::new(ValueExpr::Global(global.clone())),
                ArithOp::Add,
                Box::new(classify_value(&args[0].value.node, scope)?),
            ),
            global,
        });
    }
    let (object, args) = method_call(e, "SetValue").or_else(|| method_call(e, "SetValueInt"))?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::SetGlobalValue {
        global: receiver_object(object, scope)?,
        value: classify_value(&args[0].value.node, scope)?,
    })
}

/// `Quest.SetStage(int aiStage)` — one parameter in every shipped game.
///
/// #3496 — this and the three objective primitives were the only four of
/// 31 with no upper argument-count bound, so an over-arity call lowered
/// silently instead of declining. `SetStage` is the highest-traffic
/// effect in the domain (20,322 real calls, all one-argument), so it is
/// also the one whose false-positive lowering has the largest blast
/// radius: `SomeQuest.SetStage(10, <unmodeled term>)` became a plain
/// `SetStage { stage: 10 }`. Not reachable from vanilla content — the
/// compiler emits exactly the declared arity — but a modded `.pex` is
/// untrusted input that reaches this code.
fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetStage")?;
    if args.len() > 1 {
        return None;
    }
    let stage = u16::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetStage {
        quest: receiver_quest(object, scope)?,
        stage,
    })
}

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
fn explicit_quest_receiver(object: &Expr, scope: &Scope) -> Option<QuestRef> {
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

fn prim_start_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

fn prim_stop_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

fn prim_complete_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "CompleteQuest")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::CompleteQuest {
        quest: receiver_quest(object, scope)?,
    })
}

/// `Reset` is declared on `Quest`, `ObjectReference` *and* `Cell`
/// (SCR-D5-NEW11-02 / #2653), so the receiver must be unambiguously a
/// quest — a bare `ObjectReference Property` would otherwise be claimed
/// here and its real `Reset()` semantics silently dropped.
fn prim_reset_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Reset")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::ResetQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

/// `SetActive` is declared on `Quest` *and* `Weather`
/// (SCR-D5-NEW11-02 / #2653) — same narrow-receiver requirement as
/// [`prim_reset_quest`].
fn prim_set_quest_active(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetActive")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetQuestActive {
        quest: explicit_quest_receiver(object, scope)?,
        active: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Quest.SetObjectiveDisplayed(int aiObjective, bool abDisplayed = true,
/// bool abForce = false)` — three parameters (#3496).
fn prim_set_objective_displayed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveDisplayed")?;
    if args.len() > 3 {
        return None;
    }
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    // Optional 2nd arg `abDisplayed` defaults to true in Papyrus.
    let displayed = bool_arg(args, 1)?.unwrap_or(true);
    // #5652 — the optional 3rd arg `abForce` (redisplay an already-shown
    // objective) must be a literal, the `AddItem`-`abSilent` discipline:
    // accepting the arity bound while never reading the slot silently
    // dropped a non-literal or side-effecting 3rd expression. Not
    // applied — the stored objective state is the same either way.
    bool_arg(args, 2)?;
    Some(Effect::SetObjectiveDisplayed {
        quest: receiver_quest(object, scope)?,
        objective,
        displayed,
    })
}

/// `Quest.SetObjectiveCompleted(int aiObjective, bool abCompleted =
/// true)` — two parameters (#3496).
fn prim_set_objective_completed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveCompleted")?;
    if args.len() > 2 {
        return None;
    }
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    let completed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveCompleted {
        quest: receiver_quest(object, scope)?,
        objective,
        completed,
    })
}

/// `Quest.SetObjectiveFailed(int aiObjective, bool abFailed = true)` —
/// two parameters (#3496).
fn prim_set_objective_failed(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveFailed")?;
    if args.len() > 2 {
        return None;
    }
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    let failed = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetObjectiveFailed {
        quest: receiver_quest(object, scope)?,
        objective,
        failed,
    })
}

fn prim_complete_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "CompleteAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::CompleteAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

fn prim_fail_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "FailAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::FailAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

/// How many items an `AddItem` / `RemoveItem` moves (M47.2): a literal, or a
/// global's value read when the effect applies — `<global>.GetValueInt()`,
/// `<global>.GetValue() as Int` or `<global>.Value as Int`, the spellings
/// that carry ~210 vanilla Skyrim / FO4 / Starfield price and ransom counts.
/// The read truncates toward zero, as Papyrus's `Float` → `Int` cast does; a
/// negative value skips the effect.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ItemCount {
    Literal(u32),
    Global(ObjectRef),
    /// M47.2 — a number computed at run time (`aiCount * 2`,
    /// `Amount.GetValueInt() * Reward.GetValueInt()`).
    Value(ValueExpr),
}

/// Positional count argument `idx` (default `default` when absent). See
/// [`ItemCount`].
fn item_count_arg(
    args: &[byroredux_papyrus::ast::CallArg],
    idx: usize,
    default: u32,
    scope: &Scope,
) -> Option<ItemCount> {
    let Some(arg) = args.get(idx) else {
        return Some(ItemCount::Literal(default));
    };
    if let Some(value) = int_arg(args, idx) {
        // A negative literal declines.
        return u32::try_from(value).ok().map(ItemCount::Literal);
    }
    if as_num(&arg.value.node).is_some() {
        // A fractional or negative literal is not a count.
        return None;
    }
    global_value_read(&arg.value.node, scope)
        .map(ItemCount::Global)
        .or_else(|| classify_value(&arg.value.node, scope).map(ItemCount::Value))
}

/// A GlobalVariable named directly (`Global`) or through a property of the
/// fragment's own quest script (`kmyQuest.Global`) — for a quest fragment
/// the quest's `VMAD` carries that script's properties too, so both resolve
/// the same way at runtime.
fn global_property(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
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

/// The global a value expression reads: `<global>.GetValue()`,
/// `<global>.GetValueInt()` or `<global>.Value`, under any casts.
fn global_value_read(expr: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let mut node = expr;
    while let Expr::Cast { expr, .. } = node {
        node = &expr.node;
    }
    for name in ["GetValueInt", "GetValue"] {
        if let Some((object, args)) = method_call(node, name) {
            return args
                .is_empty()
                .then(|| global_property(object, scope))
                .flatten();
        }
    }
    match node {
        Expr::MemberAccess { object, member } if member.node.0.eq_ignore_ascii_case("value") => {
            global_property(&object.node, scope)
        }
        _ => None,
    }
}

/// M47.2 — `Game.GetCaps()` (FO4's `Game.psc`: `Return GetForm(15) as
/// MiscObject`), cast or not.
fn is_get_caps(e: &Expr) -> bool {
    let mut e = e;
    while let Expr::Cast { expr, .. } = e {
        e = &expr.node;
    }
    static_call(e, "Game", "GetCaps").is_some_and(|args| args.is_empty())
}

/// M47.2 — the player gaining or losing a literal count of `Game.GetCaps()`
/// is [`Effect::PlayerCaps`], the effect `GivePlayerCaps` /
/// `RemovePlayerCaps` lower to.
fn player_caps_transfer(
    container: &ActorRef,
    item: &Expr,
    count: &ItemCount,
    sign: i32,
) -> Option<Effect> {
    if !matches!(container, ActorRef::Player) || !is_get_caps(item) {
        return None;
    }
    let ItemCount::Literal(count) = count else {
        return None;
    };
    Some(Effect::PlayerCaps {
        delta: sign * i32::try_from(*count).ok()?,
    })
}

fn prim_add_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddItem")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    let container = receiver_actor(object, scope)?;
    if let Some(caps) = item_count_arg(args, 1, 1, scope).and_then(|count| {
        player_caps_transfer(&container, &args[0].value.node, &count, 1)
    }) {
        bool_arg(args, 2)?;
        return Some(caps);
    }
    let item = receiver_object(&args.first()?.value.node, scope)?;
    let count = item_count_arg(args, 1, 1, scope)?;
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

/// `RemoveItem(akItemToRemove, aiCount = 1, abSilent = false,
/// akOtherContainer = None)`. The count must be a non-negative literal; the
/// other container a literal `None` or a resolvable reference.
fn prim_remove_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "RemoveItem")?;
    if args.is_empty() || args.len() > 4 {
        return None;
    }
    let container = receiver_actor(object, scope)?;
    let no_destination = matches!(args.get(3).map(|arg| &arg.value.node), None | Some(Expr::NoneLit));
    if let Some(caps) = item_count_arg(args, 1, 1, scope)
        .filter(|_| no_destination)
        .and_then(|count| player_caps_transfer(&container, &args[0].value.node, &count, -1))
    {
        bool_arg(args, 2)?;
        return Some(caps);
    }
    let item = receiver_object(&args[0].value.node, scope)?;
    let count = item_count_arg(args, 1, 1, scope)?;
    bool_arg(args, 2)?;
    let destination = match args.get(3).map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(other) => Some(receiver_actor(other, scope)?),
    };
    Some(Effect::RemoveItem {
        container,
        item,
        count,
        destination,
    })
}

/// `AddToFaction(akFaction)` / `RemoveFromFaction(akFaction)` /
/// `SetFactionRank(akFaction, aiRank)` with a literal rank in `i8` range.
fn prim_change_faction(e: &Expr, scope: &Scope) -> Option<Effect> {
    // `RemoveFromAllFactions()` names no faction; the effect's faction is
    // unused.
    if let Some((object, args)) = method_call(e, "RemoveFromAllFactions") {
        if !args.is_empty() {
            return None;
        }
        return Some(Effect::ChangeFaction {
            actor: receiver_actor(object, scope)?,
            faction: ObjectRef::Property(String::new()),
            change: crate::FactionChange::RemoveAll,
        });
    }
    let (object, args, change) = if let Some((object, args)) = method_call(e, "AddToFaction") {
        (object, args, crate::FactionChange::Add)
    } else if let Some((object, args)) = method_call(e, "RemoveFromFaction") {
        (object, args, crate::FactionChange::Remove)
    } else if let Some((object, args)) = method_call(e, "SetFactionRank") {
        if args.len() != 2 {
            return None;
        }
        let rank = i8::try_from(int_arg(args, 1).or_else(|| {
            // `-1` reaches the AST as a negated literal.
            as_num(&args[1].value.node).filter(|n| n.fract() == 0.0).map(|n| n as i64)
        })?)
        .ok()?;
        (object, &args[..1], crate::FactionChange::SetRank(rank))
    } else {
        return None;
    };
    if args.len() != 1 {
        return None;
    }
    Some(Effect::ChangeFaction {
        actor: receiver_actor(object, scope)?,
        faction: receiver_object(&args[0].value.node, scope)?,
        change,
    })
}

/// `<actor>.SetValue(akAV, afValue)` — the two-argument form is the actor
/// value setter (`GlobalVariable.SetValue` takes one argument and is
/// [`prim_set_global_value`]'s). The value must be a numeric literal.
/// `ObjectReference.AddToMap(bool abFastTravel = False)`.
fn prim_add_to_map(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddToMap")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::AddToMap {
        marker: receiver_object(object, scope)?,
        fast_travel: bool_arg(args, 0)?.unwrap_or(false),
    })
}

/// `Message.Show(float afArg1 = 0.0, … afArg9 = 0.0)` (nine literal
/// numbers at most) and `Message.ShowAsHelpMessage(string asEvent, float
/// afDuration, float afInterval, int aiMaxTimes[, string asContext = "", int
/// aiPriority = 0, Message GamepadMsg = None])` with literal arguments. The context restricts a message
/// to a menu ("Pass 'NoMenu' for this message only to appear when the game is
/// not in menu mode"); only the unrestricted `""` and `"NoMenu"` are shown
/// here, any menu name declines. The priority only orders competing messages.
fn prim_show_message(e: &Expr, scope: &Scope) -> Option<Effect> {
    if let Some((object, args)) = method_call(e, "ShowAsHelpMessage") {
        if !(4..=7).contains(&args.len()) || !matches!(args[0].value.node, Expr::StringLit(_)) {
            return None;
        }
        for arg in &args[1..4] {
            as_num(&arg.value.node)?;
        }
        if let Some(context) = args.get(4) {
            match &context.value.node {
                Expr::StringLit(text) if text.is_empty() || text.eq_ignore_ascii_case("NoMenu") => {}
                _ => return None,
            }
        }
        if let Some(priority) = args.get(5) {
            as_num(&priority.value.node)?;
        }
        // Starfield's `GamepadMsg`: the text shown instead on a gamepad. The
        // keyboard message is the one shown here.
        if let Some(gamepad) = args.get(6) {
            if !matches!(gamepad.value.node, Expr::NoneLit) {
                receiver_object(&gamepad.value.node, scope)?;
            }
        }
        return Some(Effect::ShowHelpMessage {
            message: receiver_object(object, scope)?,
        });
    }
    let (object, args) = method_call(e, "Show")?;
    if args.len() > 9 {
        return None;
    }
    let args = args
        .iter()
        .map(|arg| as_num(&arg.value.node))
        .collect::<Option<Vec<f32>>>()?;
    Some(Effect::ShowMessage {
        message: receiver_object(object, scope)?,
        args,
    })
}

/// `Debug.Notification(string asNotificationText)` with a literal text.
fn prim_notification(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Debug", "Notification")?;
    if args.len() != 1 {
        return None;
    }
    let Expr::StringLit(text) = &args[0].value.node else {
        return None;
    };
    Some(Effect::Notification { text: text.clone() })
}

/// `ObjectReference.AddKeyword(apKeyword)` / `RemoveKeyword(apKeyword)` /
/// FO4's `ResetKeyword(apKeyword)`.
fn prim_change_keyword(e: &Expr, scope: &Scope) -> Option<Effect> {
    use crate::keywords::KeywordChange;
    let (object, args, change) = [
        ("AddKeyword", KeywordChange::Add),
        ("RemoveKeyword", KeywordChange::Remove),
        ("ResetKeyword", KeywordChange::Reset),
    ]
    .into_iter()
    .find_map(|(method, change)| method_call(e, method).map(|(o, a)| (o, a, change)))?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::ChangeKeyword {
        object: receiver_actor(object, scope)?,
        keyword: receiver_object(&args[0].value.node, scope)?,
        change,
    })
}

/// `ObjectReference.DeleteWhenAble()` (Skyrim: a script loop over
/// `GetParentCell().IsAttached()` then `Delete()`; FO4 / Starfield native)
/// and `Delete()`, which the CK says "flags this object reference as wanting
/// to be deleted" once nothing holds it — a created reference's deletion
/// either way, an authored one cannot be deleted.
fn prim_delete_when_able(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["DeleteWhenAble", "Delete"])?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::DeleteWhenAble {
        object: receiver_object(object, scope)?,
    })
}

/// `Actor.ShowBarterMenu()` — the same zero-argument native on Skyrim,
/// Fallout 4 and Starfield.
fn prim_show_barter_menu(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ShowBarterMenu")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::ShowBarterMenu {
        vendor: receiver_actor(object, scope)?,
    })
}

/// `Game.RewardPlayerXP(int auiXPAmount, bool abDirect = false)` with a
/// literal amount.
fn prim_reward_player_xp(e: &Expr, scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "RewardPlayerXP")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    // A literal, or the `XPMiscQuestSmall.GetValue() as Int` global FO4
    // quests reward.
    Some(Effect::RewardPlayerXp {
        amount: item_count_arg(args, 0, 0, scope)?,
        direct: bool_arg(args, 1)?.unwrap_or(false),
    })
}

/// `Actor.SetPlayerTeammate(bool abTeammate = true, bool abCanDoFavor =
/// true)` with literal arguments.
fn prim_set_player_teammate(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetPlayerTeammate")?;
    if args.len() > 2 {
        return None;
    }
    Some(Effect::SetPlayerTeammate {
        actor: receiver_actor(object, scope)?,
        teammate: bool_arg(args, 0)?.unwrap_or(true),
        can_do_favor: bool_arg(args, 1)?.unwrap_or(true),
    })
}

/// `Actor.SetDoingFavor(bool abDoingFavor = true)` with a literal argument.
fn prim_set_doing_favor(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetDoingFavor")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetDoingFavor {
        actor: receiver_actor(object, scope)?,
        doing: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `ObjectReference.SetLinkedRef(apRef, apKeyword = None)`; `None` for the
/// target or the keyword is written `None`. Starfield's signature adds
/// `abPromoteParentRefr`: it promotes a non-persistent parent reference so
/// the link survives, which [`crate::LinkedRefOverrides`] already does by
/// keying the saved link on FormIDs, so the flag changes nothing here.
fn prim_set_linked_ref(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLinkedRef")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    if let Some(promote) = args.get(2) {
        if !matches!(promote.value.node, Expr::BoolLit(_)) {
            return None;
        }
    }
    let optional = |index: usize| -> Option<Option<ObjectRef>> {
        match args.get(index).map(|arg| &arg.value.node) {
            None | Some(Expr::NoneLit) => Some(None),
            Some(other) => receiver_object(other, scope).map(Some),
        }
    };
    Some(Effect::SetLinkedRef {
        object: receiver_object(object, scope)?,
        target: optional(0)?,
        keyword: optional(1)?,
    })
}

/// `<reference>.SetLocRefType(akLoc, akRefType)` (CK *SetLocRefType -
/// ObjectReference*); the location is a reference to a `Location` or `None`.
fn prim_set_loc_ref_type(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLocRefType")?;
    if args.len() != 2 {
        return None;
    }
    if !matches!(args[0].value.node, Expr::NoneLit) {
        receiver_object(&args[0].value.node, scope)?;
    }
    let ref_type = match &args[1].value.node {
        Expr::NoneLit => None,
        other => Some(receiver_object(other, scope)?),
    };
    Some(Effect::SetLocRefType {
        object: receiver_object(object, scope)?,
        ref_type,
    })
}

/// `Game.AdvanceSkill(string asSkillName, float afMagnitude)` with literal
/// arguments.
fn prim_advance_skill(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "AdvanceSkill")?;
    if args.len() != 2 {
        return None;
    }
    let Expr::StringLit(skill) = &args[0].value.node else {
        return None;
    };
    Some(Effect::AdvanceSkill {
        skill: skill.clone(),
        magnitude: as_num(&args[1].value.node)?,
    })
}

/// `Game.AddAchievement(int aiAchievementID)` with a literal ID.
fn prim_add_achievement(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "AddAchievement")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::AddAchievement {
        id: i32::try_from(int_arg(args, 0)?).ok()?,
    })
}

/// `Game.IncrementStat(string asStatName, int aiModAmount = 1)` with literal
/// arguments.
fn prim_increment_stat(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "IncrementStat")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let Expr::StringLit(stat) = &args[0].value.node else {
        return None;
    };
    let amount = match args.get(1) {
        None => 1,
        Some(_) => i32::try_from(int_arg(args, 1)?).ok()?,
    };
    Some(Effect::IncrementStat {
        stat: stat.clone(),
        amount,
    })
}

/// `Game.GivePlayerCaps(int nCaps)` / `Game.RemovePlayerCaps(int nCaps)` with
/// a literal count; FO4's `Game.psc` bodies act only when `nCaps > 0`.
fn prim_player_caps(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let (args, sign) = match static_call(e, "Game", "GivePlayerCaps") {
        Some(args) => (args, 1),
        None => (static_call(e, "Game", "RemovePlayerCaps")?, -1),
    };
    if args.len() != 1 {
        return None;
    }
    let count = i32::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::PlayerCaps {
        delta: sign * count.max(0),
    })
}

/// `Quest.SetObjectiveSkipped(int aiObjectiveID)` — FO4's `Quest.psc` body:
/// `If !IsObjectiveCompleted(id) SetObjectiveDisplayed(id, false, false)`.
fn prim_set_objective_skipped(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetObjectiveSkipped")?;
    if args.len() != 1 {
        return None;
    }
    let objective = i32::try_from(int_arg(args, 0)?).ok()?;
    let quest = receiver_quest(object, scope)?;
    Some(Effect::Conditional {
        guards: vec![FragmentGuard::ObjectiveCompleted {
            quest: quest.clone(),
            objective,
            expected: false,
        }],
        then_effects: vec![Effect::SetObjectiveDisplayed {
            quest,
            objective,
            displayed: false,
        }],
        else_effects: vec![],
    })
}

/// `Actor.SetRelationshipRank(Actor akOther, int aiRank)` with a literal rank
/// in the CK's −4 … +4.
fn prim_set_relationship_rank(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetRelationshipRank")?;
    if args.len() != 2 {
        return None;
    }
    let (other, rank) = (&args[0], &args[1]);
    let mut other = &other.value.node;
    while let Expr::Cast { expr, .. } = other {
        other = &expr.node;
    }
    let rank = int_arg(args, 1).or_else(|| {
        // `-1` reaches the AST as a negated literal.
        as_num(&rank.value.node)
            .filter(|n| n.fract() == 0.0)
            .map(|n| n as i64)
    })?;
    if !(-4..=4).contains(&rank) {
        return None;
    }
    Some(Effect::SetRelationshipRank {
        actor: receiver_actor(object, scope)?,
        other: receiver_actor(other, scope)?,
        rank: rank as i8,
    })
}

/// `Actor.Kill(Actor akKiller = None)` and `KillSilent(Actor akKiller = None)`.
/// A player victim declines: the player's death is not a quest effect the
/// runtime models.
fn prim_kill(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, silent) = match method_call(e, "Kill") {
        Some((object, args)) => (object, args, false),
        None => {
            let (object, args) = method_call(e, "KillSilent")?;
            (object, args, true)
        }
    };
    if args.len() > 1 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    let killer = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(killer) => Some(receiver_actor(killer, scope)?),
    };
    Some(Effect::Kill { actor, killer, silent })
}

/// `ActorBase.SetEssential(bool abEssential = true)` /
/// `SetProtected(bool abProtected = true)`, on `<actor>.GetActorBase()` or an
/// ActorBase property.
fn prim_set_base_protection(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, protected) = match method_call(e, "SetEssential") {
        Some((object, args)) => (object, args, false),
        None => {
            let (object, args) = method_call(e, "SetProtected")?;
            (object, args, true)
        }
    };
    if args.len() > 1 {
        return None;
    }
    let base = match method_call(object, "GetActorBase") {
        Some((actor, [])) => {
            ActorBaseRef::OfActor(receiver_actor(actor, scope)?)
        }
        Some(_) => return None,
        None => ActorBaseRef::Form(receiver_object(object, scope)?),
    };
    Some(Effect::SetBaseProtection {
        base,
        protected,
        value: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Location.SetCleared(bool abCleared = true)`.
fn prim_set_location_cleared(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetCleared")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetLocationCleared {
        location: receiver_location(object, scope)?,
        cleared: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Keyword.SendStoryEvent(Location akLoc = None, ObjectReference akRef1 =
/// None, ObjectReference akRef2 = None, int aiValue1 = 0, int aiValue2 = 0)`
/// — every parameter materialized by the compiler. A `None` literal leaves a
/// slot empty; the integers must be literals.
fn prim_send_story_event(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SendStoryEvent")?;
    if args.len() > 5 || !matches!(object, Expr::Ident(_)) {
        return None;
    }
    let unwrap = |index: usize| {
        let mut node = &args.get(index)?.value.node;
        while let Expr::Cast { expr, .. } = node {
            node = &expr.node;
        }
        Some(node)
    };
    let location = match unwrap(0) {
        None | Some(Expr::NoneLit) => None,
        Some(node) => Some(receiver_location(node, scope)?),
    };
    let mut references = [None, None];
    for (slot, index) in references.iter_mut().zip([1, 2]) {
        *slot = match unwrap(index) {
            None | Some(Expr::NoneLit) => None,
            Some(node) => Some(receiver_actor(node, scope)?),
        };
    }
    let value = |index: usize| -> Option<i32> {
        match args.get(index) {
            None => Some(0),
            Some(_) => i32::try_from(int_arg(args, index)?).ok(),
        }
    };
    Some(Effect::SendStoryEvent {
        keyword: receiver_object(object, scope)?,
        location,
        references,
        values: [value(3)?, value(4)?],
    })
}

/// `BlockActivation(abBlocked = true)` (Skyrim); FO4 / Starfield add
/// `abHideActivateText = false`, a prompt flag accepted as a literal. The
/// receiver is the reference (an alias getter resolves through it).
fn prim_block_activation(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "BlockActivation")?;
    if args.len() > 2 {
        return None;
    }
    bool_arg(args, 1)?;
    Some(Effect::BlockActivation {
        object: receiver_object(object, scope)?,
        blocked: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `<alias>.ForceRefTo(akRef)` / `<alias>.Clear()` — the CK's *ReferenceAlias*
/// (and *Alias*) script functions. The receiver is the alias property itself.
fn prim_force_alias(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, target) = if let Some((object, args)) = method_call(e, "ForceRefTo") {
        if args.len() != 1 {
            return None;
        }
        let mut target = &args[0].value.node;
        while let Expr::Cast { expr, .. } = target {
            target = &expr.node;
        }
        (object, Some(receiver_actor(target, scope)?))
    } else {
        let (object, args) = method_call(e, "Clear")?;
        if !args.is_empty() {
            return None;
        }
        (object, None)
    };
    if !matches!(object, Expr::Ident(_)) {
        return None;
    }
    Some(Effect::ForceAlias {
        alias: receiver_object(object, scope)?,
        target,
    })
}

/// `<location alias>.ForceLocationTo(akLocation)`.
fn prim_force_location_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ForceLocationTo")?;
    if args.len() != 1 || !matches!(object, Expr::Ident(_)) {
        return None;
    }
    Some(Effect::ForceLocationTo {
        alias: receiver_object(object, scope)?,
        location: receiver_object(&args[0].value.node, scope)?,
    })
}

/// `AddPerk(akPerk)` (Skyrim) / `AddPerk(akPerk, abNotify = false)` (FO4,
/// Starfield) and `RemovePerk(akPerk)`. The notify flag is a literal-only UI
/// hint.
fn prim_change_perk(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, add) = if let Some((object, args)) = method_call(e, "AddPerk") {
        (object, args, true)
    } else {
        let (object, args) = method_call(e, "RemovePerk")?;
        (object, args, false)
    };
    if args.is_empty() || args.len() > if add { 2 } else { 1 } {
        return None;
    }
    bool_arg(args, 1)?;
    Some(Effect::ChangePerk {
        actor: receiver_actor(object, scope)?,
        perk: receiver_object(&args[0].value.node, scope)?,
        add,
    })
}

/// FO4's `<actor>.SetValue(akAV, afValue)` and its alias spelling
/// `<alias>.TryToSetActorValue(akAV, afValue)` ("sets the actor value on the
/// reference in this alias, if any" — an empty alias is a runtime no-op), and
/// — M47.2 — Skyrim's `SetAV` / `SetActorValue("Name", afValue)`.
/// M47.2 — how a call names an actor value: an `ActorValue` property, or a
/// name — Skyrim's string (`SetAV("Health", …)`) or FO4's
/// `Game.Get<Name>AV()` accessor (`GetHealthAV` names `Health`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorValueArg {
    Form(ObjectRef),
    Named(String),
}

/// M47.2 — which reading of an actor value a [`ValueExpr::ActorValueReading`]
/// takes besides the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorValueReading {
    /// `GetBaseValue` / `GetBaseActorValue`: the base, without modifiers.
    Base,
    /// `GetValuePercentage` / `GetActorValuePercentage`: the current value as
    /// a fraction (0 to 1) of its maximum.
    Percent,
}

fn actor_value_arg(e: &Expr, scope: &Scope) -> Option<ActorValueArg> {
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

fn prim_set_actor_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) =
        method_call_any(e, &["SetAV", "SetActorValue", "SetValue", "TryToSetActorValue", "TryToSetValue"])?;
    if args.len() != 2 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    let value = classify_value(&args[1].value.node, scope)?;
    Some(match actor_value_arg(&args[0].value.node, scope)? {
        ActorValueArg::Form(actor_value) => Effect::SetActorValue {
            actor,
            actor_value,
            value,
        },
        ActorValueArg::Named(name) => Effect::SetActorValueNamed { actor, name, value },
    })
}

/// `DamageValue` / `DamageActorValue` / `DamageAV` and `RestoreValue` /
/// `RestoreActorValue` / `RestoreAV` with an actor value and an amount.
fn prim_damage_actor_value(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args, restore) = method_call_any(e, &["DamageValue", "DamageActorValue", "DamageAV"])
        .map(|(object, args)| (object, args, false))
        .or_else(|| {
            method_call_any(e, &["RestoreValue", "RestoreActorValue", "RestoreAV"])
                .map(|(object, args)| (object, args, true))
        })?;
    if args.len() != 2 {
        return None;
    }
    Some(Effect::DamageActorValue {
        actor: receiver_actor(object, scope)?,
        actor_value: actor_value_arg(&args[0].value.node, scope)?,
        amount: classify_value(&args[1].value.node, scope)?,
        restore,
    })
}

fn prim_equip_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "EquipItem")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    // Prevent-unequip needs persistent locked-equipment state. Decline that
    // shape instead of claiming it and silently losing the lock contract.
    if bool_arg(args, 1)?.unwrap_or(false) {
        return None;
    }
    Some(Effect::EquipItem {
        actor: receiver_actor(object, scope)?,
        item: receiver_object(&args[0].value.node, scope)?,
        silent: bool_arg(args, 2)?.unwrap_or(false),
    })
}

/// `ObjectReference.MoveTo(akTarget, afXOffset = 0.0, afYOffset = 0.0,
/// afZOffset = 0.0, abMatchRotation = true)` — Skyrim and Fallout 4;
/// Starfield appends `abRotateOffset = false` for six parameters total
/// (verified against the shipped Base scripts: `skse64`/`f4se`
/// `scripts/vanilla/ObjectReference.psc` and Starfield's
/// `ObjectReference.psc`).
///
/// #3487 — this used to require `args.len() == 1`, the shape an *author*
/// writes. Quest fragments only ever reach this table through the `.pex`
/// frontend, and the Papyrus compiler materializes **every** parameter of
/// a call, including the ones the author omitted; the `.psc` route is
/// test-only. So the authored shape does not occur in compiled content at
/// all, and this primitive declined 3,334 of 3,334 real `MoveTo` calls —
/// a shipped, dispatch-wired, alias-aware effect that could never fire.
///
/// The original reasoning (refusing to silently drop an offset and
/// misplace the object) is kept, applied to the right input shape: a
/// trailing argument is accepted only when it is a literal equal to its
/// declared Papyrus default, which is what the compiler emits for an
/// omitted parameter. 3,253 of the corpus's 3,334 calls are exactly that;
/// the ~81 carrying a real offset still decline, and so does a
/// non-literal (a variable whose runtime value is unknown), via
/// [`as_num`]/[`bool_arg`] returning `None`.
fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    // `ReferenceAlias.TryToMoveTo(ref)` is `MoveTo(ref)` on whatever the alias
    // holds, a no-op when it holds nothing (CK *TryToMoveTo - ReferenceAlias*).
    // `ReferenceAlias.TryToMoveTo(ref)` is `MoveTo(ref)` on whatever the alias
    // holds, a no-op when it holds nothing (CK *TryToMoveTo - ReferenceAlias*).
    let (object, args) = method_call_any(e, &["MoveTo", "TryToMoveTo"])?;
    if args.is_empty() || args.len() > MOVE_TO_MAX_ARGS {
        return None;
    }
    // args[1..=3] — the three float offsets, each defaulting to `0.0`; they
    // must be literals, since a variable's run-time value is unknown here.
    let mut offset = [0.0f32; 3];
    for (slot, arg) in offset.iter_mut().zip(args.iter().skip(1)) {
        *slot = as_num(&arg.value.node)?;
    }
    // args[4] `abMatchRotation` (default `true`) and Starfield's args[5]
    // `abRotateOffset` (default `false`), which is not modelled.
    let match_rotation = bool_arg(args, 4)?.unwrap_or(true);
    if bool_arg(args, 5)?.unwrap_or(false) {
        return None;
    }
    let destination = receiver_object(&args[0].value.node, scope)?;
    if player_expr_ref(object, scope) {
        // The player's teleport (a cell transition) carries neither.
        if offset != [0.0; 3] || !match_rotation {
            return None;
        }
        return Some(Effect::MovePlayerTo { destination });
    }
    let moved = receiver_object(object, scope)?;
    Some(Effect::MoveTo { moved, destination, offset, match_rotation })
}

/// `<location>.SetKeywordData(akKeyword, afData)` (CK *SetKeywordData - Location*).
fn prim_set_location_keyword_data(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetKeywordData")?;
    if args.len() != 2 {
        return None;
    }
    Some(Effect::SetLocationKeywordData {
        location: receiver_object(object, scope)?,
        keyword: receiver_object(&args[0].value.node, scope)?,
        value: classify_value(&args[1].value.node, scope)?,
    })
}

/// `<reference>.MoveToMyEditorLocation()` (CK: "Moves this reference to its
/// original editor location and orientation").
fn prim_move_to_editor_location(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveToMyEditorLocation")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::MoveToEditorLocation {
        moved: receiver_object(object, scope)?,
    })
}

/// `Disable([abFadeOut])`, FO4/Starfield's `DisableNoWait([abFadeOut])` (the
/// same disable, returning before the fade), and — M47.2 —
/// `ReferenceAlias.TryToDisable()`, the CK's "disable a reference in an alias
/// without calling GetReference()" (an empty alias is a no-op, which an
/// unresolved receiver already is at runtime).
fn prim_disable(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["Disable", "DisableNoWait", "TryToDisable"])?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::Disable {
        object: receiver_object(object, scope)?,
        fade_out: bool_arg(args, 0)?.unwrap_or(false),
    })
}

/// #3489 — mirrors [`prim_disable`]: same receiver treatment (including
/// alias-bound `ObjectReference` properties via `receiver_object`), same
/// optional literal `bool` argument shape (`abFadeIn` here vs `abFadeOut`),
/// and the same `EnableNoWait` / `TryToEnable` spellings.
fn prim_enable(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["Enable", "EnableNoWait", "TryToEnable"])?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::Enable {
        object: receiver_object(object, scope)?,
        fade_in: bool_arg(args, 0)?.unwrap_or(false),
    })
}

fn prim_start_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartScene {
        scene: receiver_object(object, scope)?,
    })
}

fn prim_stop_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopScene {
        scene: receiver_object(object, scope)?,
    })
}

fn prim_activate(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Activate")?;
    if args.len() > 2 {
        return None;
    }
    // ObjectReference.Activate's optional first arg is the activator. The
    // MQ101 fragment corpus uses either the default or Game.GetPlayer();
    // decline other runtime expressions rather than invent their identity.
    let activator = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        // (The VM binds the player it holds as a player local.)
        Some(expression) if player_expr_ref(expression, scope) => None,
        Some(expression) => Some(receiver_object(expression, scope)?),
    };
    // `abDefaultProcessingOnly=true`: "no OnActivate event will be sent to any
    // scripts and the object will ignore the blocked flag" (CK).
    let native_only = bool_arg(args, 1)? == Some(true);
    Some(Effect::Activate {
        target: receiver_object(object, scope)?,
        activator,
        native_only,
    })
}

fn prim_set_open(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetOpen")?;
    if args.len() > 1 {
        return None;
    }
    let open = bool_arg(args, 0)?.unwrap_or(true);
    Some(Effect::SetOpen {
        target: receiver_object(object, scope)?,
        open,
    })
}

/// `<object>.Lock()` / `.Lock(true)` / `.Lock(false)` /
/// `.Lock(abLock, abAsOwner)` (#3159, #5653).
///
/// `Lock` declares TWO parameters — `Lock(bool abLock = true, bool
/// abAsOwner = false)` — and the compiler emits the declared arity, so
/// the pre-#5653 bound of one argument never matched a compiled call:
/// real content's `Lock(false, true)` was declined as over-arity and the
/// primitive only ever fired on hand-written shapes. `abAsOwner` (flag
/// the adjoining cell public/private as the owner opened or closed the
/// door) has no model here, so it follows the `AddItem`-`abSilent`
/// discipline: accepted only as a literal, parsed and not applied;
/// anything non-literal declines the whole call. Papyrus's own default
/// for `abLock` is `true`.
///
/// Starfield's dialect adds a THIRD parameter — `Lock(bool abLock, bool
/// abAsOwner, bool abTraverseLinkedLockables = true)`, read straight off
/// the shipped `ObjectReference` engine-class stub (FO4 and FO76 stubs
/// stop at two). It qualifies whether the lock propagates to *linked*
/// lockables, which [`Effect::SetLocked`] does not model for the receiver
/// either way, so it follows the same literal-only-not-applied discipline
/// as `abAsOwner`. Before this bound, `Lock(bool, false, true)` — the
/// near-universal compiled spelling, e.g. the whole SF `QF_` lock family —
/// declined as over-arity.
fn prim_lock(e: &Expr, scope: &Scope) -> Option<Effect> {
    // FO4/Starfield `Unlock(abAsOwner = false)` — "Unlocks this object" — is
    // `Lock(false, abAsOwner)`.
    if let Some((object, args)) = method_call(e, "Unlock") {
        if args.len() > 1 {
            return None;
        }
        bool_arg(args, 0)?;
        return Some(Effect::SetLocked {
            target: receiver_object(object, scope)?,
            locked: false,
        });
    }
    let (object, args) = method_call(e, "Lock")?;
    if args.len() > 3 {
        return None;
    }
    let locked = bool_arg(args, 0)?.unwrap_or(true);
    // 2nd arg `abAsOwner`, 3rd arg (Starfield) `abTraverseLinkedLockables` —
    // literal-only; see the primitive doc.
    bool_arg(args, 1)?;
    bool_arg(args, 2)?;
    Some(Effect::SetLocked {
        target: receiver_object(object, scope)?,
        locked,
    })
}

/// `<object>.SetLockLevel(<literal>)` (#3159).
///
/// Requires exactly one argument — unlike `Lock`, Papyrus gives
/// `SetLockLevel` no default, so a bare call is malformed rather than
/// meaningful and declines. The level is an authored lock difficulty that
/// the wire format carries as a single byte, so a literal outside `0..=255`
/// is not a level this engine can represent and declines too, rather than
/// truncating into a different difficulty.
fn prim_set_lock_level(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLockLevel")?;
    if args.len() != 1 {
        return None;
    }
    let level = u8::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetLockLevel {
        target: receiver_object(object, scope)?,
        level,
    })
}

fn prim_set_player_restrained(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetRestrained")?;
    if !is_game_get_player(object) || args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerRestrained {
        restrained: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_disable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "DisablePlayerControls", false)
}

fn prim_enable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "EnablePlayerControls", true)
}

/// `Game.DisablePlayerControls(abMovement, abFighting, abCamSwitch,
/// abLooking, abSneaking, abMenu, abActivate, abJournalTabs,
/// aiDisablePOVType)` — nine parameters, and **Skyrim only**.
///
/// #3487 asked for this bound to be widened to the FO4/Starfield
/// parameter count, on a corpus tally of 61 eleven-argument calls. It is
/// deliberately not widened, because those calls are not this function:
/// Fallout 4 and Starfield have no `Game.DisablePlayerControls` at all
/// (their `Game` script keeps only the `Is*ControlsEnabled` queries), and
/// moved the pair to `InputEnableLayer`, whose eleven-parameter
/// signature ends `..., abJournalTabs, abVATS, abFavorites, abRunning` —
/// arg 8 is a `bool`, where Skyrim's is `int aiDisablePOVType`.
/// [`game_call`] already declines every one of those on the receiver, so
/// widening the arity would claim nothing; if it ever did match, arg 8
/// would be read as a POV type it is not. Signatures verified against the
/// shipped Base scripts (`skse64`/`f4se` `scripts/vanilla/Game.psc` and
/// `InputEnableLayer.psc`, and Starfield's `Game.psc`).
///
/// Claiming the `InputEnableLayer` shape needs its own primitive and a
/// canonical model of input layers (priority stacking, per-layer enable),
/// which [`PlayerControlSelection`] does not have.
fn prim_player_controls(e: &Expr, method: &str, enabled: bool) -> Option<Effect> {
    let args = game_call(e, method)?;
    if args.len() > SKYRIM_PLAYER_CONTROLS_ARGS {
        return None;
    }
    // #5651 — each function fills absent arguments from its own CK-wiki
    // default table; the two calls do not share one.
    let defaults = if enabled {
        PlayerControlSelection::ENABLE_PLAYER_CONTROLS_DEFAULT
    } else {
        PlayerControlSelection::DISABLE_PLAYER_CONTROLS_DEFAULT
    };
    let selection = PlayerControlSelection {
        movement: bool_arg(args, 0)?.unwrap_or(defaults.movement),
        fighting: bool_arg(args, 1)?.unwrap_or(defaults.fighting),
        camera_switching: bool_arg(args, 2)?.unwrap_or(defaults.camera_switching),
        looking: bool_arg(args, 3)?.unwrap_or(defaults.looking),
        sneaking: bool_arg(args, 4)?.unwrap_or(defaults.sneaking),
        menu: bool_arg(args, 5)?.unwrap_or(defaults.menu),
        activation: bool_arg(args, 6)?.unwrap_or(defaults.activation),
        journal_tabs: bool_arg(args, 7)?.unwrap_or(defaults.journal_tabs),
        pov_type: optional_int_arg(args, 8, defaults.pov_type)?,
    };
    Some(Effect::SetPlayerControls { enabled, selection })
}

fn prim_set_player_ai_driven(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetPlayerAIDriven")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerAiDriven {
        ai_driven: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_set_hud_cart_mode(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetHudCartMode")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetHudCartMode {
        cart_mode: bool_arg(args, 0)?.unwrap_or(true),
    })
}

fn prim_set_in_chargen(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetInChargen")?;
    if args.len() != 3 {
        return None;
    }
    Some(Effect::SetInChargen {
        disable_saving: bool_arg(args, 0)?.unwrap_or(false),
        disable_waiting: bool_arg(args, 1)?.unwrap_or(false),
        show_controls_disabled_message: bool_arg(args, 2)?.unwrap_or(false),
    })
}

fn prim_show_race_menu(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "ShowRaceMenu")?;
    args.is_empty().then_some(Effect::ShowRaceMenu)
}

fn prim_request_save(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "RequestSave")?;
    args.is_empty().then_some(Effect::RequestSave { auto: false })
}

fn prim_request_auto_save(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "RequestAutoSave")?;
    args.is_empty().then_some(Effect::RequestSave { auto: true })
}

/// #4318 — Skyrim's own `faction.pex` declares `SetEnemy(Faction akOther,
/// Bool abSelfIsNeutralToOther = false, Bool abOtherIsNeutralToSelf = false)`.
/// A `true` flag makes that direction *neutral* rather than hostile, so
/// `SetEnemy(x, true, true)` is "mutually neutral", not enmity. #4414 made
/// the relation directed, so both flags are carried; an omitted one takes its
/// declared default, and a non-literal one is an unknown runtime value and
/// declines.
fn prim_set_enemy(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetEnemy")?;
    if !(1..=3).contains(&args.len()) {
        return None;
    }
    Some(Effect::SetEnemy {
        faction: receiver_object(object, scope)?,
        other_faction: receiver_object(&args[0].value.node, scope)?,
        self_neutral: bool_arg(args, 1)?.unwrap_or(false),
        other_neutral: bool_arg(args, 2)?.unwrap_or(false),
    })
}

/// `Faction.SetAlly(Faction akOther, bool abSelfIsFriendToOther = false,
/// bool abOtherIsFriendToSelf = false)`; literal flags only, like
/// [`prim_set_enemy`].
fn prim_set_ally(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetAlly")?;
    if !(1..=3).contains(&args.len()) {
        return None;
    }
    Some(Effect::SetAlly {
        faction: receiver_object(object, scope)?,
        other_faction: receiver_object(&args[0].value.node, scope)?,
        self_friend: bool_arg(args, 1)?.unwrap_or(false),
        other_friend: bool_arg(args, 2)?.unwrap_or(false),
    })
}

fn prim_start_combat(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StartCombat")?;
    // FO4 / Starfield add `abPreferredTarget`, a literal flag: the combat
    // runtime holds a single target, so preferring it changes nothing.
    match args.len() {
        1 => {}
        2 => {
            bool_arg(args, 1)??;
        }
        _ => return None,
    }
    let actor = receiver_actor(object, scope)?;
    // #4323 — the runtime this arms is an NPC chase-and-strike: it steers the
    // combatant's own Transform and emits hits on cooldown. On the player
    // that would fight the character controller and auto-attack, which is
    // not what `StartCombat` means, so a player combatant is unmodeled.
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::StartCombat {
        actor,
        target: receiver_actor(&args[0].value.node, scope)?,
    })
}

/// #4816 — Skyrim's `actor.psc` declares `StopCombat()` with no parameters.
/// A player receiver declines for the reason [`prim_start_combat`] gives:
/// the combat runtime this clears is the NPC one, and the player carries
/// none.
fn prim_stop_combat(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StopCombat")?;
    if !args.is_empty() {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::StopCombat { actor })
}

/// `<actor>.SetCrimeFaction(akFaction)`, a faction property or `None`.
fn prim_set_crime_faction(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetCrimeFaction")?;
    if args.len() != 1 {
        return None;
    }
    let faction = match &args[0].value.node {
        Expr::NoneLit => None,
        other => Some(receiver_object(other, scope)?),
    };
    Some(Effect::SetCrimeFaction {
        actor: receiver_actor(object, scope)?,
        faction,
    })
}

/// `<actor>.StopCombatAlarm()`, the player included (a player target is the
/// usual one: stand everyone down after a persuasion or an arrest).
fn prim_stop_combat_alarm(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "StopCombatAlarm")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopCombatAlarm {
        actor: receiver_actor(object, scope)?,
    })
}

/// #5017 — `actor.psc` declares `SetUnconscious(bool abUnconscious = true)`.
/// A player receiver declines: an unconscious player would need the
/// controls, camera and HUD handling the Creation Kit does not describe, and
/// the vanilla calls this issue targets wake NPCs.
fn prim_set_unconscious(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetUnconscious")?;
    if args.len() > 1 {
        return None;
    }
    let actor = receiver_actor(object, scope)?;
    if actor == ActorRef::Player {
        return None;
    }
    Some(Effect::SetUnconscious {
        actor,
        unconscious: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `Actor.SetGhost(bool abIsGhost = true)`. The player may be a ghost too
/// (the CS page's `player.SetGhost 1`).
fn prim_set_ghost(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetGhost")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetGhost {
        actor: receiver_actor(object, scope)?,
        ghost: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// #4415 — Skyrim's `actor.psc` declares `AddSpell(Spell akSpell, Bool
/// abVerbose = true)`; the flag only picks whether the "spell added"
/// message shows, so either form lowers.
fn prim_add_spell(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddSpell")?;
    if !(1..=2).contains(&args.len()) || (args.len() == 2 && bool_arg(args, 1)?.is_none()) {
        return None;
    }
    Some(Effect::AddSpell {
        actor: receiver_actor(object, scope)?,
        spell: receiver_object(&args[0].value.node, scope)?,
    })
}

fn prim_remove_spell(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "RemoveSpell")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::RemoveSpell {
        actor: receiver_actor(object, scope)?,
        spell: receiver_object(&args[0].value.node, scope)?,
    })
}

/// #4415 — `AddRaceSpells` / `RemoveRaceSpells` are *script-defined*
/// helpers on quest scripts (MQ101QuestScript authors both), not
/// `actor.psc` methods — the translator sees an unqualified call to the
/// own script's function. Lowering by name is sound without inlining the
/// helper's body because the semantic is stateless: re-apply / clear the
/// actor's race `SPLO` set, which spawn pre-stamped as `RaceSpells`.
/// #5407 — that soundness extends ONLY to the vanilla shape: the
/// unqualified call, in a script that itself defines the function
/// ARGLESS. A member-call spelling (`x.AddRaceSpells()` on any object)
/// and a same-named helper some other script defines with different
/// semantics are declined — both used to lower onto the player
/// unconditionally.
fn lower_race_spells(e: &Expr, method: &str, adding: bool, scope: &Scope) -> Option<Effect> {
    if !prim_race_spells_arity_ok(e, method, scope) {
        return None;
    }
    // The vanilla helpers act on the player, and the accepted spelling
    // (unqualified, own-script-defined) is exactly how they call it.
    Some(if adding {
        Effect::AddRaceSpells {
            actor: ActorRef::Player,
        }
    } else {
        Effect::RemoveRaceSpells {
            actor: ActorRef::Player,
        }
    })
}

/// Only the UNQUALIFIED, ARGLESS call to a function the calling script
/// itself defines (`ScriptName`-level function table via
/// [`Scope::own_argless_functions`]). Member-call spellings are declined
/// outright — the receiver is NOT distinguished, because no receiver is
/// accepted at all (#5407).
fn prim_race_spells_arity_ok(e: &Expr, method: &str, scope: &Scope) -> bool {
    let Expr::Call { callee, args } = e else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let Expr::Ident(name) = &callee.node else {
        return false;
    };
    name.0.eq_ignore_ascii_case(method)
        && scope
            .own_argless_functions
            .contains(&name.0.to_ascii_lowercase())
}

fn prim_add_race_spells(e: &Expr, scope: &Scope) -> Option<Effect> {
    // Arity, spelling, and own-script definition are bounded by
    // `prim_race_spells_arity_ok` inside.
    lower_race_spells(e, "AddRaceSpells", true, scope)
}

fn prim_remove_race_spells(e: &Expr, scope: &Scope) -> Option<Effect> {
    // Arity, spelling, and own-script definition are bounded by
    // `prim_race_spells_arity_ok` inside.
    lower_race_spells(e, "RemoveRaceSpells", false, scope)
}

fn prim_play_idle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "PlayIdle")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::PlayIdle {
        actor: receiver_actor(object, scope)?,
        idle: receiver_object(&args[0].value.node, scope)?,
    })
}

fn prim_set_vehicle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetVehicle")?;
    if args.len() != 1 {
        return None;
    }
    let vehicle = match &args[0].value.node {
        Expr::NoneLit => None,
        value => Some(receiver_object(value, scope)?),
    };
    Some(Effect::SetVehicle {
        actor: receiver_actor(object, scope)?,
        vehicle,
    })
}

fn prim_tether_to_horse(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "TetherToHorse")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::TetherToHorse {
        cart: receiver_object(object, scope)?,
        horse: receiver_object(&args[0].value.node, scope)?,
    })
}

fn prim_set_motion_type(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetMotionType")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let target = receiver_object(object, scope)?;
    let motion_type = motion_type_arg(&args[0].value.node, &target, scope)?;
    let allow_activate = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetMotionType {
        target,
        motion_type,
        allow_activate,
    })
}

fn motion_type_arg(value: &Expr, target: &ObjectRef, scope: &Scope) -> Option<MotionType> {
    if let Expr::Cast { expr, .. } = value {
        return motion_type_arg(&expr.node, target, scope);
    }
    if let Expr::IntLit(raw) = value {
        // `ObjectReference.SetMotionType`'s argument is NOT the NIF
        // `hkMotionType` enum (`crates/nif`'s collision importer: Keyframed
        // 6, Static 7, Character 9) that #2286 copied here. The Papyrus
        // constants differ per game — CK wiki, `SetMotionType -
        // ObjectReference`: Skyrim Dynamic 1, SphereInertia 2, BoxInertia 3,
        // Keyframed 4, Fixed 5, ThinBoxInertia 6, Character 7; Fallout 4
        // Fixed 0, Dynamic 1, Keyframed 2 — and this lowerer does not know
        // the game. Only `1` means the same in both (Dynamic); every other
        // literal declines instead of guessing (#5633). Vanilla content is
        // unaffected: MQ101's fragment keeps the named `Motion_Keyframed`
        // property read (the arm below), it is not folded to a literal.
        return match *raw {
            1 => Some(MotionType::Dynamic),
            _ => None,
        };
    }
    let Expr::MemberAccess { object, member } = value else {
        return None;
    };
    if receiver_object(&object.node, scope).as_ref() != Some(target) {
        return None;
    }
    match member.node.0.to_ascii_lowercase().as_str() {
        "motion_dynamic" => Some(MotionType::Dynamic),
        "motion_keyframed" => Some(MotionType::Keyframed),
        "motion_fixed" => Some(MotionType::Static),
        "motion_character" => Some(MotionType::CharacterKinematic),
        _ => None,
    }
}

fn prim_set_sitting_rotation(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetSittingRotation")?;
    if args.len() != 1 {
        return None;
    }
    let degrees = as_num(&args[0].value.node)?;
    degrees
        .is_finite()
        .then_some(Effect::SetSittingRotation { degrees })
}

fn prim_exit_cart(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ExitCart")?;
    if args.len() != 2 {
        return None;
    }
    // `ExitCart` is an MQ101 quest helper, so require a proven quest receiver
    // rather than accepting an arbitrary object method with the same name.
    receiver_quest(object, scope)?;
    let seat = u8::try_from(int_arg(args, 1)?).ok()?;
    if !(1..=5).contains(&seat) {
        return None;
    }
    Some(Effect::ExitCart {
        actor: receiver_object(&args[0].value.node, scope)?,
        seat,
    })
}

fn prim_player_imod_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerImodAnimation",
        CinematicAnimationEvent::PlayImod,
    )
}

fn prim_player_furniture_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerFurnitureAnimation",
        CinematicAnimationEvent::IdleFurnitureExit,
    )
}

fn prim_player_animation_event(
    e: &Expr,
    scope: &Scope,
    method: &str,
    event: CinematicAnimationEvent,
) -> Option<Effect> {
    let (object, args) = method_call(e, method)?;
    if !args.is_empty() {
        return None;
    }
    receiver_quest(object, scope)?;
    Some(Effect::RegisterPlayerAnimationEvent { event })
}

/// `Actor.EvaluatePackage()` on Skyrim; `Actor.EvaluatePackage(abResetAI
/// = false)` on Fallout 4 and Starfield (shipped `Actor.psc`).
///
/// #3487 — same default-materialization problem as [`prim_move_to`]:
/// requiring zero arguments declined 2,628 of 3,481 real calls, all of
/// them the FO4/Starfield build of the same statement, making the
/// primitive silently game-asymmetric. The compiler-emitted literal
/// `false` is accepted; a literal `true` still declines, because
/// resetting the actor's AI is a second effect that
/// [`Effect::EvaluatePackage`] does not model (2,621 of the 2,628 pass
/// the default).
fn prim_evaluate_package(e: &Expr, scope: &Scope) -> Option<Effect> {
    // M47.2 — `ReferenceAlias.TryToEvaluatePackage()` is `EvaluatePackage` on
    // the alias's actor when it is filled (the CK's own definition); an
    // empty alias resolves to nothing and is skipped at run time.
    if let Some((object, args)) = method_call(e, "TryToEvaluatePackage") {
        return args.is_empty().then(|| {
            receiver_object(object, scope).map(|actor| Effect::EvaluatePackage {
                actor: ActorRef::Object(actor),
            })
        })?;
    }
    let (object, args) = method_call(e, "EvaluatePackage")?;
    if args.len() > 1 || bool_arg(args, 0)?.unwrap_or(false) {
        return None;
    }
    Some(Effect::EvaluatePackage {
        actor: receiver_actor(object, scope)?,
    })
}

fn prim_wait(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Utility", "Wait")?;
    if args.len() != 1 {
        return None;
    }
    let seconds = as_num(&args[0].value.node)?;
    // #5681 — a wait the session can never satisfy (`3.0e38` is a finite
    // f32) would park an immortal entry in `FragmentExecutionQueue`, saved
    // with the rest of the world. Decline above the shared cap; the VM
    // fallback stops the same run at its own suspend site, so the wait
    // dies on both paths instead of moving between them.
    (seconds.is_finite() && (0.0..=MAX_PROVIDER_WAIT_SECONDS).contains(&seconds))
        .then_some(Effect::Wait { seconds })
}

fn game_call<'a>(e: &'a Expr, method: &str) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    static_call(e, "Game", method)
}

fn static_call<'a>(
    e: &'a Expr,
    type_name: &str,
    method: &str,
) -> Option<&'a [byroredux_papyrus::ast::CallArg]> {
    let (object, args) = method_call(e, method)?;
    matches!(object, Expr::Ident(name) if name.0.eq_ignore_ascii_case(type_name)).then_some(args)
}

fn optional_int_arg(
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
fn receiver_quest(object: &Expr, scope: &Scope) -> Option<QuestRef> {
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
fn quest_expr_ref(value: &Expr, scope: &Scope) -> Option<QuestRef> {
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
fn object_expr_ref(value: &Expr, scope: &Scope) -> Option<ObjectRef> {
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
fn speaker_expr_ref(value: &Expr, scope: &Scope) -> bool {
    match value {
        Expr::Cast { expr, .. } => speaker_expr_ref(&expr.node, scope),
        Expr::Ident(name) => scope.speaker_locals.contains(&name.0.to_ascii_lowercase()),
        _ => false,
    }
}

/// `<alias>.GetRef()` / `.GetActorRef()` (Fallout 4 / Starfield) or
/// `.GetReference()` / `.GetActorReference()` (Skyrim's `ReferenceAlias`
/// spelling of the same getters, M47.2).
fn alias_reference_getter(e: &Expr) -> Option<(&Expr, &[byroredux_papyrus::ast::CallArg])> {
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
fn receiver_location(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    if let Expr::Cast { expr, .. } = object {
        return receiver_location(&expr.node, scope);
    }
    match method_call(object, "GetLocation") {
        Some((alias, [])) => receiver_object(alias, scope),
        Some(_) => None,
        None => receiver_object(object, scope),
    }
}

fn player_expr_ref(value: &Expr, scope: &Scope) -> bool {
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

fn receiver_object(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
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

fn receiver_actor(object: &Expr, scope: &Scope) -> Option<ActorRef> {
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
fn bool_arg(args: &[byroredux_papyrus::ast::CallArg], idx: usize) -> Option<Option<bool>> {
    match args.get(idx) {
        None => Some(None),
        Some(arg) => as_num(&arg.value.node).map(|n| Some(n != 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::ast::{CallArg, Identifier, ScriptItem, StateItem};
    use byroredux_papyrus::parse_script;

    /// Wrap a node with a dummy span, for ASTs the `.psc` parser cannot
    /// express (e.g. a `::X_var` backing-variable identifier).
    fn sp<T>(node: T) -> Spanned<T> {
        Spanned::new(node, byroredux_papyrus::span::Span::new(0, 0))
    }

    /// A bare `Self.GetStageDone(stage)` condition atom — the minimal
    /// shape `classify_guard_atom`'s `prim_stage_done` accepts (a bare
    /// call used as a boolean, `== 1` implied), so every level of a
    /// synthetic nested-`If` chain classifies as a real `StageDoneGuard`
    /// rather than declining before `lower_statements` ever recurses.
    fn get_stage_done_condition(stage: i64) -> Spanned<Expr> {
        sp(Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier::new("Self")))),
                member: sp(Identifier::new("GetStageDone")),
            })),
            args: vec![CallArg {
                name: None,
                value: sp(Expr::IntLit(stage)),
            }],
        })
    }

    /// Build `depth` levels of nested `If Self.GetStageDone(0) ... EndIf`,
    /// each level guarded by a real `StageDoneGuard`-classifiable
    /// condition, with an empty (no-op) innermost body.
    ///
    /// Built directly from AST nodes rather than `.psc` source text on
    /// purpose (#3279): the papyrus parser's own `MAX_STMT_DEPTH` would
    /// reject a `depth` this deep before `lower_statements` ever saw it,
    /// which would prove nothing about `lower_statements`'s *own* cap —
    /// the whole point of a defense-in-depth guard is that it holds even
    /// if the upstream bound it currently rides on is ever loosened or
    /// bypassed.
    fn nested_if_chain(depth: usize) -> Vec<Spanned<Stmt>> {
        let mut body: Vec<Spanned<Stmt>> = Vec::new();
        for _ in 0..depth {
            body = vec![sp(Stmt::If {
                condition: get_stage_done_condition(0),
                body,
                elseif_clauses: vec![],
                else_body: None,
            })];
        }
        body
    }

    /// #3279 (SCR-D5-2026-08-24-02) — analogous to
    /// `stmt_depth_cap_rejects_pathological_nested_if`
    /// (`crates/papyrus/src/parser/stmt.rs`). `MAX_CONDITIONAL_DEPTH` is
    /// this recursion's own cap, independent of the two upstream ones
    /// (`MAX_STMT_DEPTH` on the `.psc` parser, `MAX_REBUILD_DEPTH` on the
    /// `.pex` decompiler's control-flow rebuild) that transitively bound
    /// it today.
    #[test]
    fn conditional_depth_cap_declines_pathological_nested_if() {
        let body = nested_if_chain((MAX_CONDITIONAL_DEPTH as usize) * 2);
        assert_eq!(
            lower_fragment(&body),
            None,
            "a pathologically nested If chain must decline once \
             MAX_CONDITIONAL_DEPTH is exceeded, not recurse unboundedly"
        );
    }

    /// A legitimate (if unusual) fragment comfortably under the cap must
    /// still lower successfully — the cap must not be so tight it
    /// rejects real content.
    #[test]
    fn conditional_depth_cap_accepts_legitimate_nesting() {
        let body = nested_if_chain(50);
        assert!(
            lower_fragment(&body).is_some(),
            "legitimate nesting well under MAX_CONDITIONAL_DEPTH must still lower"
        );
    }

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
                    if let Some(si) = st.body.first() {
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
    fn lowers_global_set_value_before_stage_handoff() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             GlobalVariable Property GameHour Auto\n\
             Function Fragment_0()\n\
             GameHour.SetValue(7.0)\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetGlobalValue {
                    global: ObjectRef::Property("GameHour".into()),
                    value: crate::translate::effects::ValueExpr::Literal(7.0),
                },
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ])
        );
    }

    #[test]
    fn provider_calls_preserve_top_level_fragment_order() {
        let providers = PapyrusProviderCatalog::engine_compatibility();
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.SetStage(10)\n\
             Game.GetModCount()\n\
             Game.IsPluginInstalled(\"Update.esm\")\n\
             EndFunction\n",
        );
        let effects = lower_fragment_with_quest_properties_and_providers(
            &body,
            &HashSet::new(),
            Some(&providers),
        )
        .unwrap();
        assert!(matches!(effects[0], Effect::SetStage { stage: 10, .. }));
        assert!(matches!(
            &effects[1],
            Effect::ProviderCall(call)
                if call.route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE
        ));
        assert!(matches!(
            &effects[2],
            Effect::ProviderCall(call)
                if call.route
                    == byroredux_sdk::compatibility::PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE
        ));

        let reordered = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Game.GetModCount()\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        let reordered_effects = lower_fragment_with_quest_properties_and_providers(
            &reordered,
            &HashSet::new(),
            Some(&providers),
        )
        .unwrap();
        assert!(matches!(reordered_effects[0], Effect::ProviderCall(_)));
        assert!(matches!(
            reordered_effects[1],
            Effect::SetStage { stage: 10, .. }
        ));

        let conditional = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             If Self.GetStageDone(2) == false\n\
               Game.GetModCount()\n\
               Self.SetStage(10)\n\
             Else\n\
               Game.IsPluginInstalled(\"Update.esm\")\n\
             EndIf\n\
             Self.SetStage(20)\n\
             EndFunction\n",
        );
        let conditional_effects = lower_fragment_with_quest_properties_and_providers(
            &conditional,
            &HashSet::new(),
            Some(&providers),
        )
        .unwrap();
        let Effect::Conditional {
            then_effects,
            else_effects,
            ..
        } = &conditional_effects[0]
        else {
            panic!("expected conditional effect");
        };
        assert!(matches!(then_effects[0], Effect::ProviderCall(_)));
        assert!(matches!(
            then_effects[1],
            Effect::SetStage { stage: 10, .. }
        ));
        assert!(matches!(else_effects[0], Effect::ProviderCall(_)));
        assert!(matches!(
            conditional_effects[1],
            Effect::SetStage { stage: 20, .. }
        ));
    }

    #[test]
    fn lowers_quest_lifecycle_and_bulk_objective_effects() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.Start()\n\
             Self.SetActive(true)\n\
             Self.FailAllObjectives()\n\
             Self.CompleteQuest()\n\
             Self.Stop()\n\
             Self.Reset()\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::StartQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::SetQuestActive {
                    quest: QuestRef::SelfRef,
                    active: true,
                },
                Effect::FailAllObjectives {
                    quest: QuestRef::SelfRef,
                },
                Effect::CompleteQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::StopQuest {
                    quest: QuestRef::SelfRef,
                },
                Effect::ResetQuest {
                    quest: QuestRef::SelfRef,
                },
            ])
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

    /// The shape the `.pex` decompiler emits for a locals-table fragment:
    /// a bare declaration, then the Champollion autocast as an assignment.
    /// Both locals are declared, so the fragment lowers (#5638 only refuses
    /// assignment to a name nothing declared).
    #[test]
    fn lowers_the_decompiled_autocast_prologue() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Quest __temp\n\
             Quest kmyQuest\n\
             __temp = Self as Quest\n\
             kmyQuest = __temp\n\
             kmyQuest.SetStage(30)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 30
            }])
        );
        // Without the declarations the same assignments are writes to
        // script state the lowerer cannot model, so the fragment declines.
        let undeclared = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             kmyQuest = Self as Quest\n\
             kmyQuest.SetStage(30)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&undeclared), None);
    }

    /// #5634: `OwningQuest` is the quest owning THIS script's alias, so only
    /// `Self.GetOwningQuest()` may map to it. `OtherAlias.GetOwningQuest()`
    /// names another alias's quest and used to stage / gate on the context
    /// quest — in a guard atom the mixed-quest check passed because both
    /// sides resolved to `OwningQuest`.
    #[test]
    fn get_owning_quest_lowers_only_on_self() {
        let on_self = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Self.GetOwningQuest().SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&on_self),
            Some(vec![Effect::SetStage { quest: QuestRef::OwningQuest, stage: 10 }])
        );

        let foreign_effect = first_fn_body(
            "ScriptName QF extends Quest\n\
             ReferenceAlias Property OtherAlias Auto\n\
             Function Fragment_1()\n\
             OtherAlias.GetOwningQuest().SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&foreign_effect), None, "foreign receiver, effect");

        let foreign_guard = first_fn_body(
            "ScriptName QF extends Quest\n\
             ReferenceAlias Property OtherAlias Auto\n\
             Function Fragment_1()\n\
             If OtherAlias.GetOwningQuest().GetStageDone(5) == 0\n\
               Self.GetOwningQuest().SetStage(10)\n\
             EndIf\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&foreign_guard), None, "foreign receiver, guard atom");
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
        // An object-targeting effect (PushActorAway) isn't in the table — the
        // whole fragment declines, never partially applies. (#3489 — this
        // used to exercise `Enable()`, then `Kill()`, then `SetGhost()`; all
        // are modeled now.)
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_4()\n\
             Self.SetStage(10)\n\
             akTarget.PushActorAway(akTarget, 5.0)\n EndFunction\n",
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
                container: ActorRef::Object(ObjectRef::Property("SomeContainer".into())),
                item: ObjectRef::Property("SomeItem".into()),
                count: ItemCount::Literal(5),
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
                container: ActorRef::Object(ObjectRef::Property("SomeContainer".into())),
                item: ObjectRef::Property("SomeItem".into()),
                count: ItemCount::Literal(5),
            }])
        );
    }

    /// M47.2 — the corpus's commonest reward shape: the player as the
    /// container, the count defaulting to 1 when omitted.
    #[test]
    fn lowers_add_item_on_the_player_with_a_default_count() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Game.GetPlayer().AddItem(Reward as Form, 1, false)\n\
             Game.GetPlayer().AddItem(Gold001)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::AddItem {
                    container: ActorRef::Player,
                    item: ObjectRef::Property("Reward".into()),
                    count: ItemCount::Literal(1),
                },
                Effect::AddItem {
                    container: ActorRef::Player,
                    item: ObjectRef::Property("Gold001".into()),
                    count: ItemCount::Literal(1),
                },
            ])
        );
    }

    /// M47.2 — `RemoveItem(item, count = 1, silent = false, other = None)`:
    /// the vanilla `Game.GetPlayer().RemoveItem(Gold, n, false, None)` shape,
    /// the defaulted forms, and a move into another container.
    #[test]
    fn lowers_remove_item_shapes() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Game.GetPlayer().RemoveItem(Gold001 as Form, 50, false, None)\n\
             SomeChest.RemoveItem(SomeItem)\n\
             Game.GetPlayer().RemoveItem(SomeItem, 2, true, SomeChest)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::RemoveItem {
                    container: ActorRef::Player,
                    item: ObjectRef::Property("Gold001".into()),
                    count: ItemCount::Literal(50),
                    destination: None,
                },
                Effect::RemoveItem {
                    container: ActorRef::Object(ObjectRef::Property("SomeChest".into())),
                    item: ObjectRef::Property("SomeItem".into()),
                    count: ItemCount::Literal(1),
                    destination: None,
                },
                Effect::RemoveItem {
                    container: ActorRef::Player,
                    item: ObjectRef::Property("SomeItem".into()),
                    count: ItemCount::Literal(2),
                    destination: Some(ActorRef::Object(ObjectRef::Property(
                        "SomeChest".into()
                    ))),
                },
            ])
        );
    }

    /// M47.2 — a count read from a global lowers to [`ItemCount::Global`] in
    /// each spelling the corpus uses; the read happens when the effect applies.
    #[test]
    fn item_counts_read_from_a_global_lower() {
        for call in [
            "Game.GetPlayer().RemoveItem(Gold001, Ransom.GetValueInt(), false, None)",
            "Game.GetPlayer().RemoveItem(Gold001, Ransom.GetValue() as Int, false, None)",
            "Game.GetPlayer().RemoveItem(Gold001, Ransom.Value as Int, false, None)",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                Some(vec![Effect::RemoveItem {
                    container: ActorRef::Player,
                    item: ObjectRef::Property("Gold001".into()),
                    count: ItemCount::Global(ObjectRef::Property("Ransom".into())),
                    destination: None,
                }]),
                "{call}"
            );
        }
    }

    /// M47.2 — `AddToMap` and the marker guards.
    #[test]
    fn map_markers_lower() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             If !LairMarker.IsMapMarkerVisible()\n LairMarker.AddToMap(false)\n EndIf\n\
             CityMarker.AddToMap(true)\n EndFunction\n",
        );
        let lair = || ObjectRef::Property("LairMarker".into());
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Conditional {
                    guards: vec![FragmentGuard::MapMarkerVisible {
                        marker: lair(),
                        expected: false
                    }],
                    then_effects: vec![Effect::AddToMap {
                        marker: lair(),
                        fast_travel: false
                    }],
                    else_effects: vec![],
                },
                Effect::AddToMap {
                    marker: ObjectRef::Property("CityMarker".into()),
                    fast_travel: true
                },
            ])
        );
    }

    /// M47.2 — `SetCleared` on a location property and on a location
    /// alias's `GetLocation()`, under an `IsCleared` guard.
    #[test]
    fn location_clearing_lowers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             If !Alias_Dungeon.GetLocation().IsCleared()\n Alias_Dungeon.GetLocation().SetCleared()\n EndIf\n\
             BleakFallsLocation.SetCleared(false)\n EndFunction\n",
        );
        let dungeon = || ObjectRef::Property("Alias_Dungeon".into());
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Conditional {
                    guards: vec![FragmentGuard::LocationCleared {
                        location: dungeon(),
                        expected: false
                    }],
                    then_effects: vec![Effect::SetLocationCleared {
                        location: dungeon(),
                        cleared: true
                    }],
                    else_effects: vec![],
                },
                Effect::SetLocationCleared {
                    location: ObjectRef::Property("BleakFallsLocation".into()),
                    cleared: false
                },
            ])
        );
    }

    /// M47.2 — enable-state, faction, 3D-loaded and distance guards.
    #[test]
    fn reference_faction_loaded_and_distance_guards_lower() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             If HallNormalState.IsDisabled() == False && Game.GetPlayer().IsInFaction(SonsFaction) == False\n\
             Self.SetStage(10)\n\
             EndIf\n\
             If !Alias_Dexion.GetReference().Is3DLoaded() && Game.GetPlayer().GetDistance(HitchingPost) < 3000.0\n\
             Self.SetStage(20)\n\
             EndIf\n EndFunction\n",
        );
        let stage = |stage| Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage,
        };
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Conditional {
                    guards: vec![
                        FragmentGuard::ReferenceEnabled {
                            object: ObjectRef::Property("HallNormalState".into()),
                            expected: true,
                        },
                        FragmentGuard::InFaction {
                            actor: ActorRef::Player,
                            faction: ObjectRef::Property("SonsFaction".into()),
                            expected: false,
                        },
                    ],
                    then_effects: vec![stage(10)],
                    else_effects: vec![],
                },
                Effect::Conditional {
                    guards: vec![
                        FragmentGuard::Loaded3D {
                            object: ObjectRef::Property("Alias_Dexion".into()),
                            expected: false,
                        },
                        FragmentGuard::Distance {
                            from: ActorRef::Player,
                            to: ActorRef::Object(ObjectRef::Property("HitchingPost".into())),
                            comparison: GuardComparison::Lt,
                            value: 3000.0,
                        },
                    ],
                    then_effects: vec![stage(20)],
                    else_effects: vec![],
                },
            ])
        );
    }

    /// M47.2 — FO4's `Game.RewardPlayerXP`, `GivePlayerCaps` /
    /// `RemovePlayerCaps`, and `SetObjectiveSkipped` (its `Quest.psc` body).
    #[test]
    fn fallout4_game_rewards_and_skipped_objectives_lower() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Game.RewardPlayerXP(50)\n\
             Game.RewardPlayerXP(25, True)\n\
             Game.GivePlayerCaps(100)\n\
             Game.RemovePlayerCaps(20)\n\
             Self.SetObjectiveSkipped(10)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::RewardPlayerXp {
                    amount: ItemCount::Literal(50),
                    direct: false
                },
                Effect::RewardPlayerXp {
                    amount: ItemCount::Literal(25),
                    direct: true
                },
                Effect::PlayerCaps { delta: 100 },
                Effect::PlayerCaps { delta: -20 },
                Effect::Conditional {
                    guards: vec![FragmentGuard::ObjectiveCompleted {
                        quest: QuestRef::SelfRef,
                        objective: 10,
                        expected: false,
                    }],
                    then_effects: vec![Effect::SetObjectiveDisplayed {
                        quest: QuestRef::SelfRef,
                        objective: 10,
                        displayed: false,
                    }],
                    else_effects: vec![],
                },
            ])
        );
    }

    /// M47.2 — `SetRelationshipRank` with a literal (negative too) and the
    /// `GetRelationshipRank` guard.
    #[test]
    fn relationship_rank_lowers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             If Alias_Ysolda.GetActorRef().GetRelationshipRank(Game.GetPlayer()) < 1\n\
             Alias_Ysolda.GetActorRef().SetRelationshipRank(Game.GetPlayer(), 1)\n\
             EndIf\n\
             Alias_Nazeem.GetActorRef().SetRelationshipRank(Game.GetPlayer(), -2)\n EndFunction\n",
        );
        let alias = |name: &str| ActorRef::Object(ObjectRef::Property(name.into()));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Conditional {
                    guards: vec![FragmentGuard::RelationshipRank {
                        actor: alias("Alias_Ysolda"),
                        other: ActorRef::Player,
                        comparison: GuardComparison::Lt,
                        value: 1.0,
                    }],
                    then_effects: vec![Effect::SetRelationshipRank {
                        actor: alias("Alias_Ysolda"),
                        other: ActorRef::Player,
                        rank: 1,
                    }],
                    else_effects: vec![],
                },
                Effect::SetRelationshipRank {
                    actor: alias("Alias_Nazeem"),
                    other: ActorRef::Player,
                    rank: -2,
                },
            ])
        );
    }

    /// M47.2 — `Kill` with and without a killer (a player victim declines),
    /// and an alias's `TryToEvaluatePackage`.
    #[test]
    fn kill_and_try_to_evaluate_package_lower() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_Kyr.GetActorRef().Kill(None)\n\
             Alias_Victim.GetActorRef().Kill(Game.GetPlayer())\n\
             Alias_Spectator1.TryToEvaluatePackage()\n EndFunction\n",
        );
        let alias = |name: &str| ObjectRef::Property(name.into());
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Kill {
                    actor: ActorRef::Object(alias("Alias_Kyr")),
                    killer: None,
                    silent: false,
                },
                Effect::Kill {
                    actor: ActorRef::Object(alias("Alias_Victim")),
                    killer: Some(ActorRef::Player),
                    silent: false,
                },
                Effect::EvaluatePackage {
                    actor: ActorRef::Object(alias("Alias_Spectator1")),
                },
            ])
        );
        let silent = first_fn_body(
            "ScriptName QF extends Quest\nFunction Fragment_8()\nAlias_Kyr.GetActorRef().KillSilent(Game.GetPlayer())\nEndFunction\n",
        );
        assert_eq!(
            lower_fragment(&silent),
            Some(vec![Effect::Kill {
                actor: ActorRef::Object(alias("Alias_Kyr")),
                killer: Some(ActorRef::Player),
                silent: true,
            }])
        );
        let player = first_fn_body(
            "ScriptName QF extends Quest\nFunction Fragment_8()\nGame.GetPlayer().Kill(None)\nEndFunction\n",
        );
        assert_eq!(lower_fragment(&player), None, "the player's death is not modelled");
    }

    /// M47.2 — `SetEssential` / `SetProtected` on an actor's base and on an
    /// ActorBase property; FO4's two-argument `StartCombat`.
    #[test]
    fn base_protection_and_preferred_target_combat_lower() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_Thonar.GetActorRef().GetActorBase().SetEssential(false)\n\
             EmperorBase.SetProtected()\n\
             Alias_Guard.GetActorRef().StartCombat(Game.GetPlayer(), false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetBaseProtection {
                    base: ActorBaseRef::OfActor(ActorRef::Object(ObjectRef::Property(
                        "Alias_Thonar".into()
                    ))),
                    protected: false,
                    value: false,
                },
                Effect::SetBaseProtection {
                    base: ActorBaseRef::Form(ObjectRef::Property("EmperorBase".into())),
                    protected: true,
                    value: true,
                },
                Effect::StartCombat {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Guard".into())),
                    target: ActorRef::Player,
                },
            ])
        );
    }

    /// M47.2 — `SendStoryEvent` with nothing, and with a location, the
    /// speaker and a value.
    #[test]
    fn send_story_event_lowers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             MQGreybeardCallKeyword.SendStoryEvent(None, None, None, 0, 0)\n\
             SomeKeyword.SendStoryEvent(SomeLocation, Game.GetPlayer() as ObjectReference, None, 3, 0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SendStoryEvent {
                    keyword: ObjectRef::Property("MQGreybeardCallKeyword".into()),
                    location: None,
                    references: [None, None],
                    values: [0, 0],
                },
                Effect::SendStoryEvent {
                    keyword: ObjectRef::Property("SomeKeyword".into()),
                    location: Some(ObjectRef::Property("SomeLocation".into())),
                    references: [Some(ActorRef::Player), None],
                    values: [3, 0],
                },
            ])
        );
    }

    /// M47.2 — `BlockActivation` with its default, an explicit unblock, and
    /// FO4's prompt flag.
    #[test]
    fn block_activation_lowers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             SomeDoor.BlockActivation()\n\
             Alias_Gate.GetRef().BlockActivation(false)\n\
             SomeDoor.BlockActivation(true, true)\n EndFunction\n",
        );
        let door = || ObjectRef::Property("SomeDoor".into());
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::BlockActivation {
                    object: door(),
                    blocked: true
                },
                Effect::BlockActivation {
                    object: ObjectRef::Property("Alias_Gate".into()),
                    blocked: false
                },
                Effect::BlockActivation {
                    object: door(),
                    blocked: true
                },
            ])
        );
    }

    /// M47.2 — FO4's alias `TryToSetActorValue` and Skyrim's named `SetAV` /
    /// `SetActorValue`.
    #[test]
    fn actor_value_writes_lower_in_each_spelling() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_Danse.TryToSetActorValue(CA_WantsToTalk, 2.0)\n\
             Game.GetPlayer().SetAV(\"Variable06\", 3 as Float)\n\
             Alias_Saadia.GetActorRef().SetActorValue(\"Aggression\", 0.0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetActorValue {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Danse".into())),
                    actor_value: ObjectRef::Property("CA_WantsToTalk".into()),
                    value: crate::translate::effects::ValueExpr::Literal(2.0),
                },
                Effect::SetActorValueNamed {
                    actor: ActorRef::Player,
                    name: "Variable06".into(),
                    value: crate::translate::effects::ValueExpr::Literal(3.0),
                },
                Effect::SetActorValueNamed {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Saadia".into())),
                    name: "Aggression".into(),
                    value: crate::translate::effects::ValueExpr::Literal(0.0),
                },
            ])
        );
    }

    /// M47.2 — writes to a global's `.Value` (directly or through the quest
    /// script's property) and quest-variable increments.
    #[test]
    fn global_value_assignments_and_counters_lower() {
        let property = |name: &str| ObjectRef::Property(name.into());
        for (statement, expected) in [
            (
                "RadioReady.Value = 1.0",
                Effect::SetGlobalValue {
                    global: property("RadioReady"),
                    value: crate::translate::effects::ValueExpr::Literal(1.0),
                },
            ),
            (
                "kmyQuest.pRDSGlobal.Value = 1 as Float",
                Effect::SetGlobalValue {
                    global: property("pRDSGlobal"),
                    value: crate::translate::effects::ValueExpr::Literal(1.0),
                },
            ),
            (
                "Counter.Value = Counter.Value + 1.0",
                Effect::SetGlobalValue {
                    global: property("Counter"),
                    value: ValueExpr::Arith(
                        Box::new(ValueExpr::Global(property("Counter"))),
                        ArithOp::Add,
                        Box::new(ValueExpr::Literal(1.0)),
                    ),
                },
            ),
            (
                "kmyQuest.SynthsFreed = kmyQuest.SynthsFreed + 1",
                Effect::ModQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "SynthsFreed".into(),
                    delta: 1.0,
                },
            ),
            (
                "kmyQuest.Tries = kmyQuest.Tries - 2",
                Effect::ModQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "Tries".into(),
                    delta: -2.0,
                },
            ),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_8()\n\
                 Quest __temp = Self as Quest\n\
                 MyQuestScript kmyQuest = __temp as MyQuestScript\n\
                 {statement}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), Some(vec![expected]), "{statement}");
        }
        // A different variable on the right is not a counter.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Quest __temp = Self as Quest\n\
             MyQuestScript kmyQuest = __temp as MyQuestScript\n\
             kmyQuest.A = kmyQuest.B + 1\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    /// M47.2 — `ForceRefTo` with the player, an alias's reference or a
    /// property, and `Clear`, on an alias property receiver.
    #[test]
    fn alias_overrides_lower() {
        let alias = || ObjectRef::Property("Alias_Giver".into());
        for (call, target) in [
            (
                "Alias_Giver.ForceRefTo(Game.GetPlayer() as ObjectReference)",
                Some(ActorRef::Player),
            ),
            (
                "Alias_Giver.ForceRefTo(Alias_Other.GetReference())",
                Some(ActorRef::Object(ObjectRef::Property("Alias_Other".into()))),
            ),
            ("Alias_Giver.Clear()", None),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                Some(vec![Effect::ForceAlias {
                    alias: alias(),
                    target,
                }]),
                "{call}"
            );
        }
    }

    /// M47.2 — `AddPerk` in both arities and `RemovePerk`.
    #[test]
    fn perk_changes_lower() {
        let perk = || ObjectRef::Property("SomePerk".into());
        for (call, expected) in [
            (
                "Game.GetPlayer().AddPerk(SomePerk)",
                Effect::ChangePerk {
                    actor: ActorRef::Player,
                    perk: perk(),
                    add: true,
                },
            ),
            (
                "Companion.AddPerk(SomePerk, false)",
                Effect::ChangePerk {
                    actor: ActorRef::Object(ObjectRef::Property("Companion".into())),
                    perk: perk(),
                    add: true,
                },
            ),
            (
                "Game.GetPlayer().RemovePerk(SomePerk)",
                Effect::ChangePerk {
                    actor: ActorRef::Player,
                    perk: perk(),
                    add: false,
                },
            ),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
        }
    }

    /// M47.2 — the FO4 / Starfield spellings of the enable and lock writes.
    #[test]
    fn no_wait_try_to_and_unlock_spellings_lower() {
        let marker = || ObjectRef::Property("Marker".into());
        for (call, expected) in [
            (
                "Marker.EnableNoWait(false)",
                Effect::Enable {
                    object: marker(),
                    fade_in: false,
                },
            ),
            (
                "Marker.TryToEnable()",
                Effect::Enable {
                    object: marker(),
                    fade_in: false,
                },
            ),
            (
                "Marker.DisableNoWait(true)",
                Effect::Disable {
                    object: marker(),
                    fade_out: true,
                },
            ),
            (
                "Marker.TryToDisable()",
                Effect::Disable {
                    object: marker(),
                    fade_out: false,
                },
            ),
            (
                "Marker.Unlock(false)",
                Effect::SetLocked {
                    target: marker(),
                    locked: false,
                },
            ),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
        }
    }

    /// M47.2 — global writes from run-time numbers: `Mod`, a
    /// self-increment, a value derived from another global, arithmetic, and
    /// `SetValueInt` with a literal.
    #[test]
    fn relative_global_writes_lower() {
        let property = |name: &str| ObjectRef::Property(name.into());
        let global = |name: &str| Box::new(ValueExpr::Global(property(name)));
        let literal = |value: f32| Box::new(ValueExpr::Literal(value));
        let set = |target: &str, value| Effect::SetGlobalValue {
            global: property(target),
            value,
        };
        for (call, expected) in [
            (
                "Counter.Mod(1.0)",
                set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, literal(1.0))),
            ),
            (
                "Counter.SetValue(Counter.GetValue() + 1.0)",
                set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, literal(1.0))),
            ),
            (
                "RadioReady.SetValue(GameDaysPassed.GetValue() + 1.0)",
                set(
                    "RadioReady",
                    ValueExpr::Arith(global("GameDaysPassed"), ArithOp::Add, literal(1.0)),
                ),
            ),
            (
                "Counter.SetValue(Counter.GetValueInt() - 2)",
                set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Sub, literal(2.0))),
            ),
            (
                "Counter.SetValue(Counter.GetValue() * 2.0)",
                set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Mul, literal(2.0))),
            ),
            (
                "Counter.Mod(Other.GetValue())",
                set("Counter", ValueExpr::Arith(global("Counter"), ArithOp::Add, global("Other"))),
            ),
            ("GameHour.SetValueInt(9)", set("GameHour", ValueExpr::Literal(9.0))),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), Some(vec![expected]), "{call}");
        }
        let call =
            "Counter.SetValue(Counter.GetValue() * Game.GetPlayer().GetEquippedWeapon().GetBaseDamage())";
        let body = first_fn_body(&format!(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
        ));
        assert_eq!(lower_fragment(&body), None, "{call}");
    }

    /// A computed item (`Game.GetCaps()`), a count from an unknown call, a
    /// non-literal silent flag, a negative count or a fifth argument all
    /// decline rather than guess.
    #[test]
    fn remove_item_declines_what_it_cannot_read() {
        for call in [
            "Game.GetPlayer().RemoveItem(Gold001, GetRansom(), false, None)",
            "Game.GetPlayer().RemoveItem(Game.GetCaps(), GetRansom(), false, None)",
            "Game.GetPlayer().RemoveItem(Gold001, 5, bQuiet, None)",
            "Game.GetPlayer().RemoveItem(Gold001, -5)",
            "Game.GetPlayer().RemoveItem(Gold001, 5, false, None, 1)",
            "Game.GetPlayer().RemoveItem()",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{call}");
        }
    }

    /// M47.2 — the player gaining or losing a literal count of
    /// `Game.GetCaps()` is `PlayerCaps`, like `GivePlayerCaps` /
    /// `RemovePlayerCaps`.
    #[test]
    fn get_caps_transfers_are_player_caps() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\nFunction Fragment_8()\n\
             Game.GetPlayer().RemoveItem(Game.GetCaps() as Form, 75, false, None)\n\
             Game.GetPlayer().AddItem(Game.GetCaps() as Form, 20, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::PlayerCaps { delta: -75 },
                Effect::PlayerCaps { delta: 20 },
            ])
        );
    }

    /// M47.2 — quest-script property writes: through a quest local bound
    /// from `Self as Quest` (QF fragments), through `GetOwningQuest()`'s
    /// cast (topic-info fragments), and a Bool literal as 0 / 1.
    #[test]
    fn lowers_quest_variable_writes() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Quest __temp = Self as Quest\n\
             MS11QuestScript kmyQuest = __temp as MS11QuestScript\n\
             kmyQuest.FriendsKilled = 3\n\
             (Self.GetOwningQuest() as MS11QuestScript).GetInHjerimTovaPointer = true\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "FriendsKilled".into(),
                    value: 3.0,
                },
                Effect::SetQuestVariable {
                    quest: QuestRef::OwningQuest,
                    variable: "GetInHjerimTovaPointer".into(),
                    value: 1.0,
                },
            ])
        );
    }

    /// A compound operator, a bare `Self` receiver (a topic-info fragment's
    /// own script, not the quest), an unknown property receiver and an index
    /// target all decline. (`X = X + 1` lowers since M47.2.)
    #[test]
    fn quest_variable_writes_decline_what_they_cannot_name() {
        for statement in [
            "kmyQuest.Count += 1",
            "Self.Count = 1",
            "SomeObjectProperty.Count = 1",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_8()\n\
                 Quest __temp = Self as Quest\n\
                 MyQuestScript kmyQuest = __temp as MyQuestScript\n\
                 {statement}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{statement}");
        }
    }

    /// The first function of `src`, params and body, lowered as an INFO
    /// fragment.
    fn lower_first_info_fragment(src: &str) -> Option<Vec<Effect>> {
        let (script, errs) = parse_script(src).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let function = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Function(f) => Some(f.clone()),
                _ => None,
            })
            .expect("a function");
        lower_info_fragment(
            &function.params,
            &function.body,
            &HashSet::new(),
            &HashSet::new(),
            None,
            None,
        )
    }

    /// M47.2 — an INFO fragment's `akSpeakerRef`, and the Champollion
    /// `akSpeaker = akSpeakerRef as Actor` local, are the line's speaker.
    #[test]
    fn info_fragments_lower_the_speaker() {
        let effects = lower_first_info_fragment(
            "ScriptName TIF__0005520F extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Actor akSpeaker = akSpeakerRef as Actor\n\
             akSpeaker.AddItem(Gold001, 10)\n\
             akSpeakerRef.RemoveItem(Ale)\n EndFunction\n",
        );
        assert_eq!(
            effects,
            Some(vec![
                Effect::AddItem {
                    container: ActorRef::Speaker,
                    item: ObjectRef::Property("Gold001".into()),
                    count: ItemCount::Literal(10),
                },
                Effect::RemoveItem {
                    container: ActorRef::Speaker,
                    item: ObjectRef::Property("Ale".into()),
                    count: ItemCount::Literal(1),
                    destination: None,
                },
            ])
        );
        // The quest/scene path has no speaker: the same body declines there
        // rather than treating `akSpeaker` as a VMAD property.
        let body = first_fn_body(
            "ScriptName TIF__0005520F extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Actor akSpeaker = akSpeakerRef as Actor\n\
             akSpeaker.AddItem(Gold001, 10)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    /// A speaker fragment with a latent tail declines (the speaker is a live
    /// entity a saved continuation cannot name) — also when the speaker is
    /// only used inside an `If` branch, which lowers against a scope clone.
    #[test]
    fn a_speaker_fragment_with_a_latent_tail_declines() {
        for body in [
            "akSpeaker.AddItem(Gold001, 10)\nUtility.Wait(1.0)",
            "If Self.GetOwningQuest().GetStageDone(10)\nakSpeaker.AddItem(Gold001, 10)\nEndIf\nUtility.Wait(1.0)",
        ] {
            let effects = lower_first_info_fragment(&format!(
                "ScriptName TIF__1 extends TopicInfo\n\
                 Function Fragment_0(ObjectReference akSpeakerRef)\n\
                 Actor akSpeaker = akSpeakerRef as Actor\n\
                 {body}\n EndFunction\n"
            ));
            assert_eq!(effects, None, "{body}");
        }
        // Without the speaker the same wait lowers.
        assert!(lower_first_info_fragment(
            "ScriptName TIF__1 extends TopicInfo\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Game.GetPlayer().AddItem(Gold001, 10)\n\
             Utility.Wait(1.0)\n EndFunction\n"
        )
        .is_some());
    }

    /// M47.2 — faction membership calls on an alias actor, the player and
    /// (in a topic-info fragment) the speaker; `SetFactionRank` takes a
    /// literal rank in `i8` range, negative included.
    #[test]
    fn lowers_faction_membership_calls() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_VictimFather.GetActorRef().AddToFaction(CR08ExclusionFaction)\n\
             Game.GetPlayer().RemoveFromFaction(PlayerFaction)\n\
             Alias_Boss.GetActorRef().SetFactionRank(BanditFaction, -1)\n EndFunction\n",
        );
        let object = |name: &str| ObjectRef::Property(name.into());
        assert_eq!(
            lower_fragment(&first_fn_body(
                "ScriptName QF extends Quest\n\
                 Function Fragment_9()\n\
                 Game.GetPlayer().RemoveFromAllFactions()\n EndFunction\n"
            )),
            Some(vec![Effect::ChangeFaction {
                actor: ActorRef::Player,
                faction: object(""),
                change: crate::FactionChange::RemoveAll,
            }])
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::ChangeFaction {
                    actor: ActorRef::Object(object("Alias_VictimFather")),
                    faction: object("CR08ExclusionFaction"),
                    change: crate::FactionChange::Add,
                },
                Effect::ChangeFaction {
                    actor: ActorRef::Player,
                    faction: object("PlayerFaction"),
                    change: crate::FactionChange::Remove,
                },
                Effect::ChangeFaction {
                    actor: ActorRef::Object(object("Alias_Boss")),
                    faction: object("BanditFaction"),
                    change: crate::FactionChange::SetRank(-1),
                },
            ])
        );
        assert_eq!(
            lower_first_info_fragment(
                "ScriptName TIF__1 extends TopicInfo\n\
                 Function Fragment_0(ObjectReference akSpeakerRef)\n\
                 Actor akSpeaker = akSpeakerRef as Actor\n\
                 akSpeaker.RemoveFromFaction(WIKillThankFaction)\n EndFunction\n"
            ),
            Some(vec![Effect::ChangeFaction {
                actor: ActorRef::Speaker,
                faction: object("WIKillThankFaction"),
                change: crate::FactionChange::Remove,
            }])
        );
        for call in [
            "Game.GetPlayer().SetFactionRank(SomeFaction, 300)",
            "Game.GetPlayer().SetFactionRank(SomeFaction, iRank)",
            "Game.GetPlayer().AddToFaction()",
            "Game.GetPlayer().AddToFaction(SomeFaction, 1)",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_8()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{call}");
        }
    }

    /// M47.2 — Skyrim's `ReferenceAlias.GetReference()` /
    /// `GetActorReference()` resolve like FO4's `GetRef()` / `GetActorRef()`.
    #[test]
    fn skyrim_alias_getters_resolve_like_fo4s() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_Treasure.GetReference().Enable(false)\n\
             Alias_Cicero.GetActorReference().EvaluatePackage()\n EndFunction\n",
        );
        let effects = lower_fragment(&body).expect("both lower");
        assert!(matches!(
            &effects[0],
            Effect::Enable { object, .. } if *object == ObjectRef::Property("Alias_Treasure".into())
        ));
        assert_eq!(
            effects[1],
            Effect::EvaluatePackage { actor: ActorRef::Object(ObjectRef::Property("Alias_Cicero".into())) }
        );
    }

    /// M47.2 — FO4 / Starfield two-argument `SetValue(akAV, afValue)` sets an
    /// actor value; the one-argument form stays `GlobalVariable.SetValue`.
    #[test]
    fn lowers_actor_value_set_value() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_SkinnyMalone.GetActorRef().SetValue(pAggression, 2.0)\n\
             SomeGlobal.SetValue(3.0)\n EndFunction\n",
        );
        let effects = lower_fragment(&body).expect("lowers");
        assert_eq!(
            effects[0],
            Effect::SetActorValue {
                actor: ActorRef::Object(ObjectRef::Property("Alias_SkinnyMalone".into())),
                actor_value: ObjectRef::Property("pAggression".into()),
                value: crate::translate::effects::ValueExpr::Literal(2.0),
            }
        );
        assert!(matches!(effects[1], Effect::SetGlobalValue { .. }));
        let computed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_8()\n\
             Alias_A.GetActorRef().SetValue(pAggression, fLevel)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&computed), None);
    }

    /// A [`ScriptLibrary`] over `.psc` sources, keyed by lowercased name.
    struct PscLibrary(HashMap<String, std::sync::Arc<byroredux_papyrus::ast::Script>>);

    impl PscLibrary {
        fn new(sources: &[&str]) -> Self {
            Self(
                sources
                    .iter()
                    .map(|src| {
                        let (script, errs) = parse_script(src).expect("parses");
                        assert!(errs.is_empty(), "{errs:?}");
                        (script.name.node.0.to_ascii_lowercase(), std::sync::Arc::new(script))
                    })
                    .collect(),
            )
        }
    }

    impl ScriptLibrary for PscLibrary {
        fn script(&self, name: &str) -> Option<std::sync::Arc<byroredux_papyrus::ast::Script>> {
            self.0.get(&name.to_ascii_lowercase()).cloned()
        }
    }

    /// M47.2 — a property typed with a script that (transitively) extends
    /// Quest is a quest property once the library supplies the `extends`
    /// chain, so a write to its variable lowers.
    #[test]
    fn quest_script_typed_properties_resolve_through_the_library() {
        let library = PscLibrary::new(&[
            "ScriptName TG04PQSScript extends TG04BaseScript\nInt Property pTG04PostChoice Auto\n",
            "ScriptName TG04BaseScript extends Quest\n",
            "ScriptName NotAQuestScript extends ObjectReference\n",
        ]);
        const SOURCE: &str = "ScriptName TIF_000D3AB3 extends TopicInfo\n\
             TG04PQSScript Property pTG04PQS Auto\n\
             NotAQuestScript Property Other Auto\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             pTG04PQS.pTG04PostChoice = 3\n\
             EndFunction\n";
        let (script, errs) = parse_script(SOURCE).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let without = crate::fragment::quest_property_names(&script);
        assert!(without.is_empty());
        let with = crate::fragment::quest_property_names_with_library(&script, Some(&library));
        assert_eq!(with, ["ptg04pqs".to_owned()].into_iter().collect());
        let body = first_fn_body(SOURCE);
        assert_eq!(
            lower_fragment_with_script_metadata(&body, &with, &HashSet::new(), None),
            Some(vec![Effect::SetQuestVariable {
                quest: QuestRef::Property("pTG04PQS".into()),
                variable: "pTG04PostChoice".into(),
                value: 3.0,
            }])
        );
        assert_eq!(
            lower_fragment_with_script_metadata(&body, &without, &HashSet::new(), None),
            None
        );
    }

    /// M47.2 — arguments to an inlined quest-script function: the speaker
    /// binds the parameter to the speaker, a fragment property rides the
    /// quest scope's bindings, and a number becomes a run-time local set in
    /// the caller's context.
    #[test]
    fn inlined_quest_script_calls_bind_their_arguments() {
        let library = PscLibrary::new(&[
            "ScriptName SpouseScript extends Quest\n\
             Function MoveSpouse(Actor Spouse, ReferenceAlias NewHouse, Int Stage)\n\
             Spouse.EvaluatePackage()\n NewHouse.TryToEvaluatePackage()\n\
             If Stage > 10\n Self.SetStage(30)\n EndIf\n EndFunction\n",
        ]);
        let source = "ScriptName TIF_Test extends TopicInfo\n\
             ReferenceAlias Property SolitudeHouse Auto\n\
             Function Fragment_0(ObjectReference akSpeakerRef)\n\
             Actor akSpeaker = akSpeakerRef as Actor\n\
             (Self.GetOwningQuest() as SpouseScript).MoveSpouse(akSpeaker, SolitudeHouse, 20)\n EndFunction\n";
        let (script, errs) = parse_script(source).expect("parses");
        assert!(errs.is_empty(), "{errs:?}");
        let function = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Function(f) => Some(f.clone()),
                _ => None,
            })
            .expect("a function");
        let lowered = lower_info_fragment(
            &function.params,
            &function.body,
            &HashSet::new(),
            &HashSet::new(),
            None,
            Some(&library),
        )
        .expect("lowers");
        let [Effect::SetLocal { local, value }, Effect::QuestScope {
            quest,
            effects,
            bindings,
        }] = lowered.as_slice()
        else {
            panic!("{lowered:?}");
        };
        assert_eq!(local, "spousescript.movespouse.stage");
        assert_eq!(value, &ValueExpr::Literal(20.0));
        assert_eq!(quest, &QuestRef::OwningQuest);
        assert_eq!(
            bindings,
            &vec![(
                "inline.spousescript.movespouse.newhouse".to_owned(),
                "SolitudeHouse".to_owned()
            )]
        );
        let [Effect::EvaluatePackage {
            actor: ActorRef::Speaker,
        }, Effect::EvaluatePackage {
            actor: ActorRef::Object(house),
        }, Effect::Conditional { guards, .. }] = effects.as_slice()
        else {
            panic!("{effects:?}");
        };
        assert_eq!(
            house,
            &ObjectRef::Property("inline.spousescript.movespouse.newhouse".into())
        );
        assert!(matches!(
            guards.as_slice(),
            [FragmentGuard::Compare { left: ValueExpr::Local(local), .. }]
                if local == "spousescript.movespouse.stage"
        ), "{guards:?}");
    }

    /// M47.2 — an INFO fragment's call into its quest's script inlines as a
    /// `QuestScope` (the callee's properties live on the quest's VMAD), and so
    /// does a statement reading a quest-script property (`kmyQuest.CW`); a
    /// latent callee declines.
    #[test]
    fn info_fragment_calls_into_the_quest_script_lower_as_quest_scopes() {
        let library = PscLibrary::new(&[
            "ScriptName RadioScript extends Quest\n\
             ObjectReference Property RadioMarker Auto\n\
             CWScript Property CW Auto\n\
             Function UpdateRadio()\n RadioMarker.Enable()\n Self.SetStage(10)\n EndFunction\n\
             Function SlowUpdate()\n Utility.Wait(1.0)\n Self.SetStage(20)\n EndFunction\n\
             Int SongCount\nInt CurrentSong\nInt LastSong01\n\
             Function Rotate()\n SongCount = SongCount + 1\n LastSong01 = CurrentSong\n CurrentSong = 0\n EndFunction\n",
            "ScriptName CWScript extends Quest\n",
        ]);
        let lowered = |statement: &str| {
            let source = format!(
                "ScriptName TIF_Test extends TopicInfo\n\
                 Function Fragment_0(ObjectReference akSpeakerRef)\n\
                 RadioScript kmyQuest = Self.GetOwningQuest() as RadioScript\n\
                 {statement}\n EndFunction\n"
            );
            let (script, errs) = parse_script(&source).expect("parses");
            assert!(errs.is_empty(), "{errs:?}");
            let function = script
                .body
                .iter()
                .find_map(|item| match &item.node {
                    ScriptItem::Function(f) => Some(f.clone()),
                    _ => None,
                })
                .expect("a function");
            lower_info_fragment(
                &function.params,
                &function.body,
                &HashSet::new(),
                &HashSet::new(),
                None,
                Some(&library),
            )
        };
        assert_eq!(
            lowered("kmyQuest.UpdateRadio()"),
            Some(vec![Effect::QuestScope {
                bindings: Vec::new(),
                quest: QuestRef::OwningQuest,
                effects: vec![
                    Effect::Enable {
                        object: ObjectRef::Property("RadioMarker".into()),
                        fade_in: false,
                    },
                    Effect::SetStage {
                        quest: QuestRef::SelfRef,
                        stage: 10,
                    },
                ],
            }])
        );
        assert_eq!(lowered("kmyQuest.SlowUpdate()"), None, "latent callee");
        // The speaker is VMAD-free; a fragment property beside a quest
        // property would resolve against the wrong VMAD, so it declines.
        assert_eq!(
            lowered("kmyQuest.RadioMarker.Disable()"),
            Some(vec![Effect::QuestScope {
                bindings: Vec::new(),
                quest: QuestRef::OwningQuest,
                effects: vec![Effect::Disable {
                    object: ObjectRef::Property("RadioMarker".into()),
                    fade_out: false,
                }],
            }])
        );
        assert_eq!(
            lowered("FragmentDoor.MoveTo(kmyQuest.RadioMarker)"),
            None,
            "a fragment property beside a quest property"
        );
        // The FO4 radio shape: the quest script's own variables.
        assert_eq!(
            lowered("kmyQuest.Rotate()"),
            Some(vec![Effect::QuestScope {
                bindings: Vec::new(),
                quest: QuestRef::OwningQuest,
                effects: vec![
                    Effect::ModQuestVariable {
                        quest: QuestRef::SelfRef,
                        variable: "SongCount".into(),
                        delta: 1.0,
                    },
                    Effect::CopyQuestVariable {
                        quest: QuestRef::SelfRef,
                        from: "CurrentSong".into(),
                        to: "LastSong01".into(),
                    },
                    Effect::SetQuestVariable {
                        quest: QuestRef::SelfRef,
                        variable: "CurrentSong".into(),
                        value: 0.0,
                    },
                ],
            }])
        );
        // A quest-script property read from the INFO resolves through the
        // quest's VMAD: the statement is scoped.
        assert_eq!(
            lowered("kmyQuest.CW.SetStage(2)"),
            Some(vec![Effect::QuestScope {
                bindings: Vec::new(),
                quest: QuestRef::OwningQuest,
                effects: vec![Effect::SetStage {
                    quest: QuestRef::Property("CW".into()),
                    stage: 2,
                }],
            }])
        );
    }

    /// M47.2 — a quest fragment's write to its own script variable
    /// (`::SudiFound_var = True`, `::chestCount_var += 1` as decompiled) is a
    /// quest script variable; without a quest VMAD it still declines.
    #[test]
    fn quest_fragment_script_variable_writes_lower() {
        use byroredux_papyrus::ast::AssignOp;
        let var = |name: &str| sp(Expr::Ident(Identifier(name.into())));
        let body = vec![
            sp(Stmt::Assign {
                target: var("::SudiFound_var"),
                op: AssignOp::Eq,
                value: sp(Expr::BoolLit(true)),
            }),
            sp(Stmt::Assign {
                target: var("::chestCount_var"),
                op: AssignOp::Eq,
                value: sp(Expr::BinaryOp {
                    left: Box::new(var("::chestCount_var")),
                    op: BinaryOp::Add,
                    right: Box::new(sp(Expr::IntLit(1))),
                }),
            }),
        ];
        let library = PscLibrary::new(&[]);
        assert_eq!(
            lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), None, Some(&library)),
            Some(vec![
                Effect::SetQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "::SudiFound_var".into(),
                    value: 1.0,
                },
                Effect::ModQuestVariable {
                    quest: QuestRef::SelfRef,
                    variable: "::chestCount_var".into(),
                    delta: 1.0,
                },
            ])
        );
        assert_eq!(lower_fragment(&body), None, "no quest VMAD");
    }

    /// M47.2 — a reference script's variable, written through a cast to its
    /// script type; a quest-script cast, an undeclared variable and an
    /// untyped receiver decline.
    #[test]
    fn reference_script_variable_writes_lower_through_a_typed_cast() {
        let library = PscLibrary::new(&[
            "ScriptName MiraakBattleBase extends Actor\nBool fireMiraakKill\n",
            "ScriptName MiraakBattleScript extends MiraakBattleBase\n",
            "ScriptName SomeQuestScript extends Quest\nBool fireMiraakKill\n",
        ]);
        let lowered = |statement: &str| {
            let body = first_fn_body(&format!(
                "ScriptName QF_Test extends Quest\nFunction Fragment_1()\n{statement}\nEndFunction\n"
            ));
            lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), None, Some(&library))
        };
        assert_eq!(
            lowered("(Alias_Miraak.GetReference() as MiraakBattleScript).fireMiraakKill = True"),
            Some(vec![Effect::SetReferenceVariable {
                object: ObjectRef::Property("Alias_Miraak".into()),
                variable: "::fireMiraakKill_var".into(),
                value: 1.0,
            }])
        );
        assert_eq!(lowered("(Alias_Miraak.GetReference() as MiraakBattleScript).Missing = True"), None);
        assert_eq!(lowered("(Alias_Miraak.GetReference() as SomeQuestScript).fireMiraakKill = True"), None);
        assert_eq!(lowered("SomeProperty.fireMiraakKill = True"), None);
    }

    /// M47.2 — a quest reached through the fragment quest's own typed
    /// property (`kmyQuest.CW`), and a property cast to a quest script,
    /// lower as quest properties on the quest path (the library supplies the
    /// types); a non-quest property type declines.
    #[test]
    fn owner_quest_properties_and_quest_script_casts_lower_with_the_library() {
        let library = PscLibrary::new(&[
            "ScriptName CWQuickstartScript extends Quest\n\
             CWScript Property CW Auto\n\
             ObjectReference Property Door Auto\n",
            "ScriptName CWScript extends Quest\nInt Property PlayerGotIntro Auto\n",
            "ScriptName MS13QuestScript extends Quest\nBool Property CamillaEscort Auto\n",
        ]);
        let lowered = |statements: &str| {
            let body = first_fn_body(&format!(
                "ScriptName QF_Test extends Quest\n\
                 Quest Property MS13 Auto\n\
                 Function Fragment_2()\n\
                 CWQuickstartScript kmyQuest = Self as CWQuickstartScript\n\
                 {statements}\n EndFunction\n"
            ));
            lower_quest_fragment(
                &body,
                &["ms13".to_owned()].into_iter().collect(),
                &HashSet::new(),
                None,
                Some(&library),
            )
        };
        assert_eq!(
            lowered("kmyQuest.CW.PlayerGotIntro = 1\n kmyQuest.CW.SetStage(2)\n (MS13 as MS13QuestScript).CamillaEscort = True"),
            Some(vec![
                Effect::SetQuestVariable {
                    quest: QuestRef::Property("CW".into()),
                    variable: "PlayerGotIntro".into(),
                    value: 1.0,
                },
                Effect::SetStage {
                    quest: QuestRef::Property("CW".into()),
                    stage: 2,
                },
                Effect::SetQuestVariable {
                    quest: QuestRef::Property("MS13".into()),
                    variable: "CamillaEscort".into(),
                    value: 1.0,
                },
            ])
        );
        assert_eq!(lowered("kmyQuest.Door.SetStage(2)"), None, "not a quest");
        // An object property of the quest script is an object receiver; a
        // quest-typed one is not.
        assert_eq!(
            lowered("kmyQuest.Door.Disable()"),
            Some(vec![Effect::Disable {
                object: ObjectRef::Property("Door".into()),
                fade_out: false,
            }])
        );
        assert_eq!(lowered("kmyQuest.CW.Disable()"), None, "a quest is no reference");
    }

    const MOTHPRIEST_SCRIPT: &str = "ScriptName DLC1VQ03MothpriestFightScript extends Quest\n\
        ReferenceAlias Property Dexion Auto\n\
        Spell Property DLC1AbMothpriestThrallFX1 Auto\n\
        Function StartUp()\n\
        Dexion.GetActorReference().AddSpell(DLC1AbMothpriestThrallFX1, false)\n\
        Self.Finish()\n\
        EndFunction\n\
        Function Finish()\n\
        Self.SetStage(99)\n\
        EndFunction\n\
        Function Loops()\n\
        Self.Loops()\n\
        EndFunction\n\
        Function WithArgs(Int n)\n\
        Self.SetStage(1)\n\
        EndFunction\n";

    /// M47.2 — a quest fragment's call into its quest's own script inlines
    /// the callee's body (transitively, `Self.Finish()` included), in the
    /// quest's scope.
    #[test]
    fn quest_fragments_inline_calls_into_the_quest_script() {
        let library = PscLibrary::new(&[MOTHPRIEST_SCRIPT]);
        let body = first_fn_body(
            "ScriptName QF_DLC1VQ03 extends Quest\n\
             Function Fragment_25()\n\
             Quest __temp\n\
             DLC1VQ03MothpriestFightScript kmyQuest\n\
             __temp = Self as Quest\n\
             kmyQuest = __temp as DLC1VQ03MothpriestFightScript\n\
             Self.SetObjectiveCompleted(41)\n\
             kmyQuest.StartUp()\n EndFunction\n",
        );
        let lowered = lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), None, Some(&library));
        assert_eq!(
            lowered,
            Some(vec![
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: 41,
                    completed: true,
                },
                Effect::AddSpell {
                    actor: ActorRef::Object(ObjectRef::Property("Dexion".into())),
                    spell: ObjectRef::Property("DLC1AbMothpriestThrallFX1".into()),
                },
                Effect::SetStage { quest: QuestRef::SelfRef, stage: 99 },
            ])
        );
        // Without a library (and on every non-quest path) the call declines.
        assert_eq!(lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), None, None), None);
    }

    /// Recursion, a callee with parameters, an unknown function and a
    /// receiver that is another quest all decline.
    #[test]
    fn quest_script_inlining_declines_what_it_cannot_bound() {
        let library = PscLibrary::new(&[MOTHPRIEST_SCRIPT]);
        for call in ["kmyQuest.Loops()", "kmyQuest.Missing()", "OtherQuest.StartUp()"] {
            let body = first_fn_body(&format!(
                "ScriptName QF_DLC1VQ03 extends Quest\n\
                 Function Fragment_25()\n\
                 DLC1VQ03MothpriestFightScript kmyQuest = Self as DLC1VQ03MothpriestFightScript\n\
                 {call}\n EndFunction\n"
            ));
            assert_eq!(
                lower_quest_fragment(&body, &HashSet::new(), &HashSet::new(), None, Some(&library)),
                None,
                "{call}"
            );
        }
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

    /// CK *IsStopped - Quest*: "completely stopped" is the opposite of running.
    #[test]
    fn is_stopped_is_the_negation_of_running() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             If Self.IsStopped()\n\
               Self.SetStage(20)\n\
             EndIf\n EndFunction\n",
        );
        let effects = lower_fragment(&body).expect("lowers");
        assert!(
            matches!(
                effects.as_slice(),
                [Effect::Conditional { guards, .. }]
                    if matches!(guards.as_slice(), [FragmentGuard::QuestRunning { running: false, .. }])
            ),
            "{effects:?}"
        );
    }

    #[test]
    fn lowers_set_loc_ref_type() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             SomeRef.SetLocRefType(SomeLoc, BossType)\n\
             SomeRef.SetLocRefType(SomeLoc, None)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetLocRefType {
                    object: ObjectRef::Property("SomeRef".into()),
                    ref_type: Some(ObjectRef::Property("BossType".into())),
                },
                Effect::SetLocRefType {
                    object: ObjectRef::Property("SomeRef".into()),
                    ref_type: None,
                },
            ])
        );
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
                offset: [0.0; 3],
                match_rotation: true,
            }])
        );
    }

    /// M47.2 — the player's `MoveTo` (a teleport) lowers to its own effect;
    /// the default-offsets rule still applies.
    #[test]
    fn lowers_the_players_move_to() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 0.0, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::MovePlayerTo {
                destination: ObjectRef::Property("SomeMarker".into()),
            }])
        );
        let offset = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_11()\n\
             Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 64.0, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&offset), None);
    }

    #[test]
    fn lowers_disable_with_optional_fade_out() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeMarker.Disable(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Disable {
                object: ObjectRef::Property("SomeMarker".into()),
                fade_out: false,
            }])
        );
    }

    /// #3489 — mirrors `lowers_disable_with_optional_fade_out`: `Enable()` is
    /// the more common of the pair in the real corpus (3,005 vs 2,587 calls)
    /// and had no primitive at all before this fix, so every fragment
    /// containing one declined in full.
    #[test]
    fn lowers_enable_with_optional_fade_in() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeMarker.Enable(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Enable {
                object: ObjectRef::Property("SomeMarker".into()),
                fade_in: false,
            }])
        );
    }

    /// CK *MoveTo - ObjectReference*: the offsets and `abMatchRotation` are
    /// part of the instruction, and lower with it.
    #[test]
    fn move_to_carries_its_offset_and_rotation_flag() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 1.0, -2.0, 10.0, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::MoveTo {
                moved: ObjectRef::Property("SomeRef".into()),
                destination: ObjectRef::Property("SomeMarker".into()),
                offset: [1.0, -2.0, 10.0],
                match_rotation: false,
            }])
        );
        // The player's teleport carries neither, so a real one declines.
        let player = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             Game.GetPlayer().MoveTo(SomeMarker, 0.0, 0.0, 10.0, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&player), None);
    }

    /// Regression for #3487. The Papyrus compiler emits every parameter
    /// of a call, including omitted ones left at their declared default,
    /// and quest fragments only ever reach the effect table through the
    /// `.pex` frontend. The two shapes here are what
    /// `SomeRef.MoveTo(SomeMarker)` compiles to on Skyrim/FO4 (five args)
    /// and on Starfield (six, with `abRotateOffset`); between them they
    /// are 3,253 of the corpus's 3,334 `MoveTo` calls, every one of which
    /// the old `args.len() == 1` guard declined.
    #[test]
    fn lowers_the_default_materialized_move_to_shapes() {
        let expected = Some(vec![Effect::MoveTo {
            moved: ObjectRef::Property("SomeRef".into()),
            destination: ObjectRef::Property("SomeMarker".into()),
            offset: [0.0; 3],
            match_rotation: true,
        }]);

        // Skyrim / Fallout 4: akTarget + 3 offsets + abMatchRotation.
        let skyrim = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&skyrim), expected);

        // Starfield: the same plus `abRotateOffset = false`.
        let starfield = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&starfield), expected);
    }

    /// #3487 — the acceptance above is exactly "every trailing argument
    /// is a literal at its Papyrus default", not "ignore the tail".
    #[test]
    fn move_to_declines_rotate_offset_variables_and_over_long_tails() {
        // Starfield's `abRotateOffset = true` is not modelled.
        let rotate_offset = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&rotate_offset), None);

        // A non-literal offset: the runtime value is unknown, so the
        // default cannot be assumed.
        let variable_offset = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             float dz = 4.0\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, dz, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&variable_offset), None);

        // Longer than any shipped game's signature.
        let too_long = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_12()\n\
             SomeRef.MoveTo(SomeMarker, 0.0, 0.0, 0.0, true, false, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&too_long), None);
    }

    #[test]
    fn lowers_scene_start_and_stop_requests() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_13()\n\
             IntroScene.Start()\n\
             OldScene.Stop()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::StartScene {
                    scene: ObjectRef::Property("IntroScene".into()),
                },
                Effect::StopScene {
                    scene: ObjectRef::Property("OldScene".into()),
                },
            ])
        );
    }

    /// Regression for #2538 / SCR-D5-NEW10-01. `Quest Property
    /// MQ101 Auto` called with `.Start()` shares the identical bare-
    /// identifier `.Start()` AST shape `lowers_scene_start_and_stop_requests`
    /// above pins for a genuine scene property — nothing in the AST
    /// distinguishes them. `lower_fragment` (no property-type context —
    /// the state every call site had pre-#2538) cannot tell them apart
    /// and reproduces the original bug's exact symptom: this assertion
    /// pins that documented limitation of the context-free path, not a
    /// desired outcome — it's why `lower_fragment_with_quest_properties`
    /// (below) exists. `populate_quest_fragments_from_script` (the real
    /// production caller) always supplies context, so this path is
    /// exercised by tests only.
    #[test]
    fn quest_start_on_a_direct_property_declines_rather_than_mislowering_to_scene() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             MQ101.Start()\n EndFunction\n",
        );
        // Without quest-property context, the ambiguity genuinely can't
        // be resolved from the AST alone — this is the same "silently
        // becomes StartScene" outcome #2538 reported, still reachable
        // whenever no context is available. Pinned here so a future
        // change to the context-free fallback is a deliberate choice,
        // not an accident.
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::StartScene {
                scene: ObjectRef::Property("MQ101".into()),
            }])
        );

        // With quest-property context (the real `populate_quest_fragments_
        // from_script` call path), the ambiguity is resolved *positively*:
        // the receiver is a known `Quest` property, so it lowers to
        // `StartQuest` — not to `StartScene` (the #2538 bug) and not to a
        // whole-fragment decline (#2538's original fix, which discarded
        // every sibling effect to avoid guessing).
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
        assert_eq!(
            lower_fragment_with_quest_properties(&body, &quest_properties),
            Some(vec![Effect::StartQuest {
                quest: QuestRef::Property("MQ101".into()),
            }]),
            "a known Quest Property called with .Start() must lower to StartQuest"
        );

        // Same statement as the decompiler actually emits it — an
        // auto-property read arrives as its backing variable `::MQ101_var`,
        // which the `.psc` Ident regex cannot express, so this shape has to
        // be built by hand (#2657). It must resolve identically.
        let decompiled = Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier("::MQ101_var".into())))),
                member: sp(Identifier("Start".into())),
            })),
            args: vec![],
        };
        assert_eq!(
            classify_effect(
                &decompiled,
                &Scope {
                    known_quest_properties: quest_properties.clone(),
                    ..Scope::default()
                }
            ),
            Some(Effect::StartQuest {
                quest: QuestRef::Property("::MQ101_var".into()),
            }),
            "the backing-variable form the decompiler emits must resolve too"
        );
    }

    /// SCR-D5-NEW11-02 (#2653) — `Reset` is declared on `Quest`,
    /// `ObjectReference` and `Cell`; `SetActive` on `Quest` and `Weather`.
    /// The zero-arg AST shape is identical, so a bare non-Quest property
    /// receiver must decline rather than be claimed as a quest effect
    /// (which would also claim every sibling effect in the fragment).
    #[test]
    fn reset_and_set_active_decline_on_a_non_quest_property_receiver() {
        let reset = first_fn_body(
            "ScriptName QF extends Quest\n\
             ObjectReference Property MyContainer Auto\n\
             Function Fragment_99()\n\
             MyContainer.Reset()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&reset),
            None,
            "ObjectReference.Reset() must not be claimed as ResetQuest"
        );

        let set_active = first_fn_body(
            "ScriptName QF extends Quest\n\
             Weather Property SomeWeather Auto\n\
             Function Fragment_99()\n\
             SomeWeather.SetActive(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&set_active),
            None,
            "Weather.SetActive() must not be claimed as SetQuestActive"
        );
    }

    /// The narrowing must not cost the genuine cases: a real `Quest`
    /// property, `Self`, and `GetOwningQuest()` all still lower.
    #[test]
    fn reset_and_set_active_still_lower_on_an_unambiguous_quest_receiver() {
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();

        let reset = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             MQ101.Reset()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment_with_quest_properties(&reset, &quest_properties),
            Some(vec![Effect::ResetQuest {
                quest: QuestRef::Property("MQ101".into()),
            }])
        );

        let self_active = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_99()\n\
             Self.SetActive(true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&self_active),
            Some(vec![Effect::SetQuestActive {
                quest: QuestRef::SelfRef,
                active: true,
            }])
        );
    }

    /// Companion: a genuinely scene-typed property with the same context
    /// present (but not naming the scene property) must still lower
    /// correctly — the fix must not make `prim_start_scene` overly broad
    /// in its refusal.
    #[test]
    fn scene_start_still_lowers_when_quest_property_context_is_present_but_unrelated() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property MQ101 Auto\n\
             Function Fragment_99()\n\
             IntroScene.Start()\n EndFunction\n",
        );
        let quest_properties: HashSet<String> = ["mq101".to_string()].into_iter().collect();
        assert_eq!(
            lower_fragment_with_quest_properties(&body, &quest_properties),
            Some(vec![Effect::StartScene {
                scene: ObjectRef::Property("IntroScene".into()),
            }])
        );
    }

    // ---- #3159: Lock / SetLockLevel ----

    /// The motivating shape. Before this primitive existed the whole
    /// fragment declined on `Lock(false)` — `lower_fragment`'s
    /// `_ => return None` arm is all-or-nothing — so the objective and the
    /// stage advance were discarded along with the unlock, and the failure
    /// presented to the player as "the quest stalled" rather than "the door
    /// is locked".
    #[test]
    fn an_unlock_no_longer_takes_its_sibling_effects_down_with_it() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(false)\n\
             Self.SetObjectiveCompleted(10, true)\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        let lowered = lower_fragment(&body).expect("the fragment must lower");
        assert_eq!(
            lowered.len(),
            3,
            "all three effects must survive, not just the two that were \
             already modeled: {lowered:?}"
        );
        assert_eq!(
            lowered[0],
            Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: false,
            }
        );
    }

    /// Papyrus defaults `abLock` to `true`, so a bare `Lock()` locks.
    #[test]
    fn a_bare_lock_call_defaults_to_locking() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: true,
            }])
        );
    }

    /// #2289 — the decline half. A non-literal lock state, a non-literal
    /// `abAsOwner`/`abTraverseLinkedLockables`, and an argument past the
    /// declared three (Starfield's dialect) must all decline rather than
    /// be guessed at, matching `prim_set_open`'s conservative shape.
    /// (#5653 narrowed the over-long leg from two arguments to three:
    /// `Lock` declares two parameters, and the compiler emits the
    /// declared arity, so the old bound at one never matched a compiled
    /// call. The Starfield dialect then added a third declared parameter —
    /// see `lock_lowers_at_the_starfield_three_parameter_arity` — moving
    /// the over-long bound to four.)
    #[test]
    fn lock_declines_on_a_non_literal_or_over_long_argument() {
        let computed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Bool Property ShouldLock Auto\n\
             Function Fragment_7()\n\
             MyDoor.Lock(ShouldLock)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&computed),
            None,
            "a variable-driven lock state must decline, not be assumed"
        );

        let computed_owner = first_fn_body(
            "ScriptName QF extends Quest\n\
             Bool Property ShouldFlag Auto\n\
             Function Fragment_7()\n\
             MyDoor.Lock(true, ShouldFlag)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&computed_owner),
            None,
            "a variable-driven abAsOwner must decline, not be assumed"
        );

        let computed_traverse = first_fn_body(
            "ScriptName QF extends Quest\n\
             Bool Property ShouldTraverse Auto\n\
             Function Fragment_7()\n\
             MyDoor.Lock(true, false, ShouldTraverse)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&computed_traverse),
            None,
            "a variable-driven abTraverseLinkedLockables must decline, not be assumed"
        );

        let extra = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(true, false, true, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&extra),
            None,
            "an argument past the three the widest Lock dialect declares must decline"
        );
    }

    /// #5653 — the compiled shape lowers: the CK compiler materializes the
    /// declared arity, so real content's `Lock(abLock, abAsOwner)` carries
    /// two arguments (the wiki's own example is `Lock(false, true)` —
    /// unlock, flagging the adjoining cell public). `abAsOwner`'s
    /// cell-privacy side effect is unmodeled and parsed-not-applied.
    #[test]
    fn lock_lowers_at_its_declared_two_parameter_arity() {
        let wiki_example = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(false, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&wiki_example),
            Some(vec![Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: false,
            }])
        );

        let compiled_default = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(true, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&compiled_default),
            Some(vec![Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: true,
            }])
        );
    }

    /// Starfield's `Lock` dialect declares a third parameter — `Lock(bool
    /// abLock, bool abAsOwner, bool abTraverseLinkedLockables = true)`,
    /// read off the shipped `ObjectReference` engine-class stub (FO4 and
    /// FO76 stubs stop at two) — and its compiler materializes all three,
    /// so the near-universal compiled spelling is `Lock(bool, false,
    /// true)` (the whole SF `QF_` lock family). The traversal flag
    /// qualifies linked lockables, which `SetLocked` does not model for
    /// the receiver either way: literal-only, parsed-not-applied, same
    /// discipline as `abAsOwner`.
    #[test]
    fn lock_lowers_at_the_starfield_three_parameter_arity() {
        let compiled = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(true, false, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&compiled),
            Some(vec![Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: true,
            }])
        );

        let unlocked = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.Lock(false, false, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&unlocked),
            Some(vec![Effect::SetLocked {
                target: ObjectRef::Property("MyDoor".into()),
                locked: false,
            }])
        );
    }

    /// `SetLockLevel` is a *separate* effect, never a spelling of
    /// `SetLocked`: in Papyrus it sets difficulty and does not lock or
    /// unlock anything. Folding the two together would make a fragment that
    /// merely re-tunes a lock silently lock a door.
    #[test]
    fn set_lock_level_lowers_to_its_own_effect_not_to_a_lock() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_7()\n\
             MyDoor.SetLockLevel(75)\n EndFunction\n",
        );
        let lowered = lower_fragment(&body).expect("must lower");
        assert_eq!(
            lowered,
            vec![Effect::SetLockLevel {
                target: ObjectRef::Property("MyDoor".into()),
                level: 75,
            }]
        );
        assert!(
            !matches!(lowered[0], Effect::SetLocked { .. }),
            "SetLockLevel must never lower to a lock state change"
        );
    }

    /// The wire format carries lock level as one byte, so a literal outside
    /// `0..=255` is not a difficulty this engine can represent — decline
    /// rather than truncate into a *different* difficulty. A bare call
    /// declines too: unlike `Lock`, Papyrus gives `SetLockLevel` no default,
    /// so there is nothing to assume.
    #[test]
    fn set_lock_level_declines_on_an_unrepresentable_or_missing_level() {
        for source in [
            "MyDoor.SetLockLevel(256)",
            "MyDoor.SetLockLevel(-1)",
            "MyDoor.SetLockLevel()",
            "MyDoor.SetLockLevel(50, 1)",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_7()\n\
                 {source}\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                None,
                "`{source}` must decline rather than truncate or assume"
            );
        }
    }

    #[test]
    fn lowers_activate_and_set_open_gate_sequence() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_14()\n\
             KeepGate.Activate(Game.GetPlayer())\n\
             KeepGate.SetOpen()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Activate {
                    target: ObjectRef::Property("KeepGate".into()),
                    activator: None,
                    native_only: false,
                },
                Effect::SetOpen {
                    target: ObjectRef::Property("KeepGate".into()),
                    open: true,
                },
            ])
        );
    }

    #[test]
    fn activate_default_processing_only_lowers_native_only() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_15()\n\
             KeepGate.Activate(Game.GetPlayer(), true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Activate {
                target: ObjectRef::Property("KeepGate".into()),
                activator: None,
                native_only: true,
            }])
        );
    }

    #[test]
    fn lowers_real_mq101_alias_activate_and_cast_set_open_shape() {
        // Parse the two statements separately: the source parser deliberately
        // discards newlines, so two adjacent chained calls are ambiguous in
        // handwritten PSC. The PEX decompiler supplies two distinct AST
        // statements, which is the production shape this combined body models.
        let mut body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_76()\n\
             Alias_KeepLever1.GetRef().Activate(Alias_Soldier.GetRef(), false)\n EndFunction\n",
        );
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_76()\n\
             (KeepGate1 as default2stateactivator).SetOpen(true)\n EndFunction\n",
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Activate {
                    target: ObjectRef::Property("Alias_KeepLever1".into()),
                    activator: Some(ObjectRef::Property("Alias_Soldier".into())),
                    native_only: false,
                },
                Effect::SetOpen {
                    target: ObjectRef::Property("KeepGate1".into()),
                    open: true,
                },
            ])
        );
    }

    #[test]
    fn lowers_exact_mq101_player_control_selection() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_191()\n\
             Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetPlayerControls {
                enabled: false,
                selection: PlayerControlSelection {
                    movement: false,
                    fighting: true,
                    camera_switching: true,
                    looking: false,
                    sneaking: false,
                    menu: false,
                    activation: true,
                    journal_tabs: true,
                    pov_type: 0,
                },
            }])
        );
    }

    /// #5651 — the two `Game.*PlayerControls` calls fill absent arguments
    /// from their OWN default tables (CK wiki): `EnablePlayerControls`
    /// defaults every domain — including camera/looking/sneaking and the
    /// journal tabs — to true, while `DisablePlayerControls` defaults
    /// camera/looking/sneaking/journal to false. The single shared
    /// `PAPYRUS_DEFAULT` this replaced was a hybrid (Enable's
    /// camera/looking/sneaking, Disable's journal tabs) matching neither
    /// signature; only `.psc` fragment bodies ever saw the fill-in, so
    /// every partial call was mis-translated.
    #[test]
    fn partial_player_controls_calls_fill_from_their_own_default_tables() {
        let disable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Game.DisablePlayerControls(true, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&disable),
            Some(vec![Effect::SetPlayerControls {
                enabled: false,
                selection: PlayerControlSelection {
                    movement: true,
                    fighting: true,
                    camera_switching: false,
                    looking: false,
                    sneaking: false,
                    menu: true,
                    activation: true,
                    journal_tabs: false,
                    pov_type: 0,
                },
            }])
        );

        let enable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Game.EnablePlayerControls(true, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&enable),
            Some(vec![Effect::SetPlayerControls {
                enabled: true,
                selection: PlayerControlSelection {
                    movement: true,
                    fighting: true,
                    camera_switching: true,
                    looking: true,
                    sneaking: true,
                    menu: true,
                    activation: true,
                    journal_tabs: true,
                    pov_type: 0,
                },
            }])
        );
    }

    #[test]
    fn lowers_player_release_and_cinematic_state_calls() {
        let mut body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.GetPlayer().SetRestrained(false)\n EndFunction\n",
        );
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.SetPlayerAIDriven(false)\n EndFunction\n",
        ));
        body.extend(first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_316()\n\
             Game.SetHudCartMode(false)\n EndFunction\n",
        ));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetPlayerRestrained { restrained: false },
                Effect::SetPlayerAiDriven { ai_driven: false },
                Effect::SetHudCartMode { cart_mode: false },
            ])
        );
    }

    #[test]
    fn lowers_mq101_chargen_exit_and_save_request() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_318` (stage 65).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_318()\n\
             Game.SetInChargen(false, true, true)\n\
             Game.RequestSave()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetInChargen {
                    disable_saving: false,
                    disable_waiting: true,
                    show_controls_disabled_message: true,
                },
                Effect::RequestSave { auto: false },
            ])
        );
    }

    #[test]
    fn lowers_mq101_show_race_menu() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_13` (stage 75).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_13()\n\
             Game.ShowRaceMenu()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), Some(vec![Effect::ShowRaceMenu]));
    }

    #[test]
    fn lowers_mq101_chargen_exit_autosave_variant() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_260` (stage 255) —
        // the `false, false, false` cleanup/fallback variant of the same call.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_260()\n\
             Game.SetInChargen(false, false, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::SetInChargen {
                disable_saving: false,
                disable_waiting: false,
                show_controls_disabled_message: false,
            }])
        );
    }

    #[test]
    fn set_in_chargen_declines_on_wrong_arg_count() {
        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_28()\n\
             Game.SetInChargen(true, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn show_race_menu_declines_with_args() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_29()\n\
             Game.ShowRaceMenu(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn request_save_primitives_decline_with_args() {
        // #4328 — both are 0-arg natives; the `is_empty` arity guard had no
        // decline test of its own.
        for call in ["Game.RequestSave(true)", "Game.RequestAutoSave(1)"] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_31()\n{call}\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{call}");
        }
    }

    #[test]
    fn only_the_counter_only_primitives_are_placeholders() {
        assert!(Effect::ShowRaceMenu.is_placeholder());
        assert!(Effect::RequestSave { auto: false }.is_placeholder());
        assert!(Effect::RequestSave { auto: true }.is_placeholder());
        // #4372 — `hud_cart_mode` has no reader, so the arm only records.
        assert!(Effect::SetHudCartMode { cart_mode: true }.is_placeholder());
        // …whereas SetInChargen's `disable_saving` gates the save command.
        assert!(!Effect::SetInChargen {
            disable_saving: true,
            disable_waiting: false,
            show_controls_disabled_message: false,
        }
        .is_placeholder());
    }

    #[test]
    fn lowers_request_auto_save() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_11` (stage 80),
        // minus the trailing `AddRaceSpells()` call — that one is a
        // MQ101QuestScript-authored helper (not a `Game` global) and still
        // declines the whole fragment. Its body (RemoveRaceSpells, then an
        // if/elseif chain over the player's race adding Conjure Familiar /
        // Sparks / Fury) needs cross-script helper inlining, elseif and a
        // player-race guard; AddSpell/RemoveSpell themselves lower (#4415).
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_30()\n\
             Game.RequestAutoSave()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::RequestSave { auto: true }])
        );
    }

    #[test]
    fn lowers_mq101_set_enemy() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_112`. The
        // decompiler emits the `Faction Property` auto-read as its backing
        // variable (`::MQ101StormcloakFaction_var`), a shape the `.psc`
        // `Ident` regex cannot express (#2657 precedent above), so this is
        // built by hand rather than parsed from source text.
        let expr = Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier(
                    "::MQ101StormcloakFaction_var".into(),
                )))),
                member: sp(Identifier("SetEnemy".into())),
            })),
            args: vec![
                CallArg {
                    name: None,
                    value: sp(Expr::Ident(Identifier("::PlayerFaction_var".into()))),
                },
                CallArg {
                    name: None,
                    value: sp(Expr::BoolLit(false)),
                },
                CallArg {
                    name: None,
                    value: sp(Expr::BoolLit(false)),
                },
            ],
        };
        assert_eq!(
            classify_effect(&expr, &Scope::default()),
            Some(Effect::SetEnemy {
                faction: ObjectRef::Property("::MQ101StormcloakFaction_var".into()),
                other_faction: ObjectRef::Property("::PlayerFaction_var".into()),
                self_neutral: false,
                other_neutral: false,
            })
        );
    }

    #[test]
    fn lowers_mq101_start_combat() {
        // Real vanilla shape: `QF_MQ101_0003372B::Fragment_112`.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_112()\n\
             Alias_TrophyRoomPrisoner01.GetActorRef().StartCombat(Game.GetPlayer())\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::StartCombat {
                actor: ActorRef::Object(ObjectRef::Property(
                    "Alias_TrophyRoomPrisoner01".into()
                )),
                target: ActorRef::Player,
            }])
        );
    }

    /// #5407 — `AddRaceSpells`/`RemoveRaceSpells` lower ONLY as the
    /// unqualified call in a script that itself defines the helper
    /// argless (MQ101QuestScript's shape). The member-call spelling and
    /// a script that doesn't define the name (another script's
    /// same-named helper, or one with parameters) must decline: both
    /// used to lower onto the player unconditionally.
    #[test]
    fn race_spells_lower_only_when_the_script_defines_the_helper() {
        let unqualified = first_fn_body(
            "ScriptName MQ101QuestScript extends Quest\n\
             Function Fragment_11()\n\
             AddRaceSpells()\n\
             RemoveRaceSpells()\n\
             EndFunction\n\
             Function AddRaceSpells()\n\
             EndFunction\n\
             Function RemoveRaceSpells()\n\
             EndFunction\n",
        );
        let own = std::collections::HashSet::from([
            "addracespells".to_string(),
            "removeracespells".to_string(),
        ]);
        assert_eq!(
            lower_fragment_with_script_metadata(&unqualified, &HashSet::new(), &own, None),
            Some(vec![
                Effect::AddRaceSpells {
                    actor: ActorRef::Player
                },
                Effect::RemoveRaceSpells {
                    actor: ActorRef::Player
                },
            ]),
            "the vanilla shape (unqualified, own-script, argless) lowers"
        );

        // No function table: the same call declines — a script that does
        // not define the helper has no contract to lower by.
        assert_eq!(
            lower_fragment_with_script_metadata(&unqualified, &HashSet::new(), &HashSet::new(), None),
            None,
            "an undefined helper name declines"
        );

        // Member-call spelling: declined even when the script defines
        // the name — `x.AddRaceSpells()` on any object used to lower
        // onto the player.
        let member = first_fn_body(
            "ScriptName MQ101QuestScript extends Quest\n\
             Function Fragment_11()\n\
             SomeAlias.AddRaceSpells()\n\
             EndFunction\n\
             Function AddRaceSpells()\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment_with_script_metadata(&member, &HashSet::new(), &own, None),
            None,
            "a member-call receiver is never accepted"
        );

        // The helper exists but takes parameters: same-named, different
        // contract — decline.
        let parameterized = first_fn_body(
            "ScriptName ModScript extends Quest\n\
             Function Fragment_0()\n\
             AddRaceSpells()\n\
             EndFunction\n\
             Function AddRaceSpells(Bool abLoud)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment_with_script_metadata(
                &parameterized,
                &HashSet::new(),
                &HashSet::new(),
                None
            ),
            None,
            "a parameterized same-named helper is not the vanilla helper"
        );
    }

    /// #4415 — `AddSpell` with or without its `abVerbose` flag, and
    /// `RemoveSpell`, lower onto the spell-list effects (the shapes
    /// `MQ101QuestScript.AddRaceSpells` / `RemoveRaceSpells` use).
    #[test]
    fn lowers_add_and_remove_spell() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Spell Property Fury Auto\n\
             Function Fragment_9()\n\
             Game.GetPlayer().AddSpell(Fury, true)\n\
             Game.GetPlayer().AddSpell(Fury)\n\
             Game.GetPlayer().RemoveSpell(Fury)\n\
             EndFunction\n",
        );
        let fury = || ObjectRef::Property("Fury".into());
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::AddSpell {
                    actor: ActorRef::Player,
                    spell: fury(),
                },
                Effect::AddSpell {
                    actor: ActorRef::Player,
                    spell: fury(),
                },
                Effect::RemoveSpell {
                    actor: ActorRef::Player,
                    spell: fury(),
                },
            ])
        );
        let variable_flag = first_fn_body(
            "ScriptName QF extends Quest\n\
             Spell Property Fury Auto\n\
             Bool Property bLoud Auto\n\
             Function Fragment_9()\n\
             Game.GetPlayer().AddSpell(Fury, bLoud)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&variable_flag),
            None,
            "a non-literal flag declines"
        );
    }

    #[test]
    fn set_enemy_defaults_omitted_flags_and_carries_literal_ones() {
        // #4318 / #4414 — each flag makes one direction neutral; an omitted
        // flag is its declared `false` default, and a non-literal flag is an
        // unknown runtime value that declines.
        let call = |flags: Vec<Expr>| Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(sp(Expr::Ident(Identifier("::SomeFaction_var".into())))),
                member: sp(Identifier("SetEnemy".into())),
            })),
            args: std::iter::once(Expr::Ident(Identifier("::OtherFaction_var".into())))
                .chain(flags)
                .map(|value| CallArg {
                    name: None,
                    value: sp(value),
                })
                .collect(),
        };
        let lowered = |self_neutral, other_neutral| {
            Some(Effect::SetEnemy {
                faction: ObjectRef::Property("::SomeFaction_var".into()),
                other_faction: ObjectRef::Property("::OtherFaction_var".into()),
                self_neutral,
                other_neutral,
            })
        };
        let scope = Scope::default();
        assert_eq!(
            classify_effect(&call(vec![]), &scope),
            lowered(false, false)
        );
        assert_eq!(
            classify_effect(&call(vec![Expr::BoolLit(true)]), &scope),
            lowered(true, false)
        );
        assert_eq!(
            classify_effect(
                &call(vec![Expr::BoolLit(false), Expr::BoolLit(true)]),
                &scope
            ),
            lowered(false, true)
        );
        assert_eq!(
            classify_effect(
                &call(vec![Expr::BoolLit(true), Expr::BoolLit(true)]),
                &scope
            ),
            lowered(true, true)
        );
        for declined in [
            vec![
                Expr::Ident(Identifier("bNeutral".into())),
                Expr::BoolLit(false),
            ],
            vec![
                Expr::BoolLit(false),
                Expr::BoolLit(false),
                Expr::BoolLit(false),
            ],
        ] {
            let expr = call(declined);
            assert_eq!(classify_effect(&expr, &scope), None, "{expr:?}");
        }
    }

    /// #4816 — `StopCombat()` on an NPC lowers; with an argument, or on the
    /// player, it declines.
    #[test]
    fn lowers_stop_combat_on_an_npc_only() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_40()\n\
             Alias_TrophyRoomPrisoner01.GetActorRef().StopCombat()\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::StopCombat {
                actor: ActorRef::Object(ObjectRef::Property(
                    "Alias_TrophyRoomPrisoner01".into()
                )),
            }])
        );
        for call in [
            "Game.GetPlayer().StopCombat()",
            "Alias_TrophyRoomPrisoner01.GetActorRef().StopCombat(Game.GetPlayer())",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_41()\n{call}\nEndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{call}");
        }
    }

    /// #5017 — `SetUnconscious` on an NPC lowers with its default (`true`)
    /// and an explicit `false` (the wake-up call); the player, or a second
    /// argument, declines.
    #[test]
    fn lowers_set_unconscious_on_an_npc_only() {
        let lowered = |call: &str| {
            lower_fragment(&first_fn_body(&format!(
                "ScriptName QF extends Quest\nFunction Fragment_1()\n{call}\nEndFunction\n"
            )))
        };
        let robot = || ActorRef::Object(ObjectRef::Property("Alias_Protectron".into()));
        assert_eq!(
            lowered("Alias_Protectron.GetActorRef().SetUnconscious()"),
            Some(vec![Effect::SetUnconscious {
                actor: robot(),
                unconscious: true,
            }])
        );
        assert_eq!(
            lowered("Alias_Protectron.GetActorRef().SetUnconscious(false)"),
            Some(vec![Effect::SetUnconscious {
                actor: robot(),
                unconscious: false,
            }])
        );
        for call in [
            "Game.GetPlayer().SetUnconscious(true)",
            "Alias_Protectron.GetActorRef().SetUnconscious(false, true)",
        ] {
            assert_eq!(lowered(call), None, "{call}");
        }
    }

    #[test]
    fn start_combat_declines_on_wrong_arg_count() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_32()\n\
             Game.GetPlayer().StartCombat()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn start_combat_declines_a_player_combatant() {
        // #4323 — the player target in `lowers_mq101_start_combat` resolves
        // fine, so this declines on the receiver alone: the NPC chase-and-
        // strike runtime must never be armed on the player.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_33()\n\
             Game.GetPlayer().StartCombat(Game.GetPlayer())\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn lowers_alias_actor_evaluate_package() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_292()\n\
             Alias_Hadvar.GetActorRef().EvaluatePackage()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::EvaluatePackage {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Hadvar".into())),
            }])
        );
    }

    #[test]
    fn lowers_mq101_player_idle_and_animation_event_helpers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_295()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             Game.GetPlayer().PlayIdle(IdleExecutionerAlduinReactionDeath)\n\
             kmyQuest.PlayerImodAnimation()\n\
             kmyQuest.PlayerFurnitureAnimation()\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::PlayIdle {
                    actor: ActorRef::Player,
                    idle: ObjectRef::Property("IdleExecutionerAlduinReactionDeath".into()),
                },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::PlayImod,
                },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::IdleFurnitureExit,
                },
            ])
        );
    }

    #[test]
    fn lowers_mq101_cart_exit_motion_and_sitting_rotation() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_177()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 3)\n\
             Alias_Cart.GetRef().SetMotionType(Alias_Cart.GetRef().Motion_Keyframed, true)\n\
             Game.SetSittingRotation(-55.0)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::ExitCart {
                    actor: ObjectRef::Property("Alias_Prisoner".into()),
                    seat: 3,
                },
                Effect::SetMotionType {
                    target: ObjectRef::Property("Alias_Cart".into()),
                    motion_type: MotionType::Keyframed,
                    allow_activate: true,
                },
                Effect::SetSittingRotation { degrees: -55.0 },
            ])
        );
    }

    /// #5633 (premise error in #2286): the integer argument of Papyrus
    /// `SetMotionType` uses per-game constants (Skyrim Keyframed 4 / Fixed 5
    /// / Character 7; Fallout 4 Fixed 0 / Keyframed 2), not the NIF
    /// `hkMotionType` table the literal arm used to copy (Keyframed 6, Static
    /// 7). Only `1` (Dynamic) is the same in both games; every other literal
    /// declines. The named `Motion_*` property read — what vanilla MQ101
    /// actually carries — still lowers (see the MQ101 cart test).
    #[test]
    fn literal_motion_types_lower_only_where_every_game_agrees() {
        let lower = |raw: i64| {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_177()\n\
                 Cart.SetMotionType({raw}, true)\n\
                 EndFunction\n"
            ));
            lower_fragment(&body)
        };
        assert_eq!(
            lower(1),
            Some(vec![Effect::SetMotionType {
                target: ObjectRef::Property("Cart".into()),
                motion_type: MotionType::Dynamic,
                allow_activate: true,
            }]),
            "1 is Motion_Dynamic in Skyrim and Fallout 4"
        );
        for raw in [0, 2, 3, 4, 5, 6, 7, 8, 9] {
            assert_eq!(
                lower(raw),
                None,
                "literal {raw} means a different motion type per game — decline"
            );
        }
    }

    #[test]
    fn tracks_actor_and_vehicle_locals_across_a_latent_wait() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_175()\n\
             Actor rider = Alias_Rider.GetActorRef()\n\
             ObjectReference cart = Alias_Cart.GetRef()\n\
             rider.SetVehicle(cart)\n\
             Utility.Wait(0.25)\n\
             rider.SetVehicle(None)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetVehicle {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    vehicle: Some(ObjectRef::Property("Alias_Cart".into())),
                },
                Effect::Wait { seconds: 0.25 },
                Effect::SetVehicle {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    vehicle: None,
                },
            ])
        );
    }

    /// #5681 — a wait past the shared cap (or one whose f32 shape is
    /// infinite) must decline, not lower into an immortal continuation.
    /// The boundary itself still lowers.
    #[test]
    fn waits_past_the_shared_cap_decline() {
        let at_cap = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Utility.Wait(86400.0)\n\
             Self.SetStage(20)\n EndFunction\n",
        );
        assert!(
            lower_fragment(&at_cap).is_some(),
            "exactly MAX_PROVIDER_WAIT_SECONDS is a legal (if long) wait"
        );
        for seconds in ["86401.0", "340282346638528859811704183484516925440.0"] {
            let over = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_1()\n\
                 Utility.Wait({seconds})\n\
                 Self.SetStage(20)\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&over),
                None,
                "Utility.Wait({seconds}) must decline, not park an immortal continuation"
            );
        }
    }

    #[test]
    fn lowers_mq101_cart_load_gate_tether_and_equipment_sequence() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_175()\n\
             Actor player = Game.GetPlayer()\n\
             Actor rider = Alias_Rider.GetActorRef()\n\
             ObjectReference cart = Alias_Cart.GetRef()\n\
             While !player.Is3DLoaded() || !rider.Is3DLoaded()\n\
                 Utility.Wait(0.2)\n\
             EndWhile\n\
             cart.TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference)\n\
             rider.EquipItem(ArmorGag as Form, false, true)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::WaitForActors3DLoaded {
                    actors: vec![
                        ActorRef::Player,
                        ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    ],
                    poll_seconds: 0.2,
                },
                Effect::TetherToHorse {
                    cart: ObjectRef::Property("Alias_Cart".into()),
                    horse: ObjectRef::Property("Alias_Horse".into()),
                },
                Effect::EquipItem {
                    actor: ActorRef::Object(ObjectRef::Property("Alias_Rider".into())),
                    item: ObjectRef::Property("ArmorGag".into()),
                    silent: true,
                },
            ])
        );
    }

    /// M47.2 — `ElseIf` is the else branch of the clause before it.
    #[test]
    fn else_if_chains_lower_as_nested_conditionals() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\nFunction Fragment_1()\n\
             If Self.GetStageDone(10)\n Self.SetStage(20)\n\
             ElseIf Self.GetStageDone(5)\n Self.SetStage(15)\n\
             Else\n Self.SetStage(1)\n EndIf\n EndFunction\n",
        );
        let stage = |stage| Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage,
        };
        let done = |stage| FragmentGuard::StageDone {
            quest: QuestRef::SelfRef,
            stage,
            done: true,
        };
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards: vec![done(10)],
                then_effects: vec![stage(20)],
                else_effects: vec![Effect::Conditional {
                    guards: vec![done(5)],
                    then_effects: vec![stage(15)],
                    else_effects: vec![stage(1)],
                }],
            }])
        );
    }

    /// M47.2 — a `While … Wait` poll on a modelled condition lowers to
    /// `WaitWhile`; a loop doing more than waiting still declines.
    #[test]
    fn declines_unmodeled_loop_and_prevent_unequip() {
        let poll = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             While Self.GetStage() < 10\n Utility.Wait(0.2)\n EndWhile\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&poll),
            Some(vec![Effect::WaitWhile {
                guards: vec![FragmentGuard::Stage {
                    quest: QuestRef::SelfRef,
                    comparison: GuardComparison::Lt,
                    value: 10.0,
                }],
                poll_seconds: 0.2,
            }])
        );
        let arbitrary_loop = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             While Self.GetStage() < 10\n Self.SetStage(5)\n Utility.Wait(0.2)\n EndWhile\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&arbitrary_loop), None);

        let locked_item = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_2()\n\
             Alias_Rider.GetActorRef().EquipItem(ArmorGag, true, false)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&locked_item), None);
    }

    #[test]
    fn lowers_get_stage_done_conditional() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Self.GetStageDone(2) == false && Self.GetStageDone(6) == 0\n\
               Self.SetStage(35)\n\
             Else\n\
               Self.SetStage(7)\n\
             EndIf\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards: vec![
                    FragmentGuard::StageDone {
                        quest: QuestRef::SelfRef,
                        stage: 2,
                        done: false,
                    },
                    FragmentGuard::StageDone {
                        quest: QuestRef::SelfRef,
                        stage: 6,
                        done: false,
                    },
                ],
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 35,
                }],
                else_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 7,
                }],
            }]),
        );
    }

    /// #5631: `Return` is Champollion's terminator only as the body's final
    /// statement. An early one is control flow — lowering it as nothing
    /// applied the rest of the body unconditionally.
    #[test]
    fn return_lowers_only_as_the_final_statement() {
        let set = |stage| Effect::SetStage { quest: QuestRef::SelfRef, stage };
        let trailing = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.SetStage(10)\n\
             Return\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&trailing), Some(vec![set(10)]));

        let early_in_if = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             If Self.GetStageDone(10)\n\
               Return\n\
             EndIf\n\
             Self.SetStage(20)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&early_in_if), None, "early return inside an If");

        let mid_body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.SetStage(10)\n\
             Return\n\
             Self.SetStage(20)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&mid_body), None, "return before more statements");

        let tail_of_a_branch = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             If Self.GetStageDone(10)\n\
               Self.SetStage(20)\n\
               Return\n\
             EndIf\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&tail_of_a_branch), None, "return nested in a branch");
    }

    /// #5632: the branches lower against clones of the scope, so a local
    /// DECLARED inside one stays inside it — but re-binding a local declared
    /// before the `If` changes what the name means after `EndIf`, and the
    /// clone is dropped. The tail used to lower against the old binding.
    #[test]
    fn rebinding_a_pre_branch_local_inside_a_branch_declines() {
        let rebinds = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property OtherQ Auto\n\
             Function Fragment_0()\n\
             Quest k = Self\n\
             If Self.GetStageDone(10)\n\
               k = OtherQ\n\
             EndIf\n\
             k.SetStage(20)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&rebinds), None);

        // Re-binding to the very same target changes nothing after the
        // branch, so it still lowers.
        let same = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Quest k = Self\n\
             If Self.GetStageDone(10)\n\
               k = Self\n\
             EndIf\n\
             k.SetStage(20)\n\
             EndFunction\n",
        );
        assert!(lower_fragment(&same).is_some());

        // A local declared inside the branch never leaks out, so it is fine.
        let declared_inside = first_fn_body(
            "ScriptName QF extends Quest\n\
             Quest Property OtherQ Auto\n\
             Function Fragment_0()\n\
             If Self.GetStageDone(10)\n\
               Quest j = OtherQ\n\
               j.SetStage(5)\n\
             EndIf\n\
             EndFunction\n",
        );
        assert!(lower_fragment(&declared_inside).is_some());
    }

    /// #5638: only a local this function declared can be re-bound. A write
    /// to a script variable (or property) by bare name is persistent state;
    /// it used to be recorded as a side-effect-free local and dropped.
    #[test]
    fn assignment_to_a_script_variable_declines() {
        let script_var = first_fn_body(
            "ScriptName QF extends Quest\n\
             Bool bFlag\n\
             Function Fragment_0()\n\
             bFlag = True\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(lower_fragment(&script_var), None);

        let declared_local = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Int n = 1\n\
             n = 2\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&declared_local),
            Some(vec![Effect::SetStage { quest: QuestRef::SelfRef, stage: 10 }])
        );
    }

    /// M47.2 — actor values as run-time numbers: by property, by name, and
    /// by FO4's `Game.Get<Name>AV()` accessor; and `SetValue` with one.
    #[test]
    fn actor_values_lower_as_run_time_numbers() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\nActorValue Property Counter Auto\n\
             Function Fragment_5()\n\
             If Self.GetStage() >= Game.GetPlayer().GetAV(\"Health\")\n Self.SetStage(10)\n EndIf\n\
             Game.GetPlayer().SetValue(Counter, Game.GetPlayer().GetValue(Counter) + 1)\n\
             Game.GetPlayer().SetValue(Game.GetHealthAV(), 100.0)\n EndFunction\n",
        );
        let effects = lower_fragment(&body).expect("lowers");
        let counter = || ObjectRef::Property("Counter".into());
        assert!(matches!(
            &effects[0],
            Effect::Conditional { guards, .. } if matches!(
                guards.as_slice(),
                [FragmentGuard::Compare { right: ValueExpr::ActorValueNamed(ActorRef::Player, name), .. }]
                    if name == "Health"
            )
        ), "{effects:?}");
        assert_eq!(
            effects[1],
            Effect::SetActorValue {
                actor: ActorRef::Player,
                actor_value: counter(),
                value: ValueExpr::Arith(
                    Box::new(ValueExpr::ActorValue(ActorRef::Player, counter())),
                    ArithOp::Add,
                    Box::new(ValueExpr::Literal(1.0)),
                ),
            }
        );
        assert_eq!(
            effects[2],
            Effect::SetActorValueNamed {
                actor: ActorRef::Player,
                name: "Health".into(),
                value: ValueExpr::Literal(100.0),
            }
        );
    }

    #[test]
    fn declines_unmodeled_conditional_guard() {
        // `GetStage() >= 5`, disjunctions, faction tests and comparisons of
        // two run-time numbers lower since M47.2; an unmodelled read (alone,
        // inside a disjunction or inside a comparison) and a fragment-script
        // variable (`Self.x`) still decline.
        for condition in [
            "Game.GetPlayer().IsSneaking()",
            "Self.GetStageDone(5) || Game.GetPlayer().IsSneaking()",
            "Self.bDone",
            "Self.GetStage() >= Game.GetPlayer().GetEquippedWeapon().GetBaseDamage()",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_5()\n\
                 If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
            ));
            assert_eq!(lower_fragment(&body), None, "{condition}");
        }
    }

    /// M47.2 — `ShowAsHelpMessage` lowers with its Fallout 4 context and
    /// priority and Starfield's gamepad message; a menu context declines.
    #[test]
    fn help_message_lowers_with_context_priority_and_gamepad_message() {
        let lowered = |call: &str| {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\nMessage Property Help Auto\nMessage Property PadHelp Auto\n\
                 Function Fragment_1()\n {call}\n EndFunction\n"
            ));
            lower_fragment(&body)
        };
        for call in [
            "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3)",
            "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"NoMenu\", 2)",
            "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"\", 0, PadHelp)",
            "Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"\", 0, None)",
        ] {
            assert!(lowered(call).is_some(), "{call}");
        }
        assert!(lowered("Help.ShowAsHelpMessage(\"Jump\", 5.0, 30.0, 3, \"ContainerMenu\", 0)").is_none());
    }

    /// M47.2 — `IsInCombat()` is a guard on the actor's combat state.
    #[test]
    fn lowers_the_in_combat_guard() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Game.GetPlayer().IsInCombat()\n Self.SetStage(10)\n EndIf\n EndFunction\n",
        );
        let lowered = lower_fragment(&body).expect("lowers");
        assert!(
            format!("{lowered:?}").contains("ActorInCombat"),
            "{lowered:?}"
        );
    }

    /// M47.2 — two run-time numbers compared: a random roll, a level against
    /// a global, and arithmetic.
    #[test]
    fn run_time_number_comparisons_lower_as_compare_guards() {
        let lowered = |condition: &str| {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_5()\n\
                 If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
            ));
            match lower_fragment(&body)?.as_slice() {
                [Effect::Conditional { guards, .. }] => Some(guards.clone()),
                _ => None,
            }
        };
        assert_eq!(
            lowered("Utility.RandomInt(1, 3) == 1"),
            Some(vec![FragmentGuard::Compare {
                left: ValueExpr::RandomInt(1, 3),
                comparison: GuardComparison::Eq,
                right: ValueExpr::Literal(1.0),
            }])
        );
        assert_eq!(
            lowered("Game.GetPlayer().GetLevel() < MinLevel.GetValue() + 2"),
            Some(vec![FragmentGuard::Compare {
                left: ValueExpr::Level(ActorRef::Player),
                comparison: GuardComparison::Lt,
                right: ValueExpr::Arith(
                    Box::new(ValueExpr::Global(ObjectRef::Property("MinLevel".into()))),
                    ArithOp::Add,
                    Box::new(ValueExpr::Literal(2.0)),
                ),
            }])
        );
        assert_eq!(
            lowered("!(Self.GetStage() >= Self.GetStage())"),
            Some(vec![FragmentGuard::Compare {
                left: ValueExpr::Stage(QuestRef::SelfRef),
                comparison: GuardComparison::Lt,
                right: ValueExpr::Stage(QuestRef::SelfRef),
            }]),
            "negation flips the comparison"
        );
    }

    /// M47.2 — a local holding a run-time number feeds a later comparison;
    /// with a suspension in the same fragment it declines.
    #[test]
    fn run_time_locals_lower_and_never_suspend() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             Int roll = Utility.RandomInt(1, 3)\n\
             If roll == 1\n Self.SetStage(10)\n Else\n Self.SetStage(20)\n EndIf\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::SetLocal {
                    local: "roll".into(),
                    value: ValueExpr::RandomInt(1, 3),
                },
                Effect::Conditional {
                    guards: vec![FragmentGuard::Compare {
                        left: ValueExpr::Local("roll".into()),
                        comparison: GuardComparison::Eq,
                        right: ValueExpr::Literal(1.0),
                    }],
                    then_effects: vec![Effect::SetStage {
                        quest: QuestRef::SelfRef,
                        stage: 10
                    }],
                    else_effects: vec![Effect::SetStage {
                        quest: QuestRef::SelfRef,
                        stage: 20
                    }],
                },
            ])
        );
        let suspends = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             Int roll = Utility.RandomInt(1, 3)\n\
             Utility.Wait(1.0)\n\
             If roll == 1\n Self.SetStage(10)\n EndIf\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&suspends), None, "locals do not survive a suspension");
    }

    /// M47.2 — actor, quest, scene and location guard atoms.
    #[test]
    fn actor_quest_scene_and_location_guards_lower() {
        let property = |name: &str| ObjectRef::Property(name.into());
        for (condition, guard) in [
            (
                "Victim.IsDead()",
                FragmentGuard::ActorDead {
                    actor: ActorRef::Object(property("Victim")),
                    dead: true,
                },
            ),
            (
                "!Game.GetPlayer().HasPerk(SomePerk)",
                FragmentGuard::HasPerk {
                    actor: ActorRef::Player,
                    perk: property("SomePerk"),
                    expected: false,
                },
            ),
            (
                "Game.GetPlayer().GetItemCount(Gold001) >= 100",
                FragmentGuard::ItemCount {
                    container: ActorRef::Player,
                    item: property("Gold001"),
                    comparison: GuardComparison::Ge,
                    value: 100.0,
                },
            ),
            (
                "Self.IsCompleted()",
                FragmentGuard::QuestCompleted {
                    quest: QuestRef::SelfRef,
                    completed: true,
                },
            ),
            (
                "SomeScene.IsPlaying() == False",
                FragmentGuard::ScenePlaying {
                    scene: property("SomeScene"),
                    playing: false,
                },
            ),
            (
                "Game.GetPlayer().GetCurrentLocation().HasKeyword(LocTypeCity)",
                FragmentGuard::LocationKeyword {
                    actor: ActorRef::Player,
                    keyword: property("LocTypeCity"),
                    expected: true,
                },
            ),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_5()\n\
                 If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                Some(vec![Effect::Conditional {
                    guards: vec![guard],
                    then_effects: vec![Effect::SetStage {
                        quest: QuestRef::SelfRef,
                        stage: 10
                    }],
                    else_effects: vec![],
                }]),
                "{condition}"
            );
        }
        // M47.2 — an actor's own keywords.
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             If Game.GetPlayer().HasKeyword(SomeKeyword)\n Self.SetStage(10)\n EndIf\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::Conditional {
                guards: vec![FragmentGuard::HasKeyword {
                    object: ActorRef::Player,
                    keyword: ObjectRef::Property("SomeKeyword".into()),
                    expected: true,
                }],
                then_effects: vec![Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10
                }],
                else_effects: vec![],
            }])
        );
    }

    /// M47.2 — `GetLinkedRef([keyword])` as a receiver and as a local's
    /// initializer.
    #[test]
    fn linked_refs_lower_as_linked_object_refs() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\nReferenceAlias Property Alias_Gate Auto\n\
             Keyword Property LinkGate Auto\nFunction Fragment_5()\n\
             Alias_Gate.GetReference().GetLinkedRef(LinkGate).Disable()\n\
             ObjectReference lever = Alias_Gate.GetRef().GetLinkedRef()\n\
             lever.Enable()\n EndFunction\n",
        );
        let gate = || Box::new(ObjectRef::Property("Alias_Gate".into()));
        assert_eq!(
            lower_fragment(&body),
            Some(vec![
                Effect::Disable {
                    object: ObjectRef::Linked {
                        from: gate(),
                        keyword: Some(Box::new(ObjectRef::Property("LinkGate".into()))),
                    },
                    fade_out: false,
                },
                Effect::Enable {
                    object: ObjectRef::Linked {
                        from: gate(),
                        keyword: None,
                    },
                    fade_in: false,
                },
            ])
        );
    }

    /// M47.2 — reference identity: two aliases' references, and an alias
    /// against `None`; a bare property comparison (a number, as far as the
    /// name says) is not a reference comparison.
    #[test]
    fn reference_comparisons_lower_as_same_reference_guards() {
        let lower = |condition: &str| {
            lower_fragment(&first_fn_body(&format!(
                "ScriptName QF extends Quest\nReferenceAlias Property Alias_A Auto\n\
                 ReferenceAlias Property Alias_B Auto\nFunction Fragment_5()\n\
                 If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
            )))
        };
        let guard = |condition: &str| match lower(condition).as_deref() {
            Some([Effect::Conditional { guards, .. }]) => guards.clone(),
            other => panic!("{condition}: {other:?}"),
        };
        let a = || ActorRef::Object(ObjectRef::Property("Alias_A".into()));
        assert_eq!(
            guard("Alias_A.GetActorRef() == Alias_B.GetActorRef()"),
            vec![FragmentGuard::SameReference {
                left: a(),
                right: Some(ActorRef::Object(ObjectRef::Property("Alias_B".into()))),
                expected: true,
            }]
        );
        assert_eq!(
            guard("!(Alias_A.GetReference() == None)"),
            vec![FragmentGuard::SameReference {
                left: a(),
                right: None,
                expected: false,
            }]
        );
        assert_eq!(
            guard("Game.GetPlayer() != Alias_A.GetActorRef()"),
            vec![FragmentGuard::SameReference {
                left: ActorRef::Player,
                right: Some(a()),
                expected: false,
            }]
        );
        assert_eq!(lower("SomeCount == OtherCount"), None);
    }

    /// M47.2 — disjunctions: `||` becomes [`FragmentGuard::Any`], nested ones
    /// flatten, and negation distributes by De Morgan.
    #[test]
    fn disjunctive_guards_lower() {
        let done = |stage, done| FragmentGuard::StageDone {
            quest: QuestRef::SelfRef,
            stage,
            done,
        };
        for (condition, guards) in [
            (
                "Self.GetStageDone(5) || Self.GetStageDone(6) || Self.GetStageDone(7)",
                vec![FragmentGuard::Any(vec![
                    vec![done(5, true)],
                    vec![done(6, true)],
                    vec![done(7, true)],
                ])],
            ),
            (
                "Self.GetStageDone(1) && (Self.GetStageDone(5) || !Self.GetStageDone(6))",
                vec![
                    done(1, true),
                    FragmentGuard::Any(vec![vec![done(5, true)], vec![done(6, false)]]),
                ],
            ),
            (
                // !(A || B) is !A && !B.
                "!(Self.GetStageDone(5) || Self.GetStageDone(6))",
                vec![done(5, false), done(6, false)],
            ),
            (
                // !(A && B) is !A || !B.
                "!(Self.GetStageDone(5) && Self.GetStageDone(6))",
                vec![FragmentGuard::Any(vec![vec![done(5, false)], vec![done(6, false)]])],
            ),
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_5()\n\
                 If {condition}\n Self.SetStage(10)\n EndIf\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                Some(vec![Effect::Conditional {
                    guards,
                    then_effects: vec![Effect::SetStage {
                        quest: QuestRef::SelfRef,
                        stage: 10
                    }],
                    else_effects: vec![],
                }]),
                "{condition}"
            );
        }
    }

    /// M47.2 — the guard atoms the corpus uses beyond `GetStageDone`: negated
    /// stage tests (and FO4's `IsStageDone` spelling), `IsRunning`, stage
    /// comparisons with the literal on either side, objective state, global
    /// reads and quest-script variables.
    #[test]
    fn lowers_the_widened_guard_atoms() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_5()\n\
             Quest __temp = Self as Quest\n\
             MyQuestScript kmyQuest = __temp as MyQuestScript\n\
             If !Self.GetStageDone(100) && Self.IsStageDone(5) == false && OtherQuest.IsRunning()\n\
             Self.SetStage(10)\n\
             EndIf\n\
             If 20 > OtherQuest.GetStage() && Self.IsObjectiveDisplayed(10) && !Self.IsObjectiveCompleted(60)\n\
             Self.SetStage(11)\n\
             EndIf\n\
             If Ransom.GetValueInt() == 3 && Ransom.Value < 1.0 && kmyQuest.bDeaconTalkedTo == 1\n\
             Self.SetStage(12)\n\
             EndIf\n EndFunction\n",
        );
        let mut quests = HashSet::new();
        quests.insert("otherquest".to_string());
        let other = || QuestRef::Property("OtherQuest".into());
        let then = |stage| vec![Effect::SetStage { quest: QuestRef::SelfRef, stage }];
        assert_eq!(
            lower_fragment_with_quest_properties(&body, &quests),
            Some(vec![
                Effect::Conditional {
                    guards: vec![
                        FragmentGuard::StageDone { quest: QuestRef::SelfRef, stage: 100, done: false },
                        FragmentGuard::StageDone { quest: QuestRef::SelfRef, stage: 5, done: false },
                        FragmentGuard::QuestRunning { quest: other(), running: true },
                    ],
                    then_effects: then(10),
                    else_effects: vec![],
                },
                Effect::Conditional {
                    guards: vec![
                        FragmentGuard::Stage {
                            quest: other(),
                            comparison: GuardComparison::Lt,
                            value: 20.0,
                        },
                        FragmentGuard::ObjectiveDisplayed {
                            quest: QuestRef::SelfRef,
                            objective: 10,
                            expected: true,
                        },
                        FragmentGuard::ObjectiveCompleted {
                            quest: QuestRef::SelfRef,
                            objective: 60,
                            expected: false,
                        },
                    ],
                    then_effects: then(11),
                    else_effects: vec![],
                },
                Effect::Conditional {
                    guards: vec![
                        FragmentGuard::Global {
                            global: ObjectRef::Property("Ransom".into()),
                            comparison: GuardComparison::Eq,
                            value: 3.0,
                        },
                        FragmentGuard::Global {
                            global: ObjectRef::Property("Ransom".into()),
                            comparison: GuardComparison::Lt,
                            value: 1.0,
                        },
                        FragmentGuard::QuestVariable {
                            quest: QuestRef::SelfRef,
                            variable: "bDeaconTalkedTo".into(),
                            comparison: GuardComparison::Eq,
                            value: 1.0,
                        },
                    ],
                    then_effects: then(12),
                    else_effects: vec![],
                },
            ])
        );
    }

    #[test]
    fn empty_fragment_is_understood_as_noop() {
        let body =
            first_fn_body("ScriptName QF extends Quest\n Function Fragment_6()\n EndFunction\n");
        assert_eq!(lower_fragment(&body), Some(vec![]));
    }

    // ── Decline-path coverage for SCR-D5-NEW5-02 / #2289 ────────────────
    // One `?`/arg-count/arg-type guard test per primitive that previously
    // shipped with a positive-path test only.

    /// Regression for #3496. `prim_set_stage` and the three objective
    /// primitives read only positional args 0 and 1 and, alone among the
    /// primitives, bounded neither — so an over-arity call lowered
    /// silently instead of declining. Vanilla content cannot produce one
    /// (the compiler emits exactly the declared arity, and all four
    /// signatures are inside the read range), but a modded `.pex` is
    /// untrusted input that reaches this code.
    #[test]
    fn stage_and_objective_primitives_decline_over_arity_calls() {
        // `Quest.SetStage(int aiStage)` — one parameter.
        let set_stage = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_50()\n\
             Self.SetStage(10, 1)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&set_stage), None);

        // `SetObjectiveDisplayed(aiObjective, abDisplayed = true,
        // abForce = false)` — three.
        let displayed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_51()\n\
             Self.SetObjectiveDisplayed(10, true, false, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&displayed), None);

        // `SetObjectiveCompleted`/`SetObjectiveFailed(aiObjective,
        // ab… = true)` — two.
        let completed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_52()\n\
             Self.SetObjectiveCompleted(10, true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&completed), None);

        let failed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_53()\n\
             Self.SetObjectiveFailed(10, true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&failed), None);
    }

    /// The positive side of the bounds above: each primitive's real
    /// signature must still lower at full arity, so the fix is a bound at
    /// the declared parameter count and not a narrowing (#3496).
    #[test]
    fn stage_and_objective_primitives_still_lower_at_full_arity() {
        let displayed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_54()\n\
             Self.SetObjectiveDisplayed(10, true, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&displayed),
            Some(vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 10,
                displayed: true,
            }])
        );

        let completed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_55()\n\
             Self.SetObjectiveCompleted(10, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&completed),
            Some(vec![Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 10,
                completed: false,
            }])
        );

        let failed = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_56()\n\
             Self.SetObjectiveFailed(10, false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&failed),
            Some(vec![Effect::SetObjectiveFailed {
                quest: QuestRef::SelfRef,
                objective: 10,
                failed: false,
            }])
        );
    }

    /// #5652 — `SetObjectiveDisplayed` accepts its 3rd argument only as a
    /// literal: a non-literal (or side-effecting) `abForce` expression
    /// must decline the call instead of being silently dropped while the
    /// arity bound pretends three arguments were handled.
    #[test]
    fn set_objective_displayed_declines_non_literal_third_arguments() {
        // A variable is the simplest non-literal 3rd arg a fragment body
        // can carry — not a bool literal, so `bool_arg` declines.
        let non_literal = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_57()\n\
             Int force\n\
             force = 1\n\
             Self.SetObjectiveDisplayed(10, true, force)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&non_literal), None);

        // The literal shapes still lower, including the 3rd arg true.
        let forced = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_58()\n\
             Self.SetObjectiveDisplayed(10, false, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&forced),
            Some(vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 10,
                displayed: false,
            }])
        );
    }

    /// #5068 — a named call argument binds nothing: the whole translate
    /// chain indexes `CallArg`s positionally, so a named argument
    /// (`Self.SetObjectiveDisplayed(objective = 10, true)`, or the issue's
    /// `SetEnemy(other, abOtherIsNeutralToSelf = true)` inverting the flag
    /// direction) would have lowered into the wrong parameter slot instead
    /// of declining. `.pex` never produces the shape (the decompiler emits
    /// `name: None`); hand-authored `.psc` can, and `method_call` now
    /// declines any named argument — the same discipline the provider seam
    /// applies.
    #[test]
    fn named_call_arguments_decline_instead_of_binding_positionally() {
        let named = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Self.SetObjectiveDisplayed(objective = 10, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&named),
            None,
            "a named argument must decline, not bind positionally"
        );

        let positional = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_1()\n\
             Self.SetObjectiveDisplayed(10, true)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&positional),
            Some(vec![Effect::SetObjectiveDisplayed {
                quest: QuestRef::SelfRef,
                objective: 10,
                displayed: true,
            }]),
            "the equivalent positional shape still lowers"
        );
    }

    /// #3496's completeness check, mechanized: every effect primitive must
    /// bound its argument count, directly or through the primitive it
    /// delegates to. The finding came out of a hand sweep of all 31
    /// `prim_*` bodies that found exactly four unguarded; a hand sweep
    /// does not survive the next primitive being added, so it becomes a
    /// gate here.
    #[test]
    fn every_effect_primitive_bounds_its_argument_count() {
        let src = include_str!("effects.rs");
        // Column-0 `fn prim_` only: the test functions in this module are
        // indented, so the scan cannot pick up its own body.
        let mut unguarded = Vec::new();
        let mut seen = 0usize;
        for chunk in src.split("\nfn prim_").skip(1) {
            let name = chunk.split('(').next().unwrap_or_default();
            let body = chunk.split("\n}\n").next().unwrap_or_default();
            seen += 1;
            let bounded = body.contains("args.len()") || body.contains("args.is_empty()");
            // A primitive that forwards to another primitive inherits its
            // bound (the `PlayerImodAnimation`/`PlayerFurnitureAnimation`
            // and player-controls pairs).
            let delegates = body.contains("prim_");
            if !bounded && !delegates {
                unguarded.push(name.to_owned());
            }
        }
        assert!(seen > 30, "the primitive scan matched only {seen} bodies");
        assert!(
            unguarded.is_empty(),
            "every effect primitive must bound its argument count against its \
             real Papyrus signature — an unbounded one silently lowers an \
             over-arity call from a modded .pex instead of declining \
             (#3496). Unguarded: {unguarded:?}"
        );
    }

    #[test]
    fn set_open_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_20()\n\
             KeepGate.SetOpen(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn set_player_restrained_declines_on_non_player_receiver_and_extra_arg() {
        let non_player = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_21()\n\
             SomeActor.SetRestrained(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&non_player), None);

        let extra_arg = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_45()\n\
             Game.GetPlayer().SetRestrained(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&extra_arg), None);
    }

    #[test]
    fn player_controls_toggles_decline_with_too_many_args() {
        // Shared `prim_player_controls`'s `args.len() > 9` guard — covers
        // both DisablePlayerControls and EnablePlayerControls.
        let disable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_22()\n\
             Game.DisablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&disable), None);

        let enable = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_23()\n\
             Game.EnablePlayerControls(false, true, true, false, false, false, true, true, 0, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&enable), None);
    }

    #[test]
    fn set_player_ai_driven_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_24()\n\
             Game.SetPlayerAIDriven(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn set_hud_cart_mode_declines_with_extra_arg() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_25()\n\
             Game.SetHudCartMode(true, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    #[test]
    fn play_idle_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_26()\n\
             Game.GetPlayer().PlayIdle()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_27()\n\
             Game.GetPlayer().PlayIdle(SomeIdle, SomeIdle)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn set_vehicle_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_28()\n\
             Alias_Rider.GetActorRef().SetVehicle()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_29()\n\
             Alias_Rider.GetActorRef().SetVehicle(Alias_Cart.GetRef(), Alias_Cart.GetRef())\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn tether_to_horse_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_30()\n\
             Alias_Cart.GetRef().TetherToHorse()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_47()\n\
             Alias_Cart.GetRef().TetherToHorse(Alias_Horse.GetActorRef() as ObjectReference, Alias_Horse.GetActorRef() as ObjectReference)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn set_motion_type_declines_on_arg_count_and_unrecognized_member() {
        // #2289 flags this as the most structurally intricate untested case.
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_31()\n\
             Cart.SetMotionType()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let three_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_32()\n\
             Cart.SetMotionType(6, true, false)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&three_args), None);

        // An unrecognized member access on the (correctly matched) receiver
        // falls through `motion_type_arg`'s match arm to `None`.
        let unrecognized_member = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_33()\n\
             Cart.SetMotionType(Cart.Motion_NotARealValue, true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&unrecognized_member), None);
    }

    #[test]
    fn set_sitting_rotation_declines_on_wrong_arg_count() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_34()\n\
             Game.SetSittingRotation()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let two_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_48()\n\
             Game.SetSittingRotation(-55.0, 1.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&two_args), None);
    }

    #[test]
    fn exit_cart_declines_on_wrong_arg_count_and_seat_out_of_range() {
        let wrong_arg_count = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_35()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&wrong_arg_count), None);

        let seat_too_low = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_36()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&seat_too_low), None);

        let seat_too_high = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_37()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.ExitCart(Alias_Prisoner, 6)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&seat_too_high), None);
    }

    #[test]
    fn player_animation_events_decline_with_args() {
        let imod = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_38()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.PlayerImodAnimation(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&imod), None);

        let furniture = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_39()\n\
             Quest temp = Self as Quest\n\
             mq101questscript kmyQuest = temp as mq101questscript\n\
             kmyQuest.PlayerFurnitureAnimation(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&furniture), None);
    }

    /// `abResetAI = true` resets the actor's AI on top of the package
    /// re-evaluation, which `Effect::EvaluatePackage` does not model —
    /// so it still declines after #3487 widened the arity bound.
    #[test]
    fn evaluate_package_declines_with_args() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_40()\n\
             Alias_Hadvar.GetActorRef().EvaluatePackage(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&body), None);
    }

    /// Regression for #3487. Skyrim's `Actor.EvaluatePackage()` takes no
    /// parameters; FO4's and Starfield's take `abResetAI = false`, so the
    /// identical statement compiles to a one-argument call there. The
    /// old zero-argument guard made this primitive silently
    /// game-asymmetric — working on Skyrim, declining 2,628 of 3,481
    /// calls on FO4/Starfield.
    #[test]
    fn lowers_the_default_materialized_evaluate_package_shape() {
        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_40()\n\
             Alias_Hadvar.GetActorRef().EvaluatePackage(false)\n EndFunction\n",
        );
        assert_eq!(
            lower_fragment(&body),
            Some(vec![Effect::EvaluatePackage {
                actor: ActorRef::Object(ObjectRef::Property("Alias_Hadvar".into())),
            }])
        );
    }

    #[test]
    fn wait_declines_on_wrong_arg_count_and_negative_seconds() {
        let zero_args = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_41()\n\
             Utility.Wait()\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&zero_args), None);

        let negative = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_42()\n\
             Utility.Wait(-1.0)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&negative), None);
    }

    #[test]
    fn start_and_stop_scene_decline_with_args() {
        let start = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_43()\n\
             IntroScene.Start(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&start), None);

        let stop = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_44()\n\
             OldScene.Stop(true)\n EndFunction\n",
        );
        assert_eq!(lower_fragment(&stop), None);
    }

    // ── #2540 / SCR-D5-NEW10-02 — the objective-index `u16`→`i32` widen ──

    /// A genuinely negative index (i32 on FO3/FNV per `QuestObjective::
    /// index`'s doc) arrives from the PEX decompiler as a direct signed
    /// `IntLit`, never as a `.psc`-source `UnaryOp::Neg` — the `.psc` text
    /// parser lexes `-N` as `Minus` + `IntLit(N)`, which `int_arg` does not
    /// unwrap. Matching #2657's `::X_var` precedent, this decompiled shape
    /// has to be built by hand rather than round-tripped through the `.psc`
    /// parser to exercise the real input this primitive actually receives.
    #[test]
    fn objective_primitives_lower_a_negative_index() {
        for (method, expected) in [
            (
                "SetObjectiveDisplayed",
                Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    displayed: true,
                },
            ),
            (
                "SetObjectiveCompleted",
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    completed: true,
                },
            ),
            (
                "SetObjectiveFailed",
                Effect::SetObjectiveFailed {
                    quest: QuestRef::SelfRef,
                    objective: -5,
                    failed: true,
                },
            ),
        ] {
            let call = Expr::Call {
                callee: Box::new(sp(Expr::MemberAccess {
                    object: Box::new(sp(Expr::Ident(Identifier("Self".into())))),
                    member: sp(Identifier(method.into())),
                })),
                args: vec![CallArg {
                    name: None,
                    value: sp(Expr::IntLit(-5)),
                }],
            };
            assert_eq!(
                classify_effect(&call, &Scope::default()),
                Some(expected),
                "{method} must lower a negative index correctly"
            );
        }
    }

    /// An index literal outside `i32`'s range must still decline via
    /// `i32::try_from(..).ok()?`, not silently truncate.
    #[test]
    fn objective_primitives_decline_on_index_overflow() {
        for method in [
            "SetObjectiveDisplayed",
            "SetObjectiveCompleted",
            "SetObjectiveFailed",
        ] {
            let body = first_fn_body(&format!(
                "ScriptName QF extends Quest\n\
                 Function Fragment_46()\n\
                 Self.{method}(5000000000)\n EndFunction\n"
            ));
            assert_eq!(
                lower_fragment(&body),
                None,
                "{method} must decline an i32-overflowing index"
            );
        }
    }

    /// #3934 — one installed extension must not change how vanilla content
    /// lowers. `Self` is the SDK's reserved provider for instance-method
    /// aliases, so any extension that publishes one makes
    /// `catalog.contains_provider("self")` true; with the catalog consulted
    /// first, `Self.SetStage(10)` then missed the route, hard-errored as an
    /// unbound extender call, and declined the whole fragment — across the
    /// entire quest-fragment population, on every game.
    #[test]
    fn a_self_provider_alias_does_not_shadow_the_canonical_effect_table() {
        use byroredux_sdk::identity::{ComponentId, ExtensionId, ScriptFunctionId};
        use byroredux_sdk::script_function::{PapyrusFunctionAlias, ScriptFunctionDeclaration};

        let extension = ExtensionId::new("audit.fixture").expect("valid extension id");
        let mut providers = PapyrusProviderCatalog::engine_compatibility();
        providers
            .insert(
                &extension,
                &ScriptFunctionDeclaration {
                    id: ScriptFunctionId::new("touch").expect("valid function id"),
                    component: ComponentId::new("runtime").expect("valid component id"),
                    parameters: Vec::new(),
                    result: None,
                    // Reserved instance-method spelling: `Self.Touch()`.
                    papyrus: Some(PapyrusFunctionAlias {
                        provider: "Self".to_owned(),
                        function: "Touch".to_owned(),
                    }),
                    description: "fixture instance method".to_owned(),
                },
            )
            .expect("the alias must install");

        let body = first_fn_body(
            "ScriptName QF extends Quest\n\
             Function Fragment_0()\n\
             Self.SetStage(10)\n\
             EndFunction\n",
        );
        let effects = lower_fragment_with_quest_properties_and_providers(
            &body,
            &HashSet::new(),
            Some(&providers),
        )
        .expect("a canonical SetStage must still lower with a Self alias installed");
        assert_eq!(
            effects,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 10,
            }],
            "the canonical effect table must win over a manifest alias"
        );
    }
}
