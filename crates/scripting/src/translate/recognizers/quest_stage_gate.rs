//! Recognizer: quest-advance-on-activate → [`QuestAdvanceOnActivate`].
//!
//! A **generic** recognizer — the abstraction payoff. It matches a
//! behavior *family*, not one named script: an `OnActivate` handler whose
//! whole job is to advance a quest. Three shapes are covered, spanning
//! the bulk of the Skyrim/FO4 quest-door / quest-activator / `default*`
//! script population — one recognizer, many scripts:
//!
//! 1. **Guarded** — `SetStage(Z)` behind one-or-more `GetStageDone(N) == E`
//!    predicates (DA10MainDoorScript, the canonical member).
//! 2. **Player-gated** — any of the above wrapped in, or conjoined with,
//!    `akActionRef == Game.GetPlayer()` (MG07 / TG05 doors, the common
//!    `default*` player-only idiom) → [`ActivatorGate::PlayerOnly`].
//! 3. **Unconditional** — the handler body is *exactly* one `SetStage(Z)`
//!    call (the `defaultSetStageOnActivate` family) → an empty
//!    [`ConditionList`] (which the dispatch system advances unconditionally).
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
//!
//! **Correctness is conservative.** The extractor declines (a silent
//! miss) the instant it meets a condition term it doesn't model — a
//! `GetStage() < N` comparison, a `HasPerk`, an `||` — or a body that
//! carries logic beyond the advance. A missed script is harmless (no
//! behavior attached); a *misread* one would change gameplay, so the
//! recognizer never guesses past what it fully understands.

use byroredux_papyrus::ast::{Event, Script, ScriptItem, StateItem, Stmt};
use byroredux_papyrus::span::Spanned;

use crate::papyrus_demo::quest_advance::{ActivatorGate, QuestAdvanceOnActivate};
use crate::quest_stages::QuestFormId;
use crate::translate::archetype::{RecognizeCtx, Recognized};
use crate::translate::compose::{
    classify_guard_atom, int_arg, method_call, quest_via, split_and, GuardMatch, QuestRef,
};
use crate::translate::source::ScriptSource;
use byroredux_plugin::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};

/// The extracted gate: predicates + target stage + how the quest is
/// named + whether activation is player-gated.
struct StageGate {
    /// `(stage, expected)` pairs from the `GetStageDone` predicates.
    /// Empty for the unconditional `defaultSetStageOnActivate` family.
    conditions: Vec<(u16, f32)>,
    target_stage: u16,
    quest_via: QuestRef,
    /// True when the advance is wrapped in `If akActionRef == Game.GetPlayer()`
    /// — the MG07 / TG05 / `default*` player-only idiom. Lowers to
    /// [`ActivatorGate::PlayerOnly`].
    player_only: bool,
}

pub fn recognize(ctx: &RecognizeCtx<'_>) -> Option<Recognized> {
    let ScriptSource::PapyrusSource(script) = ctx.source else {
        return None;
    };
    let event = find_advance_event(script)?;
    let gate = extract_stage_gate(event)?;

    // Resolve the owning quest by how the script named it.
    let owning_quest = match &gate.quest_via {
        QuestRef::OwningQuest => ctx.owning_quest?,
        // `Self` as a quest only arises in quest stage/scene fragments
        // (handled by the fragment lowerer), never in a REFR's
        // `OnActivate`/`OnTriggerEnter` handler — a REFR's `Self` is the
        // reference, which has no `SetStage`. Decline if it ever appears.
        QuestRef::SelfRef => return None,
        QuestRef::Property(name) => ctx
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
        activator_gate: if gate.player_only {
            ActivatorGate::PlayerOnly
        } else {
            ActivatorGate::Any
        },
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

/// The script's quest-advance handler — an `OnActivate` (use-key) or
/// `OnTriggerEnter` (volume-crossing) event, searched at top level and
/// inside any `State`. Returns the [`Event`] (not just its body) so the
/// activator/triggerer parameter name is available for player-gate
/// detection. Both handlers take the entering reference as their first
/// parameter (`akActionRef`), so the same extraction serves both.
///
/// `OnActivate` is preferred when a script declares both — the
/// interactive path is the more common authored intent; a script with
/// distinct advance logic per handler is rare and falls to whichever the
/// extractor fully understands.
fn find_advance_event(script: &Script) -> Option<&Event> {
    const HANDLERS: [&str; 2] = ["OnActivate", "OnTriggerEnter"];
    for handler in HANDLERS {
        for item in &script.body {
            match &item.node {
                ScriptItem::Event(e) if e.name.node.eq_ignore_case(handler) => {
                    return Some(e);
                }
                ScriptItem::State(st) => {
                    for si in &st.body {
                        if let StateItem::Event(e) = &si.node {
                            if e.name.node.eq_ignore_case(handler) {
                                return Some(e);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract the stage gate from an `OnActivate` handler. Covers three
/// shapes in the quest-advance-on-activate family, each conservative —
/// any term the extractor doesn't fully understand makes it decline
/// (a silent miss is always safe; a misread is not):
///
/// 1. **Guarded** — `If <GetStageDone-conjunction> … SetStage(Z) …` (the
///    DA10 family).
/// 2. **Player-gated** — any of the above wrapped in (or conjoined with)
///    `akActionRef == Game.GetPlayer()` (MG07 / TG05 / many `default*`).
/// 3. **Unconditional** — the body (after peeling an optional player
///    gate) is *exactly* one `SetStage(Z)` call (`defaultSetStageOnActivate`).
fn extract_stage_gate(event: &Event) -> Option<StageGate> {
    let player_param = event.params.first().map(|p| p.name.node.0.as_str());

    // Peel an outer `If akActionRef == Game.GetPlayer()` wrapper, if the
    // whole handler body is exactly that guard.
    let (body, player_only) = peel_player_gate(&event.body, player_param);

    // Shape 1/2: a guarded SetStage inside an `If`. Mirroring shape 3's
    // single-statement invariant, the post-peel body must be EXACTLY the
    // guarded `If` — a sibling statement (a `Self.Disable()` before or
    // after the gate) means the handler carries logic beyond the advance,
    // so decline rather than emit and silently drop the sibling.
    if let [only] = body {
        if let Some(gate) = match_guarded_if(&only.node, player_param, player_only) {
            return Some(gate);
        }
    }

    // Shape 3: unconditional — the body is exactly one `SetStage(Z)`.
    if let Some((target_stage, quest_via)) = single_set_stage(body) {
        return Some(StageGate {
            conditions: Vec::new(),
            target_stage,
            quest_via,
            player_only,
        });
    }

    None
}

/// If the handler body is exactly `If <player-gate> … EndIf` (no elseif /
/// else), return the inner body and `true`. Otherwise the body unchanged
/// and `false`. Only peels a *pure* player gate — a condition that also
/// carries stage predicates is left for [`classify_condition`] to handle.
fn peel_player_gate<'a>(
    body: &'a [Spanned<Stmt>],
    player_param: Option<&str>,
) -> (&'a [Spanned<Stmt>], bool) {
    if let [only] = body {
        if let Stmt::If {
            condition,
            body: inner,
            elseif_clauses,
            else_body,
        } = &only.node
        {
            if elseif_clauses.is_empty()
                && else_body.is_none()
                && crate::translate::compose::is_player_gate(&condition.node, player_param)
            {
                return (inner, true);
            }
        }
    }
    (body, false)
}

/// Match a single guarded `If <GetStageDone-conjunction> … SetStage(Z) …`
/// statement (shapes 1/2). The condition must classify cleanly through the
/// guard-primitive table and the `If` body must contain a `SetStage`;
/// otherwise `None` (the caller then declines). `outer_player` carries a
/// peeled outer player gate so it folds into the returned gate.
fn match_guarded_if(
    stmt: &Stmt,
    player_param: Option<&str>,
    outer_player: bool,
) -> Option<StageGate> {
    let Stmt::If {
        condition, body, ..
    } = stmt
    else {
        return None;
    };
    // Decline if the condition mixes in any term we don't model
    // (e.g. `GetStage() < N`, a `HasPerk`, an `||`) — recognizing it
    // as a plain AND-of-GetStageDone would change its behavior.
    let (conditions, quest_via, inner_player) = classify_if_condition(&condition.node, player_param)?;
    let (target_stage, set_via) = find_set_stage(body)?;
    let quest_via = quest_via.unwrap_or_else(|| set_via.clone());
    // If both the predicate and the SetStage name a quest, they must
    // agree (a sane quest-gate script writes the quest it gates on).
    if quest_via != set_via {
        return None;
    }
    Some(StageGate {
        conditions,
        target_stage,
        quest_via,
        player_only: outer_player || inner_player,
    })
}

/// Classify an `If` condition through the [`compose`] guard-primitive
/// table. Splits the condition into atomic predicates on `&&` (a `||` is
/// left as one atom that no primitive claims), runs each atom through the
/// table, and accumulates the result. Returns `None` the moment it meets
/// an atom no primitive models — the caller then declines rather than
/// silently dropping the unmodeled guard. On success: the
/// `(stage, expected)` predicates, how the quest is named (first naming
/// wins), and whether a player-gate term was present.
///
/// [`compose`]: crate::translate::compose
fn classify_if_condition(
    cond: &byroredux_papyrus::ast::Expr,
    player_param: Option<&str>,
) -> Option<(Vec<(u16, f32)>, Option<QuestRef>, bool)> {
    let mut atoms = Vec::new();
    split_and(cond, &mut atoms);

    let mut conditions = Vec::new();
    let mut quest_via: Option<QuestRef> = None;
    let mut player = false;
    for atom in atoms {
        // First match wins; an atom the table can't claim → decline whole.
        match classify_guard_atom(atom, player_param)? {
            GuardMatch::PlayerGate => player = true,
            GuardMatch::StageDone {
                via,
                stage,
                expected,
            } => {
                conditions.push((stage, expected));
                quest_via.get_or_insert(via);
            }
        }
    }
    Some((conditions, quest_via, player))
}

/// The single `SetStage(Z)` of a body that contains *only* that call —
/// the conservative unconditional shape. `None` if the body has any
/// other statement (which might be a guard we don't model).
fn single_set_stage(body: &[Spanned<Stmt>]) -> Option<(u16, QuestRef)> {
    let [only] = body else {
        return None;
    };
    let Stmt::ExprStmt(e) = &only.node else {
        return None;
    };
    let (object, args) = method_call(&e.node, "SetStage")?;
    let target = u16::try_from(int_arg(args, 0)?).ok()?;
    Some((target, quest_via(object)?))
}

/// Find the first `SetStage(Z)` statement in a body → `(Z, quest_via)`.
fn find_set_stage(body: &[Spanned<Stmt>]) -> Option<(u16, QuestRef)> {
    body.iter().find_map(|stmt| {
        let Stmt::ExprStmt(e) = &stmt.node else {
            return None;
        };
        let (object, args) = method_call(&e.node, "SetStage")?;
        let target = u16::try_from(int_arg(args, 0)?).ok()?;
        Some((target, quest_via(object)?))
    })
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

    /// A VMAD-bound `Quest Property` instance for the synthetic
    /// `default*`-shaped tests below (binds `MyQuest` → a FormID).
    fn quest_property_instance(script_name: &str, form_id: u32) -> ScriptInstanceData {
        ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: script_name.into(),
                status: 1,
                properties: vec![ScriptProperty {
                    name: "MyQuest".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id,
                        alias: -1,
                    },
                }],
            }],
        }
    }

    /// Unconditional `defaultSetStageOnActivate` shape: the handler body
    /// is a single `MyQuest.SetStage(30)` with no guard. Recognized as a
    /// quest advance with an empty `ConditionList` (the system treats an
    /// empty list as "advance unconditionally").
    #[test]
    fn recognizes_unconditional_set_stage() {
        let src = "ScriptName SetStageActi extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   MyQuest.SetStage(30)\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("SetStageActi", 0x0000_ABCD);

        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("unconditional SetStage recognized");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.owning_quest, QuestFormId(0x0000_ABCD));
        assert_eq!(got.target_stage, 30);
        assert!(got.conditions.is_empty(), "unconditional → no predicates");
        assert!(matches!(got.activator_gate, ActivatorGate::Any));
    }

    /// Player-gated shape: `If akActionRef == Game.GetPlayer()` wrapping a
    /// guarded SetStage (MG07 / TG05 / `default*` door idiom). The gate
    /// lowers to `ActivatorGate::PlayerOnly`; the inner stage predicate is
    /// still extracted.
    #[test]
    fn recognizes_player_gated_advance() {
        let src = "ScriptName PlayerDoor extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If akActionRef == Game.GetPlayer()\n\
                   If MyQuest.GetStageDone(10) == 1\n\
                   MyQuest.SetStage(20)\n\
                   EndIf\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("PlayerDoor", 0x0000_1111);

        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("player-gated advance recognized");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.target_stage, 20);
        assert_eq!(got.conditions.len(), 1);
        assert_eq!(got.conditions[0].param_2, 10);
        assert!(
            matches!(got.activator_gate, ActivatorGate::PlayerOnly),
            "the player gate lowers to PlayerOnly",
        );
    }

    /// Player-gate term conjoined inline (not nested):
    /// `If akActionRef == Game.GetPlayer() && MyQuest.GetStageDone(5) == 1`.
    #[test]
    fn recognizes_inline_player_gate_conjunction() {
        let src = "ScriptName InlineGate extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If akActionRef == Game.GetPlayer() && MyQuest.GetStageDone(5) == 1\n\
                   MyQuest.SetStage(15)\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("InlineGate", 0x0000_2222);
        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("inline player-gate conjunction recognized");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.target_stage, 15);
        assert_eq!(got.conditions.len(), 1);
        assert_eq!(got.conditions[0].param_2, 5);
        assert!(matches!(got.activator_gate, ActivatorGate::PlayerOnly));
    }

    /// The player gate is recognized regardless of operand order and
    /// through an `as Actor` cast — `Game.GetPlayer() as Actor == akActionRef`
    /// is the same gate as `akActionRef == Game.GetPlayer()`. Exercises the
    /// reversed-pair and cast-unwrap branches of the gate detector.
    #[test]
    fn player_gate_reversed_order_and_cast() {
        let src = "ScriptName RevGate extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If (Game.GetPlayer() as Actor) == akActionRef\n\
                   MyQuest.SetStage(7)\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("RevGate", 0x0000_7777);
        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("reversed/cast player gate recognized");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.target_stage, 7);
        assert!(
            matches!(got.activator_gate, ActivatorGate::PlayerOnly),
            "reversed-order / cast player gate still lowers to PlayerOnly",
        );
    }

    /// Correctness guard: a condition that mixes in a term the extractor
    /// doesn't model (`MyQuest.GetStage() >= 10`) must DECLINE, not get
    /// misread as an unconditional or partial advance. A silent miss is
    /// always safe; a behavior change is not.
    #[test]
    fn declines_unmodeled_condition_term() {
        let src = "ScriptName CmpDoor extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If MyQuest.GetStage() >= 10\n\
                   MyQuest.SetStage(20)\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("CmpDoor", 0x0000_3333);
        assert!(
            translate_script(&source, GameKind::Skyrim, Some(&instance), None).is_none(),
            "an unmodeled GetStage comparison must decline, not misread",
        );
    }

    /// Trigger-volume shape: the advance lives in an `OnTriggerEnter`
    /// handler (the `default*Trigger` family) rather than `OnActivate`.
    /// The same extraction applies — the entering reference is the first
    /// parameter either way — so the recognizer claims it and produces the
    /// same `QuestAdvanceOnActivate`, which the dispatch system fires from
    /// an `OnTriggerEnterEvent`.
    #[test]
    fn recognizes_on_trigger_enter_advance() {
        let src = "ScriptName TriggerBox extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnTriggerEnter(ObjectReference akActionRef)\n\
                   If akActionRef == Game.GetPlayer()\n\
                   MyQuest.SetStage(25)\n\
                   EndIf\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("TriggerBox", 0x0000_5555);

        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("OnTriggerEnter advance recognized");
        assert_eq!(recognized.archetype, "quest_stage_gate@TriggerBox");

        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.target_stage, 25);
        assert!(got.conditions.is_empty());
        assert!(
            matches!(got.activator_gate, ActivatorGate::PlayerOnly),
            "the player gate lowers to PlayerOnly even on the trigger path",
        );
    }

    /// Unconditional advance via the alias-owning quest: the receiver is
    /// `Self.GetOwningQuest()` (not a Quest Property), so the owning quest
    /// comes from the attach context, and there are no stage predicates.
    #[test]
    fn recognizes_unconditional_owning_quest_advance() {
        let src = "ScriptName AliasDoor extends ReferenceAlias\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   Self.GetOwningQuest().SetStage(50)\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);

        // No quest property — resolves only with the alias-owning quest.
        assert!(
            translate_script(&source, GameKind::Skyrim, None, None).is_none(),
            "without the owning quest the advance can't bind — declines",
        );
        let recognized = translate_script(&source, GameKind::Skyrim, None, Some(0x0001_9999))
            .expect("owning-quest unconditional advance recognized");
        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        let got = q.get(entity).unwrap();
        assert_eq!(got.owning_quest, QuestFormId(0x0001_9999));
        assert_eq!(got.target_stage, 50);
        assert!(got.conditions.is_empty());
    }

    /// When a script declares BOTH handlers, `OnActivate` wins (the
    /// interactive path is the authored intent; the priority is fixed).
    #[test]
    fn on_activate_wins_over_on_trigger_enter() {
        let src = "ScriptName DualHandler extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnTriggerEnter(ObjectReference akActionRef)\n\
                   MyQuest.SetStage(99)\n\
                   EndEvent\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   MyQuest.SetStage(10)\n\
                   EndEvent\n";
        let (script, errors) = parse_script(src).expect("parses");
        assert!(errors.is_empty(), "{errors:?}");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("DualHandler", 0x0000_6666);
        let recognized = translate_script(&source, GameKind::Skyrim, Some(&instance), None)
            .expect("dual-handler recognized");
        let mut world = byroredux_core::ecs::world::World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let q = world.query::<QuestAdvanceOnActivate>().unwrap();
        assert_eq!(
            q.get(entity).unwrap().target_stage,
            10,
            "OnActivate's SetStage(10) must win over OnTriggerEnter's SetStage(99)",
        );
    }

    /// A handler with no `SetStage` at all isn't a quest advance — decline.
    #[test]
    fn declines_handler_without_set_stage() {
        let src = "ScriptName Chatty extends ObjectReference\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   Debug.Notification(\"hi\")\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);
        assert!(translate_script(&source, GameKind::Skyrim, None, None).is_none());
    }

    /// The advance names a `Quest Property`, but the VMAD instance doesn't
    /// bind it (property absent) — the quest can't be resolved, so decline
    /// rather than attach an advance pointing at form 0.
    #[test]
    fn declines_when_quest_property_unbound() {
        let src = "ScriptName UnboundQuest extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   MyQuest.SetStage(20)\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);

        // VMAD present but binds a DIFFERENT property name.
        let instance = ScriptInstanceData {
            version: 5,
            object_format: 2,
            scripts: vec![ScriptInstance {
                name: "UnboundQuest".into(),
                status: 1,
                properties: vec![ScriptProperty {
                    name: "SomeOtherQuest".into(),
                    status: 1,
                    value: PropertyValue::Object {
                        form_id: 0x0001_2345,
                        alias: -1,
                    },
                }],
            }],
        };
        assert!(
            translate_script(&source, GameKind::Skyrim, Some(&instance), None).is_none(),
            "an unbound Quest property can't resolve — must decline",
        );
        // And with no VMAD at all: also declines.
        assert!(translate_script(&source, GameKind::Skyrim, None, None).is_none());
    }

    /// A body with extra logic alongside the SetStage is NOT treated as
    /// unconditional — `single_set_stage` requires the body be exactly the
    /// one call, so a script that also does something we don't model
    /// declines rather than dropping that behavior.
    /// The guarded shape holds the same exactly-one-statement invariant
    /// as shape 3: a guarded `If guard / SetStage / EndIf` followed by a
    /// sibling `Self.Disable()` carries logic beyond the advance, so it
    /// must DECLINE rather than emit the advance and silently drop the
    /// sibling. Pins SCR-D5-01.
    #[test]
    fn declines_guarded_with_extra_statements() {
        let src = "ScriptName NoisyGate extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   If MyQuest.GetStageDone(10) == 1\n\
                   MyQuest.SetStage(20)\n\
                   EndIf\n\
                   Self.Disable()\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("NoisyGate", 0x0000_8888);
        assert!(
            translate_script(&source, GameKind::Skyrim, Some(&instance), None).is_none(),
            "a sibling statement beside the guarded If must decline, not silently drop",
        );
    }

    #[test]
    fn declines_unconditional_with_extra_statements() {
        let src = "ScriptName NoisyActi extends ObjectReference\n\
                   Quest Property MyQuest Auto\n\
                   Event OnActivate(ObjectReference akActionRef)\n\
                   MyQuest.SetStage(30)\n\
                   Self.Disable()\n\
                   EndEvent\n";
        let (script, _) = parse_script(src).expect("parses");
        let source = ScriptSource::PapyrusSource(&script);
        let instance = quest_property_instance("NoisyActi", 0x0000_4444);
        assert!(
            translate_script(&source, GameKind::Skyrim, Some(&instance), None).is_none(),
            "extra unmodeled statements must decline, not silently drop",
        );
    }
}
