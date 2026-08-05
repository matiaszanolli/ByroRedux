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
use crate::CinematicAnimationEvent;
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
    assert_eq!(
        world.resource::<QuestObjectiveState>().get(Q, 1),
        Default::default()
    );
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
    world
        .resource_mut::<QuestStageFragments>()
        .insert(Q, 50, effects);
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
    let bindings = [
        (0u16, "Fragment_6"),
        (200, "Fragment_3"),
        (10, "Fragment_5"),
    ];

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
        (5u16, "Fragment_0"), // lowers cleanly
        (6, "Fragment_1"),    // unmodeled call → declines
        (7, "Fragment_99"),   // absent → skipped
    ];
    let world = fixture();
    let inserted = {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        populate_quest_fragments_from_script(&mut frags, Q, &script, &bindings)
    };
    assert_eq!(inserted, 1, "only the fully-lowered fragment registers");
    let frags = world.resource::<QuestStageFragments>();
    assert!(frags.get(Q, 5).is_some());
    assert!(
        frags.get(Q, 6).is_none(),
        "declined fragment not registered"
    );
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

/// #2186 — the same dispatch, but the `Quest Property` is alias-bound.
///
/// The effect must be SKIPPED, not resolved against the raw `form_id`
/// stored beside the alias index. Resolving it anyway means a `SetStage`
/// silently mutating an unrelated quest's state — a wrong lowering, not a
/// decline. `ObjectRef::Property` already declined this way; the
/// `QuestRef::Property` path went through the lax `object_form_id`.
#[test]
fn dispatch_skips_property_targeted_effect_when_the_property_is_alias_bound() {
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
                        // Identical to the resolving test above except
                        // for this: alias-bound, so the form_id beside it
                        // is not the intended target.
                        value: PropertyValue::Object {
                            form_id: OTHER.0,
                            alias: 3,
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
        0,
        "an alias-bound Quest property must decline — advancing OTHER here \
         would be a wrong lowering, mutating a quest the author never targeted"
    );
    assert_eq!(
        world.resource::<QuestStageState>().get_stage(Q),
        10,
        "Q itself must also be untouched"
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
        world
            .resource::<QuestObjectiveState>()
            .get(OTHER, 42)
            .completed,
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
    let mut pool = world.remove_resource::<FormIdPool>().unwrap_or_default();
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
    assert_eq!(
        inv.items[0].base_form_id, 0x0000_9999,
        "prior stack untouched"
    );
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
fn dispatch_scene_start_via_registered_vmad() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };
    use byroredux_plugin::esm::records::ScenRecord;

    const SCENE_FORM: u32 = 0x0000_8000;
    let mut world = fixture();
    crate::install_scene_records(
        &mut world,
        [ScenRecord {
            form_id: SCENE_FORM,
            ..Default::default()
        }],
    );
    let scene_entity = world
        .resource::<crate::SceneRegistry>()
        .scene_entity(SCENE_FORM)
        .unwrap();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_Test".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "IntroScene".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: SCENE_FORM,
                            alias: -1,
                        },
                    }],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![Effect::StartScene {
                scene: crate::translate::compose::ObjectRef::Property("IntroScene".into()),
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);

    assert!(world.has::<crate::SceneStartRequest>(scene_entity));
}

#[test]
fn dispatch_activate_then_set_open_updates_mq101_style_gate() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const GATE_FORM: u32 = 0x0009_0A05;
    const LEVER_ALIAS: i16 = 40;
    const SOLDIER_ALIAS: i16 = 41;
    let mut world = fixture();
    let gate = spawn_with_form_id(&mut world, GATE_FORM);
    let lever = world.spawn();
    let soldier = world.spawn();
    {
        let mut bindings = world.resource_mut::<crate::SceneActorBindings>();
        bindings.bind(Q, i32::from(LEVER_ALIAS), lever);
        bindings.bind(Q, i32::from(SOLDIER_ALIAS), soldier);
    }
    world.insert(
        gate,
        crate::TwoStateActivator {
            do_once: true,
            ..Default::default()
        },
    );
    world.insert(gate, crate::ScriptVariables::default());
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_MQ101".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "Alias_KeepLever1".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: LEVER_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "Alias_BarracksRoomSoldier02".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: SOLDIER_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "KeepGate1".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: GATE_FORM,
                                alias: -1,
                            },
                        },
                    ],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::Activate {
                    target: crate::translate::compose::ObjectRef::Property(
                        "::Alias_KeepLever1_var".into(),
                    ),
                    activator: Some(crate::translate::compose::ObjectRef::Property(
                        "::Alias_BarracksRoomSoldier02_var".into(),
                    )),
                },
                Effect::SetOpen {
                    target: crate::translate::compose::ObjectRef::Property(
                        "::KeepGate1_var".into(),
                    ),
                    open: true,
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);

    assert_eq!(
        world.get::<crate::ActivateEvent>(lever).unwrap().activator,
        soldier
    );
    assert!(world.get::<crate::TwoStateActivator>(gate).unwrap().is_open);
    assert_eq!(
        world
            .get::<crate::ScriptVariables>(gate)
            .unwrap()
            .get_by_name("::isOpen_var"),
        Some(1.0)
    );
}

#[test]
fn dispatch_player_control_and_evaluate_package_effects() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const HADVAR_ALIAS: i16 = 1;
    let mut world = fixture();
    let player = world.resource::<PlayerEntity>().0;
    let hadvar = world.spawn();
    world
        .resource_mut::<crate::SceneActorBindings>()
        .bind(Q, i32::from(HADVAR_ALIAS), hadvar);
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_MQ101".into(),
                    status: 0,
                    properties: vec![ScriptProperty {
                        name: "Alias_Hadvar".into(),
                        status: 1,
                        value: PropertyValue::Object {
                            form_id: 0,
                            alias: HADVAR_ALIAS,
                        },
                    }],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetPlayerRestrained { restrained: true },
                Effect::SetPlayerControls {
                    enabled: false,
                    selection: crate::PlayerControlSelection {
                        movement: true,
                        fighting: false,
                        looking: true,
                        ..crate::PlayerControlSelection::PAPYRUS_DEFAULT
                    },
                },
                Effect::SetPlayerAiDriven { ai_driven: true },
                Effect::SetHudCartMode { cart_mode: true },
                Effect::EvaluatePackage {
                    actor: crate::translate::compose::ObjectRef::Property(
                        "::Alias_Hadvar_var".into(),
                    ),
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);

    assert!(
        world
            .get::<crate::ActorControlState>(player)
            .unwrap()
            .restrained
    );
    let controls = world.resource::<crate::PlayerControlState>();
    assert!(!controls.movement_enabled);
    assert!(controls.fighting_enabled, "unselected domain is untouched");
    assert!(!controls.looking_enabled);
    assert!(controls.ai_driven);
    assert!(controls.hud_cart_mode);
    assert!(world.has::<crate::EvaluatePackageRequest>(hadvar));
}

#[test]
fn dispatch_cart_animation_vehicle_and_motion_effects() {
    use crate::cinematic::CinematicAnimationEvent;
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::MotionType;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const RIDER_ALIAS: i16 = 40;
    const CART_ALIAS: i16 = 41;
    const PLAYER_IDLE: u32 = 0x0001_1000;
    const EXIT_IDLE_D: u32 = 0x0001_1003;
    let mut world = fixture();
    let player = world.resource::<PlayerEntity>().0;
    let rider = world.spawn();
    let cart = world.spawn();
    world.register::<byroredux_core::ecs::components::Transform>();
    world.insert(
        player,
        byroredux_core::ecs::components::Transform::from_translation(
            byroredux_core::math::Vec3::new(12.0, 0.0, 0.0),
        ),
    );
    let rider_exit_rotation = byroredux_core::math::Quat::from_rotation_y(0.75);
    world.insert(
        rider,
        byroredux_core::ecs::components::Transform::new(
            byroredux_core::math::Vec3::ZERO,
            rider_exit_rotation,
            1.0,
        ),
    );
    world.insert(
        cart,
        byroredux_core::ecs::components::Transform::from_translation(
            byroredux_core::math::Vec3::new(10.0, 0.0, 0.0),
        ),
    );
    {
        let mut bindings = world.resource_mut::<crate::SceneActorBindings>();
        bindings.bind(Q, i32::from(RIDER_ALIAS), rider);
        bindings.bind(Q, i32::from(CART_ALIAS), cart);
    }
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "MQ101QuestScript".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "Alias_Rider".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: RIDER_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "Alias_Cart".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: CART_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "PlayerIdle".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: PLAYER_IDLE,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "IdleCartPassengerDExit".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: EXIT_IDLE_D,
                                alias: -1,
                            },
                        },
                    ],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetVehicle {
                    actor: ActorRef::Player,
                    vehicle: Some(crate::translate::compose::ObjectRef::Property(
                        "Alias_Cart".into(),
                    )),
                },
                Effect::PlayIdle {
                    actor: ActorRef::Player,
                    idle: crate::translate::compose::ObjectRef::Property("PlayerIdle".into()),
                },
                Effect::ExitCart {
                    actor: crate::translate::compose::ObjectRef::Property("Alias_Rider".into()),
                    seat: 3,
                },
                Effect::SetMotionType {
                    target: crate::translate::compose::ObjectRef::Property("Alias_Cart".into()),
                    motion_type: MotionType::Keyframed,
                    allow_activate: true,
                },
                Effect::SetSittingRotation { degrees: -55.0 },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::PlayImod,
                },
                Effect::RegisterPlayerAnimationEvent {
                    event: CinematicAnimationEvent::IdleFurnitureExit,
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);

    let player_cinematic = world.get::<crate::ActorCinematicState>(player).unwrap();
    assert_eq!(player_cinematic.requested_idle_form_id, Some(PLAYER_IDLE));
    assert_eq!(player_cinematic.vehicle, Some(cart));
    assert_eq!(
        player_cinematic.vehicle_local_translation,
        Some(byroredux_core::math::Vec3::new(2.0, 0.0, 0.0))
    );
    let rider_cinematic = world.get::<crate::ActorCinematicState>(rider).unwrap();
    assert_eq!(rider_cinematic.vehicle, None);
    assert_eq!(rider_cinematic.cart_seat, Some(3));
    assert_eq!(rider_cinematic.requested_idle_form_id, Some(EXIT_IDLE_D));
    assert_eq!(
        rider_cinematic.awaited_event,
        Some(CinematicAnimationEvent::ExitCartEnd)
    );
    assert_eq!(
        rider_cinematic.exit_root_motion_rotation,
        Some(rider_exit_rotation)
    );
    assert_eq!(
        world
            .get::<crate::MotionTypeChangeRequest>(cart)
            .unwrap()
            .motion_type,
        MotionType::Keyframed
    );
    let presentation = world.resource::<crate::CinematicPresentationState>();
    assert_eq!(presentation.sitting_rotation_degrees, -55.0);
    assert!(presentation.is_player_animation_event_registered(CinematicAnimationEvent::PlayImod));
    assert!(presentation
        .is_player_animation_event_registered(CinematicAnimationEvent::IdleFurnitureExit));
}

#[test]
fn furniture_exit_callback_advances_quest_and_dispatches_stage_fragment_once() {
    let world = fixture();
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        fragments.insert(Q, 160, vec![Effect::SetSittingRotation { degrees: 0.0 }]);
    }
    {
        let mut presentation = world.resource_mut::<crate::CinematicPresentationState>();
        presentation.sitting_rotation_degrees = -55.0;
        presentation.register_player_animation_event(
            CinematicAnimationEvent::IdleFurnitureExit,
            Q,
            Vec::new(),
        );
    }

    let advance = crate::dispatch_player_cinematic_animation_event(
        &world,
        CinematicAnimationEvent::IdleFurnitureExit,
    )
    .expect("registered callback should advance MQ101");
    assert_eq!(advance.new_stage, 160);
    quest_fragment_dispatch_system(&world);

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 160);
    let presentation = world.resource::<crate::CinematicPresentationState>();
    assert_eq!(presentation.sitting_rotation_degrees, 0.0);
    assert!(!presentation
        .is_player_animation_event_registered(CinematicAnimationEvent::IdleFurnitureExit));
    assert_eq!(presentation.player_animation_event_serial, 1);
    drop(presentation);

    assert!(crate::dispatch_player_cinematic_animation_event(
        &world,
        CinematicAnimationEvent::IdleFurnitureExit
    )
    .is_none());
}

#[test]
fn play_imod_callback_resolves_vmad_applications_before_stage_handoff() {
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const PLAYER_ALDUIN_IMAD: u32 = 0x0010_1DAC;
    const DRAGON_ATTACK_BLUR_IMAD: u32 = 0x0010_CDC7;
    let world = fixture();
    {
        let mut fragments = world.resource_mut::<QuestStageFragments>();
        fragments.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "MQ101QuestScript".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "PlayerAlduinIMOD".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: PLAYER_ALDUIN_IMAD,
                                alias: -1,
                            },
                        },
                        ScriptProperty {
                            name: "CGDragonAttackBlurLong".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: DRAGON_ATTACK_BLUR_IMAD,
                                alias: -1,
                            },
                        },
                    ],
                }],
                ..Default::default()
            },
        );
        fragments.insert(
            Q,
            10,
            vec![Effect::RegisterPlayerAnimationEvent {
                event: CinematicAnimationEvent::PlayImod,
            }],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);
    quest_fragment_dispatch_system(&world);

    crate::dispatch_player_cinematic_animation_event(&world, CinematicAnimationEvent::PlayImod)
        .expect("fragment should register callback");

    assert_eq!(world.resource::<QuestStageState>().get_stage(Q), 145);
    let presentation = world.resource::<crate::CinematicPresentationState>();
    assert_eq!(
        presentation.applied_image_space_modifiers,
        vec![
            crate::ImageSpaceModifierApplication {
                form_id: PLAYER_ALDUIN_IMAD,
                strength: 1.0,
            },
            crate::ImageSpaceModifierApplication {
                form_id: DRAGON_ATTACK_BLUR_IMAD,
                strength: 1.0,
            },
        ]
    );
    assert!(!presentation.is_player_animation_event_registered(CinematicAnimationEvent::PlayImod));
}

#[test]
fn actor_3d_load_gate_polls_without_blocking_then_resumes() {
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::Transform;

    let mut world = fixture();
    world.register::<Transform>();
    let player = world.resource::<PlayerEntity>().0;
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::WaitForActors3DLoaded {
                    actors: vec![ActorRef::Player],
                    poll_seconds: 0.2,
                },
                Effect::SetHudCartMode { cart_mode: true },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);
    assert!(!world.resource::<crate::PlayerControlState>().hud_cart_mode);

    fragment_continuation_system(&world, 0.2);
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);
    world.insert(player, Transform::IDENTITY);
    fragment_continuation_system(&world, 0.2);

    assert!(world.resource::<FragmentExecutionQueue>().is_empty());
    assert!(world.resource::<crate::PlayerControlState>().hud_cart_mode);
}

/// Regression: #2288 (SCR-D6-NEW5-02) — a `WaitForActors3DLoaded`
/// continuation whose actors never resolve (e.g. a permanently-unloadable
/// alias target, or a quest reset out from under the suspended tail) must
/// eventually be dropped instead of re-arming forever. Sibling of
/// `actor_3d_load_gate_polls_without_blocking_then_resumes`, but the
/// actor's `Transform` is never inserted.
#[test]
fn actor_3d_load_gate_gives_up_after_max_wait_and_declines_tail() {
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::Transform;

    let mut world = fixture();
    world.register::<Transform>();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::WaitForActors3DLoaded {
                    actors: vec![ActorRef::Player],
                    poll_seconds: 10.0,
                },
                Effect::SetHudCartMode { cart_mode: true },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);

    // The player entity never receives a Transform, so actors_3d_loaded
    // never resolves. Each 10s tick burns one poll_seconds interval;
    // after MAX_ACTORS_3D_LOADED_WAIT_SECONDS worth of retries the entry
    // must be dropped rather than retained for the rest of the process.
    let ticks = (MAX_ACTORS_3D_LOADED_WAIT_SECONDS / 10.0).ceil() as u32 + 1;
    for _ in 0..ticks {
        fragment_continuation_system(&world, 10.0);
    }

    assert!(
        world.resource::<FragmentExecutionQueue>().is_empty(),
        "WaitForActors3DLoaded must give up once the wait cap is exceeded, not retry forever"
    );
    // The declined tail's SetHudCartMode must never fire — this is a
    // give-up, not a resume.
    assert!(!world.resource::<crate::PlayerControlState>().hud_cart_mode);
}

#[test]
fn dispatch_tethers_cart_and_equips_carried_armor() {
    use crate::translate::effects::ActorRef;
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, ItemStack, Transform};
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::script_instance::{
        PropertyValue, ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    const RIDER_ALIAS: i16 = 40;
    const CART_ALIAS: i16 = 41;
    const HORSE_ALIAS: i16 = 42;
    const GAG: u32 = 0x0004_6B44;
    const GAG_SLOT: u32 = 1 << 12;

    let mut world = fixture();
    world.register::<Transform>();
    let rider = world.spawn();
    let cart = world.spawn();
    let horse = world.spawn();
    world.insert(
        rider,
        Inventory {
            items: vec![ItemStack::new(0xdead, 1), ItemStack::new(GAG, 1)],
        },
    );
    world.insert(rider, EquipmentSlots::new());
    world.insert(cart, Transform::from_translation(Vec3::ZERO));
    world.insert(
        horse,
        Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
    );
    crate::install_equip_item_catalog(&mut world, [(GAG, GAG_SLOT)]);
    {
        let mut bindings = world.resource_mut::<crate::SceneActorBindings>();
        bindings.bind(Q, i32::from(RIDER_ALIAS), rider);
        bindings.bind(Q, i32::from(CART_ALIAS), cart);
        bindings.bind(Q, i32::from(HORSE_ALIAS), horse);
    }
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert_vmad(
            Q,
            ScriptInstanceData {
                scripts: vec![ScriptInstance {
                    name: "QF_MQ101".into(),
                    status: 0,
                    properties: vec![
                        ScriptProperty {
                            name: "Alias_Rider".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: RIDER_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "Alias_Cart".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: CART_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "Alias_Horse".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: 0,
                                alias: HORSE_ALIAS,
                            },
                        },
                        ScriptProperty {
                            name: "ArmorGag".into(),
                            status: 1,
                            value: PropertyValue::Object {
                                form_id: GAG,
                                alias: -1,
                            },
                        },
                    ],
                }],
                ..Default::default()
            },
        );
        frags.insert(
            Q,
            10,
            vec![
                Effect::TetherToHorse {
                    cart: crate::translate::compose::ObjectRef::Property("Alias_Cart".into()),
                    horse: crate::translate::compose::ObjectRef::Property("Alias_Horse".into()),
                },
                Effect::EquipItem {
                    actor: ActorRef::Object(crate::translate::compose::ObjectRef::Property(
                        "Alias_Rider".into(),
                    )),
                    item: crate::translate::compose::ObjectRef::Property("ArmorGag".into()),
                    silent: false,
                },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);

    let tether = world.get::<crate::HorseTetherState>(cart).unwrap();
    assert_eq!(tether.horse, horse);
    assert_eq!(tether.horse_local_translation, Vec3::new(-10.0, 0.0, 0.0));
    assert_eq!(
        world
            .get::<crate::MotionTypeChangeRequest>(cart)
            .unwrap()
            .motion_type,
        byroredux_core::ecs::components::MotionType::Keyframed
    );
    assert_eq!(
        world.get::<EquipmentSlots>(rider).unwrap().at(12),
        Some(InventoryIndex(1))
    );
}

#[test]
fn utility_wait_resumes_fragment_tail_in_order() {
    let world = fixture();
    {
        let mut frags = world.resource_mut::<QuestStageFragments>();
        frags.insert(
            Q,
            10,
            vec![
                Effect::SetSittingRotation { degrees: 10.0 },
                Effect::Wait { seconds: 0.5 },
                Effect::SetSittingRotation { degrees: -55.0 },
                Effect::Wait { seconds: 0.25 },
                Effect::SetHudCartMode { cart_mode: true },
            ],
        );
    }
    world.resource_mut::<QuestStageState>().set_stage(Q, 10);
    emit_advance(&world, Q, 10);

    quest_fragment_dispatch_system(&world);
    assert_eq!(
        world
            .resource::<crate::CinematicPresentationState>()
            .sitting_rotation_degrees,
        10.0
    );
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);

    fragment_continuation_system(&world, 0.6);
    assert_eq!(
        world
            .resource::<crate::CinematicPresentationState>()
            .sitting_rotation_degrees,
        -55.0
    );
    assert_eq!(world.resource::<FragmentExecutionQueue>().len(), 1);
    assert!(!world.resource::<crate::PlayerControlState>().hud_cart_mode);

    fragment_continuation_system(&world, 0.3);
    assert!(world.resource::<FragmentExecutionQueue>().is_empty());
    assert!(world.resource::<crate::PlayerControlState>().hud_cart_mode);
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
