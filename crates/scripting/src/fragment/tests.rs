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
    let world = World::new();
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
    let advances = apply_effects(&effects, Q, None, &world, &mut stages, &mut objectives);

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
    let world = World::new();
    let mut stages = QuestStageState::default();
    let mut objectives = QuestObjectiveState::default();
    let effects = vec![Effect::SetStage {
        quest: QuestRef::Property("SomeOtherQuest".into()),
        stage: 99,
    }];
    let advances = apply_effects(&effects, Q, None, &world, &mut stages, &mut objectives);
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

    // QuestStageState before QuestObjectiveState — matches
    // `quest_fragment_dispatch_system`'s acquisition order for this
    // pair (#313).
    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 100);
    let obj = world.resource::<QuestObjectiveState>();
    assert!(obj.get(Q, 10).completed);
    assert!(obj.get(Q, 20).displayed);
}

#[test]
fn populate_from_script_binds_stages_to_the_right_fragments() {
    // Exercises the production population entry (the AST half of
    // `populate_quest_fragments_from_pex`): a QF_ script with several
    // Fragment_N functions + the QUST VMAD stage→Fragment_N bindings →
    // each stage dispatches its own fragment's effects. Mirrors the real
    // DA15Return shape where the Fragment_N ordinal ≠ the stage.
    use byroredux_papyrus::parse_script;

    let (script, errs) = parse_script(
        "ScriptName QF_TestQuest_00099 extends Quest\n\
         Function Fragment_6()\n Self.SetStage(200)\n EndFunction\n\
         Function Fragment_3()\n Self.SetObjectiveCompleted(10)\n EndFunction\n\
         Function Fragment_5()\n Self.SetObjectiveDisplayed(20)\n EndFunction\n",
    )
    .expect("QF_ script parses");
    assert!(errs.is_empty(), "{errs:?}");

    // Bindings as the VMAD decoder would surface them: stage → Fragment_N,
    // deliberately out of ordinal order.
    let bindings = [(0u16, "Fragment_6"), (200, "Fragment_3"), (10, "Fragment_5")];

    let world = fixture();
    let inserted = {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings)
    };
    assert_eq!(inserted, 3, "all three fragments lower + register");

    // Stage 10 → Fragment_5 → SetObjectiveDisplayed(20).
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);
    assert!(
        world.resource::<QuestObjectiveState>().get(Q, 20).displayed,
        "stage 10's fragment (Fragment_5) ran"
    );
    assert!(
        !world.resource::<QuestObjectiveState>().get(Q, 10).completed,
        "stage 3's fragment must NOT have run — wrong stage"
    );

    // Stage 0 → Fragment_6 → SetStage(200) → cascades to Fragment_3 →
    // SetObjectiveCompleted(10).
    emit_advance(&world, Q, 0);
    quest_fragment_dispatch_system(&world);
    assert_eq!(
        world.resource::<QuestStageState>().get_stage(Q),
        200,
        "Fragment_6 advanced the quest to stage 200"
    );
    assert!(
        world.resource::<QuestObjectiveState>().get(Q, 10).completed,
        "the stage-200 cascade ran Fragment_3"
    );
}

#[test]
fn populate_from_script_skips_absent_and_declined_fragments() {
    use byroredux_papyrus::parse_script;

    let (script, errs) = parse_script(
        "ScriptName QF_X_0 extends Quest\n\
         Function Fragment_0()\n Self.SetStage(20)\n EndFunction\n\
         Function Fragment_1()\n Debug.Notification(\"hi\")\n EndFunction\n",
    )
    .expect("parses");
    assert!(errs.is_empty(), "{errs:?}");

    let bindings = [
        (5u16, "Fragment_0"),  // lowers cleanly
        (6, "Fragment_1"),     // unmodeled call → declines
        (7, "Fragment_99"),    // absent → skipped
    ];
    let world = fixture();
    let inserted = {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings)
    };
    assert_eq!(inserted, 1, "only the fully-lowered fragment registers");
    let frags = world.resource::<QuestStageFragments>();
    assert!(frags.get(Q, 5).is_some());
    assert!(frags.get(Q, 6).is_none(), "declined fragment not registered");
    assert!(frags.get(Q, 7).is_none(), "absent fragment not registered");
}

#[test]
fn dispatch_resolves_property_targeted_effect_via_registered_vmad() {
    // The counterpart to `property_targeted_effect_skipped_without_vmad`:
    // once the quest's own VMAD scripts-section is registered (what a real
    // cell load does from `QustRecord.script_instance`), a fragment's
    // `Quest Property OtherQuest` reference resolves to the bound quest and
    // its SetStage actually lands — on the OTHER quest, not Q.
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const OTHER: QuestFormId = QuestFormId(0x0009_9999);

    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_TestQuest_00012345".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "OtherQuest".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: OTHER.0,
                            alias: -1,
                        },
                    }],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::SetStage {
                quest: QuestRef::Property("OtherQuest".into()),
                stage: 77,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    assert_eq!(
        world.resource::<QuestStageState>().get_stage(OTHER),
        77,
        "the Property-bound OTHER quest advanced"
    );
    assert_eq!(
        world.resource::<QuestStageState>().get_stage(Q),
        10,
        "Q itself must be untouched — the effect targeted OTHER, not Q"
    );
}

#[test]
fn cascade_does_not_drop_a_different_quests_transition_that_collides_on_stage_number() {
    // #2124 — regression for the cascade guard comparing `adv.new_stage`
    // against the *currently-dispatching* fragment's own stage instead of
    // `adv.previous_stage`. Here Q's stage-10 fragment sets a DIFFERENT
    // quest (OTHER) to stage 10 too — the same numeric value Q is
    // currently dispatching at, purely by coincidence (quest stage
    // numbers cluster around round values across independently-authored
    // quests). Pre-fix, `adv.new_stage(10) != stage(10)` was false, so
    // OTHER's genuine 0→10 transition was silently never queued and its
    // stage-10 fragment never ran.
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const OTHER: QuestFormId = QuestFormId(0x0009_9999);

    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_TestQuest_00012345".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "OtherQuest".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: OTHER.0,
                            alias: -1,
                        },
                    }],
                }],
            },
        );
        // Q's own stage-10 fragment: set OTHER to stage 10 — the same
        // number Q is currently dispatching at.
        frags.insert(
            Q,
            10,
            vec![Effect::SetStage {
                quest: QuestRef::Property("OtherQuest".into()),
                stage: 10,
            }],
        );
        // OTHER's stage-10 fragment: an observable, unmistakably genuine
        // side effect that must run once the cascade reaches it.
        frags.insert(
            OTHER,
            10,
            vec![Effect::SetObjectiveCompleted {
                quest: QuestRef::SelfRef,
                objective: 42,
                completed: true,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    assert_eq!(
        world.resource::<QuestStageState>().get_stage(OTHER),
        10,
        "OTHER's SetStage still lands even though the cascade guard was buggy"
    );
    assert!(
        world.resource::<QuestObjectiveState>().get(OTHER, 42).completed,
        "OTHER's stage-10 fragment must have been cascaded into and run — \
         the pre-#2124 guard compared against Q's own dispatching stage and \
         silently dropped this transition on the 10==10 coincidence"
    );
}

/// Spawn an entity carrying a `FormIdComponent` resolving to `form_id_raw`
/// — the fixture every object-targeting-effect dispatch test needs so
/// `resolve_entity_by_global_form_id` can find it. Mirrors the identical
/// helper in `byroredux/src/systems/escort.rs`'s tests.
fn spawn_with_form_id(
    world: &mut World,
    form_id_raw: u32,
) -> byroredux_core::ecs::storage::EntityId {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    world.register::<FormIdComponent>();
    let mut pool = world
        .remove_resource::<FormIdPool>()
        .unwrap_or_else(FormIdPool::new);
    let fid = pool.intern(FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(form_id_raw),
    });
    world.insert_resource(pool);

    let entity = world.spawn();
    world.insert(entity, FormIdComponent(fid));
    entity
}

#[test]
fn dispatch_add_item_via_registered_vmad() {
    use byroredux_core::ecs::components::Inventory;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const CONTAINER_FORM: u32 = 0x0000_5000;
    const ITEM_FORM: u32 = 0x0000_1234;

    let mut world = fixture();
    world.register::<Inventory>();
    let container = spawn_with_form_id(&mut world, CONTAINER_FORM);

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_Test".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "SomeContainer".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: CONTAINER_FORM,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "SomeItem".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: ITEM_FORM,
                                alias: -1,
                            },
                        },
                    ],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::AddItem {
                container: crate::translate::compose::ObjectRef::Property("SomeContainer".into()),
                item: crate::translate::compose::ObjectRef::Property("SomeItem".into()),
                count: 3,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let inv = world
        .get::<Inventory>(container)
        .expect("AddItem inserted an Inventory on demand");
    assert_eq!(inv.items.len(), 1);
    assert_eq!(inv.items[0].base_form_id, ITEM_FORM);
    assert_eq!(inv.items[0].count, 3);
}

#[test]
fn dispatch_add_item_pushes_onto_an_existing_inventory() {
    use byroredux_core::ecs::components::{Inventory, ItemStack};
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const CONTAINER_FORM: u32 = 0x0000_5001;
    const ITEM_FORM: u32 = 0x0000_1235;

    let mut world = fixture();
    world.register::<Inventory>();
    let container = spawn_with_form_id(&mut world, CONTAINER_FORM);
    world.insert(
        container,
        Inventory {
            items: vec![ItemStack::new(0x0000_9999, 1)],
        },
    );

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_Test".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "SomeContainer".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: CONTAINER_FORM,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "SomeItem".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: ITEM_FORM,
                                alias: -1,
                            },
                        },
                    ],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::AddItem {
                container: crate::translate::compose::ObjectRef::Property("SomeContainer".into()),
                item: crate::translate::compose::ObjectRef::Property("SomeItem".into()),
                count: 3,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let inv = world.get::<Inventory>(container).expect("still present");
    assert_eq!(inv.items.len(), 2, "pushed onto the existing stack list");
    assert_eq!(inv.items[0].base_form_id, 0x0000_9999, "prior stack untouched");
    assert_eq!(inv.items[1].base_form_id, ITEM_FORM);
    assert_eq!(inv.items[1].count, 3);
}

#[test]
fn dispatch_move_to_via_registered_vmad() {
    use byroredux_core::ecs::components::{GlobalTransform, Transform};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const MOVED_FORM: u32 = 0x0000_6000;
    const DEST_FORM: u32 = 0x0000_7000;

    let mut world = fixture();
    world.register::<Transform>();
    world.register::<GlobalTransform>();
    let moved = spawn_with_form_id(&mut world, MOVED_FORM);
    world.insert(moved, Transform::from_translation(Vec3::ZERO));
    let destination = spawn_with_form_id(&mut world, DEST_FORM);
    world.insert(
        destination,
        GlobalTransform::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY, 1.0),
    );

    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "QF_Test".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "SomeRef".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: MOVED_FORM,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "SomeMarker".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: DEST_FORM,
                                alias: -1,
                            },
                        },
                    ],
                }],
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::MoveTo {
                moved: crate::translate::compose::ObjectRef::Property("SomeRef".into()),
                destination: crate::translate::compose::ObjectRef::Property("SomeMarker".into()),
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    let transform = world.get::<Transform>(moved).expect("still present");
    assert_eq!(transform.translation, Vec3::new(10.0, 20.0, 30.0));
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
