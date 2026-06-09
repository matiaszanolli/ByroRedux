//! Recognizer: quest-stage-gate → [`QuestAdvanceOnActivate`].
//!
//! A **generic** recognizer — the abstraction payoff. It matches a
//! behavior *family*, not one named script: an `OnActivate` handler that
//! guards a `SetStage(Z)` behind one-or-more `GetStageDone(N) == E`
//! predicates. DA10MainDoorScript is the canonical member, but the same
//! recognizer covers every Skyrim quest-door / quest-activator script of
//! this shape — one recognizer, many scripts.
//!
//! It extracts the stage logic (`N`, `E`, `Z`) from the AST literals and
//! resolves the *owning quest* from the attach context:
//! - `Self.GetOwningQuest()` (alias-attached, DA10) → `ctx.owning_quest`.
//! - `MyQuest.SetStage(..)` (a `Quest Property`) → the VMAD-bound FormID
//!   of that property in `ctx.script_instance`.
//!
//! Per the M47.1 lowering, the predicates become a generic
//! [`ConditionList`] of `GetStageDone` CTDAs — the same data shape the
//! `da10_main_door` hand-builder produces.

use byroredux_papyrus::ast::{BinaryOp, Expr, Script, ScriptItem, StateItem, Stmt};
use byroredux_papyrus::span::Spanned;

use crate::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
use crate::quest_stages::QuestFormId;
use crate::translate::archetype::{RecognizeCtx, Recognized};
use crate::translate::source::ScriptSource;
use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};

/// How a quest-stage call names its quest.
#[derive(Debug, Clone, PartialEq)]
enum QuestVia {
    /// `Self.GetOwningQuest()` — the quest owning this alias.
    OwningQuest,
    /// A `Quest Property NAME` — bound from VMAD by property name.
    Property(String),
}

/// The extracted gate: predicates + target stage + how the quest is named.
struct StageGate {
    /// `(stage, expected)` pairs from the `GetStageDone` predicates.
    conditions: Vec<(u16, f32)>,
    target_stage: u16,
    quest_via: QuestVia,
}

pub fn recognize(ctx: &RecognizeCtx<'_>) -> Option<Recognized> {
    let ScriptSource::PapyrusSource(script) = ctx.source else {
        return None;
    };
    let event_body = find_on_activate_body(script)?;
    let gate = extract_stage_gate(event_body)?;

    // Resolve the owning quest by how the script named it.
    let owning_quest = match &gate.quest_via {
        QuestVia::OwningQuest => ctx.owning_quest?,
        QuestVia::Property(name) => ctx
            .script_instance?
            .scripts
            .iter()
            .find_map(|s| s.object_form_id(name))?,
    };

    let conditions: Vec<Condition> = gate
        .conditions
        .iter()
        .map(|(stage, expected)| Condition {
            function_index: 59, // GetStageDone
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(*expected),
            param_1: owning_quest,
            param_2: *stage as u32,
            run_on: RunOn::Subject,
            reference_form_id: 0,
            extra_data_id: 0,
            or_next: false,
        })
        .collect();

    let component = QuestAdvanceOnActivate {
        owning_quest: QuestFormId(owning_quest),
        conditions,
        target_stage: gate.target_stage,
        // No player gate is extracted yet; DA10's family doesn't filter.
        activator_gate: ActivatorGate::Any,
    };

    Some(Recognized::new(
        format!("quest_stage_gate@{}", script.name.node),
        move |world, entity| {
            if let Some(mut q) = world.query_mut::<QuestAdvanceOnActivate>() {
                q.insert(entity, component.clone());
            }
        },
    ))
}

/// The statement body of the script's `OnActivate` handler — searched at
/// top level and inside any `State`.
fn find_on_activate_body(script: &Script) -> Option<&[Spanned<Stmt>]> {
    for item in &script.body {
        match &item.node {
            ScriptItem::Event(e) if e.name.node.eq_ignore_case("OnActivate") => {
                return Some(&e.body);
            }
            ScriptItem::State(st) => {
                for si in &st.body {
                    if let StateItem::Event(e) = &si.node {
                        if e.name.node.eq_ignore_case("OnActivate") {
                            return Some(&e.body);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the stage gate from an `OnActivate` body: the first `If`
/// whose condition is a conjunction of `GetStageDone(N) == E` predicates
/// and whose body calls `SetStage(Z)`. `None` if the shape doesn't match.
fn extract_stage_gate(body: &[Spanned<Stmt>]) -> Option<StageGate> {
    for stmt in body {
        let Stmt::If {
            condition, body, ..
        } = &stmt.node
        else {
            continue;
        };
        let mut conditions = Vec::new();
        let mut quest_via: Option<QuestVia> = None;
        collect_stage_predicates(&condition.node, &mut conditions, &mut quest_via);
        if conditions.is_empty() {
            continue;
        }
        // SetStage(Z) in the if-body (sets the target + confirms the via).
        let (target_stage, set_via) = find_set_stage(body)?;
        let quest_via = quest_via.or(Some(set_via.clone()))?;
        // If both the predicate and the SetStage name a quest, they must
        // agree (a sane quest-gate script writes the quest it gates on).
        if quest_via != set_via {
            continue;
        }
        return Some(StageGate {
            conditions,
            target_stage,
            quest_via,
        });
    }
    None
}

/// Walk an `If` condition collecting `GetStageDone(stage) == expected`
/// predicates (And-combined), recording how the quest is named.
fn collect_stage_predicates(e: &Expr, out: &mut Vec<(u16, f32)>, via: &mut Option<QuestVia>) {
    match e {
        Expr::BinaryOp {
            op: BinaryOp::And,
            left,
            right,
        } => {
            collect_stage_predicates(&left.node, out, via);
            collect_stage_predicates(&right.node, out, via);
        }
        Expr::BinaryOp {
            op: BinaryOp::Eq,
            left,
            right,
        } => {
            // One side is GetStageDone(stage); the other is the expected
            // literal (`1`/`0`, possibly `as Bool`).
            let staged = as_get_stage_done(&left.node)
                .map(|sv| (sv, as_num(&right.node)))
                .or_else(|| as_get_stage_done(&right.node).map(|sv| (sv, as_num(&left.node))));
            if let Some(((stage, q), Some(expected))) = staged {
                out.push((stage, expected));
                via.get_or_insert(q);
            }
        }
        // A bare `GetStageDone(stage)` used as a boolean → `== 1`.
        _ => {
            if let Some((stage, q)) = as_get_stage_done(e) {
                out.push((stage, 1.0));
                via.get_or_insert(q);
            }
        }
    }
}

/// If `e` is a `…GetStageDone(stage)` call, return `(stage, quest_via)`.
fn as_get_stage_done(e: &Expr) -> Option<(u16, QuestVia)> {
    let (object, args) = method_call(e, "GetStageDone")?;
    let stage = int_arg(args, 0)?;
    Some((u16::try_from(stage).ok()?, quest_via(object)?))
}

/// Find the first `SetStage(Z)` statement in a body → `(Z, quest_via)`.
fn find_set_stage(body: &[Spanned<Stmt>]) -> Option<(u16, QuestVia)> {
    body.iter().find_map(|stmt| {
        let Stmt::ExprStmt(e) = &stmt.node else {
            return None;
        };
        let (object, args) = method_call(&e.node, "SetStage")?;
        let target = u16::try_from(int_arg(args, 0)?).ok()?;
        Some((target, quest_via(object)?))
    })
}

/// Classify the receiver of a `GetStageDone`/`SetStage` call.
fn quest_via(object: &Expr) -> Option<QuestVia> {
    if method_call(object, "GetOwningQuest").is_some() {
        Some(QuestVia::OwningQuest)
    } else if let Expr::Ident(name) = object {
        Some(QuestVia::Property(name.0.clone()))
    } else {
        None
    }
}

/// If `e` is `<object>.<method>(args)`, return `(&object, args)`.
fn method_call<'a>(
    e: &'a Expr,
    method: &str,
) -> Option<(&'a Expr, &'a [byroredux_papyrus::ast::CallArg])> {
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
fn int_arg(args: &[byroredux_papyrus::ast::CallArg], idx: usize) -> Option<i64> {
    match &args.get(idx)?.value.node {
        Expr::IntLit(n) => Some(*n),
        _ => None,
    }
}

/// A numeric literal — `Int`/`Float`/`Bool`, unwrapping a `… as Bool`
/// cast (the `== 1 as Bool` idiom Champollion emits).
fn as_num(e: &Expr) -> Option<f32> {
    match e {
        Expr::IntLit(n) => Some(*n as f32),
        Expr::FloatLit(f) => Some(*f as f32),
        Expr::BoolLit(b) => Some(if *b { 1.0 } else { 0.0 }),
        Expr::Cast { expr, .. } => as_num(&expr.node),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::papyrus_demo::quest_advance::da10_main_door;
    use crate::translate::translate_script;
    use byroredux_papyrus::parse_script;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const DA10_SRC: &str = include_str!("../../../../../docs/r5/source/DA10MainDoorScript.psc");
    const DA10_QUEST: u32 = 0x0002_2f08; // the real DA10 quest FormID

    #[test]
    fn recognizes_da10_and_reproduces_hand_builder() {
        let (script, errors) = parse_script(DA10_SRC).expect("DA10 .psc parses");
        assert!(errors.is_empty(), "clean parse: {errors:?}");
        let source = ScriptSource::PapyrusSource(&script);

        // DA10 uses GetOwningQuest → owning quest comes from the alias
        // attach context (here the real DA10 quest FormID).
        let recognized = translate_script(&source, GameKind::Skyrim, None, Some(DA10_QUEST))
            .expect("quest_stage_gate recognized");
        assert_eq!(recognized.archetype, "quest_stage_gate@DA10MainDoorScript");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);

        let q = world
            .query::<QuestAdvanceOnActivate>()
            .expect("QuestAdvanceOnActivate registered");
        let got = q.get(entity).expect("component spawned");
        let expected = da10_main_door(QuestFormId(DA10_QUEST));

        // The recognizer must reproduce the hand-builder byte-for-byte.
        assert_eq!(got.owning_quest, expected.owning_quest);
        assert_eq!(got.target_stage, expected.target_stage); // 40
        assert_eq!(got.conditions.len(), expected.conditions.len()); // 2
        for (g, e) in got.conditions.iter().zip(expected.conditions.iter()) {
            assert_eq!(g.function_index, e.function_index); // 59
            assert_eq!(g.param_1, e.param_1); // quest
            assert_eq!(g.param_2, e.param_2); // stage 37 / 40
            assert_eq!(g.comparand, e.comparand); // 1.0 / 0.0
        }
    }

    #[test]
    fn declines_when_owning_quest_unavailable() {
        // Same DA10 shape, but the alias-owning quest isn't supplied:
        // the recognizer can read the stage logic but can't bind the
        // quest, so it declines (silent miss until the attach context
        // provides it).
        let (script, _) = parse_script(DA10_SRC).unwrap();
        let source = ScriptSource::PapyrusSource(&script);
        assert!(translate_script(&source, GameKind::Skyrim, None, None).is_none());
    }

    #[test]
    fn binds_quest_property_form_via_vmad() {
        // The other quest-gate form: `MyQuest.SetStage(20)` gated on
        // `MyQuest.GetStageDone(10)`, where MyQuest is a Quest Property
        // bound by VMAD to a FormID.
        let src = "ScriptName GenericDoor extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If MyQuest.GetStageDone(10) == 1\n\
                   MyQuest.SetStage(20)\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);

        // VMAD binds MyQuest → FormID 0x00012345.
        let instance = ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "GenericDoor".into(),
                status: 1,
                properties: vec![ScriptProperty {
                    name: "MyQuest".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: 0x0001_2345,
                        alias: -1,
                    },
                }],
            }],
        };

        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("recognized via VMAD quest-property binding");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.owning_quest, QuestFormId(0x0001_2345));
        assert_eq!(got.target_stage, 20);
        assert_eq!(got.conditions.len(), 1);
        assert_eq!(got.conditions[0].param_2, 10); // stage 10
    }
}
