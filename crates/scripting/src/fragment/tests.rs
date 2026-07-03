//! Runtime tests for quest-stage fragment dispatch — `apply_effects`
//! against the canonical stores, and `quest_fragment_dispatch_system`
//! end-to-end (marker → fragment → state, including the SetStage cascade).

use super::*;
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
};
use crate::translate::compose::QuestRef;
use crate::translate::effects::Effect;
use byroredux_core::ecs::world::World;

const Q: QuestFormId = QuestFormId(0x0001_2345);

/// A world with the scripting subsystem registered, a player entity, and
/// a fresh quest-stage store — the standard fixture for dispatch tests.
fn fixture() -> World {
    let mut world = World::new();
    crate::register(&mut world);
    let player = world.spawn();
    world.insert_resource(PlayerEntity(player));
    world.insert_resource(QuestStageState::default());
    world
}

/// Place a single-entry `QuestStageAdvancedBatch` (the signal
/// `quest_advance_system` would emit) on the player entity, the sink the
/// dispatcher reads.
fn emit_advance(world: &World, quest: QuestFormId, new_stage: u16) {
    emit_advances(world, &[(quest, new_stage)]);
}

/// #1864 / SCR-D7-NEW-01 — place a batch carrying every `(quest, new_stage)`
/// pair given, exactly like `quest_advance_system`'s phase 3 does for
/// multiple same-frame advances.
fn emit_advances(world: &World, advances: &[(QuestFormId, u16)]) {
    let player = world.resource::<PlayerEntity>().0;
    let mut q = world.query_mut::<QuestStageAdvancedBatch>().unwrap();
    q.insert(
        player,
        QuestStageAdvancedBatch(
            advances
                .iter()
                .map(|&(quest, new_stage)| QuestStageAdvanced {
                    quest,
                    previous_stage: 0,
                    new_stage,
                })
                .collect(),
        ),
    );
}

#[test]
fn apply_effects_writes_stage_and_objectives() {
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let effects = vec![
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
        Effect::SetStage {
            quest: QuestRef::SelfRef,
            stage: 30,
        },
    ];
    let advances = apply_effects(&effects, Q, None, &mut stages, &mut objectives);

    assert_eq!(stages.get_stage(Q), 30);
    assert!(objectives.get(Q, 10).completed);
    assert!(objectives.get(Q, 20).displayed);
    // Only the SetStage produces a QuestStageAdvanced.
    assert_eq!(advances.len(), 1);
    assert_eq!(advances[0].new_stage, 30);
}

#[test]
fn property_targeted_effect_skipped_without_vmad() {
    // A Quest Property reference can't resolve with no VMAD on hand — the
    // effect is skipped, never guessed against a wrong quest.
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let effects = vec![Effect::SetStage {
        quest: QuestRef::Property("SomeOtherQuest".into()),
        stage: 99,
    }];
    let advances = apply_effects(&effects, Q, None, &mut stages, &mut objectives);
    assert!(advances.is_empty());
    assert_eq!(stages.get_stage(Q), 0, "no quest was touched");
}

#[test]
fn dispatch_system_runs_the_stage_fragment() {
    let world = fixture();
    // Register a fragment for (Q, stage 10): complete objective 1, show 2.
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetObjectiveCompleted {
                    quest: QuestRef::SelfRef,
                    objective: 1,
                    completed: true,
                },
                Effect::SetObjectiveDisplayed {
                    quest: QuestRef::SelfRef,
                    objective: 2,
                    displayed: true,
                },
            ],
        );
    }
    // Simulate the quest reaching stage 10, then dispatch.
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let objectives = world.resource::<QuestObjectiveState>();
    assert!(objectives.get(Q, 1).completed);
    assert!(objectives.get(Q, 2).displayed);
}

#[test]
fn dispatch_cascades_chained_set_stage() {
    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        // Stage 10's fragment advances to stage 20...
        frags.insert(
            Q,
            10,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20,
            }],
        );
        // ...and stage 20's fragment completes an objective.
        frags.insert(
            Q,
            20,
            vec![Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 5,
                completed: true,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    // The cascade ran stage 20's fragment too.
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 20);
    assert!(world.resource::<QuestObjectiveState>().get(Q, 5).completed);
}

#[test]
fn dispatch_is_noop_with_no_registered_fragments() {
    let world = fixture();
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    // No fragments registered (the runtime-today state) — must not panic
    // and must leave objective state untouched.
    quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<QuestObjectiveState>().get(Q, 1), Default::default());
}

#[test]
fn end_to_end_lower_then_dispatch() {
    // The full b2 pipe: a fragment .psc body → lower_fragment → register
    // at a stage → QuestStageAdvanced → dispatch → canonical state.
    use crate::translate::effects::lower_fragment;
    use byroredux_papyrus::ast::ScriptItem;
    use byroredux_papyrus::parse_script;

    let (script, errs) = parse_script(
        "ScriptName QF_Quest_0100 extends Quest\n\
         Function Fragment_0()\n\
         Quest kmyQuest = Self.GetOwningQuest()\n\
         kmyQuest.SetObjectiveCompleted(10)\n\
         kmyQuest.SetObjectiveDisplayed(20)\n\
         kmyQuest.SetStage(100)\n\
         EndFunction\n",
    )
    .expect("fragment parses");
    assert!(errs.is_empty(), "{errs:?}");
    let body = script
        .body
        .iter()
        .find_map(|i| match &i.node {
            ScriptItem::Function(f) => Some(f.body.clone()),
            _ => None,
        })
        .expect("fragment fn");
    let effects = lower_fragment(&body).expect("fragment lowers");

    let world = fixture();
    world.resource_mut::<QuestStageFragments>().insert(Q, 50, effects);
    world.resource_mut::<QuestStageState>().set_stage(Q, 50);
    emit_advance(&world, Q, 50);
    quest_fragment_dispatch_system(&world);

    let obj = world.resource::<QuestObjectiveState>();
    assert!(obj.get(Q, 10).completed);
    assert!(obj.get(Q, 20).displayed);
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 100);
}

#[test]
fn dispatch_ignores_stage_without_a_fragment() {
    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![Effect::SetStage {
                quest: QuestRef::SelfRef,
                stage: 20,
            }],
        );
    }
    // Advance announces stage 15, for which no fragment is registered.
    world.resource_mut::<QuestStageState>().set_stage(Q, 15);
    emit_advance(&world, Q, 15);
    quest_fragment_dispatch_system(&world);
    // Stage 10's fragment must NOT have run (we never reached stage 10).
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 15);
}

/// #1864 / SCR-D7-NEW-01 — two independently-recognized quest-advance REFRs
/// firing in the same tick must both be observable, not silently collapsed
/// to the last one written onto the shared sink entity. Emits a single
/// batch carrying advances for two DIFFERENT quests (mirroring
/// `quest_advance_system` phase 3's real-world trigger) and asserts the
/// dispatcher runs BOTH quests' fragments.
#[test]
fn two_same_frame_advances_for_different_quests_are_both_observed() {
    const Q2: QuestFormId = QuestFormId(0x0002_2222);

    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 1,
                completed: true,
            }],
        );
        frags.insert(
            Q2,
            20,
            vec![Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 2,
                completed: true,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    world.resource_mut::<QuestStageState>().set_stage(Q2, 20);
    // ONE batch, two advances — exactly what quest_advance_system emits
    // when two different scripted doors/triggers fire in the same frame.
    emit_advances(&world, &[(Q, 10), (Q2, 20)]);
    quest_fragment_dispatch_system(&world);

    let objectives = world.resource::<QuestObjectiveState>();
    assert!(
        objectives.get(Q, 1).completed,
        "the FIRST same-frame advance must not be lost"
    );
    assert!(
        objectives.get(Q2, 2).completed,
        "the SECOND same-frame advance must not be lost"
    );
}
