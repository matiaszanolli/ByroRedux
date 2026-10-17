//! Quest-scoped primitives (#5801): global values, stages, quest
//! lifecycle, objectives, aliases and Story Manager events. Applied by
//! `fragment/effects/quest_scoped.rs`.

use super::*;

/// `GlobalVariable.SetValue(afValue)` / `SetValueInt(aiValue)` with a
/// run-time number — a literal, `S.GetValue() + 1.0` (the radio-ready and
/// counter idioms), any [`ValueExpr`] — and `G.Mod(afHowMuch)`, which the CK
/// defines as adding to the value.
pub(super) fn prim_set_global_value(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
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

pub(super) fn prim_start_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

pub(super) fn prim_stop_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopQuest {
        quest: explicit_quest_receiver(object, scope)?,
    })
}

pub(super) fn prim_complete_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_reset_quest(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_set_quest_active(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_set_objective_displayed(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_set_objective_completed(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_set_objective_failed(e: &Expr, scope: &Scope) -> Option<Effect> {
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

pub(super) fn prim_complete_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "CompleteAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::CompleteAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

pub(super) fn prim_fail_all_objectives(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "FailAllObjectives")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::FailAllObjectives {
        quest: receiver_quest(object, scope)?,
    })
}

/// `Quest.SetObjectiveSkipped(int aiObjectiveID)` — FO4's `Quest.psc` body:
/// `If !IsObjectiveCompleted(id) SetObjectiveDisplayed(id, false, false)`.
pub(super) fn prim_set_objective_skipped(e: &Expr, scope: &Scope) -> Option<Effect> {
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

/// `Keyword.SendStoryEvent(Location akLoc = None, ObjectReference akRef1 =
/// None, ObjectReference akRef2 = None, int aiValue1 = 0, int aiValue2 = 0)`
/// — every parameter materialized by the compiler. A `None` literal leaves a
/// slot empty; the integers must be literals.
pub(super) fn prim_send_story_event(e: &Expr, scope: &Scope) -> Option<Effect> {
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

/// `<alias>.ForceRefTo(akRef)` / `<alias>.Clear()` — the CK's *ReferenceAlias*
/// (and *Alias*) script functions. The receiver is the alias property itself.
pub(super) fn prim_force_alias(e: &Expr, scope: &Scope) -> Option<Effect> {
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
pub(super) fn prim_force_location_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ForceLocationTo")?;
    if args.len() != 1 || !matches!(object, Expr::Ident(_)) {
        return None;
    }
    Some(Effect::ForceLocationTo {
        alias: receiver_object(object, scope)?,
        location: receiver_object(&args[0].value.node, scope)?,
    })
}
