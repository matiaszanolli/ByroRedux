//! Guard and value classification (#5801, split from `effects.rs`): a
//! Papyrus condition → conjunctive [`FragmentGuard`]s, a numeric expression
//! → a [`ValueExpr`], or decline.

use super::*;

/// Papyrus calls that return a `Float` (#5974): the actor-value and global
/// readings and `GetDistance`.
const FLOAT_READS: &[&str] = &[
    "GetValue",
    "GetAV",
    "GetActorValue",
    "GetBaseValue",
    "GetBaseActorValue",
    "GetValuePercentage",
    "GetActorValuePercentage",
    "GetDistance",
];

/// Papyrus calls that return an `Int` (#5974).
const INT_READS: &[&str] = &["GetValueInt", "GetStage", "GetItemCount", "GetLevel"];

/// Whether `e` is a `Float` by its own syntax (#5974): a float literal, a cast
/// to `Float`, a call that returns one, or arithmetic with such an operand
/// (Papyrus promotes `Int op Float` to `Float`). `false` means *not provably*,
/// not "an Int": the lowering has no type table, so an identifier is unknown.
fn provably_float(e: &Expr) -> bool {
    match e {
        Expr::FloatLit(_) => true,
        Expr::Cast { target_type, .. } => matches!(target_type.node, byroredux_papyrus::ast::Type::Float),
        Expr::UnaryOp { operand, .. } => provably_float(&operand.node),
        Expr::BinaryOp {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
            left,
            right,
        } => provably_float(&left.node) || provably_float(&right.node),
        _ => method_call_any(e, FLOAT_READS).is_some(),
    }
}

/// Whether `e` is an integral `Int` by its own syntax (#5974): an int literal,
/// a call that returns an `Int`, `Utility.RandomInt`, a cast to `Int` of such
/// a value, or `+ - *` over those. A cast of anything else to `Int` is NOT
/// provably integral: [`classify_value`] strips casts, so the truncation a
/// `Float as Int` performs would be lost.
fn provably_int_valued(e: &Expr) -> bool {
    match e {
        Expr::IntLit(_) => true,
        Expr::UnaryOp { op: UnaryOp::Neg, operand } => provably_int_valued(&operand.node),
        Expr::Cast { expr, target_type } => {
            matches!(target_type.node, byroredux_papyrus::ast::Type::Int) && provably_int_valued(&expr.node)
        }
        Expr::BinaryOp {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul,
            left,
            right,
        } => provably_int_valued(&left.node) && provably_int_valued(&right.node),
        _ => method_call_any(e, INT_READS).is_some() || static_call(e, "Utility", "RandomInt").is_some(),
    }
}

/// `SetValueInt(aiValue)` stores an `Int`: the value truncates. The lowering
/// has no `Int` type, so it keeps the call only when the value is integral by
/// construction ([`provably_int_valued`]) and otherwise declines (#5974).
pub(super) fn int_store_is_exact(value: &Expr) -> bool {
    provably_int_valued(value)
}

/// M47.2 — a numeric expression as a [`ValueExpr`]: a literal, one of the
/// numeric guard readings, `GetLevel()`, `Utility.RandomInt` with literal
/// bounds, or arithmetic over those.
///
/// #5974 — arithmetic here is `f32`. That is exact for `+ - *` over integral
/// values, but Papyrus `Int / Int` truncates, so a division is lowered only
/// when an operand is provably `Float` (then the whole expression is float
/// arithmetic, as in Papyrus); an unknown or integer division declines.
pub(super) fn classify_value(e: &Expr, scope: &Scope) -> Option<ValueExpr> {
    let mut e = e;
    while let Expr::Cast { expr, target_type } = e {
        // #6215 — `x as Int` truncates and `ValueExpr` is `f32` arithmetic with
        // no truncation: only an operand that is already integral casts exactly.
        if matches!(target_type.node, byroredux_papyrus::ast::Type::Int) && !provably_int_valued(&expr.node) {
            return None;
        }
        e = &expr.node;
    }
    if let Some(value) = as_num(e) {
        return Some(ValueExpr::Literal(value));
    }
    if let Expr::Ident(name) = e {
        let key = name.0.to_ascii_lowercase();
        // #5978 — a script variable that holds a reference, string or array
        // is not a number: declining beats a read that defaults to 0.0.
        if scope.non_numeric_script_variables.contains(&key) {
            return None;
        }
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
            BinaryOp::Div => {
                if !provably_float(&left.node) && !provably_float(&right.node) {
                    return None;
                }
                ArithOp::Div
            }
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
        // #6215 — a truncated global has no exact `ValueExpr`: the VM runs it.
        GuardReading::GlobalInt(_) => return None,
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
    /// #6215 — `<global>.GetValueInt()` / `<global>.GetValue() as Int`: the
    /// stored float truncated toward zero.
    GlobalInt(ObjectRef),
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
pub(super) fn classify_condition(
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
    if let Expr::Cast { expr, target_type } = e {
        let inner = guard_reading(&expr.node, scope)?;
        if !matches!(target_type.node, byroredux_papyrus::ast::Type::Int) {
            return Some(inner);
        }
        // #6215 — `as Int` truncates: a global becomes its truncated reading, an
        // integral or boolean reading is unchanged, and a float reading of no
        // known integral type (a quest variable, a distance) declines.
        return match inner {
            GuardReading::Global(global) | GuardReading::GlobalInt(global) => Some(GuardReading::GlobalInt(global)),
            GuardReading::QuestVariable(..) | GuardReading::Distance(..) => None,
            integral => Some(integral),
        };
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
    for (name, reading) in [
        ("GetValue", GuardReading::Global as fn(ObjectRef) -> GuardReading),
        // #6215 — "returns the value of the global as an int": truncated.
        ("GetValueInt", GuardReading::GlobalInt),
    ] {
        if let Some((object, args)) = method_call(e, name) {
            if !args.is_empty() {
                return None;
            }
            return global_receiver(object, scope).map(reading);
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
pub(super) fn global_receiver(object: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let Expr::Ident(name) = object else {
        return None;
    };
    let key = name.0.to_ascii_lowercase();
    if scope.declares(&key) || scope.speaker_locals.contains(&key) {
        return None;
    }
    receiver_object(object, scope)
}

/// #6215 — `trunc(G) <op> c` as guards on the stored float `G` itself, exactly:
/// Papyrus's `Float → Int` truncates toward zero, so for an integer `c`
/// `trunc(G) >= c` is `G >= c` when `c > 0` and `G > c - 1` otherwise, and
/// `trunc(G) <= c` is `G < c + 1` when `c >= 0` and `G <= c` otherwise; `>` /
/// `<` shift `c` by one, `==` is both bounds and `!=` either outside one. A
/// non-integral `c` declines (the run is left to the VM).
fn truncated_global_guard(global: ObjectRef, comparison: GuardComparison, value: f32) -> Option<FragmentGuard> {
    if value.fract() != 0.0 || !value.is_finite() {
        return None;
    }
    let c = value;
    let at_least = |c: f32| if c > 0.0 { (GuardComparison::Ge, c) } else { (GuardComparison::Gt, c - 1.0) };
    let at_most = |c: f32| if c >= 0.0 { (GuardComparison::Lt, c + 1.0) } else { (GuardComparison::Le, c) };
    let alternatives: Vec<Vec<(GuardComparison, f32)>> = match comparison {
        GuardComparison::Ge => vec![vec![at_least(c)]],
        GuardComparison::Gt => vec![vec![at_least(c + 1.0)]],
        GuardComparison::Le => vec![vec![at_most(c)]],
        GuardComparison::Lt => vec![vec![at_most(c - 1.0)]],
        GuardComparison::Eq => vec![vec![at_least(c), at_most(c)]],
        GuardComparison::Ne => vec![vec![at_most(c - 1.0)], vec![at_least(c + 1.0)]],
    };
    let atom = |(comparison, value): (GuardComparison, f32)| FragmentGuard::Global { global: global.clone(), comparison, value };
    Some(match <[_; 1]>::try_from(alternatives) {
        Ok([alternative]) if alternative.len() == 1 => atom(alternative[0]),
        Ok([alternative]) => FragmentGuard::Any(vec![alternative.into_iter().map(atom).collect()]),
        Err(alternatives) => FragmentGuard::Any(
            alternatives.into_iter().map(|alternative| alternative.into_iter().map(atom).collect()).collect(),
        ),
    })
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
        GuardReading::GlobalInt(global) => truncated_global_guard(global, comparison, value)?,
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

#[cfg(test)]
mod truncation_tests {
    use super::*;

    fn holds(guard: &FragmentGuard, x: f32) -> bool {
        match guard {
            FragmentGuard::Global { comparison, value, .. } => comparison.apply(x, *value),
            FragmentGuard::Any(alternatives) => alternatives.iter().any(|all| all.iter().all(|guard| holds(guard, x))),
            other => panic!("unexpected guard {other:?}"),
        }
    }

    /// #6215 — the rewrite agrees with Papyrus's truncate-then-compare for every
    /// comparison, integral literal and value, negatives and fractions included.
    #[test]
    fn truncated_global_guard_matches_truncate_then_compare() {
        use GuardComparison::*;
        for comparison in [Eq, Ne, Lt, Le, Gt, Ge] {
            for c in -3..=3 {
                let guard = truncated_global_guard(ObjectRef::Property("G".into()), comparison, c as f32).unwrap();
                for step in -20..=20 {
                    let x = step as f32 * 0.25;
                    assert_eq!(
                        holds(&guard, x),
                        comparison.apply(x.trunc(), c as f32),
                        "trunc({x}) {comparison:?} {c}"
                    );
                }
            }
        }
        assert!(truncated_global_guard(ObjectRef::Property("G".into()), Eq, 0.5).is_none());
    }
}
